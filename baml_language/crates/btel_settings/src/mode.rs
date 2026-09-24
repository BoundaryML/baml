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
    match std::env::var(ENV_VAR) {
        Ok(value) if value == "off" => Ok(None),
        Ok(value) => value.parse().map(Some),
        Err(std::env::VarError::NotPresent) => Ok(Some(AutoTelemetryLevel::default())),
        Err(std::env::VarError::NotUnicode(_)) => Err(InvalidTelemetryLevel),
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InvalidTelemetryLevel;
impl fmt::Display for InvalidTelemetryLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{ENV_VAR} must be off, low, medium, or high")
    }
}
impl std::error::Error for InvalidTelemetryLevel {}
impl FromStr for AutoTelemetryLevel {
    type Err = InvalidTelemetryLevel;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "low" => Ok(Self::Low),
            "medium" => Ok(Self::Medium),
            "high" => Ok(Self::High),
            _ => Err(InvalidTelemetryLevel),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        for invalid in ["off", "auto", "", "HIGH", " medium "] {
            assert_eq!(
                invalid.parse::<AutoTelemetryLevel>(),
                Err(InvalidTelemetryLevel)
            );
        }
    }
}
