//! This crate provides the BAML CLI for Rust.

fn main() {
    let args = std::env::args().collect::<Vec<String>>();
    let exit_code = baml::invoke_cli(
        args.iter()
            .map(std::string::String::as_str)
            .collect::<Vec<&str>>()
            .as_slice(),
    );
    std::process::exit(exit_code);
}
