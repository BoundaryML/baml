//! Tests for FFI functionality (CLI, version, etc.)

use baml::{invoke_cli, version};

#[test]
fn version_returns_non_empty_string() {
    let v = version();
    assert!(!v.is_empty());
    assert_ne!(v, "unknown");
}

#[test]
fn cli_version_exits_successfully() {
    let exit_code = invoke_cli(&["baml", "--version"]);
    assert_eq!(exit_code, 0, "baml --version should exit with 0");
}

#[test]
fn cli_help_exits_successfully() {
    let exit_code = invoke_cli(&["baml", "--help"]);
    assert_eq!(exit_code, 0, "baml --help should exit with 0");
}
