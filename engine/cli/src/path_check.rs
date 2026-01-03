//! Path mismatch detection for BAML CLI.
//!
//! Detects when the running BAML binary differs from what the user's PATH suggests
//! should run, and warns them. This helps diagnose issues like:
//! - Shell hash caching (Unix)
//! - PATH order problems
//! - Virtual environment not activated
//! - Multiple BAML installations

use std::path::PathBuf;
use which::which_all;

/// Command names to search for in PATH.
/// Users may invoke either `baml` or `baml-cli`.
const BAML_COMMAND_NAMES: &[&str] = &["baml", "baml-cli"];

/// Environment variable to disable path checking.
const DISABLE_ENV_VAR: &str = "BAML_NO_PATH_CHECK";

/// Find all instances of baml/baml-cli in PATH.
fn find_all_baml_in_path() -> Vec<PathBuf> {
    let mut results = Vec::new();
    for cmd in BAML_COMMAND_NAMES {
        if let Ok(iter) = which_all(cmd) {
            results.extend(iter);
        }
    }
    results
}

/// Check if the invoked path matches what PATH suggests should run.
/// Prints a warning to stderr if there's a mismatch.
///
/// # Arguments
/// * `argv0` - The first argument from argv (the invoked command path)
pub fn check_path_mismatch(argv0: &str) {
    // Check if disabled via environment variable
    if std::env::var(DISABLE_ENV_VAR).is_ok() {
        return;
    }

    let invoked_path = PathBuf::from(argv0);

    // Find ALL instances of baml/baml-cli in PATH
    let all_in_path = find_all_baml_in_path();

    if all_in_path.is_empty() {
        return; // Not in PATH at all
    }

    // Canonicalize for comparison (resolves symlinks, normalizes paths)
    let actual = match invoked_path.canonicalize() {
        Ok(p) => p,
        Err(_) => return, // Can't determine actual path
    };

    let first_expected = match all_in_path[0].canonicalize() {
        Ok(p) => p,
        Err(_) => return, // Can't determine expected path
    };

    // Early exit if they match (common case optimization)
    if actual == first_expected {
        return;
    }

    warn_user(&invoked_path, &all_in_path, &actual);
}

fn warn_user(invoked: &PathBuf, all_in_path: &[PathBuf], actual_canonical: &PathBuf) {
    use colored::Colorize;

    eprintln!(
        "{} Running {}",
        "Warning:".yellow().bold(),
        invoked.display()
    );

    if all_in_path.len() > 1 {
        eprintln!("PATH contains {} versions:", all_in_path.len());
        for (i, p) in all_in_path.iter().enumerate() {
            let is_running = p.canonicalize().ok().as_ref() == Some(actual_canonical);
            let marker = if is_running {
                " <- running this".yellow().to_string()
            } else if i == 0 {
                " <- expected".cyan().to_string()
            } else {
                String::new()
            };
            eprintln!("  {}. {}{}", i + 1, p.display(), marker);
        }
    } else {
        eprintln!("But PATH suggests: {}", all_in_path[0].display());
    }

    eprintln!();

    #[cfg(windows)]
    {
        eprintln!("Check your PATH environment variable order");
        eprintln!("Or ensure your virtual environment is activated");
    }

    #[cfg(not(windows))]
    {
        eprintln!(
            "Run '{}' to clear your shell's command cache",
            "hash -r".cyan()
        );
        eprintln!("Or ensure your virtual environment is activated");
    }

    eprintln!();
    eprintln!(
        "To disable this warning, set {}=1",
        DISABLE_ENV_VAR.cyan()
    );
    eprintln!();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_all_baml_in_path_runs_without_panic() {
        // Just ensure it doesn't panic; actual results depend on system state
        let _ = find_all_baml_in_path();
    }

    #[test]
    fn test_check_path_mismatch_disabled_by_env() {
        std::env::set_var(DISABLE_ENV_VAR, "1");
        // Should return early without doing anything
        check_path_mismatch("/some/path/baml-cli");
        std::env::remove_var(DISABLE_ENV_VAR);
    }

    #[test]
    fn test_check_path_mismatch_handles_nonexistent_path() {
        // Should not panic with a path that doesn't exist
        check_path_mismatch("/nonexistent/path/to/baml-cli");
    }
}
