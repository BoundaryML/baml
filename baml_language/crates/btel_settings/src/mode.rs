//! Defaults for functions requesting auto (`null` / no explicit policy).
//! Global disablement is represented by absence, not an automatic level.
use std::{fmt, str::FromStr};

pub const ENV_VAR: &str = "BAML_TELEMETRY";

/// Used only when a function has no explicit telemetry policy.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum AutoTelemetryLevel {
    /// Do not observe the function.
    Low,
    /// Timing for ordinary functions; spans and default captures for LLM functions.
    #[default]
    Medium,
    /// Spans for supported functions, without enabling additional value capture.
    High,
}

/// Read once per engine. `None` disables all telemetry, including explicit policies.
pub fn from_env() -> Result<Option<AutoTelemetryLevel>, InvalidTelemetryLevel> {
    from_value(baml_env::os_var(ENV_VAR))
}

/// Parse a runtime switch using shared environment normalization. Disabling
/// booleans turn recording off; enabling booleans keep the default detail level.
fn from_value(
    value: Option<std::ffi::OsString>,
) -> Result<Option<AutoTelemetryLevel>, InvalidTelemetryLevel> {
    let value = baml_env::string_from(ENV_VAR, value).map_err(|_| InvalidTelemetryLevel)?;
    match value {
        None => Ok(Some(AutoTelemetryLevel::default())),
        Some(value) => match baml_env::parse_bool(&value) {
            Some(false) => Ok(None),
            Some(true) => Ok(Some(AutoTelemetryLevel::default())),
            None => value.parse().map(Some),
        },
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InvalidTelemetryLevel;
impl fmt::Display for InvalidTelemetryLevel {
    /// Describe the accepted recording levels and boolean switch values.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{ENV_VAR} must be off, low, medium, high, or a boolean")
    }
}
impl std::error::Error for InvalidTelemetryLevel {}
impl FromStr for AutoTelemetryLevel {
    type Err = InvalidTelemetryLevel;
    /// Parse only automatic detail levels; global enablement belongs to `from_env`.
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        baml_env::parse_choice(
            value,
            &[
                ("low", Self::Low),
                ("medium", Self::Medium),
                ("high", Self::High),
            ],
        )
        .ok_or(InvalidTelemetryLevel)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Automatic function levels exclude global disablement and the ambiguous `auto`.
    #[test]
    fn automatic_levels_exclude_global_disablement_and_ambiguous_auto_name() {
        assert_eq!(AutoTelemetryLevel::default(), AutoTelemetryLevel::Medium);
        for (name, level) in [
            ("low", AutoTelemetryLevel::Low),
            ("medium", AutoTelemetryLevel::Medium),
            ("high", AutoTelemetryLevel::High),
        ] {
            assert_eq!(name.parse(), Ok(level));
        }
        assert_eq!("HIGH".parse(), Ok(AutoTelemetryLevel::High));
        assert_eq!(" medium ".parse(), Ok(AutoTelemetryLevel::Medium));
        for invalid in ["off", "auto", ""] {
            assert_eq!(
                invalid.parse::<AutoTelemetryLevel>(),
                Err(InvalidTelemetryLevel)
            );
        }
    }

    /// CLI/engine switches and artifact bindings agree on supported values.
    #[test]
    fn runtime_switch_uses_the_same_booleans_and_levels_as_artifacts() {
        let artifact = crate::artifact::ArtifactTelemetry {
            recording_level_envvar: Some(ENV_VAR.into()),
            ..Default::default()
        };
        for value in [
            None,
            Some(""),
            Some(" "),
            Some("off"),
            Some("OFF"),
            Some("0"),
            Some("false"),
            Some("no"),
            Some("on"),
            Some("1"),
            Some("true"),
            Some("yes"),
            Some("low"),
            Some("Medium"),
            Some(" high "),
        ] {
            let raw = value.map(std::ffi::OsString::from);
            assert_eq!(
                from_value(raw.clone()).unwrap(),
                artifact.resolve_level_with(|_| raw.clone()),
                "{value:?}"
            );
        }
        for value in ["none", "disabled", "hgih", "auto"] {
            assert_eq!(from_value(Some(value.into())), Err(InvalidTelemetryLevel));
        }
        #[cfg(unix)]
        assert_eq!(
            from_value(Some(std::os::unix::ffi::OsStringExt::from_vec(vec![0xff]))),
            Err(InvalidTelemetryLevel)
        );
    }
}
