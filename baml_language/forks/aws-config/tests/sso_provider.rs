#![allow(clippy::doc_markdown)]

//! Integration tests for the SSO credential provider, ported from upstream
//! `aws-config`'s `src/profile/credentials.rs` `sso_tests` module (the
//! `create_test_fs` fixture + `create_inner_provider_exactly_once` case) and
//! the `src/sso/credentials.rs` portal-call behavior.
//!
//! The slim fork resolves SSO credentials through `resolve_credentials`:
//!   * read the active profile (config + credentials files),
//!   * determine start_url/region/cache-key (sso_session vs inline),
//!   * read the cached `accessToken` from `~/.aws/sso/cache/<sha1>.json`,
//!   * GET `https://portal.sso.{region}.amazonaws.com/federation/credentials`
//!     with header `x-amz-sso_bearer_token: <accessToken>`,
//!   * map the response `roleCredentials` -> `Credentials`.
//!
//! These tests use `MockIo` only; `file_contains("/sso/cache/", ...)` returns
//! the token JSON for whatever sha1 path the resolver computes, so we never
//! compute the hash ourselves.

mod common;

use std::sync::{Arc, Mutex};

use aws_config::{ConfigError, HttpResponse, resolve_credentials};
use common::MockIo;

type CapturedPortalRequest = Arc<Mutex<Option<(String, Vec<(String, String)>)>>>;

/// Build the portal `roleCredentials` JSON the SSO endpoint returns on success.
/// Mirrors upstream's fake client body
/// (`{"roleCredentials":{"accessKeyId":..,"secretAccessKey":..,"sessionToken":..}}`).
fn role_credentials_body() -> &'static str {
    r#"{"roleCredentials":{"accessKeyId":"ASIARTESTID","secretAccessKey":"TESTSECRETKEY","sessionToken":"TESTSESSIONTOKEN","expiration":1651516560000}}"#
}

/// Shape (a): profile references an `sso_session`; the `[sso-session NAME]`
/// section in the config file supplies `sso_start_url`/`sso_region`. The token
/// cache key is the session name.
///
/// Mirrors upstream `sso_tests::create_test_fs` + `create_inner_provider_exactly_once`:
/// profile `default` has `sso_session = dev`, `sso_account_id = 012345678901`,
/// `sso_role_name = SampleRole`; `[sso-session dev]` gives region `us-east-1`
/// and the start URL; cached `accessToken` is `secret-access-token`; the portal
/// returns the role credentials and asserts the bearer-token header.
#[tokio::test]
async fn sso_session_resolves_role_credentials() {
    let config = r#"
[profile default]
sso_session = dev
sso_account_id = 012345678901
sso_role_name = SampleRole
region = us-east-1

[sso-session dev]
sso_region = us-east-1
sso_start_url = https://d-abc123.awsapps.com/start
"#;
    let token_json = r#"{"accessToken":"secret-access-token","expiresAt":"2199-11-14T04:05:45Z"}"#;

    // Capture the headers/url the resolver sends so we can assert the bearer
    // token and the account_id/role_name/region in the URL.
    let seen: CapturedPortalRequest = Arc::new(Mutex::new(None));
    let seen_h = seen.clone();

    let io = MockIo::new()
        .env("HOME", "/home")
        .file("/home/.aws/config", config)
        .file_contains("/sso/cache/", token_json)
        .http(move |method, url, headers| {
            assert_eq!(method, "GET");
            *seen_h.lock().unwrap() = Some((url.to_string(), headers.to_vec()));
            Ok(HttpResponse {
                status: 200,
                body: role_credentials_body().to_string(),
            })
        });

    let creds = resolve_credentials(&io, None).await.expect("sso creds");
    assert_eq!(creds.access_key_id, "ASIARTESTID");
    assert_eq!(creds.secret_access_key, "TESTSECRETKEY");
    assert_eq!(creds.session_token.as_deref(), Some("TESTSESSIONTOKEN"));

    let (url, headers) = seen.lock().unwrap().clone().expect("portal was called");
    // Region us-east-1, account_id and role_name present in the query.
    assert!(
        url.starts_with("https://portal.sso.us-east-1.amazonaws.com/federation/credentials?"),
        "unexpected portal url: {url}"
    );
    assert!(url.contains("account_id=012345678901"), "url: {url}");
    assert!(url.contains("role_name=SampleRole"), "url: {url}");
    // The bearer token header carries the cached access token.
    let bearer = headers
        .iter()
        .find(|(k, _)| k == "x-amz-sso_bearer_token")
        .map(|(_, v)| v.as_str());
    assert_eq!(bearer, Some("secret-access-token"));
}

