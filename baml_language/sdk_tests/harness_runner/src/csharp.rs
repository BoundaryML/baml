//! C# consumer execution, shared by the SDK test crate.
use std::{
    env,
    path::{Path, PathBuf},
    process::{Command, Output},
};

/// Resolve the consumer crate only after its native setup has run.
pub fn manifest_dir() -> PathBuf {
    assert_eq!(
        env::var("SDK_TEST_CSHARP_SETUP").as_deref(),
        Ok("1"),
        "C# native test setup did not run"
    );
    PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR is not set"))
}

/// Run an already built consumer, retaining MSBuild or application arguments.
pub fn run_project(project: &Path, arguments: &[&str]) -> Output {
    run_command(
        Command::new("dotnet")
            .args(["run", "--no-build", "--project"])
            .arg(project)
            .args(["--configuration", "Release"])
            .args(arguments),
        &project.display().to_string(),
    )
}

/// Execute a special launch mode (publish, an assembly, or a compile probe).
pub fn run_command(command: &mut Command, context: &str) -> Output {
    let output = command
        .output()
        .unwrap_or_else(|error| panic!("failed to launch {context}: {error}"));
    assert!(
        output.status.success(),
        "{context} failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

/// A successful process must also confirm the scenario actually executed.
pub fn assert_marker(output: &Output, marker: &str) {
    assert!(
        String::from_utf8_lossy(&output.stdout).contains(marker),
        "C# success marker {marker:?} is missing: {}",
        String::from_utf8_lossy(&output.stdout)
    );
}
