//! Shared location of BAML per-user state.

use std::{ffi::OsString, path::PathBuf};

/// Resolve the BAML home directory (`~/.baml`), the root under which the
/// toolchain stores installed releases, config, and other per-user state.
///
/// Resolution order:
///   1. `$BAML_HOME`, if set to a non-empty value.
///   2. The user's home directory joined with `.baml`.
///   3. A relative `.baml` as a last resort when no home directory is known.
///
/// This is the single source of truth shared by the `baml` wrapper and the
/// `baml-cli` toolchain binary; don't reimplement it.
pub fn baml_home() -> PathBuf {
    baml_home_from(std::env::var_os("BAML_HOME"), dirs::home_dir())
}

fn baml_home_from(baml_home: Option<OsString>, home_dir: Option<PathBuf>) -> PathBuf {
    baml_home
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| home_dir.map(|home| home.join(".baml")))
        .unwrap_or_else(|| PathBuf::from(".baml"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn baml_home_uses_non_empty_environment_value() {
        assert_eq!(
            baml_home_from(
                Some(OsString::from("/custom/baml")),
                Some(PathBuf::from("/home/tester")),
            ),
            PathBuf::from("/custom/baml")
        );
    }

    #[test]
    fn baml_home_ignores_empty_environment_value() {
        assert_eq!(
            baml_home_from(Some(OsString::new()), Some(PathBuf::from("/home/tester"))),
            PathBuf::from("/home/tester/.baml")
        );
    }

    #[test]
    fn baml_home_uses_home_directory_when_environment_value_is_absent() {
        assert_eq!(
            baml_home_from(None, Some(PathBuf::from("/home/tester"))),
            PathBuf::from("/home/tester/.baml")
        );
    }

    #[test]
    fn baml_home_uses_relative_directory_when_no_home_is_available() {
        assert_eq!(baml_home_from(None, None), PathBuf::from(".baml"));
    }
}
