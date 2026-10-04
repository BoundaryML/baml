//! Publisher-owned recording policy carried by packed binaries and generated bridges.
//!
//! Each producer reads its own manifest table. The engine reads the explicitly
//! permitted recording variable once; omission keeps the baked default.
//!
//! ```toml
//! [pack]
//! on_initial_telemetry_failure = "warn"
//! initial_telemetry_warning_message = "Telemetry unavailable; continuing."
//! [pack.env_var_names]
//! BAML_TELEMETRY = "ACME_TELEMETRY"
//! ```
use std::ffi::OsString;

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

use crate::mode::AutoTelemetryLevel;

/// Recording defaults for functions whose telemetry policy is automatic.
#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    BorshSerialize,
    BorshDeserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum RecordingLevel {
    Off,
    Low,
    #[default]
    Medium,
    High,
}

impl RecordingLevel {
    pub fn auto_level(self) -> Option<AutoTelemetryLevel> {
        match self {
            Self::Off => None,
            Self::Low => Some(AutoTelemetryLevel::Low),
            Self::Medium => Some(AutoTelemetryLevel::Medium),
            Self::High => Some(AutoTelemetryLevel::High),
        }
    }

    fn parse(value: &str) -> Option<Self> {
        match value {
            "off" => Some(Self::Off),
            "low" => Some(Self::Low),
            "medium" => Some(Self::Medium),
            "high" => Some(Self::High),
            _ => None,
        }
    }
}

#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    BorshSerialize,
    BorshDeserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum InitialFailureAction {
    #[default]
    Abort,
    Warn,
    Ignore,
}

/// This policy applies until the first successful ingestion authorization only.
#[derive(
    Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize,
)]
#[serde(deny_unknown_fields)]
pub struct InitialFailurePolicy {
    pub action: InitialFailureAction,
    pub warning_message: Option<String>,
}

#[derive(
    Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize,
)]
#[serde(deny_unknown_fields)]
pub struct ArtifactTelemetry {
    pub recording_level: RecordingLevel,
    pub recording_level_envvar: Option<String>,
    pub initial_failure: InitialFailurePolicy,
}

#[derive(Clone, Copy, Debug)]
pub enum ArtifactKind {
    Pack,
    Bridge,
}

impl ArtifactKind {
    fn table(self) -> &'static str {
        match self {
            Self::Pack => "pack",
            Self::Bridge => "bridge",
        }
    }
}

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct PolicyError(pub String);

impl ArtifactTelemetry {
    /// Resolve only publisher settings; ambient runtime values are never baked in.
    pub fn from_manifest(manifest: &toml::Value, kind: ArtifactKind) -> Result<Self, PolicyError> {
        let name = kind.table();
        let Some(value) = manifest.get(name) else {
            return Ok(Self::default());
        };
        let table = value
            .as_table()
            .ok_or_else(|| PolicyError(format!("[{name}] must be a table.")))?;
        let mut policy = Self::default();
        if let Some(value) = table.get("on_initial_telemetry_failure") {
            policy.initial_failure.action = match value.as_str() {
                Some("abort") => InitialFailureAction::Abort,
                Some("warn") => InitialFailureAction::Warn,
                Some("ignore") => InitialFailureAction::Ignore,
                _ => {
                    return Err(PolicyError(format!(
                        "{name}.on_initial_telemetry_failure must be abort, warn, or ignore."
                    )));
                }
            };
        }
        if let Some(value) = table.get("initial_telemetry_warning_message") {
            policy.initial_failure.warning_message = Some(
                value
                    .as_str()
                    .ok_or_else(|| {
                        PolicyError(format!(
                            "{name}.initial_telemetry_warning_message must be a string."
                        ))
                    })?
                    .to_owned(),
            );
        }
        if let Some(names) = table.get("env_var_names") {
            let names = names
                .as_table()
                .ok_or_else(|| PolicyError(format!("[{name}.env_var_names] must be a table.")))?;
            if let Some(value) = names.get("BAML_TELEMETRY") {
                policy.recording_level_envvar = match value {
                    toml::Value::Boolean(false) => None,
                    toml::Value::String(envvar) if valid_envvar(envvar) => Some(envvar.clone()),
                    _ => {
                        return Err(PolicyError(format!(
                            "{name}.env_var_names.BAML_TELEMETRY must be false or a valid environment-variable name (for example, \"ACME_TELEMETRY\")."
                        )));
                    }
                };
            }
        }
        Ok(policy)
    }

    pub fn resolve_level(&self) -> Result<Option<AutoTelemetryLevel>, PolicyError> {
        self.resolve_level_with(baml_env::os_var)
    }

    /// Inject the environment for tests and hosts; no process-wide mutation is needed.
    pub fn resolve_level_with(
        &self,
        mut read: impl FnMut(&str) -> Option<OsString>,
    ) -> Result<Option<AutoTelemetryLevel>, PolicyError> {
        let Some(name) = &self.recording_level_envvar else {
            return Ok(self.recording_level.auto_level());
        };
        let Some(value) = read(name) else {
            return Ok(self.recording_level.auto_level());
        };
        let level = value
            .to_str()
            .and_then(RecordingLevel::parse)
            .ok_or_else(|| PolicyError(format!("{name} must be off, low, medium, or high.")))?;
        Ok(level.auto_level())
    }
}

