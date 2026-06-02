// Prose docs reference protocol names (SigV4, SigV4a) and header constants that
// read fine without backticks; signing inherently takes many parameters.
#![allow(clippy::doc_markdown, clippy::too_many_arguments)]

//! A slim, self-contained AWS Signature Version 4 (SigV4) signer.
//!
//! This replaces the upstream `aws-sigv4` crate for BAML's only use case:
//! signing AWS Bedrock Converse HTTP requests. It implements the exact same
//! canonical-request construction and signing algorithm as the AWS SDK for the
//! default HTTP signing settings (header location, double URI-path encoding,
//! URI-path normalization, no payload-checksum header, session token included
//! in the canonical request), so signatures are byte-for-byte identical to the
//! ones the upstream crate produces.
//!
//! Compared to `aws-sigv4` + `aws-smithy-*` + `aws-credential-types`, this
//! drops the Smithy runtime, async machinery, event streams, presigning, and
//! SigV4a entirely. The only dependencies are `hmac`, `sha2`, `hex`, and
//! `percent-encoding`.

use std::{collections::BTreeMap, fmt};

use hmac::{Hmac, Mac};
use percent_encoding::{AsciiSet, CONTROLS, utf8_percent_encode};
use sha2::{Digest, Sha256};
use web_time::{SystemTime, UNIX_EPOCH};

type HmacSha256 = Hmac<Sha256>;

const ALGORITHM: &str = "AWS4-HMAC-SHA256";

/// Headers excluded from signing (altered by proxies or computed during
/// signing). Mirrors `SigningSettings::default().excluded_headers` in the
/// upstream `aws-sigv4`.
const EXCLUDED_HEADERS: [&str; 4] = [
    "authorization",
    "user-agent",
    "x-amzn-trace-id",
    "transfer-encoding",
];

/// Base set of characters that must be percent-encoded, matching
/// `aws_smithy_http::urlencode::BASE_SET`. This is the encoding used for query
/// strings and (with `/` re-added) for single-segment path labels.
const BASE_SET: &AsciiSet = &CONTROLS
    .add(b' ')
    .add(b'/')
    .add(b':')
    .add(b',')
    .add(b'?')
    .add(b'#')
    .add(b'[')
    .add(b']')
    .add(b'{')
    .add(b'}')
    .add(b'|')
    .add(b'@')
    .add(b'!')
    .add(b'$')
    .add(b'&')
    .add(b'\'')
    .add(b'(')
    .add(b')')
    .add(b'*')
    .add(b'+')
    .add(b';')
    .add(b'=')
    .add(b'%')
    .add(b'<')
    .add(b'>')
    .add(b'"')
    .add(b'^')
    .add(b'`')
    .add(b'\\');

/// Greedy path-label set: like [`BASE_SET`] but leaves `/` untouched so that an
/// already-encoded path keeps its segment separators. Matches
/// `aws_smithy_http::label::GREEDY`.
const GREEDY_SET: &AsciiSet = &BASE_SET.remove(b'/');

// ---------------------------------------------------------------------------
// Credentials
// ---------------------------------------------------------------------------

/// AWS credentials used for signing.
#[derive(Clone)]
pub struct Credentials {
    pub access_key_id: String,
    pub secret_access_key: String,
    pub session_token: Option<String>,
}

impl Credentials {
    pub fn new(
        access_key_id: impl Into<String>,
        secret_access_key: impl Into<String>,
        session_token: Option<String>,
    ) -> Self {
        Self {
            access_key_id: access_key_id.into(),
            secret_access_key: secret_access_key.into(),
            session_token,
        }
    }
}

impl fmt::Debug for Credentials {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Never print the secret or session token.
        f.debug_struct("Credentials")
            .field("access_key_id", &self.access_key_id)
            .field("secret_access_key", &"** REDACTED **")
            .field(
                "session_token",
                &self.session_token.as_ref().map(|_| "** REDACTED **"),
            )
            .finish()
    }
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub enum SigningError {
    /// The request URL could not be parsed into scheme/authority/path.
    InvalidUri(String),
}

impl fmt::Display for SigningError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SigningError::InvalidUri(u) => write!(f, "invalid request URI for signing: {u}"),
        }
    }
}

impl std::error::Error for SigningError {}

// ---------------------------------------------------------------------------
// Public signing entry point
// ---------------------------------------------------------------------------

