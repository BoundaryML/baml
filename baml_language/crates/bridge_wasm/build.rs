use std::{path::PathBuf, process::Command, time::UNIX_EPOCH};

fn git_output(args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn track_git_path(path: &str) {
    let path = PathBuf::from(path);
    let path = if path.is_absolute() {
        path
    } else {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(path)
    };
    println!("cargo:rerun-if-changed={}", path.display());
}

fn main() {
    if std::env::var("BRIDGE_WASM_FORCE_RERUN").is_ok() {
        // Point at a non-existent file so cargo always re-runs this build script,
        // even when only dependency crates changed (not files in bridge_wasm itself).
        println!("cargo:rerun-if-changed=FORCE_RERUN");
    }

    // Build script runs on the host; [`std::time::SystemTime`] is correct at this callsite.
    #[allow(clippy::disallowed_types)]
    let ts = std::time::SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    println!("cargo:rustc-env=BRIDGE_WASM_BUILD_TS={ts}");

    if let Some(path) = git_output(&["rev-parse", "--git-path", "HEAD"]) {
        track_git_path(&path);
    }
    if let Some(head_ref) = git_output(&["symbolic-ref", "-q", "HEAD"])
        && let Some(path) = git_output(&["rev-parse", "--git-path", &head_ref])
    {
        track_git_path(&path);
    }

    let git_sha = git_output(&["rev-parse", "--short=7", "HEAD"])
        .filter(|sha| sha.len() == 7 && sha.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .unwrap_or_default();
    println!("cargo:rustc-env=BRIDGE_WASM_GIT_SHA={git_sha}");
}
