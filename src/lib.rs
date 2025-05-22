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
#[derive(Debug)]
pub struct Process {
    pid: u32,
    cmdline: Option<String>,
    binary_path: Option<PathBuf>,
    owner: Option<String>,
    start_time: Option<DateTime<Local>>,
    state: Option<String>,
}

#[derive(Error, Debug)]
pub enum PsError {
    #[error("io error")]
    FailedToReadFile(#[from] std::io::Error),
    #[error("failed to get uptime from stat")]
    FailedToGetUptimeFromStat,
    #[error("failed to parse as float")]
    FailedToParseAsFloat(#[from] std::num::ParseFloatError),
    #[error("failed to get system time")]
    FailedToGetSystemTime(#[from] std::time::SystemTimeError),
}

// ? - Is this whats meant by convert state?
fn find_state(status: &str) -> Option<String> {
    for line in status.lines() {
        if line.starts_with("State:") {
            // Split into two, ie: [State, S sleeping] using tab formatting char, return the state without the word
            // ? - map allows safety apparently?
            let process_state = line.split_once('\t').map(|x| x.1);
            return process_state.map(|s| s.to_string());
        }
    }
    None
}

// Pretty print implementation
impl fmt::Display for Process {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        // Print the header row - so many pr
        // if f.alternate() {
        //     writeln!(
        //         f,
        //         "{:<10} {:<15} {:<15} {:<30} {:<20} {:<15}",
        //         "PID", "Owner", "Cmdline", "Binary Path", "Start Time", "State",
        //     )?;
        // }

        let start_time = match self.start_time {
            // Format the datetime as a normal readable string
            Some(date_time) => date_time.format("%Y-%m-%d %H:%M:%S").to_string(),
            None => "unknown".to_owned(),
        };

        writeln!(
            f,
            "{:<10} {:<15} {:<15} {:<30} {:<20} {:<15}",
            self.pid,
            self.owner.as_deref().unwrap_or("-"),
            self.cmdline.as_deref().unwrap_or("-"),
            self.binary_path
                .clone()
                // .as_deref()
                // .and_then(|p| p.to_str())
                .unwrap_or(PathBuf::new())
                .to_string_lossy(),
            start_time,
            self.state.as_deref().unwrap_or("-")
        )
    }
}

// Note: functions that don't need ownership - take reference
fn get_start_time(
    uptime_path: &PathBuf,
    stat_path: &PathBuf,
    system_clock_tick_rate: f64,
) -> Result<DateTime<Local>, PsError> {
    let uptime_res = std::fs::read_to_string(uptime_path)?;

    let uptime_seconds: f64 = uptime_res
        .split_whitespace()
        // Next gets first as a Some (), ok_or checks if some (if there is a value) if not errors
        // ? - previously i used .expectes() that is bad practises as it will cause panics, now note the ok_or()
        .next()
        .ok_or(PsError::FailedToGetUptimeFromStat)?
        // tries to turn "48267.42" into f64
        .parse()?;

    let stat = std::fs::read_to_string(stat_path)?;

    let stats: Vec<&str> = stat.split_whitespace().collect();
    // start_time is at the 22nd column
    let time_stat_str = stats[21];
    let time_stat: f64 = time_stat_str.parse()?;
    // convert start_time to seconds since boot
    let start_time_in_seconds = time_stat / system_clock_tick_rate;
    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs_f64();
    let boot_time = now - uptime_seconds;
    let process_start_time = (boot_time + start_time_in_seconds) as u64;

    let duration = Duration::from_secs(process_start_time);
    let system_time = UNIX_EPOCH + duration;
    let date_time: DateTime<Local> = system_time.into();

    Ok(date_time)
}

fn get_process(dir_ent: DirEntry, system_clock_tick_rate: f64) -> Option<Process> {
    let path = "/proc";
    // Only parse filenames if they are numbers (process')
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

            // TODO in future may want to know what went wrong
            // Want the first part of cmdline, without the -- tags, to make it shorter
            if let Ok(cmd) = std::fs::read_to_string(cmdline) {
                // Note this is kinda jenk for NGINX
                // let first = cmd.split_whitespace().next().unwrap_or("-");
                process.cmdline = Some(cmd);
            }

            // .ok() converts result into success case or None
            process.binary_path = std::fs::read_link(binary_path).ok();
            if let Ok(metadata) = dir_ent.metadata() {
                let owner_id = metadata.uid();
                let owner = unsafe {
                    // getpwuid_r is thread safe because we provide our own buffer
                    // libc::getpwuid_r(uid, pwd, buf, buflen, result)
                    // Will return null if no matching entry
                    let res = libc::getpwuid(owner_id);
                    if res.is_null() {
                        // let err_num = *libc::__errno_location();
                        // println!(
                        //     "Failed to lookup user with id {}, error number {}, for process {}",
                        //     owner_id, err_num, filename
                        // );
                        Some(owner_id.to_string())
                    } else {
                        let passwd = *res;
                        // Construct rust string from raw pointer
                        let owner = CStr::from_ptr(passwd.pw_name);
                        // to string lossy converts the bytes it can to string or gives up
                        Some(owner.to_string_lossy().to_string())
                    }
                };
                process.owner = owner;
            }

            // TODO in display
            // let start_time = date_time.format("%Y-%m-%d %H:%M:%S").to_string();

            match get_start_time(&uptime_path, &stat_path, system_clock_tick_rate) {
                Ok(date_time) => process.start_time = Some(date_time),
                // If error, then log
                Err(e) => eprintln!("{}", e),
            }

            if let Ok(state_res) = std::fs::read_to_string(state_path) {
                let process_state = find_state(&state_res);
                process.state = process_state;
            }

            Some(process)
        }
        // TODO would be good to know process with name failed to be converted to u32
        Err(_) => None,
    }
}

pub fn get_processes() -> Vec<Process> {
    let res = std::fs::read_dir("/proc").unwrap();

    // THis is just for linux
    let system_clock_tick_rate = unsafe { libc::sysconf(libc::_SC_CLK_TCK) } as f64;
    if system_clock_tick_rate == -1.0 {
        panic!("raaaa")
    }

    let mut vec_of_processs = vec![];
    for content in res {
        let content = content.unwrap();
        // Only want directories
        if !content.path().is_dir() {
            continue;
        }

        // TODO function that takes in directory and returns process
        // TODO this may return a None, when  process ends before we get a chance to look at it, this is fine
        if let Some(process) = get_process(content, system_clock_tick_rate) {
            vec_of_processs.push(process);
        }
    }
    vec_of_processs
}