fn valid_envvar(value: &str) -> bool {
    let mut bytes = value.bytes();
    matches!(bytes.next(), Some(b'a'..=b'z' | b'A'..=b'Z' | b'_'))
        && bytes.all(|byte| matches!(byte, b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'_'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn omitted_and_false_never_read_ambient_variables() {
        for kind in [ArtifactKind::Pack, ArtifactKind::Bridge] {
            for input in [
                String::new(),
                format!("[{}.env_var_names]\nBAML_TELEMETRY = false", kind.table()),
            ] {
                let manifest: toml::Value = toml::from_str(&input).unwrap();
                let policy = ArtifactTelemetry::from_manifest(&manifest, kind).unwrap();
                assert_eq!(
                    policy.resolve_level_with(|_| panic!(
                        "locked artifacts must not read a recording variable"
                    )),
                    Ok(Some(AutoTelemetryLevel::Medium))
                );
            }
        }
    }

    #[test]
    fn explicit_names_read_only_the_selected_variable_and_keep_unset_defaults() {
        for name in ["BAML_TELEMETRY", "ACME_TELEMETRY"] {
            let policy = ArtifactTelemetry {
                recording_level_envvar: Some(name.into()),
                ..Default::default()
            };
            for (value, level) in [
                ("off", None),
                ("low", Some(AutoTelemetryLevel::Low)),
                ("medium", Some(AutoTelemetryLevel::Medium)),
                ("high", Some(AutoTelemetryLevel::High)),
            ] {
                assert_eq!(
                    policy.resolve_level_with(|read_name| {
                        assert_eq!(read_name, name);
                        Some(value.into())
                    }),
                    Ok(level)
                );
            }
            assert_eq!(
                policy.resolve_level_with(|_| None),
                Ok(Some(AutoTelemetryLevel::Medium))
            );
        }
    }

    #[test]
    fn invalid_runtime_values_name_the_accepted_variable_exactly() {
        let policy = ArtifactTelemetry {
            recording_level_envvar: Some("ACME_TELEMETRY".into()),
            ..Default::default()
        };
        for value in ["", " high ", "HIGH", "local", "cloud"] {
            assert_eq!(
                policy
                    .resolve_level_with(|_| Some(value.into()))
                    .unwrap_err()
                    .to_string(),
                r#"ACME_TELEMETRY must be off, low, medium, or high."#
            );
        }
    }

    #[test]
    fn pack_and_bridge_configuration_are_independent() {
        let manifest: toml::Value = toml::from_str(
            r#"
[pack]
on_initial_telemetry_failure = "warn"
initial_telemetry_warning_message = "Telemetry unavailable; continuing."
[pack.env_var_names]
BAML_TELEMETRY = "ACME_TELEMETRY"
[bridge]
on_initial_telemetry_failure = "ignore"
"#,
        )
        .unwrap();
        let pack = ArtifactTelemetry::from_manifest(&manifest, ArtifactKind::Pack).unwrap();
        assert_eq!(pack.initial_failure.action, InitialFailureAction::Warn);
        assert_eq!(
            pack.initial_failure.warning_message.as_deref(),
            Some("Telemetry unavailable; continuing.")
        );
        assert_eq!(
            pack.recording_level_envvar.as_deref(),
            Some("ACME_TELEMETRY")
        );
        let bridge = ArtifactTelemetry::from_manifest(&manifest, ArtifactKind::Bridge).unwrap();
        assert_eq!(bridge.initial_failure.action, InitialFailureAction::Ignore);
        assert_eq!(bridge.recording_level_envvar, None);
        assert_eq!(bridge.initial_failure.warning_message, None);
    }

    #[test]
    fn invalid_bindings_and_failure_actions_are_actionable() {
        for value in ["true", "\"\"", "\"A=B\"", "\"1VAR\"", "42"] {
            let manifest: toml::Value =
                toml::from_str(&format!("[pack.env_var_names]\nBAML_TELEMETRY = {value}")).unwrap();
            assert_eq!(
                ArtifactTelemetry::from_manifest(&manifest, ArtifactKind::Pack)
                    .unwrap_err()
                    .to_string(),
                r#"pack.env_var_names.BAML_TELEMETRY must be false or a valid environment-variable name (for example, "ACME_TELEMETRY")."#
            );
        }
        let manifest: toml::Value = toml::from_str(
            r#"[bridge]
on_initial_telemetry_failure = "required""#,
        )
        .unwrap();
        assert_eq!(
            ArtifactTelemetry::from_manifest(&manifest, ArtifactKind::Bridge)
                .unwrap_err()
                .to_string(),
            r#"bridge.on_initial_telemetry_failure must be abort, warn, or ignore."#
        );
    }
}
