//! Integration tests for baml-sys library loading.
#![allow(clippy::print_stderr)]

use std::env;

use baml_sys::{ensure_library, version, BamlSysError, ENV_LIBRARY_PATH};

/// Test that version returns a non-empty string when library is available.
/// This test is skipped if the library is not present.
#[test]
fn test_version_when_library_available() {
    // Skip if library not configured
    if env::var(ENV_LIBRARY_PATH).is_err() && env::var("CI").is_err() {
        eprintln!("Skipping test_version_when_library_available: BAML_LIBRARY_PATH not set");
        return;
    }

    match version() {
        Ok(v) => {
            assert!(!v.is_empty(), "Version should not be empty");
            // Version should match the crate's version exactly
            assert_eq!(
                v,
                baml_sys::VERSION,
                "Library version should match crate version"
            );
        }
        Err(BamlSysError::LibraryNotFound { .. }) => {
            eprintln!("Library not found, skipping test");
        }
        Err(e) => {
            panic!("Unexpected error: {e}");
        }
    }
}

/// Test that `ensure_library` returns a valid path when library is available.
#[test]
fn test_ensure_library_returns_path() {
    if env::var(ENV_LIBRARY_PATH).is_err() && env::var("CI").is_err() {
        eprintln!("Skipping test_ensure_library_returns_path: BAML_LIBRARY_PATH not set");
        return;
    }

    match ensure_library() {
        Ok(path) => {
            assert!(
                path.exists(),
                "Library path should exist: {}",
                path.display()
            );
        }
        Err(BamlSysError::LibraryNotFound { .. }) => {
            eprintln!("Library not found, skipping test");
        }
        Err(e) => {
            panic!("Unexpected error: {e}");
        }
    }
}

/// Test that error messages are helpful.
#[test]
fn test_error_display() {
    use std::path::PathBuf;

    let err = BamlSysError::LibraryNotFound {
        searched_paths: vec![PathBuf::from("/path/one"), PathBuf::from("/path/two")],
    };
    let msg = err.to_string();
    assert!(
        msg.contains("not found"),
        "Error should mention 'not found': {msg}"
    );
    assert!(
        msg.contains("/path/one"),
        "Error should list searched paths: {msg}"
    );

    let err = BamlSysError::VersionMismatch {
        expected: "1.0.0".to_string(),
        actual: "2.0.0".to_string(),
    };
    let msg = err.to_string();
    assert!(
        msg.contains("1.0.0"),
        "Error should show expected version: {msg}"
    );
    assert!(
        msg.contains("2.0.0"),
        "Error should show actual version: {msg}"
    );

    let err = BamlSysError::ChecksumMismatch {
        expected: "abc123".to_string(),
        actual: "def456".to_string(),
    };
    let msg = err.to_string();
    assert!(
        msg.contains("abc123"),
        "Error should show expected checksum: {msg}"
    );
    assert!(
        msg.contains("def456"),
        "Error should show actual checksum: {msg}"
    );
}

/// Test platform detection logic.
#[test]
fn test_platform_constants() {
    // Verify VERSION is set from Cargo.toml
    assert!(!baml_sys::VERSION.is_empty());
    assert!(
        baml_sys::VERSION.chars().next().unwrap().is_ascii_digit(),
        "VERSION should start with a digit"
    );

    // Verify environment variable names are set
    assert_eq!(baml_sys::ENV_LIBRARY_PATH, "BAML_LIBRARY_PATH");
    assert_eq!(baml_sys::ENV_CACHE_DIR, "BAML_CACHE_DIR");
    assert_eq!(
        baml_sys::ENV_DISABLE_DOWNLOAD,
        "BAML_LIBRARY_DISABLE_DOWNLOAD"
    );
}
