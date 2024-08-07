use std::process::Command;

pub fn main() {
    // This is derived from the rustc commands that 'rake compile' runs, e.g.
    //
    // cargo rustc --package ruby_ffi --manifest-path /Users/sam/baml/engine/language_client_ruby/ext/ruby_ffi/Cargo.toml --target-dir /Users/sam/baml/engine/target --lib --profile release -- -C linker=clang -L native=/Users/sam/.local/share/mise/installs/ruby/3.1.6/lib -L native=/opt/homebrew/opt/gmp/lib -C link-arg=-Wl,-undefined,dynamic_lookup
    //
    // You need to run 'rake compile' from language_client_ruby itself to see these paths.

    match Command::new("mise").args(["where", "ruby"]).output() {
        Ok(output) => {
            let ruby_path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            println!("cargo:rustc-link-search=native={}/lib", ruby_path);
        }
        Err(e) => {
            println!(
                "cargo:rustc-warning=Failed to execute 'mise where ruby': {}",
                e
            );
        }
    }

    println!("cargo:rustc-link-arg=-Wl,-undefined,dynamic_lookup");
}
