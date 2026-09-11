use std::{env, path::PathBuf, process::Command};

fn git_output(args: &[&str]) -> Option<String> {
    let output = Command::new("git").args(args).output().ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn track_git_head() {
    let Some(head) = git_output(&["rev-parse", "--git-path", "HEAD"]) else {
        return;
    };
    println!("cargo:rerun-if-changed={head}");

    if let Some(reference) = git_output(&["symbolic-ref", "-q", "HEAD"])
        && let Some(path) = git_output(&["rev-parse", "--git-path", &reference])
    {
        // A packed ref has no loose file. Watching a nonexistent path makes
        // Cargo rerun this script (and rebuild the compiler) on every command.
        // Watch an existing ancestor to notice a future loose ref, plus the
        // packed-ref file for updates to the current packed value.
        let mut watched = PathBuf::from(path);
        while !watched.exists() {
            if !watched.pop() {
                break;
            }
        }
        if watched.exists() {
            println!("cargo:rerun-if-changed={}", watched.display());
        }
        if let Some(packed) = git_output(&["rev-parse", "--git-path", "packed-refs"])
            && PathBuf::from(&packed).exists()
        {
            println!("cargo:rerun-if-changed={packed}");
        }
    }
}

fn main() {
    println!("cargo:rerun-if-env-changed=BAML_GIT_SHA");
    track_git_head();

    let fingerprint = env::var("BAML_GIT_SHA")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .map(|value| value.trim().to_owned())
        .or_else(|| git_output(&["rev-parse", "HEAD"]))
        .unwrap_or_else(|| baml_version::CANONICAL_VERSION.to_owned());

    println!("cargo:rustc-env=BAML_ARTIFACT_BUILD_FINGERPRINT={fingerprint}");

    // Keep the build script tied to the fallback source even in source archives.
    let manifest_dir = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("manifest dir"));
    println!(
        "cargo:rerun-if-changed={}",
        manifest_dir.join("../baml_version/src/lib.rs").display()
    );
}
