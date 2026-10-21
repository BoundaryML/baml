//! Process execution and assertions shared by SDK toolchain adapters.
use std::process::{Command, Output};

/// Execute a command and report its captured output if it fails.
pub fn run_command(command: &mut Command, context: &str) -> Output {
    let output = command
        .output()
        .unwrap_or_else(|error| panic!("failed to launch {context}: {error}"));
    assert_exit_status(&output, context, &[]);
    output
}

pub(crate) fn assert_exit_status(output: &Output, context: &str, allowed_exit_codes: &[i32]) {
    let accepted = output.status.success()
        || output
            .status
            .code()
            .is_some_and(|code| allowed_exit_codes.contains(&code));
    assert!(
        accepted,
        "{context} failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Require a consumer to confirm that the expected scenario actually ran.
pub fn assert_stdout_contains(output: &Output, marker: &str) {
    assert!(
        String::from_utf8_lossy(&output.stdout).contains(marker),
        "expected stdout marker {marker:?} is missing:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
