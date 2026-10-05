//! Shared rules for reading BAML-owned environment variables.
//!
//! - Unset, empty and whitespace-only all mean "unset": the typed accessors
//!   return `Ok(None)`.
//! - Booleans are `1/true/yes/on` or `0/false/no/off`, case-insensitive.
//!   Anything else is an error, never a silent fallback.
//! - Enums match case-insensitively; an unknown value is an error.
//! - Paths must be absolute.
//!
//! This crate only parses. Callers decide how an [`EnvError`] surfaces (CLI
//! exit, loader error, engine construction failure).

use std::{
    env,
    ffi::OsString,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum EnvError {
    #[error("{name} is not valid unicode")]
    NotUnicode { name: String },
    #[error("{name} must be a boolean (1/true/yes/on or 0/false/no/off), got {value:?}")]
    InvalidBool { name: String, value: String },
    #[error("{name} must be one of {expected}, got {value:?}")]
    InvalidChoice {
        name: String,
        expected: String,
        value: String,
    },
    #[error("{name} must be an absolute path, got {value:?}")]
    NotAbsolute { name: String, value: String },
}

/// Parse the shared boolean spelling; `None` for any other word.
pub fn parse_bool(value: &str) -> Option<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

/// Case-insensitive lookup of `value` in `choices`.
pub fn parse_choice<T: Copy>(value: &str, choices: &[(&str, T)]) -> Option<T> {
    let value = value.trim();
    choices
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case(value))
        .map(|(_, parsed)| *parsed)
}

/// The value exactly as the process received it. For paths and standard
/// variables (`HOME`, `PATH`, ...) that must not be trimmed or validated.
pub fn os_var(name: &str) -> Option<OsString> {
    env::var_os(name)
}

/// Like [`os_var`] but as a `String`; `None` when unset or not unicode.
pub fn raw_var(name: &str) -> Option<String> {
    env::var(name).ok()
}

/// Whether the variable is set at all, even to an empty value.
pub fn has_var(name: &str) -> bool {
    env::var_os(name).is_some()
}

/// Trimmed string value; `None` when unset or empty.
pub fn string_var(name: &str) -> Result<Option<String>, EnvError> {
    string_from(name, env::var_os(name))
}

/// Boolean variable; `None` when unset or empty.
pub fn bool_var(name: &str) -> Result<Option<bool>, EnvError> {
    bool_from(name, env::var_os(name))
}

/// Enum variable matched case-insensitively against `choices`.
pub fn choice_var<T: Copy>(name: &str, choices: &[(&str, T)]) -> Result<Option<T>, EnvError> {
    choice_from(name, env::var_os(name), choices)
}

/// Absolute-path variable; `None` when unset or empty.
pub fn path_var(name: &str) -> Result<Option<PathBuf>, EnvError> {
    path_from(name, env::var_os(name))
}

/// Like the `*_var` accessor, over an already-read value (for callers with an
/// injectable environment).
pub fn string_from(name: &str, raw: Option<OsString>) -> Result<Option<String>, EnvError> {
    let Some(raw) = raw else { return Ok(None) };
    let value = raw.into_string().map_err(|_| EnvError::NotUnicode {
        name: name.to_string(),
    })?;
    let value = value.trim();
    Ok((!value.is_empty()).then(|| value.to_string()))
}

pub fn bool_from(name: &str, raw: Option<OsString>) -> Result<Option<bool>, EnvError> {
    let Some(value) = string_from(name, raw)? else {
        return Ok(None);
    };
    match parse_bool(&value) {
        Some(parsed) => Ok(Some(parsed)),
        None => Err(EnvError::InvalidBool {
            name: name.to_string(),
            value,
        }),
    }
}

