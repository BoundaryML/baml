#![allow(clippy::doc_markdown)]

//! Integration coverage for the EC2 IMDS credential provider, mirroring the
//! behavioral cases from upstream `aws-config`'s `imds/credentials.rs` and
//! `imds/client.rs` test modules — adapted to our slim resolver.
//!
//! Upstream exercises a full Smithy IMDS client (token caching, retries,
//! expiry, endpoint-mode overrides) that we did not port. We mirror only the
//! cases that map onto our resolver's behavior: the `IMDSv2` token → role →
//! credentials happy path, the `AWS_EC2_METADATA_DISABLED` short-circuit, the
//! `IMDSv1` fallback when the token PUT fails, and the empty-role error.

mod common;

use std::sync::{Arc, Mutex};

use aws_config::*;
use common::MockIo;

const IMDS_TOKEN_URL: &str = "http://169.254.169.254/latest/api/token";
const IMDS_ROLE_URL: &str = "http://169.254.169.254/latest/meta-data/iam/security-credentials/";
const IMDS_CREDS_URL: &str =
    "http://169.254.169.254/latest/meta-data/iam/security-credentials/myrole";

/// JSON body returned by the per-role IMDS credentials endpoint.
const CREDS_JSON: &str = r#"{
  "Code": "Success",
  "AccessKeyId": "ASIAIMDSEXAMPLE",
  "SecretAccessKey": "imds-secret-key",
  "Token": "imds-session-token",
  "Expiration": "2099-01-01T00:00:00Z"
}"#;

/// Look up a header value (case-sensitive) from the request header slice.
fn header<'a>(headers: &'a [(String, String)], key: &str) -> Option<&'a str> {
    headers
        .iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.as_str())
}

/// Mirrors upstream `imds_credentials_provider` happy path: `IMDSv2` fetches a
/// session token, then the role name, then the role credentials. Asserts the
/// concrete credential values and that the token header is forwarded on both
/// metadata GETs.
#[tokio::test]
async fn imdsv2_happy_path_returns_credentials_and_forwards_token() {
    // Records the token header observed on each metadata GET so we can assert
    // it was forwarded.
    let role_token: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    let creds_token: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    let role_token_h = role_token.clone();
    let creds_token_h = creds_token.clone();

    let mock = MockIo::new().http(move |method, url, headers| {
        match (method, url) {
            ("PUT", IMDS_TOKEN_URL) => {
                // The TTL header must be present on the token request.
                assert_eq!(
                    header(headers, "x-aws-ec2-metadata-token-ttl-seconds"),
                    Some("21600"),
                    "token PUT must carry the TTL header"
                );
                Ok(HttpResponse {
                    status: 200,
                    body: "IMDS-SESSION-TOKEN".to_string(),
                })
            }
            ("GET", IMDS_ROLE_URL) => {
                *role_token_h.lock().unwrap() =
                    header(headers, "x-aws-ec2-metadata-token").map(str::to_string);
                Ok(HttpResponse {
                    status: 200,
                    body: "myrole".to_string(),
                })
            }
            ("GET", IMDS_CREDS_URL) => {
                *creds_token_h.lock().unwrap() =
                    header(headers, "x-aws-ec2-metadata-token").map(str::to_string);
                Ok(HttpResponse {
                    status: 200,
                    body: CREDS_JSON.to_string(),
                })
            }
            other => panic!("unexpected IMDS request: {other:?}"),
        }
    });

    let creds = resolve_credentials(&mock, None)
        .await
        .expect("IMDSv2 should yield credentials");

    assert_eq!(creds.access_key_id, "ASIAIMDSEXAMPLE");
    assert_eq!(creds.secret_access_key, "imds-secret-key");
    assert_eq!(creds.session_token.as_deref(), Some("imds-session-token"));

    // Exactly three calls: token PUT, role GET, creds GET.
    assert_eq!(mock.http_calls(), 3);
    assert_eq!(mock.last_http_url().as_deref(), Some(IMDS_CREDS_URL));

    // The IMDSv2 session token must be forwarded on both metadata GETs.
    assert_eq!(
        role_token.lock().unwrap().as_deref(),
        Some("IMDS-SESSION-TOKEN"),
        "role GET must carry the session token header"
    );
    assert_eq!(
        creds_token.lock().unwrap().as_deref(),
        Some("IMDS-SESSION-TOKEN"),
        "creds GET must carry the session token header"
    );
}

