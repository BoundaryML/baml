//! The dotnet command adapter for the C# consumer projects.
use std::{
    env,
    path::{Path, PathBuf},
    process::{Command, Output},
};

/// Resolve the consumer crate only after its native setup has run.
pub fn manifest_dir() -> PathBuf {
    crate::__check_setup_ran("SDK_TEST_CSHARP_SETUP");
    PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR is not set"))
}

/// Run an already built consumer, retaining MSBuild or application arguments.
pub fn run_project(project: &Path, arguments: &[&str]) -> Output {
    crate::run_command(
        Command::new("dotnet")
            .args(["run", "--no-build", "--project"])
            .arg(project)
            .args(["--configuration", "Debug"])
            .args(arguments),
        &project.display().to_string(),
    )
}