fn choice_from<T: Copy>(
    name: &str,
    raw: Option<OsString>,
    choices: &[(&str, T)],
) -> Result<Option<T>, EnvError> {
    let Some(value) = string_from(name, raw)? else {
        return Ok(None);
    };
    match parse_choice(&value, choices) {
        Some(parsed) => Ok(Some(parsed)),
        None => Err(EnvError::InvalidChoice {
            name: name.to_string(),
            expected: choices
                .iter()
                .map(|(choice, _)| *choice)
                .collect::<Vec<_>>()
                .join(", "),
            value,
        }),
    }
}

pub fn path_from(name: &str, raw: Option<OsString>) -> Result<Option<PathBuf>, EnvError> {
    let Some(value) = string_from(name, raw)? else {
        return Ok(None);
    };
    if Path::new(&value).is_absolute() {
        Ok(Some(PathBuf::from(value)))
    } else {
        Err(EnvError::NotAbsolute {
            name: name.to_string(),
            value,
        })
    }
}

/// The BAML root: `baml_home` when non-empty, else `<home_dir>/.baml`, else a
/// relative `.baml`. The one definition of the rule; the Go and C++ bridge
/// loaders keep copies because they run before the native library is
/// available, so change them together with this. The Rust bridge loader
/// compiles this file through a symlink.
pub fn baml_home_from(baml_home: Option<OsString>, home_dir: Option<PathBuf>) -> PathBuf {
    baml_home
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| home_dir.map(|home| home.join(".baml")))
        .unwrap_or_else(|| PathBuf::from(".baml"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[expect(
        clippy::unnecessary_wraps,
        reason = "mirrors `env::var_os`, which the parsers take"
    )]
    fn os(value: &str) -> Option<OsString> {
        Some(OsString::from(value))
    }

    #[test]
    fn bool_words_are_case_insensitive() {
        for word in ["1", "true", "TRUE", "Yes", "on", " ON "] {
            assert_eq!(parse_bool(word), Some(true), "{word}");
        }
        for word in ["0", "false", "False", "NO", "off"] {
            assert_eq!(parse_bool(word), Some(false), "{word}");
        }
        for word in ["", "2", "enabled", "t"] {
            assert_eq!(parse_bool(word), None, "{word}");
        }
    }

    #[test]
    fn unset_and_empty_are_none() {
        assert_eq!(bool_from("X", None), Ok(None));
        assert_eq!(bool_from("X", os("")), Ok(None));
        assert_eq!(bool_from("X", os("  ")), Ok(None));
        assert_eq!(string_from("X", os("  ")), Ok(None));
    }

    #[test]
    fn invalid_bool_is_an_error() {
        assert_eq!(
            bool_from("X", os("maybe")),
            Err(EnvError::InvalidBool {
                name: "X".into(),
                value: "maybe".into()
            })
        );
    }

    #[test]
    fn choices_match_case_insensitively_and_reject_unknowns() {
        let choices = [("low", 1), ("high", 2)];
        assert_eq!(choice_from("X", os("HIGH"), &choices), Ok(Some(2)));
        assert_eq!(choice_from("X", None, &choices), Ok(None));
        assert!(matches!(
            choice_from("X", os("medium"), &choices),
            Err(EnvError::InvalidChoice { .. })
        ));
    }

    #[test]
    fn paths_must_be_absolute() {
        #[cfg(unix)]
        assert_eq!(
            path_from("X", os("/tmp/x")),
            Ok(Some(PathBuf::from("/tmp/x")))
        );
        assert!(matches!(
            path_from("X", os("relative/dir")),
            Err(EnvError::NotAbsolute { .. })
        ));
        assert_eq!(path_from("X", os("")), Ok(None));
    }

    #[test]
    fn raw_accessors_report_set_and_unset() {
        // Cargo sets this for every test process.
        assert!(has_var("CARGO_MANIFEST_DIR"));
        assert!(os_var("CARGO_MANIFEST_DIR").is_some());
        assert!(raw_var("CARGO_MANIFEST_DIR").is_some());
        assert!(!has_var("BAML_ENV_TEST_NEVER_SET"));
        assert_eq!(raw_var("BAML_ENV_TEST_NEVER_SET"), None);
    }
}