/// Mirrors upstream's handling of `AWS_EC2_METADATA_DISABLED`: when set to
/// `true` the IMDS provider yields nothing and makes no network call. With no
/// other provider configured the chain ends in `NoCredentials`.
#[tokio::test]
async fn metadata_disabled_skips_imds_entirely() {
    let mock =
        MockIo::new()
            .env("AWS_EC2_METADATA_DISABLED", "true")
            .http(|_method, _url, _headers| {
                panic!("IMDS must not be contacted when metadata is disabled");
            });

    let err = resolve_credentials(&mock, None)
        .await
        .expect_err("disabled IMDS with no other provider must fail");

    assert!(
        matches!(err, ConfigError::NoCredentials(_)),
        "expected NoCredentials, got {err:?}"
    );
    // No HTTP call should have been attempted for IMDS.
    assert_eq!(mock.http_calls(), 0);
}

/// Mirrors upstream `imds/client.rs` token-fallback behavior: when the token
/// PUT does not return 200, the client falls back to `IMDSv1` and the subsequent
/// metadata GETs carry NO token header. Credentials still resolve.
#[tokio::test]
async fn token_put_failure_falls_back_to_imdsv1_without_token_header() {
    let role_had_token: Arc<Mutex<Option<bool>>> = Arc::new(Mutex::new(None));
    let creds_had_token: Arc<Mutex<Option<bool>>> = Arc::new(Mutex::new(None));
    let role_h = role_had_token.clone();
    let creds_h = creds_had_token.clone();

    let mock = MockIo::new().http(move |method, url, headers| {
        match (method, url) {
            ("PUT", IMDS_TOKEN_URL) => Ok(HttpResponse {
                // Non-200: e.g. older instances / IMDSv2 unavailable.
                status: 403,
                body: String::new(),
            }),
            ("GET", IMDS_ROLE_URL) => {
                *role_h.lock().unwrap() =
                    Some(header(headers, "x-aws-ec2-metadata-token").is_some());
                Ok(HttpResponse {
                    status: 200,
                    body: "myrole".to_string(),
                })
            }
            ("GET", IMDS_CREDS_URL) => {
                *creds_h.lock().unwrap() =
                    Some(header(headers, "x-aws-ec2-metadata-token").is_some());
                Ok(HttpResponse {
                    status: 200,
                    body: CREDS_JSON.to_string(),
                })
            }
            other => panic!("unexpected IMDS request: {other:?}"),
        }
    });

    let creds = resolve_credentials(&mock, None)
        .await
        .expect("IMDSv1 fallback should still yield credentials");

    assert_eq!(creds.access_key_id, "ASIAIMDSEXAMPLE");
    assert_eq!(creds.secret_access_key, "imds-secret-key");
    assert_eq!(creds.session_token.as_deref(), Some("imds-session-token"));

    // Both metadata GETs must omit the token header in the IMDSv1 fallback.
    assert_eq!(
        *role_had_token.lock().unwrap(),
        Some(false),
        "role GET must NOT carry a token header in IMDSv1 fallback"
    );
    assert_eq!(
        *creds_had_token.lock().unwrap(),
        Some(false),
        "creds GET must NOT carry a token header in IMDSv1 fallback"
    );
}

/// Mirrors upstream's "no instance profile attached" case: the role listing
/// endpoint returns an empty body, which our resolver surfaces as
/// `NoCredentials` rather than attempting a per-role fetch.
#[tokio::test]
async fn empty_role_body_yields_no_credentials() {
    let mock = MockIo::new().http(move |method, url, _headers| {
        match (method, url) {
            ("PUT", IMDS_TOKEN_URL) => Ok(HttpResponse {
                status: 200,
                body: "IMDS-SESSION-TOKEN".to_string(),
            }),
            ("GET", IMDS_ROLE_URL) => Ok(HttpResponse {
                // No instance profile is attached: empty role listing.
                status: 200,
                body: "   ".to_string(),
            }),
            ("GET", IMDS_CREDS_URL) => {
                panic!("must not fetch per-role credentials when no role is listed")
            }
            other => panic!("unexpected IMDS request: {other:?}"),
        }
    });

    let err = resolve_credentials(&mock, None)
        .await
        .expect_err("empty role body must not yield credentials");

    assert!(
        matches!(err, ConfigError::NoCredentials(_)),
        "expected NoCredentials, got {err:?}"
    );

    // Token PUT + role GET only; the per-role creds GET is never made.
    assert_eq!(mock.http_calls(), 2);
}