/// Sign an HTTP request with SigV4 and return the headers that must be added to
/// it: `x-amz-date`, `authorization`, and (when the credentials carry a session
/// token) `x-amz-security-token`.
///
/// `headers` are the request's existing headers (e.g. `content-type`). They are
/// folded into the canonical request exactly as the AWS SDK would.
pub fn sign_request(
    method: &str,
    url: &str,
    headers: &[(&str, &str)],
    body: &[u8],
    credentials: &Credentials,
    region: &str,
    service: &str,
    time: SystemTime,
) -> Result<Vec<(String, String)>, SigningError> {
    let parts = UrlParts::parse(url)?;

    let date_stamp = format_date(time);
    let date_time = format_date_time(time);
    let payload_hash = sha256_hex(body);

    // --- Canonical headers (only signed headers appear in the block) --------
    // Group lowercased header names to their trimmed, comma-joined values.
    let mut grouped: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (name, value) in headers {
        grouped
            .entry(name.to_ascii_lowercase())
            .or_default()
            .push(trim_all(value));
    }
    grouped
        .entry("host".to_string())
        .or_default()
        .push(parts.host_header());
    grouped
        .entry("x-amz-date".to_string())
        .or_default()
        .push(date_time.clone());
    if let Some(token) = &credentials.session_token {
        grouped
            .entry("x-amz-security-token".to_string())
            .or_default()
            .push(token.clone());
    }

    let mut canonical_headers = String::new();
    let mut signed_names: Vec<&str> = Vec::new();
    for (name, values) in &grouped {
        if EXCLUDED_HEADERS.contains(&name.as_str()) {
            continue;
        }
        signed_names.push(name);
        canonical_headers.push_str(name);
        canonical_headers.push(':');
        canonical_headers.push_str(&values.join(","));
        canonical_headers.push('\n');
    }
    let signed_headers = signed_names.join(";");

    // --- Canonical request --------------------------------------------------
    let canonical_path = canonicalize_path(&parts.path);
    let canonical_query = canonicalize_query(&parts.query);

    let canonical_request = format!(
        "{method}\n{canonical_path}\n{canonical_query}\n{canonical_headers}\n{signed_headers}\n{payload_hash}"
    );

    // --- String to sign -----------------------------------------------------
    let scope = format!("{date_stamp}/{region}/{service}/aws4_request");
    let string_to_sign = format!(
        "{ALGORITHM}\n{date_time}\n{scope}\n{}",
        sha256_hex(canonical_request.as_bytes())
    );

    // --- Signature ----------------------------------------------------------
    let signing_key =
        generate_signing_key(&credentials.secret_access_key, &date_stamp, region, service);
    let signature = hex::encode(hmac(&signing_key, string_to_sign.as_bytes()));

    let authorization = format!(
        "{ALGORITHM} Credential={}/{scope}, SignedHeaders={signed_headers}, Signature={signature}",
        credentials.access_key_id
    );

    let mut out = vec![
        ("x-amz-date".to_string(), date_time),
        ("authorization".to_string(), authorization),
    ];
    if let Some(token) = &credentials.session_token {
        out.push(("x-amz-security-token".to_string(), token.clone()));
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// HMAC / SHA helpers
// ---------------------------------------------------------------------------

fn hmac(key: &[u8], data: &[u8]) -> Vec<u8> {
    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC accepts any key length");
    mac.update(data);
    mac.finalize().into_bytes().to_vec()
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

/// Derive the SigV4 signing key:
/// `HMAC(HMAC(HMAC(HMAC("AWS4"+secret, date), region), service), "aws4_request")`.
fn generate_signing_key(secret: &str, date_stamp: &str, region: &str, service: &str) -> Vec<u8> {
    let k_date = hmac(format!("AWS4{secret}").as_bytes(), date_stamp.as_bytes());
    let k_region = hmac(&k_date, region.as_bytes());
    let k_service = hmac(&k_region, service.as_bytes());
    hmac(&k_service, b"aws4_request")
}

// ---------------------------------------------------------------------------
// Path / query canonicalization
// ---------------------------------------------------------------------------

/// Normalize then double-encode the path, matching the AWS default settings
/// (`UriPathNormalizationMode::Enabled` + `PercentEncodingMode::Double`).
fn canonicalize_path(path: &str) -> String {
    let normalized = normalize_uri_path(path);
    // Double mode: the path is already URI-encoded, so re-encode it greedily
    // (which also turns each existing `%` into `%25`).
    utf8_percent_encode(&normalized, GREEDY_SET).to_string()
}

/// RFC 3986 dot-segment removal, matching `normalize_uri_path` in `aws-sigv4`.
fn normalize_uri_path(uri_path: &str) -> String {
    if uri_path.is_empty() {
        return "/".to_string();
    }
    let with_slash = if uri_path.starts_with('/') {
        uri_path.to_string()
    } else {
        format!("/{uri_path}")
    };
    if !(with_slash.contains('.') || with_slash.contains("//")) {
        return with_slash;
    }

    let mut normalized: Vec<&str> = Vec::new();
    for segment in with_slash.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                normalized.pop();
            }
            other => normalized.push(other),
        }
    }
    let mut result = normalized.join("/");
    if !result.starts_with('/') {
        result.insert(0, '/');
    }
    let ends_with_slash = ["/", "/.", "/./", "/..", "/../"]
        .iter()
        .any(|s| with_slash.ends_with(s));
    if ends_with_slash && !result.ends_with('/') {
        result.push('/');
    }
    result
}

