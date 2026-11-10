//! Selected system fields and explicit `AI_AGENT` harness-name metadata for BCS.
//! No environment enumeration or named-marker contents are captured. `AI_AGENT`
//! is caller-provided metadata, subject to size/control bounds, not redaction.

use std::{ffi::OsString, sync::OnceLock};

use btel_types::context::{Context, ContextPatch, ContextValue};

struct System {
    os: &'static str,
    version: Option<String>,
    distro: Option<String>,
}

/// Enrich only the recorded process context, without changing execution context
/// or the caller's identity/metadata. System probes run at most once, and only
/// after the engine has selected enabled cloud recording.
pub(super) fn context(launch: &Context) -> Context {
    static SYSTEM: OnceLock<System> = OnceLock::new();
    launch.with_patch(&collect(SYSTEM.get_or_init(system), baml_env::os_var))
}

/// Probe native OS information, retaining only the version and distribution
/// alongside the normalized target OS name; unknown details stay absent.
fn system() -> System {
    let info = os_info::get();
    System {
        os: match std::env::consts::OS {
            "macos" => "Mac OS X",
            "windows" => "Windows",
            "linux" => "Linux",
            "freebsd" => "FreeBSD",
            other => other,
        },
        version: (!matches!(info.version(), os_info::Version::Unknown))
            .then(|| info.version().to_string()),
        distro: (cfg!(target_os = "linux")
            && !matches!(
                info.os_type(),
                os_info::Type::Linux | os_info::Type::Unknown
            ))
        .then(|| info.os_type().to_string()),
    }
}

