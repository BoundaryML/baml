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

// Inline because the published Rust bridge compiles this file through a symlink.
pub mod agent {
    //! Coding-agent signals shared by CLI output policy and runtime telemetry.
    //!
    //! Reuses the Vercel-derived detector from BAML PR #4726, including its
    //! review fixes for non-Unicode named markers:
    //! <https://github.com/BoundaryML/baml/blob/3c89421458a11134544a6d7f1a4e5ee9d2b0b1e3/baml_language/crates/baml_cli/src/output.rs>

    use std::ffi::OsString;

    use super::EnvError;

    /// Dev escape hatch used by hooks and task runners launched inside an agent.
    pub const DISABLE_AGENT_DETECTION_ENV: &str = "DEV_BAML_CLI_DISABLE_AGENT_DETECTION";

    // Named markers can contain session IDs, paths or credentials: only their
    // presence is used. AI_AGENT is handled separately as an explicit name.
    const AGENT_ENV_VARS: &[(&str, &str)] = &[
        ("CLAUDECODE", "claude"),
        ("CODEX_SANDBOX", "codex"),
        ("PI_CODING_AGENT", "pi"),
        ("OPENCODE_CLIENT", "opencode"),
        ("CURSOR_TRACE_ID", "cursor"),
        ("REPL_ID", "replit"),
    ];

    /// The explicit `AI_AGENT` name, a known harness label, or `unknown` for
    /// the boolean-ish `AGENT` compatibility marker. Named marker values are
    /// never reported; they need only be nonempty and different from `0`.
    pub fn detect() -> Result<Option<String>, EnvError> {
        detect_with(super::os_var)
    }

    /// Injectable form of [`detect`], without changing process environment.
    pub fn detect_with(
        mut read: impl FnMut(&str) -> Option<OsString>,
    ) -> Result<Option<String>, EnvError> {
        if super::bool_from(
            DISABLE_AGENT_DETECTION_ENV,
            read(DISABLE_AGENT_DETECTION_ENV),
        )? == Some(true)
        {
            return Ok(None);
        }
        // AI_AGENT follows @vercel/detect-agent: its value names the harness
        // and takes precedence over inherited markers from a parent harness.
        if let Some(name) = read("AI_AGENT")
            .and_then(|value| value.into_string().ok())
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty() && value != "0")
        {
            return Ok(Some(name));
        }

        // Named markers do not need Unicode decoding: their contents never
        // become a reported name. Preserve the reviewed PR #4726 behavior.
        for &(variable, harness) in AGENT_ENV_VARS {
            if read(variable).is_some_and(|value| !value.is_empty() && value != "0") {
                return Ok(Some(harness.to_owned()));
            }
        }

        Ok(read("AGENT")
            .filter(|value| !value.is_empty() && value != "0")
            .map(|_| "unknown".to_owned()))
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        /// Each named marker yields only its fixed label, with empty/zero
        /// handling and no disclosure of the marker's value.
        fn every_existing_marker_has_a_bounded_label() {
            for &(marker, label) in AGENT_ENV_VARS {
                assert_eq!(
                    detect_with(|name| (name == marker).then(|| "private-session-value".into()))
                        .unwrap()
                        .as_deref(),
                    Some(label)
                );
                for value in ["", "0"] {
                    assert_eq!(
                        detect_with(|name| (name == marker).then(|| value.into())),
                        Ok(None)
                    );
                }
            }
            assert_eq!(detect_with(|_| None), Ok(None));
            assert_eq!(
                detect_with(|name| (name == "AGENT").then(|| "false".into()))
                    .unwrap()
                    .as_deref(),
                Some("unknown")
            );
        }

        #[test]
        /// The explicit `AI_AGENT` name outranks inherited named markers, while
        /// overlapping named markers retain the reviewed deterministic order.
        fn explicit_harness_wins_over_named_markers() {
            assert_eq!(
                detect_with(|name| match name {
                    "AI_AGENT" => Some(" custom-harness ".into()),
                    "CURSOR_TRACE_ID" => Some("private-session-value".into()),
                    _ => None,
                })
                .unwrap()
                .as_deref(),
                Some("custom-harness")
            );
            assert_eq!(
                detect_with(
                    |name| matches!(name, "CLAUDECODE" | "CODEX_SANDBOX").then(|| "1".into())
                )
                .unwrap()
                .as_deref(),
                Some("claude")
            );
        }

        #[test]
        /// Empty, whitespace-only or zero explicit names fall through to named
        /// markers; AGENT provides a final unknown-harness fallback.
        fn empty_explicit_names_fall_back_to_markers() {
            for value in ["", "  ", "0", " 0 "] {
                assert_eq!(
                    detect_with(|name| match name {
                        "AI_AGENT" => Some(value.into()),
                        "CODEX_SANDBOX" | "AGENT" => Some("1".into()),
                        _ => None,
                    })
                    .unwrap()
                    .as_deref(),
                    Some("codex")
                );
            }
        }

        #[test]
        /// The dev override accepts shared boolean spellings while retaining
        /// the CLI's error for an invalid setting.
        fn escape_hatch_retains_strict_cli_validation() {
            for value in ["true", "1", "on", "YES"] {
                assert_eq!(
                    detect_with(|name| Some(if name == DISABLE_AGENT_DETECTION_ENV {
                        value.into()
                    } else {
                        "1".into()
                    })),
                    Ok(None)
                );
            }
            assert!(detect_with(|_| Some("invalid".into())).is_err());
        }

        #[cfg(unix)]
        #[test]
        /// Named markers still classify a harness without decoding their values;
        /// a non-Unicode explicit name falls through to those markers.
        fn non_unicode_named_markers_still_detect_harness() {
            use std::os::unix::ffi::OsStringExt as _;
            assert_eq!(
                detect_with(|name| matches!(name, "AI_AGENT" | "CODEX_SANDBOX")
                    .then(|| OsString::from_vec(vec![0xff])))
                .unwrap()
                .as_deref(),
                Some("codex")
            );
        }
    }
}

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

static HOTSWAP: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Turn function hot swapping on for this process (BEP-087), as `baml run
/// --hot` does. Set once, before anything compiles, so escape analysis and the
/// engine agree for the life of the process.
pub fn set_hotswap_enabled() {
    HOTSWAP.store(true, std::sync::atomic::Ordering::Relaxed);
}

/// Whether [`set_hotswap_enabled`] was called.
pub fn hotswap_enabled() -> bool {
    HOTSWAP.load(std::sync::atomic::Ordering::Relaxed)
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