/// Shape (b): inline SSO settings on the profile (no `sso_session`); the cache
/// key is the start URL. Mirrors the doc-comment profile in
/// `profile/credentials.rs` (`sso_start_url`/`sso_region`/`sso_account_id`/
/// `sso_role_name` directly on the profile).
#[tokio::test]
async fn inline_sso_resolves_role_credentials() {
    let config = r#"
[profile default]
sso_start_url = https://example.com/start
sso_region = us-east-2
sso_account_id = 123456789011
sso_role_name = readOnly
"#;
    let token_json = r#"{"accessToken":"inline-token-xyz"}"#;

    let seen: CapturedPortalRequest = Arc::new(Mutex::new(None));
    let seen_h = seen.clone();

    let io = MockIo::new()
        .env("HOME", "/home")
        .file("/home/.aws/config", config)
        .file_contains("/sso/cache/", token_json)
        .http(move |_method, url, headers| {
            *seen_h.lock().unwrap() = Some((url.to_string(), headers.to_vec()));
            Ok(HttpResponse {
                status: 200,
                body: role_credentials_body().to_string(),
            })
        });

    let creds = resolve_credentials(&io, None).await.expect("sso creds");
    assert_eq!(creds.access_key_id, "ASIARTESTID");
    assert_eq!(creds.secret_access_key, "TESTSECRETKEY");
    assert_eq!(creds.session_token.as_deref(), Some("TESTSESSIONTOKEN"));

    let (url, headers) = seen.lock().unwrap().clone().expect("portal was called");
    assert!(
        url.starts_with("https://portal.sso.us-east-2.amazonaws.com/federation/credentials?"),
        "unexpected portal url: {url}"
    );
    assert!(url.contains("account_id=123456789011"), "url: {url}");
    assert!(url.contains("role_name=readOnly"), "url: {url}");
    let bearer = headers
        .iter()
        .find(|(k, _)| k == "x-amz-sso_bearer_token")
        .map(|(_, v)| v.as_str());
    assert_eq!(bearer, Some("inline-token-xyz"));
}

/// Query values must be URL-encoded. IAM Identity Center role names can contain
/// characters like `+`, `=`, `,`, and `@` that have meaning in URLs if left raw.
#[tokio::test]
async fn sso_role_and_account_query_values_are_percent_encoded() {
    let config = r#"
[profile default]
sso_start_url = https://example.com/start
sso_region = us-east-2
sso_account_id = 123456789011
sso_role_name = Team+Role=Read,Write@Test
"#;
    let token_json = r#"{"accessToken":"inline-token-xyz"}"#;

    let seen: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    let seen_h = seen.clone();

    let io = MockIo::new()
        .env("HOME", "/home")
        .file("/home/.aws/config", config)
        .file_contains("/sso/cache/", token_json)
        .http(move |_method, url, _headers| {
            *seen_h.lock().unwrap() = Some(url.to_string());
            Ok(HttpResponse {
                status: 200,
                body: role_credentials_body().to_string(),
            })
        });

    let _ = resolve_credentials(&io, None).await.expect("sso creds");
    let url = seen.lock().unwrap().clone().expect("portal was called");
    assert!(url.contains("account_id=123456789011"), "url: {url}");
    assert!(
        url.contains("role_name=Team%2BRole%3DRead%2CWrite%40Test"),
        "url: {url}"
    );
}

/// Missing token cache file -> `Err(Io)`. Upstream surfaces a provider error
/// when `load_cached_token` cannot find the cache; the slim fork returns
/// `ConfigError::Io` ("SSO token cache not found ..."). No `file_contains` for
/// the cache path here, so `read_file` returns `None`.
#[tokio::test]
async fn missing_token_cache_is_io_error() {
    let config = r#"
[profile default]
sso_start_url = https://example.com/start
sso_region = us-east-2
sso_account_id = 123456789011
sso_role_name = readOnly
"#;

    // http handler should never be reached (cache read fails first), but install
    // one that would fail the test if it were called.
    let io = MockIo::new()
        .env("HOME", "/home")
        .file("/home/.aws/config", config)
        .http(|_m, _u, _h| panic!("portal must not be called when the token cache is missing"));

    let err = resolve_credentials(&io, None)
        .await
        .expect_err("missing cache must error");
    assert!(
        matches!(err, ConfigError::Io(_)),
        "expected ConfigError::Io, got {err:?}"
    );
}

/// Non-200 portal response -> `Err(Io)`. Mirrors the failure path of the SSO
/// `GetRoleCredentials` call (the fork maps a non-200 status to
/// `ConfigError::Io`).
#[tokio::test]
async fn portal_non_200_is_io_error() {
    let config = r#"
[profile default]
sso_start_url = https://example.com/start
sso_region = us-east-2
sso_account_id = 123456789011
sso_role_name = readOnly
"#;
    let token_json = r#"{"accessToken":"inline-token-xyz"}"#;

    let io = MockIo::new()
        .env("HOME", "/home")
        .file("/home/.aws/config", config)
        .file_contains("/sso/cache/", token_json)
        .http(|_m, _u, _h| {
            Ok(HttpResponse {
                status: 403,
                body: "{\"message\":\"forbidden\"}".to_string(),
            })
        });

    let err = resolve_credentials(&io, None)
        .await
        .expect_err("non-200 portal response must error");
    assert!(
        matches!(err, ConfigError::Io(_)),
        "expected ConfigError::Io, got {err:?}"
    );
}
