//! What a traced HTTP request records. A header or query value outside these
//! allowlists is stored as a hash, never as itself: requests carry API keys.

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

#[cfg(test)]
mod tests {
    use super::*;

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
