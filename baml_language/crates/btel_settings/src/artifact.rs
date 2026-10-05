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
use std::{ffi::OsString, fmt, fmt::Write as _};

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

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
    #[serde(default)]
    pub project: Option<String>,
    #[serde(default)]
    pub api_url: Option<String>,
    #[serde(default)]
    pub destination_overrides: DestinationOverrides,
    #[serde(default)]
    pub embedded: Option<Box<EmbeddedIngestion>>,
}

/// An omitted binding follows credential-dependent defaults: embedded credentials
/// are fixed, while ordinary artifacts accept the standard Boundary variables.
#[derive(
    Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize,
)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum DestinationOverrides {
    #[default]
    Default,
    Disabled,
    Named {
        project: String,
        api_key: String,
    },
}

/// Public, distributable ingestion credential. Debug output still redacts it.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(transparent)]
pub struct PublicCredential(String);
impl PublicCredential {
    pub fn new(value: String) -> Self {
        Self(value)
    }
    pub fn expose(&self) -> &str {
        &self.0
    }
}
impl fmt::Debug for PublicCredential {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("[REDACTED]")
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BytecodeDigest {
    pub version: u32,
    pub algorithm: String,
    pub value: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct IngestionDestination {
    pub org_id: String,
    pub project_id: String,
    pub environment_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EmbeddedIngestion {
    pub token: PublicCredential,
    pub token_id: String,
    pub build_id: String,
    pub product_id: String,
    pub destination: IngestionDestination,
    pub bytecode_digest: BytecodeDigest,
}

/// The public credential is never reused when the caller selects a destination.
#[derive(Debug, PartialEq, Eq)]
pub enum IngestionSelection<'a> {
    Embedded(&'a EmbeddedIngestion),
    Caller {
        project: Option<String>,
        api_key: Option<String>,
    },
}

/// Fingerprint v1 covers the exact versioned program/dispatch bytes and publisher
/// policy. Credentials are excluded; fresh tokens can identify the same build.
pub fn build_digest(
    payload: &[u8],
    policy: &ArtifactTelemetry,
) -> Result<BytecodeDigest, PolicyError> {
    let mut unsigned = policy.clone();
    unsigned.embedded = None;
    let policy_bytes = borsh::to_vec(&unsigned)
        .map_err(|_| PolicyError("Could not encode artifact telemetry policy.".into()))?;
    let mut hash = Sha256::new();
    hash.update(b"baml-telemetry-build-v1\0");
    hash.update((payload.len() as u64).to_le_bytes());
    hash.update(payload);
    hash.update(policy_bytes);
    Ok(BytecodeDigest {
        version: 1,
        algorithm: "sha256".into(),
        value: format!("{:x}", hash.finalize()),
    })
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

const PRODUCER_FIELDS: &[&str] = &[
    "on_initial_telemetry_failure",
    "initial_telemetry_warning_message",
    "telemetry_environment",
    "env_var_names",
];
const ENVVAR_FIELDS: &[&str] = &["BAML_TELEMETRY", "BOUNDARY_PROJECT", "BOUNDARY_API_KEY"];

/// Validate publisher-owned namespaces before any build can provision credentials.
/// Other manifest tables (package, generators, etc.) have their own parsers.
fn validate_manifest_settings(manifest: &toml::Value) -> Result<(), PolicyError> {
    for (name, fields) in [
        ("boundary", &["project", "api_url"][..]),
        ("pack", PRODUCER_FIELDS),
        ("bridge", PRODUCER_FIELDS),
    ] {
        let Some(value) = manifest.get(name) else {
            continue;
        };
        let table = value
            .as_table()
            .ok_or_else(|| PolicyError(format!("[{name}] must be a table.")))?;
        validate_fields(table, name, fields)?;
        if name != "boundary" {
            if let Some(names) = table.get("env_var_names") {
                let name = format!("{name}.env_var_names");
                let table = names
                    .as_table()
                    .ok_or_else(|| PolicyError(format!("[{name}] must be a table.")))?;
                validate_fields(table, &name, ENVVAR_FIELDS)?;
            }
            if let Some(value) = table.get("telemetry_environment") {
                if value.as_str().is_none_or(str::is_empty) {
                    return Err(PolicyError(format!(
                        "{name}.telemetry_environment must be a non-empty string."
                    )));
                }
            }
        }
    }
    Ok(())
}

fn validate_fields(table: &toml::Table, name: &str, fields: &[&str]) -> Result<(), PolicyError> {
    for field in table.keys() {
        if fields.contains(&field.as_str()) {
            continue;
        }
        let mut message = format!("Unknown setting `{name}.{field}` in baml.toml.\n");
        if let Some((distance, suggestion)) = fields
            .iter()
            .map(|candidate| (strsim::levenshtein(field, candidate), candidate))
            .min_by_key(|(distance, _)| *distance)
            && distance <= 2
        {
            writeln!(message, "\n  Did you mean `{name}.{suggestion}`?")
                .expect("writing to a String cannot fail");
        }
        write!(message, "\n  Valid [{name}] settings:").expect("writing to a String cannot fail");
        for field in fields {
            write!(message, "\n    {field}").expect("writing to a String cannot fail");
        }
        return Err(PolicyError(message));
    }
    Ok(())
}

impl ArtifactTelemetry {
    /// Resolve only publisher settings; ambient runtime values are never baked in.
    pub fn from_manifest(manifest: &toml::Value, kind: ArtifactKind) -> Result<Self, PolicyError> {
        validate_manifest_settings(manifest)?;
        let name = kind.table();
        let mut policy = Self::default();
        if let Some(boundary) = manifest.get("boundary") {
            if !boundary.is_table() {
                return Err(PolicyError("[boundary] must be a table.".into()));
            }
            for (field, output) in [
                ("project", &mut policy.project),
                ("api_url", &mut policy.api_url),
            ] {
                if let Some(value) = boundary.get(field) {
                    *output = Some(
                        value
                            .as_str()
                            .filter(|value| !value.is_empty())
                            .ok_or_else(|| {
                                PolicyError(format!("boundary.{field} must be a non-empty string."))
                            })?
                            .to_owned(),
                    );
                }
            }
        }
        let Some(value) = manifest.get(name) else {
            return Ok(policy);
        };
        let table = value
            .as_table()
            .ok_or_else(|| PolicyError(format!("[{name}] must be a table.")))?;
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
            policy.destination_overrides = match (
                names.get("BOUNDARY_PROJECT"),
                names.get("BOUNDARY_API_KEY"),
            ) {
                (None, None) => DestinationOverrides::Default,
                (Some(toml::Value::Boolean(false)), Some(toml::Value::Boolean(false))) => {
                    DestinationOverrides::Disabled
                }
                (Some(toml::Value::String(project)), Some(toml::Value::String(api_key)))
                    if valid_envvar(project)
                        && valid_envvar(api_key)
                        && project != api_key
                        && project != "BOUNDARY_API_URL"
                        && api_key != "BOUNDARY_API_URL" =>
                {
                    DestinationOverrides::Named {
                        project: project.clone(),
                        api_key: api_key.clone(),
                    }
                }
                _ => {
                    return Err(PolicyError(format!(
                        "{name}.env_var_names.BOUNDARY_PROJECT and BOUNDARY_API_KEY must both be false or distinct valid environment-variable names."
                    )));
                }
            };
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
        if let Some(level) = &policy.recording_level_envvar {
            if level == "BOUNDARY_API_URL"
                || matches!(&policy.destination_overrides, DestinationOverrides::Named { project, api_key } if level == project || level == api_key)
            {
                return Err(PolicyError(format!(
                    "{name}.env_var_names.BAML_TELEMETRY must use a name distinct from the destination bindings and BOUNDARY_API_URL."
                )));
            }
        }
        Ok(policy)
    }

    pub fn select_ingestion_with(
        &self,
        mut read: impl FnMut(&str) -> Option<OsString>,
    ) -> Result<IngestionSelection<'_>, PolicyError> {
        let names = match &self.destination_overrides {
            DestinationOverrides::Default if self.embedded.is_none() => {
                Some(("BOUNDARY_PROJECT", "BOUNDARY_API_KEY"))
            }
            DestinationOverrides::Named { project, api_key } => {
                Some((project.as_str(), api_key.as_str()))
            }
            DestinationOverrides::Default | DestinationOverrides::Disabled => None,
        };
        let read_value = |read: &mut dyn FnMut(&str) -> Option<OsString>,
                          name: &str|
         -> Result<Option<String>, PolicyError> {
            read(name)
                .map(|value| {
                    value
                        .into_string()
                        .ok()
                        .filter(|value| !value.is_empty())
                        .ok_or_else(|| {
                            PolicyError(format!("{name} must be a non-empty UTF-8 value."))
                        })
                })
                .transpose()
        };
        let (project, api_key) = if let Some((project, api_key)) = names {
            (
                read_value(&mut read, project)?,
                read_value(&mut read, api_key)?,
            )
        } else {
            (None, None)
        };
        if project.is_none()
            && api_key.is_none()
            && let Some(embedded) = &self.embedded
        {
            return Ok(IngestionSelection::Embedded(embedded));
        }
        // A supplied key owns its destination; an omitted project can be inferred
        // from its fixed ingestion scope instead of borrowing the publisher's target.
        let project = project.or_else(|| {
            if api_key.is_none() {
                self.project.clone()
            } else {
                None
            }
        });
        Ok(IngestionSelection::Caller { project, api_key })
    }

    pub fn verify_build(&self, payload: &[u8]) -> Result<(), PolicyError> {
        if let Some(embedded) = &self.embedded {
            if embedded.bytecode_digest != build_digest(payload, self)? {
                return Err(PolicyError("Embedded telemetry does not match this artifact. Rebuild with `baml pack --embed-telemetry` or `baml generate --embed-telemetry`.".into()));
            }
            if !embedded.token.expose().starts_with("bdry_public_")
                || embedded.build_id.is_empty()
                || embedded.destination.org_id.is_empty()
                || embedded.destination.project_id.is_empty()
                || embedded.destination.environment_id.is_empty()
            {
                return Err(PolicyError("The embedded telemetry credential is invalid. Rebuild the artifact with `--embed-telemetry`.".into()));
            }
        }
        Ok(())
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
    fn unknown_publisher_settings_fail_with_exact_actionable_messages() {
        for (input, expected) in [
            (
                r#"[pack]
on_inital_telemetry_failure = "ignore""#,
                r#"Unknown setting `pack.on_inital_telemetry_failure` in baml.toml.

  Did you mean `pack.on_initial_telemetry_failure`?

  Valid [pack] settings:
    on_initial_telemetry_failure
    initial_telemetry_warning_message
    telemetry_environment
    env_var_names"#,
            ),
            (
                r#"[bridge]
extra = true"#,
                r#"Unknown setting `bridge.extra` in baml.toml.

  Valid [bridge] settings:
    on_initial_telemetry_failure
    initial_telemetry_warning_message
    telemetry_environment
    env_var_names"#,
            ),
            (
                r#"[pack.env_var_names]
BOUNDARY_API_KE = "CUSTOM_KEY""#,
                r#"Unknown setting `pack.env_var_names.BOUNDARY_API_KE` in baml.toml.

  Did you mean `pack.env_var_names.BOUNDARY_API_KEY`?

  Valid [pack.env_var_names] settings:
    BAML_TELEMETRY
    BOUNDARY_PROJECT
    BOUNDARY_API_KEY"#,
            ),
            (
                r#"[boundary]
api_ur = "https://api.cloud.boundaryml.com""#,
                r#"Unknown setting `boundary.api_ur` in baml.toml.

  Did you mean `boundary.api_url`?

  Valid [boundary] settings:
    project
    api_url"#,
            ),
        ] {
            let manifest: toml::Value = input.parse().unwrap();
            // Both producers reject malformed publisher settings before minting.
            for kind in [ArtifactKind::Pack, ArtifactKind::Bridge] {
                assert_eq!(
                    ArtifactTelemetry::from_manifest(&manifest, kind)
                        .unwrap_err()
                        .to_string(),
                    expected
                );
            }
        }
    }

    #[test]
    fn telemetry_environment_is_validated_even_when_unused_or_overridden() {
        for name in ["pack", "bridge"] {
            for value in ["42", "false", "\"\"", "{}"] {
                let manifest: toml::Value = format!("[{name}]\ntelemetry_environment = {value}")
                    .parse()
                    .unwrap();
                for kind in [ArtifactKind::Pack, ArtifactKind::Bridge] {
                    assert_eq!(
                        ArtifactTelemetry::from_manifest(&manifest, kind)
                            .unwrap_err()
                            .to_string(),
                        format!(r#"{name}.telemetry_environment must be a non-empty string."#)
                    );
                }
            }
        }
    }

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

#[cfg(test)]
mod ingestion_tests {
    use super::*;
    fn embedded(policy: &ArtifactTelemetry, payload: &[u8]) -> Box<EmbeddedIngestion> {
        Box::new(EmbeddedIngestion {
            token: PublicCredential::new(format!("bdry_public_{}", "a".repeat(64))),
            token_id: "token".into(),
            build_id: "build".into(),
            product_id: "product".into(),
            destination: IngestionDestination {
                org_id: "o".into(),
                project_id: "p".into(),
                environment_id: "e".into(),
            },
            bytecode_digest: build_digest(payload, policy).unwrap(),
        })
    }
    #[test]
    fn embedded_defaults_do_not_read_ambient_identity_or_destination() {
        for overrides in [
            DestinationOverrides::Default,
            DestinationOverrides::Disabled,
        ] {
            let mut policy = ArtifactTelemetry {
                destination_overrides: overrides,
                ..Default::default()
            };
            policy.embedded = Some(embedded(&policy, b"program"));
            assert_eq!(
                policy
                    .select_ingestion_with(|_| panic!("publisher credentials are fixed"))
                    .unwrap(),
                IngestionSelection::Embedded(policy.embedded.as_deref().unwrap())
            );
        }
    }
    #[test]
    fn explicit_bindings_select_caller_credentials_without_reusing_public_token() {
        let mut policy = ArtifactTelemetry {
            project: Some("publisher/app".into()),
            destination_overrides: DestinationOverrides::Named {
                project: "ACME_PROJECT".into(),
                api_key: "ACME_KEY".into(),
            },
            ..Default::default()
        };
        policy.embedded = Some(embedded(&policy, b"program"));
        for (project, key, expected_project) in [
            (
                Some("customer/app"),
                Some("bdry_secret_customer"),
                Some("customer/app"),
            ),
            (Some("customer/app"), None, Some("customer/app")),
            (None, Some("bdry_secret_customer"), None),
            (None, Some("local"), None),
        ] {
            assert_eq!(
                policy
                    .select_ingestion_with(|name| match name {
                        "ACME_PROJECT" => project.map(Into::into),
                        "ACME_KEY" => key.map(Into::into),
                        _ => panic!("unpermitted variable {name}"),
                    })
                    .unwrap(),
                IngestionSelection::Caller {
                    project: expected_project.map(Into::into),
                    api_key: key.map(Into::into)
                }
            );
        }
        assert_eq!(
            policy.select_ingestion_with(|_| None).unwrap(),
            IngestionSelection::Embedded(policy.embedded.as_deref().unwrap())
        );
    }
    #[test]
    fn ordinary_artifacts_use_standard_bindings_and_manifest_project() {
        let policy = ArtifactTelemetry {
            project: Some("org/app".into()),
            ..Default::default()
        };
        assert_eq!(
            policy.select_ingestion_with(|_| None).unwrap(),
            IngestionSelection::Caller {
                project: Some("org/app".into()),
                api_key: None
            }
        );
        assert_eq!(
            policy
                .select_ingestion_with(
                    |name| (name == "BOUNDARY_API_KEY").then(|| "bdry_secret_customer".into())
                )
                .unwrap(),
            IngestionSelection::Caller {
                project: None,
                api_key: Some("bdry_secret_customer".into())
            }
        );
    }
    #[test]
    fn destination_bindings_are_a_pair_and_errors_are_actionable() {
        for table in ["pack", "bridge"] {
            let manifest: toml::Value =
                format!("[{table}.env_var_names]\nBOUNDARY_PROJECT = \"CUSTOM_PROJECT\"")
                    .parse()
                    .unwrap();
            let kind = if table == "pack" {
                ArtifactKind::Pack
            } else {
                ArtifactKind::Bridge
            };
            assert_eq!(
                ArtifactTelemetry::from_manifest(&manifest, kind)
                    .unwrap_err()
                    .to_string(),
                format!(
                    r#"{table}.env_var_names.BOUNDARY_PROJECT and BOUNDARY_API_KEY must both be false or distinct valid environment-variable names."#
                )
            );
        }
        let policy = ArtifactTelemetry::default();
        assert_eq!(
            policy
                .select_ingestion_with(|_| Some("".into()))
                .unwrap_err()
                .to_string(),
            r#"BOUNDARY_PROJECT must be a non-empty UTF-8 value."#
        );
    }
    #[test]
    fn incomplete_embedded_destinations_fail_local_validation() {
        for field in ["org_id", "project_id", "environment_id"] {
            let mut policy = ArtifactTelemetry::default();
            let mut credential = embedded(&policy, b"program");
            match field {
                "org_id" => credential.destination.org_id.clear(),
                "project_id" => credential.destination.project_id.clear(),
                "environment_id" => credential.destination.environment_id.clear(),
                _ => unreachable!(),
            }
            policy.embedded = Some(credential);
            assert_eq!(
                policy.verify_build(b"program").unwrap_err().to_string(),
                r#"The embedded telemetry credential is invalid. Rebuild the artifact with `--embed-telemetry`."#,
                "empty {field} must fail even when the bytecode digest matches"
            );
        }
    }
    #[test]
    fn fingerprint_excludes_credentials_and_rejects_other_programs_or_policy() {
        let mut policy = ArtifactTelemetry::default();
        let original = build_digest(b"complete bytecode", &policy).unwrap();
        policy.embedded = Some(embedded(&policy, b"complete bytecode"));
        assert_eq!(
            original,
            build_digest(b"complete bytecode", &policy).unwrap()
        );
        policy.verify_build(b"complete bytecode").unwrap();
        assert_eq!(
            policy
                .verify_build(b"other bytecode")
                .unwrap_err()
                .to_string(),
            r#"Embedded telemetry does not match this artifact. Rebuild with `baml pack --embed-telemetry` or `baml generate --embed-telemetry`."#
        );
        policy.recording_level = RecordingLevel::Off;
        assert_ne!(
            original,
            build_digest(b"complete bytecode", &policy).unwrap()
        );
        assert_eq!(
            format!("{:?}", policy.embedded.unwrap().token),
            "[REDACTED]"
        );
    }
}
