#![allow(clippy::doc_markdown)]

//! Behavioral parity tests for the ECS / container credential provider.
//!
//! Ported from the upstream `aws-config` crate's `src/ecs.rs` test module
//! (`aws-config-1.8.5`), mirroring the cases that apply to our slim resolver.
//! Upstream tests that exercise machinery we did not port — DNS validation of
//! the full URI (`validate_uri_https`, `valid_uri_loopback`, `valid_uri_ecs_eks`,
//! `all_addrs_local`, `all_addrs_not_local`, `real_dns_lookup`), 5xx retry
//! (`retry_5xx`), and the JSON-file-driven `run_config_tests` — are skipped.
//!
mod common;

use aws_config::*;
use common::MockIo;

/// Shared "valid credentials" body, modeled on upstream `ok_creds_response()`
/// (PascalCase fields + `Token`). Our slim parser ignores `AccountId` /
/// `Expiration`.
const OK_CREDS_BODY: &str = r#"{
    "AccessKeyId" : "AKID",
    "SecretAccessKey" : "SECRET",
    "Token" : "TOKEN....=",
    "AccountId" : "AID",
    "Expiration" : "2009-02-13T23:31:30Z"
}"#;

fn assert_ok_creds(creds: &Credentials) {
    assert_eq!(creds.access_key_id, "AKID");
    assert_eq!(creds.secret_access_key, "SECRET");
    assert_eq!(creds.session_token.as_deref(), Some("TOKEN....="));
}

/// Mirrors upstream `load_valid_creds_auth`: a relative URI is resolved against
/// the container base host `http://169.254.170.2`, and the inline
/// `AWS_CONTAINER_AUTHORIZATION_TOKEN` is sent as the `Authorization` header.
#[tokio::test]
async fn relative_uri_with_inline_auth_token() {
    let mock = MockIo::new()
        .env("AWS_CONTAINER_CREDENTIALS_RELATIVE_URI", "/credentials")
        .env("AWS_CONTAINER_AUTHORIZATION_TOKEN", "Basic password")
        .http(|method, url, headers| {
            assert_eq!(method, "GET");
            assert_eq!(url, "http://169.254.170.2/credentials");
            let auth = headers
                .iter()
                .find(|(k, _)| k == "Authorization")
                .map(|(_, v)| v.as_str());
            assert_eq!(auth, Some("Basic password"));
            Ok(HttpResponse {
                status: 200,
                body: OK_CREDS_BODY.to_string(),
            })
        });

    let creds = resolve_credentials(&mock, None)
        .await
        .expect("valid credentials");
    assert_ok_creds(&creds);
    assert_eq!(mock.http_calls(), 1);
    assert_eq!(
        mock.last_http_url().as_deref(),
        Some("http://169.254.170.2/credentials")
    );
}

/// Mirrors upstream `load_valid_creds_auth_file`: a full URI is used verbatim,
/// and the `Authorization` token is read from the file named by
/// `AWS_CONTAINER_AUTHORIZATION_TOKEN_FILE` (trimmed of surrounding whitespace).
#[tokio::test]
async fn full_uri_with_auth_token_from_file() {
    let token_path =
        "/var/run/secrets/pods.eks.amazonaws.com/serviceaccount/eks-pod-identity-token";
    let mock = MockIo::new()
        .env(
            "AWS_CONTAINER_CREDENTIALS_FULL_URI",
            "http://169.254.170.23/v1/credentials",
        )
        .env("AWS_CONTAINER_AUTHORIZATION_TOKEN_FILE", token_path)
        // Surrounding newline/space exercises the `.trim()` in `from_container`.
        .file(token_path, "  Basic password\n")
        .http(|method, url, headers| {
            assert_eq!(method, "GET");
            assert_eq!(url, "http://169.254.170.23/v1/credentials");
            let auth = headers
                .iter()
                .find(|(k, _)| k == "Authorization")
                .map(|(_, v)| v.as_str());
            assert_eq!(auth, Some("Basic password"));
            Ok(HttpResponse {
                status: 200,
                body: OK_CREDS_BODY.to_string(),
            })
        });

    let creds = resolve_credentials(&mock, None)
        .await
        .expect("valid credentials");
    assert_ok_creds(&creds);
    assert_eq!(
        mock.last_http_url().as_deref(),
        Some("http://169.254.170.23/v1/credentials")
    );
}

/// Mirrors upstream `query_params_should_be_included_in_credentials_http_request`:
/// query parameters on a relative URI survive into the request URL.
#[tokio::test]
async fn relative_uri_preserves_query_params() {
    let mock = MockIo::new()
        .env(
            "AWS_CONTAINER_CREDENTIALS_RELATIVE_URI",
            "/my-credentials/?applicationName=test2024",
        )
        .http(|_method, url, _headers| {
            assert_eq!(
                url,
                "http://169.254.170.2/my-credentials/?applicationName=test2024"
            );
            Ok(HttpResponse {
                status: 200,
                body: OK_CREDS_BODY.to_string(),
            })
        });

    let creds = resolve_credentials(&mock, None)
        .await
        .expect("valid credentials");
    assert_ok_creds(&creds);
    assert_eq!(
        mock.last_http_url().as_deref(),
        Some("http://169.254.170.2/my-credentials/?applicationName=test2024")
    );
}

