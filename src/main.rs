fn main() {
    let processes = ps::get_processes();

    for process in processes {
        println!("{}", process);
    }
}