/// Build the canonical query string: percent-encode each key and value, then
/// sort by the encoded pairs and join with `&`.
fn canonicalize_query(query: &str) -> String {
    if query.is_empty() {
        return String::new();
    }
    let mut pairs: Vec<(String, String)> = Vec::new();
    for part in query.split('&') {
        if part.is_empty() {
            continue;
        }
        let (raw_k, raw_v) = match part.split_once('=') {
            Some((k, v)) => (k, v),
            None => (part, ""),
        };
        let k = decode_then_encode(raw_k);
        let v = decode_then_encode(raw_v);
        pairs.push((k, v));
    }
    pairs.sort();
    pairs
        .into_iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join("&")
}

/// Percent-decode (so a value is not double-encoded) then re-encode with the
/// base set, matching `form_urlencoded::parse` + `query::fmt_string`.
fn decode_then_encode(s: &str) -> String {
    let decoded = percent_encoding::percent_decode_str(s).decode_utf8_lossy();
    utf8_percent_encode(&decoded, BASE_SET).to_string()
}

/// Trim leading/trailing ASCII spaces and collapse internal runs of spaces to a
/// single space, matching `trim_all` in `aws-sigv4`.
fn trim_all(value: &str) -> String {
    let trimmed = value.trim_matches(' ');
    let mut out = String::with_capacity(trimmed.len());
    let mut prev_space = false;
    for ch in trimmed.chars() {
        if ch == ' ' {
            if !prev_space {
                out.push(ch);
            }
            prev_space = true;
        } else {
            out.push(ch);
            prev_space = false;
        }
    }
    out
}

// ---------------------------------------------------------------------------
// URL parsing
// ---------------------------------------------------------------------------

struct UrlParts {
    scheme: String,
    authority: String,
    path: String,
    query: String,
}

impl UrlParts {
    fn parse(url: &str) -> Result<Self, SigningError> {
        let (scheme, rest) = url
            .split_once("://")
            .ok_or_else(|| SigningError::InvalidUri(url.to_string()))?;

        // Authority ends at the first '/', '?', or '#'.
        let authority_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
        let authority = &rest[..authority_end];
        let remainder = &rest[authority_end..];

        let (path, query) = match remainder.split_once('?') {
            Some((p, q)) => {
                // Drop any fragment from the query.
                let q = q.split('#').next().unwrap_or("");
                (p, q)
            }
            None => (remainder.split('#').next().unwrap_or(""), ""),
        };

        if authority.is_empty() {
            return Err(SigningError::InvalidUri(url.to_string()));
        }

        Ok(Self {
            scheme: scheme.to_ascii_lowercase(),
            authority: authority.to_string(),
            path: path.to_string(),
            query: query.to_string(),
        })
    }

    /// The value for the `host` header: the authority with the default port for
    /// the scheme stripped (matching the AWS SDK / RFC 2616 §14.23).
    fn host_header(&self) -> String {
        if let Some((host, port)) = self.authority.rsplit_once(':') {
            // Only treat as host:port when `port` is all digits (avoids
            // mishandling IPv6 literals, which we don't expect here anyway).
            if !port.is_empty() && port.bytes().all(|b| b.is_ascii_digit()) {
                let default = matches!(
                    (self.scheme.as_str(), port),
                    ("http", "80") | ("https", "443")
                );
                if default {
                    return host.to_string();
                }
            }
        }
        self.authority.clone()
    }
}

// ---------------------------------------------------------------------------
// Date formatting (no `time`/`chrono` dependency)
// ---------------------------------------------------------------------------

/// Civil date (year, month, day) from a Unix day number, via Howard Hinnant's
/// algorithm. `days` is days since 1970-01-01.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    // `d` is in [1, 31] and `mp +/- 9/3` is in [1, 12]; both fit a u32.
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32; // [1, 31]
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32; // [1, 12]
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

fn unix_secs(time: SystemTime) -> i64 {
    match time.duration_since(UNIX_EPOCH) {
        // Seconds since the epoch fit i64 for any realistic signing time.
        #[allow(clippy::cast_possible_wrap)]
        Ok(d) => d.as_secs() as i64,
        // Pre-epoch times are not expected; clamp to epoch.
        Err(_) => 0,
    }
}

