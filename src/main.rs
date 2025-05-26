#![warn(clippy::pedantic)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Note: Error in box (heap pointer) bc we don't know the type (and so compiler doesn't know its size).
    let processes = ps::get_processes()?;

    for process in processes {
        println!("{process}");
    }

    // unit type is empty pair of brackets - means nothing
    Ok(())
}
