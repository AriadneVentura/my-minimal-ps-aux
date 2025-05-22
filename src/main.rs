// Put error in box (heapp pointer) bc dont know type (so compiler doesnt kno size)
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let processes = ps::get_processes()?;

    for process in processes {
        println!("{}", process);
    }

    // unit type is empty pair of brackets - means nothing
    Ok(())
}