/// Build the fixed scalar schema from selected system fields, exact CI status
/// and the shared detector. Optional omissions also remove caller spoofing.
fn collect(system: &System, mut read: impl FnMut(&str) -> Option<OsString>) -> ContextPatch {
    let text = |value: &str| {
        // Missing or implausible probe results are omitted, never fatal. Bound
        // strings by rejecting oversized values rather than truncating UTF-8.
        (!value.trim().is_empty() && value.len() <= 128 && !value.chars().any(char::is_control))
            .then(|| ContextValue::String(value.to_owned()))
    };
    // CI is a standard variable, not a BAML boolean: exactly CI=true, without
    // forwarding provider names or treating arbitrary nonempty values as true.
    let is_ci = read("CI").is_some_and(|value| value == "true");
    // Invalid dev-only overrides must never fail SDK startup. CLI output policy
    // still surfaces the detector's strict validation error to its own caller.
    let agent = baml_env::agent::detect_with(&mut read).ok().flatten();
    ContextPatch {
        metadata: [
            ("os", text(system.os)),
            ("os_version", system.version.as_deref().and_then(text)),
            ("os_distro", system.distro.as_deref().and_then(text)),
            ("architecture", text(std::env::consts::ARCH)),
            ("is_ci", Some(ContextValue::Bool(is_ci))),
            ("agent_harness", agent.as_deref().and_then(text)),
        ]
        .into_iter()
        .map(|(key, value)| (format!("baml.environment.{key}"), value))
        .collect(),
        distinct_id: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn platform() -> System {
        System {
            os: "Linux",
            version: Some("24.04".into()),
            distro: Some("Ubuntu".into()),
        }
    }

    #[test]
    /// The complete allowlist preserves scalar types and never copies named-marker
    /// values or reads unrelated environment variables.
    fn schema_is_explicit_and_values_keep_their_types() {
        let context = Context::default().with_patch(&collect(&platform(), |name| match name {
            "CI" => Some("true".into()),
            "CODEX_SANDBOX" => Some("private-session-marker".into()),
            baml_env::agent::DISABLE_AGENT_DETECTION_ENV | "CLAUDECODE" | "AI_AGENT" => None,
            _ => panic!("unexpected environment lookup: {name}"),
        }));
        let metadata = context.metadata();
        assert_eq!(metadata.len(), 6);
        for (key, expected) in [
            ("os", "Linux"),
            ("os_version", "24.04"),
            ("os_distro", "Ubuntu"),
            ("architecture", std::env::consts::ARCH),
            ("agent_harness", "codex"),
        ] {
            assert_eq!(
                metadata[&format!("baml.environment.{key}")],
                ContextValue::String(expected.into())
            );
        }
        assert_eq!(metadata["baml.environment.is_ci"], ContextValue::Bool(true));
    }

    #[test]
    /// Only the literal standard CI value enables the flag; other values
    /// collapse to false instead of being transmitted as provider names.
    fn ci_means_exactly_true_and_never_contains_the_raw_value() {
        for (value, expected) in [
            (None, false),
            (Some(""), false),
            (Some("false"), false),
            (Some("0"), false),
            (Some("1"), false),
            (Some("TRUE"), false),
            (Some(" true "), false),
            (Some("private-provider-name"), false),
            (Some("true"), true),
        ] {
            let patch = collect(&platform(), |name| {
                (name == "CI").then(|| value.map(OsString::from)).flatten()
            });
            assert_eq!(
                patch.metadata["baml.environment.is_ci"],
                Some(ContextValue::Bool(expected))
            );
        }
    }

    #[test]
    /// Unknown, oversized and invalid optional values are omitted without
    /// turning metadata collection into an SDK startup failure.
    fn missing_platform_values_and_malformed_override_are_best_effort() {
        let mut system = platform();
        system.version = Some(String::new());
        system.distro = Some("x".repeat(129));
        let patch = collect(&system, |name| match name {
            baml_env::agent::DISABLE_AGENT_DETECTION_ENV => Some("invalid".into()),
            "CLAUDECODE" => Some("1".into()),
            _ => None,
        });
        for key in ["os_version", "os_distro", "agent_harness"] {
            assert_eq!(patch.metadata[&format!("baml.environment.{key}")], None);
        }
        system.version = None;
        system.distro = Some("bad\nvalue".into());
        let patch = collect(&system, |_| None);
        assert_eq!(patch.metadata["baml.environment.os_version"], None);
        assert_eq!(patch.metadata["baml.environment.os_distro"], None);
    }

    #[test]
    /// Telemetry consumes the shared detector's explicit name and escape hatch.
    fn shared_detector_override_and_generic_marker_reach_metadata() {
        for (disabled, expected) in [("true", None), ("false", Some("custom-harness"))] {
            let patch = collect(&platform(), |name| match name {
                baml_env::agent::DISABLE_AGENT_DETECTION_ENV => Some(disabled.into()),
                "AI_AGENT" => Some(" custom-harness ".into()),
                "CODEX_SANDBOX" => Some("private-marker-value".into()),
                _ => None,
            });
            assert_eq!(
                patch.metadata["baml.environment.agent_harness"],
                expected.map(|value| ContextValue::String(value.into()))
            );
        }
    }

    #[test]
    /// Explicit harness names remain bounded metadata; malformed names are
    /// omitted without disrupting collection of the other environment fields.
    fn malformed_explicit_harness_names_are_omitted() {
        for value in ["x".repeat(129), "bad\nname".to_owned()] {
            let patch = collect(&platform(), |name| {
                (name == "AI_AGENT").then(|| value.clone().into())
            });
            assert_eq!(patch.metadata["baml.environment.agent_harness"], None);
            assert_eq!(
                patch.metadata["baml.environment.os"],
                Some(ContextValue::String("Linux".into()))
            );
        }
    }

    #[test]
    /// Reserved environment fields are authoritative only in the recorded copy;
    /// explicit identity and unrelated caller metadata survive unchanged.
    fn automatic_fields_preserve_unrelated_metadata_and_identity() {
        let launch = Context::default().with_patch(&ContextPatch {
            metadata: [
                (
                    "owner".into(),
                    Some(ContextValue::String("customer".into())),
                ),
                (
                    "baml.environment.os".into(),
                    Some(ContextValue::String("spoofed".into())),
                ),
                (
                    "baml.environment.agent_harness".into(),
                    Some(ContextValue::String("spoofed".into())),
                ),
            ]
            .into(),
            distinct_id: Some("explicit-identity".into()),
        });
        let recorded = launch.with_patch(&collect(&platform(), |_| None));
        assert_eq!(recorded.distinct_id(), launch.distinct_id());
        assert_eq!(recorded.metadata()["owner"], launch.metadata()["owner"]);
        assert_eq!(
            recorded.metadata()["baml.environment.os"],
            ContextValue::String("Linux".into())
        );
        assert!(
            !recorded
                .metadata()
                .contains_key("baml.environment.agent_harness")
        );
        assert_eq!(
            launch.metadata()["baml.environment.os"],
            ContextValue::String("spoofed".into())
        );
    }
}
