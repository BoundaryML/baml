use std::process::Command;

fn store_git_commit_hash() {
    let output = Command::new("git").args(["rev-parse", "HEAD"]).output();

    if let Ok(output) = output {
        let git_hash = String::from_utf8(output.stdout).unwrap();
        println!("cargo:rustc-env=GIT_HASH={git_hash}");
    } else {
        println!("cargo:rustc-env=GIT_HASH=unknown");
    }
}

fn main() {
    store_git_commit_hash();
}
