# a minimal reimplementation in Rust
`ps aux` is a Linux command that displays all system processes and reports user-focused metrics like CPU load, memory usage, and process ownership.

This is my version of that tool, built entirely from scratch by manually parsing `/proc` and formatting process information, without depending on any third-party crates.

This was a learning project to explore how Linux exposes process metadata and how to retrieve and present it while also gaining hands-on experience working with low-level system data in rust.


# Future Improvements
Memory and CPU usage reporting
Cross-platform abstractions (where possible)

Running
`cargo run`