/// Mirrors upstream `load_valid_creds_no_auth`: a relative URI with no
/// authorization token set sends no `Authorization` header.
#[tokio::test]
async fn relative_uri_no_auth_token() {
    let mock = MockIo::new()
        .env("AWS_CONTAINER_CREDENTIALS_RELATIVE_URI", "/credentials")
        .http(|method, url, headers| {
            assert_eq!(method, "GET");
            assert_eq!(url, "http://169.254.170.2/credentials");
            assert!(
                !headers.iter().any(|(k, _)| k == "Authorization"),
                "no Authorization header expected, got: {headers:?}"
            );
            Ok(HttpResponse {
                status: 200,
                body: OK_CREDS_BODY.to_string(),
            })
        });

    let creds = resolve_credentials(&mock, None)
        .await
        .expect("valid credentials");
    assert_ok_creds(&creds);
}

/// Mirrors upstream `auth_file_precedence_over_env`: the token file wins over
/// the inline `AWS_CONTAINER_AUTHORIZATION_TOKEN`.
#[tokio::test]
async fn auth_token_file_takes_precedence_over_inline_env() {
    let token_path =
        "/var/run/secrets/pods.eks.amazonaws.com/serviceaccount/eks-pod-identity-token";
    let mock = MockIo::new()
        .env(
            "AWS_CONTAINER_CREDENTIALS_FULL_URI",
            "http://169.254.170.23/v1/credentials",
        )
        .env("AWS_CONTAINER_AUTHORIZATION_TOKEN", "inline-token")
        .env("AWS_CONTAINER_AUTHORIZATION_TOKEN_FILE", token_path)
        .file(token_path, "file-token")
        .http(|_method, _url, headers| {
            let auth = headers
                .iter()
                .find(|(k, _)| k == "Authorization")
                .map(|(_, v)| v.as_str());
            assert_eq!(auth, Some("file-token"));
            Ok(HttpResponse {
                status: 200,
                body: OK_CREDS_BODY.to_string(),
            })
        });

    let creds = resolve_credentials(&mock, None)
        .await
        .expect("valid credentials");
    assert_ok_creds(&creds);
}

/// Mirrors upstream `fs_missing_file`: a configured token file that cannot be
/// read is a provider error, and no credential endpoint request is made.
#[tokio::test]
async fn missing_auth_token_file_is_io_error() {
    let mock = MockIo::new()
        .env(
            "AWS_CONTAINER_CREDENTIALS_FULL_URI",
            "http://169.254.170.23/v1/credentials",
        )
        .env(
            "AWS_CONTAINER_AUTHORIZATION_TOKEN_FILE",
            "/does/not/exist/token",
        )
        // No `.file(...)` registered: the read returns None.
        .http(|_method, _url, headers| {
            panic!("credential endpoint must not be called; headers: {headers:?}")
        });

    let err = resolve_credentials(&mock, None)
        .await
        .expect_err("missing auth token file must error");
    assert!(
        matches!(err, ConfigError::Io(_)),
        "expected ConfigError::Io, got: {err:?}"
    );
    assert_eq!(mock.http_calls(), 0);
}

/// A non-200 response from the container endpoint surfaces as
/// `ConfigError::Io`. (Upstream models 5xx as retryable; our slim resolver does
/// not retry, so any non-200 is a terminal `Io` error.)
#[tokio::test]
async fn non_200_response_yields_io_error() {
    let mock = MockIo::new()
        .env("AWS_CONTAINER_CREDENTIALS_RELATIVE_URI", "/credentials")
        .http(|_method, _url, _headers| {
            Ok(HttpResponse {
                status: 500,
                body: String::new(),
            })
        });

    let err = resolve_credentials(&mock, None)
        .await
        .expect_err("non-200 must error");
    assert!(
        matches!(err, ConfigError::Io(_)),
        "expected ConfigError::Io, got: {err:?}"
    );
}

/// A 200 response whose JSON is missing required fields surfaces as
/// `ConfigError::Parse`. Mirrors upstream `json_credentials_missing_akid`
/// applied through the container provider.
#[tokio::test]
async fn malformed_creds_json_yields_parse_error() {
    let mock = MockIo::new()
        .env("AWS_CONTAINER_CREDENTIALS_RELATIVE_URI", "/credentials")
        .http(|_method, _url, _headers| {
            Ok(HttpResponse {
                status: 200,
                // Missing AccessKeyId.
                body: r#"{"SecretAccessKey":"SECRET","Token":"TOK"}"#.to_string(),
            })
        });

    let err = resolve_credentials(&mock, None)
        .await
        .expect_err("missing AccessKeyId must error");
    assert!(
        matches!(err, ConfigError::Parse(_)),
        "expected ConfigError::Parse, got: {err:?}"
    );
}

/// With neither container env var set, the container provider yields nothing.
/// With no other provider configured the chain ends in
/// `ConfigError::NoCredentials`, and crucially the http handler is never called.
#[tokio::test]
async fn no_container_env_yields_no_credentials() {
    // Disable IMDS (step 4 of the chain) so the only provider that could make an
    // HTTP call is the container provider; with no container env it must not.
    let mock =
        MockIo::new()
            .env("AWS_EC2_METADATA_DISABLED", "true")
            .http(|_method, _url, _headers| {
                panic!("container provider must not make an HTTP call when no env is set");
            });

    let err = resolve_credentials(&mock, None)
        .await
        .expect_err("no providers configured");
    assert!(
        matches!(err, ConfigError::NoCredentials(_)),
        "expected ConfigError::NoCredentials, got: {err:?}"
    );
    assert_eq!(mock.http_calls(), 0);
}