/// Format as `YYYYMMDD`.
pub fn format_date(time: SystemTime) -> String {
    let secs = unix_secs(time);
    let days = secs.div_euclid(86_400);
    let (y, m, d) = civil_from_days(days);
    format!("{y:04}{m:02}{d:02}")
}

/// Format as `YYYYMMDD'T'HHMMSS'Z'`.
pub fn format_date_time(time: SystemTime) -> String {
    let secs = unix_secs(time);
    let days = secs.div_euclid(86_400);
    let tod = secs.rem_euclid(86_400);
    let (y, m, d) = civil_from_days(days);
    let hh = tod / 3600;
    let mm = (tod % 3600) / 60;
    let ss = tod % 60;
    format!("{y:04}{m:02}{d:02}T{hh:02}{mm:02}{ss:02}Z")
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use web_time::Duration;

    use super::*;

    fn at(secs: u64) -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(secs)
    }

    // 2015-08-30T12:36:00Z
    const T_20150830: u64 = 1_440_938_160;

    #[test]
    fn date_formatting() {
        assert_eq!(format_date(at(T_20150830)), "20150830");
        assert_eq!(format_date_time(at(T_20150830)), "20150830T123600Z");
        assert_eq!(format_date_time(UNIX_EPOCH), "19700101T000000Z");
    }

    #[test]
    fn sha256_empty() {
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn signing_key_kat() {
        // Known-answer test from the AWS SigV4 documentation (iam example).
        let key = generate_signing_key(
            "wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY",
            "20150830",
            "us-east-1",
            "iam",
        );
        let creq = "AWS4-HMAC-SHA256\n20150830T123600Z\n20150830/us-east-1/iam/aws4_request\nf536975d06c0309214f805bb90ccff089219ecd68b2577efef23edd43b7e1a59";
        assert_eq!(
            hex::encode(hmac(&key, creq.as_bytes())),
            "5d672d79c15b13162d9279b0855cfba6789a8edb4c82c400e06b5924a6f2b5d7"
        );
    }

    #[test]
    fn get_vanilla_full_signature() {
        // From the official aws-sig-v4-test-suite `get-vanilla` case.
        let creds = Credentials::new(
            "AKIDEXAMPLE",
            "wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY",
            None,
        );
        let signed = sign_request(
            "GET",
            "https://example.amazonaws.com/",
            &[],
            b"",
            &creds,
            "us-east-1",
            "service",
            at(T_20150830),
        )
        .unwrap();
        let authz = signed
            .iter()
            .find(|(k, _)| k == "authorization")
            .map(|(_, v)| v.as_str())
            .unwrap();
        assert_eq!(
            authz,
            "AWS4-HMAC-SHA256 Credential=AKIDEXAMPLE/20150830/us-east-1/service/aws4_request, \
             SignedHeaders=host;x-amz-date, \
             Signature=5fa00fa31553b73ebf1942676e86291e8372ff2a2260956d9b8aae1d763fbf31"
        );
    }

    #[test]
    fn host_header_strips_default_port() {
        let p = UrlParts::parse("https://example.amazonaws.com/").unwrap();
        assert_eq!(p.host_header(), "example.amazonaws.com");

        let p = UrlParts::parse("https://example.amazonaws.com:443/foo").unwrap();
        assert_eq!(p.host_header(), "example.amazonaws.com");

        let p = UrlParts::parse("http://localhost:4566/model/x/converse").unwrap();
        assert_eq!(p.host_header(), "localhost:4566");
        assert_eq!(p.path, "/model/x/converse");
    }

    #[test]
    fn double_encodes_already_encoded_path() {
        // A Bedrock model path with an encoded colon must be re-encoded for the
        // canonical request (`%3A` -> `%253A`).
        assert_eq!(
            canonicalize_path("/model/anthropic.claude-3-haiku-20240307-v1%3A0/converse"),
            "/model/anthropic.claude-3-haiku-20240307-v1%253A0/converse"
        );
    }

    #[test]
    fn signed_headers_include_request_headers_and_token() {
        let creds = Credentials::new("AKID", "SECRET", Some("token123".to_string()));
        let signed = sign_request(
            "POST",
            "https://bedrock-runtime.us-east-1.amazonaws.com/model/m/converse",
            &[("content-type", "application/json")],
            br#"{"messages":[]}"#,
            &creds,
            "us-east-1",
            "bedrock",
            at(T_20150830),
        )
        .unwrap();
        let authz = signed.iter().find(|(k, _)| k == "authorization").unwrap();
        assert!(
            authz
                .1
                .contains("SignedHeaders=content-type;host;x-amz-date;x-amz-security-token")
        );
        assert!(signed.iter().any(|(k, _)| k == "x-amz-security-token"));
        assert!(signed.iter().any(|(k, _)| k == "x-amz-date"));
    }
}
