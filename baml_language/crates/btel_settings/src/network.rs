//! What a traced HTTP request records. A header or query value outside these
//! allowlists is stored as a hash, never as itself: requests carry API keys.
use std::fmt;

/// Read once per engine. `off` keeps request and response bodies out of the
/// recording; their events are still recorded, with their times.
pub const BODIES_ENV_VAR: &str = "BAML_TELEMETRY_HTTP_BODIES";

/// **Observability semantics.** A body over this size is recorded as truncated
/// instead of its content. Identical bodies share one CAS blob.
pub const BODY_CAPTURE_MAX_BYTES: usize = 8 * 1024 * 1024;

/// **Privacy boundary.** Request headers recorded as they are, by lowercase name.
pub const REQUEST_HEADERS: &[&str] = &[
    "content-type",
    "content-length",
    "accept",
    "accept-encoding",
    "user-agent",
    "anthropic-version",
    "anthropic-beta",
    "openai-beta",
    "idempotency-key",
];

/// **Privacy boundary.** Response headers recorded as they are, by lowercase name.
pub const RESPONSE_HEADERS: &[&str] = &[
    "content-type",
    "content-length",
    "content-encoding",
    "date",
    "retry-after",
    "retry-after-ms",
    "request-id",
    "x-request-id",
    "x-amzn-requestid",
    "cf-ray",
    "openai-processing-ms",
];

/// **Privacy boundary.** Response headers recorded as they are, by lowercase prefix.
pub const RESPONSE_HEADER_PREFIXES: &[&str] = &["x-ratelimit-", "anthropic-ratelimit-"];

/// **Privacy boundary.** URL query parameters whose values are recorded as they are.
pub const QUERY_PARAMETERS: &[&str] = &[];

/// A hashed value reads `sha256:` and the first hex digits of its SHA-256.
pub const HASH_PREFIX: &str = "sha256:";
pub const HASH_HEX_DIGITS: usize = 16;

/// Read once per engine: whether bodies are recorded. Unset means they are.
pub fn bodies_from_env() -> Result<bool, InvalidHttpBodies> {
    match std::env::var(BODIES_ENV_VAR) {
        Ok(value) => parse_bodies(&value),
        Err(std::env::VarError::NotPresent) => Ok(true),
        Err(std::env::VarError::NotUnicode(_)) => Err(InvalidHttpBodies),
    }
}

fn parse_bodies(value: &str) -> Result<bool, InvalidHttpBodies> {
    match value {
        "on" => Ok(true),
        "off" => Ok(false),
        _ => Err(InvalidHttpBodies),
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InvalidHttpBodies;
impl fmt::Display for InvalidHttpBodies {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{BODIES_ENV_VAR} must be on or off")
    }
}
impl std::error::Error for InvalidHttpBodies {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bodies_switch_accepts_only_on_and_off() {
        assert_eq!(parse_bodies("on"), Ok(true));
        assert_eq!(parse_bodies("off"), Ok(false));
        for invalid in ["", "OFF", "false", " off"] {
            assert_eq!(parse_bodies(invalid), Err(InvalidHttpBodies));
        }
    }

    #[test]
    fn allowlists_are_lowercase_header_names() {
        for name in REQUEST_HEADERS
            .iter()
            .chain(RESPONSE_HEADERS)
            .chain(RESPONSE_HEADER_PREFIXES)
        {
            assert_eq!(*name, name.to_ascii_lowercase());
        }
    }
}
