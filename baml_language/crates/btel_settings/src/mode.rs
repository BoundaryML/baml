//! Engine-wide telemetry selection, read once before engine initialization.
use std::{fmt, str::FromStr};

pub const ENV_VAR: &str = "BAML_TELEMETRY";
pub const DEFAULT_MODE: TelemetryMode = TelemetryMode::Auto;

/// Visibility changes what is observed; these are not equivalent-work speed knobs.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TelemetryMode {
    /// No telemetry clock, VM state, processor or publisher.
    Off,
    /// Hidden function defaults; explicit policies can enable observation.
    Low,
    /// Existing function defaults and capture policies.
    Auto,
    /// Otherwise-visible functions become spans; hidden functions stay hidden.
    High,
}

impl TelemetryMode {
    /// Snapshot per engine. Later environment changes do not affect active VMs.
    pub fn from_env() -> Result<Self, InvalidTelemetryMode> {
        match std::env::var(ENV_VAR) {
            Ok(value) => value.parse(),
            Err(std::env::VarError::NotPresent) => Ok(DEFAULT_MODE),
            Err(std::env::VarError::NotUnicode(_)) => Err(InvalidTelemetryMode),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InvalidTelemetryMode;
impl fmt::Display for InvalidTelemetryMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{ENV_VAR} must be off, low, auto, or high")
    }
}
impl std::error::Error for InvalidTelemetryMode {}
impl FromStr for TelemetryMode {
    type Err = InvalidTelemetryMode;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "off" => Ok(Self::Off),
            "low" => Ok(Self::Low),
            "auto" => Ok(Self::Auto),
            "high" => Ok(Self::High),
            _ => Err(InvalidTelemetryMode),
        }
    }
}
