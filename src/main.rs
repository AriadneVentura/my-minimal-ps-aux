#![warn(clippy::pedantic)]
fn main() -> Result<(), Box<dyn std::error::Error>> {

    // Use a boxed error (heap pointer) because we don't know the type (and so compiler doesn't know its size).
    // `Box<dyn Error>` allows returning any error that implements `std::error::Error`.
    let processes = ps::get_processes()?;

    for process in processes {
        println!("{process}");
    }

    Ok(())
}
