use chrono::{DateTime, Local};
use thiserror::Error;

use std::{
    ffi::CStr,
    fmt,
    fs::DirEntry,
    os::unix::fs::MetadataExt,
    path::PathBuf,
    time::{Duration, SystemTime, UNIX_EPOCH},
    vec,
};

/// Represents a single process discovered under `/proc`.
#[derive(Debug)]
pub struct Process {
    pid: u32,
    cmdline: Option<String>,
    binary_path: Option<PathBuf>,
    owner: Option<String>,
    start_time: Option<DateTime<Local>>,
    state: Option<String>,
}

/// Errors that can occur when reading or parsing process information.
#[derive(Error, Debug)]
pub enum PsError {
    /// Generic I/O error while reading from `/proc`.
    #[error("I/O error")]
    FailedToReadFile(#[from] std::io::Error),

    /// Failed to extract uptime from `/proc/uptime`.
    #[error("Failed to get uptime from stat")]
    FailedToGetUptimeFromStat,

    /// Failed to parse numeric values (e.g., uptime, ticks).
    #[error("Failed to parse as float")]
    FailedToParseAsFloat(#[from] std::num::ParseFloatError),

    /// Failed to get the current system time.
    #[error("Failed to get system time")]
    FailedToGetSystemTime(#[from] std::time::SystemTimeError),

    /// Failed to read the system clock tick rate via `sysconf`.
    #[error("Failed to get system clock tick rate: {0}")]
    FailedToGetSysClockTickRate(i32),
}

/// Extracts the process state from `/proc/<pid>/status`.
///
/// Looks for a line starting with `State:` and returns the status
/// string (e.g., `"S (sleeping)"`).
fn find_state(status: &str) -> Option<String> {
    for line in status.lines() {
        if line.starts_with("State:") {
            // Map allows safety.
            let process_state = line.split_once('\t').map(|x| x.1);
            return process_state.map(|s| s.to_string());
        }
    }
    None
}

/// Pretty-print implementation for `Process`.
impl fmt::Display for Process {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if f.alternate() {
            writeln!(
                f,
                "{:<10} {:<15} {:<15} {:<30} {:<20} {:<15}",
                "PID", "Owner", "Cmdline", "Binary Path", "Start Time", "State",
            )?;
        }

        let start_time = match self.start_time {
            // Format the datetime as a normal readable string.
            Some(date_time) => date_time.format("%Y-%m-%d %H:%M:%S").to_string(),
            None => "unknown".to_owned(),
        };

        writeln!(
            f,
            "{:<10} {:<15} {:<15} {:<30}      {:<20} {:<15}",
            self.pid,
            self.owner.as_deref().unwrap_or("-"),
            self.cmdline.as_deref().unwrap_or("-"),
            self.binary_path
                .clone()
                // Pathbuf implements default so can use unwrap_or_
                .unwrap_or_default()
                .to_string_lossy(),
            start_time,
            self.state.as_deref().unwrap_or("-")
        )
    }
}

/// Returns the start time of a process by reading `/proc/uptime` and `/proc/<pid>/stat`.
///
/// * `uptime_path` — Path to `/proc/uptime`  
/// * `stat_path` — Path to `/proc/<pid>/stat`  
/// * `system_clock_tick_rate` — Clock ticks per second from `sysconf(_SC_CLK_TCK)`
///
/// Note: functions that don't need ownership take reference.
fn get_start_time(
    uptime_path: &PathBuf,
    stat_path: &PathBuf,
    system_clock_tick_rate: f64,
) -> Result<DateTime<Local>, PsError> {
    let uptime_res = std::fs::read_to_string(uptime_path)?;

    let uptime_seconds: f64 = uptime_res
        .split_whitespace()
        .next()
        // ok_or checks some (if there is a value) if not errors.
        .ok_or(PsError::FailedToGetUptimeFromStat)?
        // tries to turn "48267.42" into f64.
        .parse()?;

    let stat = std::fs::read_to_string(stat_path)?;
    let stats: Vec<&str> = stat.split_whitespace().collect();

    // start_time is at the 22nd column.
    let time_stat_str = stats[21];
    let time_stat: f64 = time_stat_str.parse()?;
    // convert start_time to seconds since boot.
    let start_time_in_seconds = time_stat / system_clock_tick_rate;
    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs_f64();
    let boot_time = now - uptime_seconds;
    let process_start_time = (boot_time + start_time_in_seconds) as u64;

    let duration = Duration::from_secs(process_start_time);
    let system_time = UNIX_EPOCH + duration;
    let date_time: DateTime<Local> = system_time.into();

    Ok(date_time)
}

/// Attempts to parse a single process directory into a [`Process`] struct.
///
/// This reads values from several `/proc/<pid>/...` files:
/// - `cmdline`  
/// - `exe` (symlink)  
/// - `stat` (start time)  
/// - `status` (state)  
/// - directory metadata (UID → username)
///
/// Returns `None` if the directory name is not a PID or if the process
/// disappears during parsing.
fn get_process(dir_ent: DirEntry, system_clock_tick_rate: f64) -> Option<Process> {
    let path = "/proc";
    // Only parse filenames if they are numbers (process').
    match dir_ent.file_name().to_string_lossy().parse::<u32>() {
        Ok(filename) => {
            let cmdline = format!("{path}/{filename}/cmdline");
            let binary_path = format!("{path}/{filename}/exe");
            let stat_path = PathBuf::from(format!("{path}/{filename}/stat"));
            let uptime_path = PathBuf::from(format!("{path}/uptime"));
            let state_path = format!("{path}/{filename}/status");

            let mut process = Process {
                pid: filename,
                cmdline: None,
                binary_path: None,
                owner: None,
                start_time: None,
                state: None,
            };

            // Read command line.
            if let Ok(cmd) = std::fs::read_to_string(cmdline) {
                process.cmdline = Some(cmd);
            }

            // Read executable symlink.
            // Note: .ok() converts result into success case or None
            process.binary_path = std::fs::read_link(binary_path).ok();

            // Extract owner name from UID.
            if let Ok(metadata) = dir_ent.metadata() {
                let owner_id = metadata.uid();
                let owner = unsafe {
                    // getpwuid_r is thread safe because we provide our own buffer
                    // Will return null if no matching entry
                    let res = libc::getpwuid(owner_id);
                    if res.is_null() {
                        Some(owner_id.to_string())
                    } else {
                        let passwd = *res;
                        // Construct rust string from raw pointer
                        let owner = CStr::from_ptr(passwd.pw_name);
                        // Note: to_string_lossy converts the bytes it can to string or gives up
                        Some(owner.to_string_lossy().to_string())
                    }
                };
                process.owner = owner;
            }

            // Start time.
            match get_start_time(&uptime_path, &stat_path, system_clock_tick_rate) {
                Ok(date_time) => process.start_time = Some(date_time),
                Err(e) => eprintln!("{}", e),
            }

            // Process state.
            if let Ok(state_res) = std::fs::read_to_string(state_path) {
                let process_state = find_state(&state_res);
                process.state = process_state;
            }

            Some(process)
        }

        Err(_) => None,
    }
}

/// Reads and returns all processes from `/proc`, similar to `ps aux`.
///
/// This function:
/// 1. Reads `/proc`  
/// 2. Determines the system clock tick rate (`sysconf(_SC_CLK_TCK)`)  
/// 3. Iterates over all numeric directories  
/// 4. Attempts to parse them into [`Process`] structs  
///
/// Returns a vector of all successfully parsed processes.
pub fn get_processes() -> Result<Vec<Process>, PsError> {
    let res = std::fs::read_dir("/proc").unwrap();

    // This is just for linux, windows might have an easier way of accessing this information.
    let system_clock_tick_rate = unsafe { libc::sysconf(libc::_SC_CLK_TCK) } as f64;
    // If libc returns -1 that option does exist
    if system_clock_tick_rate == -1.0 {
        // This is not thread safe
        let err_num = unsafe { *libc::__errno_location() };
        return Err(PsError::FailedToGetSysClockTickRate(err_num));
    }

    let mut vec_of_processs = vec![];
    for content in res {
        let content = content.unwrap();
        // Only want directories
        if !content.path().is_dir() {
            continue;
        }

        // Note: This may return a None, when  process ends before we get a chance to look at it, this is fine.
        if let Some(process) = get_process(content, system_clock_tick_rate) {
            vec_of_processs.push(process);
        }
    }
    Ok(vec_of_processs)
}
