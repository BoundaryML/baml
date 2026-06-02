#![allow(clippy::doc_markdown)]

//! JSON credential parsing matrix, ported from the upstream `aws-config`
//! `src/json_credentials.rs` `#[cfg(test)] mod test` module.
//!
//! Upstream parses the JSON payload directly via `parse_json_credentials`. Our
//! slim resolver has no such public entry point, so we exercise the same parser
//! (`credentials_from_json`) end-to-end through the ECS/container provider:
//! `AWS_CONTAINER_CREDENTIALS_FULL_URI` + an `http` handler that returns each
//! JSON variant. The container provider feeds the response body straight into
//! `credentials_from_json`, so the observable resolve result reflects the
//! parser's behavior.
//!
//! The slim `Credentials` type does not store `Expiration` or `AccountID`, but
//! the container/IMDS JSON parser still validates the upstream-required
//! refreshable shape before returning the core credential fields.

mod common;

use aws_config::{ConfigError, HttpResponse, resolve_credentials};
use common::MockIo;

/// Build a `MockIo` whose container endpoint returns `body` with HTTP 200.
fn container_returning(body: &'static str) -> MockIo {
    MockIo::new()
        .env(
            "AWS_CONTAINER_CREDENTIALS_FULL_URI",
            "http://localhost/creds",
        )
        .http(move |method, url, _headers| {
            assert_eq!(method, "GET");
            assert_eq!(url, "http://localhost/creds");
            Ok(HttpResponse {
                status: 200,
                body: body.to_string(),
            })
        })
}

/// Upstream `json_credentials_success_response` / `json_credentials_ecs`:
/// the canonical `PascalCase` ECS shape `{AccessKeyId, SecretAccessKey, Token}`.
#[tokio::test]
async fn pascal_case_with_token() {
    let body = r#"{
        "AccessKeyId" : "ASIARTEST",
        "SecretAccessKey" : "xjtest",
        "Token" : "IQote///test",
        "Expiration" : "2021-09-18T03:31:56Z"
    }"#;
    let creds = resolve_credentials(&container_returning(body), None)
        .await
        .expect("pascal-case credentials should resolve");
    assert_eq!(creds.access_key_id, "ASIARTEST");
    assert_eq!(creds.secret_access_key, "xjtest");
    assert_eq!(creds.session_token.as_deref(), Some("IQote///test"));
}

/// camelCase spelling `{accessKeyId, secretAccessKey, sessionToken}` — the
/// alternate casing our parser accepts (documented as matching the AWS SDK's
/// case handling). Not a distinct upstream test, but covers the camelCase arm
/// of the same `parse_json_credentials` logic.
#[tokio::test]
async fn camel_case_with_session_token() {
    let body = r#"{
        "accessKeyId" : "AKIDCAMEL",
        "secretAccessKey" : "SECRETCAMEL",
        "sessionToken" : "TOKCAMEL",
        "expiresAt" : "2021-09-18T03:31:56Z"
    }"#;
    let creds = resolve_credentials(&container_returning(body), None)
        .await
        .expect("camel-case credentials should resolve");
    assert_eq!(creds.access_key_id, "AKIDCAMEL");
    assert_eq!(creds.secret_access_key, "SECRETCAMEL");
    assert_eq!(creds.session_token.as_deref(), Some("TOKCAMEL"));
}

/// `SessionToken` (`PascalCase`) is accepted as an alias for `Token`. Exercises
/// the `pick(&["Token", "SessionToken", "sessionToken"])` alias chain.
#[tokio::test]
async fn pascal_case_session_token_alias() {
    let body = r#"{
        "AccessKeyId" : "AKIDALIAS",
        "SecretAccessKey" : "SECRETALIAS",
        "SessionToken" : "TOKALIAS",
        "Expiration" : "2021-09-18T03:31:56Z"
    }"#;
    let creds = resolve_credentials(&container_returning(body), None)
        .await
        .expect("SessionToken alias should resolve");
    assert_eq!(creds.access_key_id, "AKIDALIAS");
    assert_eq!(creds.secret_access_key, "SECRETALIAS");
    assert_eq!(creds.session_token.as_deref(), Some("TOKALIAS"));
}

/// Upstream `json_credentials_required_session_token`: the container/IMDS JSON
/// shape requires a `Token`/`SessionToken` field.
#[tokio::test]
async fn missing_token_is_parse_error() {
    let body = r#"{
        "AccessKeyId" : "ASIARTEST",
        "SecretAccessKey" : "xjtest",
        "Expiration" : "2021-09-18T03:31:56Z"
    }"#;
    let err = resolve_credentials(&container_returning(body), None)
        .await
        .expect_err("missing token must error");
    assert!(
        matches!(err, ConfigError::Parse(_)),
        "expected Parse, got {err:?}"
    );
}

/// Upstream requires `Expiration`/`ExpiresAt` for refreshable container/IMDS
/// credentials. The fork does not store the value, but still validates its
/// presence for parity.
#[tokio::test]
async fn missing_expiration_is_parse_error() {
    let body = r#"{
        "AccessKeyId" : "ASIARTEST",
        "SecretAccessKey" : "xjtest",
        "Token" : "IQote///test"
    }"#;
    let err = resolve_credentials(&container_returning(body), None)
        .await
        .expect_err("missing expiration must error");
    assert!(
        matches!(err, ConfigError::Parse(_)),
        "expected Parse, got {err:?}"
    );
}

/// Upstream `json_credentials_missing_akid`: a payload without `AccessKeyId`
/// is a parse error. Fork surfaces `ConfigError::Parse("missing AccessKeyId")`.
#[tokio::test]
async fn missing_access_key_id_is_parse_error() {
    let body = r#"{
        "SecretAccessKey" : "xjtest",
        "Token" : "IQote///test",
        "Expiration" : "2021-09-18T03:31:56Z"
    }"#;
    let err = resolve_credentials(&container_returning(body), None)
        .await
        .expect_err("missing AccessKeyId must error");
    assert!(
        matches!(err, ConfigError::Parse(_)),
        "expected Parse, got {err:?}"
    );
}

/// Missing `SecretAccessKey` -> parse error. Mirrors the missing-required-field
/// behavior of upstream, applied to the secret key.
#[tokio::test]
async fn missing_secret_access_key_is_parse_error() {
    let body = r#"{
        "AccessKeyId" : "ASIARTEST",
        "Token" : "IQote///test",
        "Expiration" : "2021-09-18T03:31:56Z"
    }"#;
    let err = resolve_credentials(&container_returning(body), None)
        .await
        .expect_err("missing SecretAccessKey must error");
    assert!(
        matches!(err, ConfigError::Parse(_)),
        "expected Parse, got {err:?}"
    );
}

/// Upstream `json_credentials_ecs`: extra/unknown fields (e.g. `RoleArn`,
/// `AccountID`, `Expiration`, `Code`, `LastUpdated`) are ignored; the three
/// credential fields still parse cleanly.
#[tokio::test]
async fn extra_fields_are_ignored() {
    let body = r#"{
        "Code" : "Success",
        "LastUpdated" : "2021-09-17T20:57:08Z",
        "Type" : "AWS-HMAC",
        "RoleArn" : "arn:aws:iam::123456789:role/ecs-task-role",
        "AccessKeyId" : "ASIARTEST",
        "SecretAccessKey" : "SECRETTEST",
        "Token" : "tokenTEST",
        "AccountID" : "111122223333",
        "Expiration" : "2009-02-13T23:31:30Z"
    }"#;
    let creds = resolve_credentials(&container_returning(body), None)
        .await
        .expect("extra fields should be ignored");
    assert_eq!(creds.access_key_id, "ASIARTEST");
    assert_eq!(creds.secret_access_key, "SECRETTEST");
    assert_eq!(creds.session_token.as_deref(), Some("tokenTEST"));
}

/// Upstream parses keys case-insensitively, not just Pascal/camel case.
#[tokio::test]
async fn all_credential_keys_are_case_insensitive() {
    let body = r#"{
        "code" : "Success",
        "accesskeyid" : "AKIDLOWER",
        "secretaccesskey" : "SECRETLOWER",
        "sessiontoken" : "TOKLOWER",
        "expiration" : "2021-09-18T03:31:56Z"
    }"#;
    let creds = resolve_credentials(&container_returning(body), None)
        .await
        .expect("case-insensitive fields should resolve");
    assert_eq!(creds.access_key_id, "AKIDLOWER");
    assert_eq!(creds.secret_access_key, "SECRETLOWER");
    assert_eq!(creds.session_token.as_deref(), Some("TOKLOWER"));
}

/// Upstream models non-`Success` `Code` responses as credential endpoint
/// errors. The fork maps that to `ConfigError::Io`.
#[tokio::test]
async fn code_error_response_is_io_error() {
    let body = r#"{
        "Code" : "ErrorCode",
        "Message" : "Helpful error message."
    }"#;
    let err = resolve_credentials(&container_returning(body), None)
        .await
        .expect_err("error response must not resolve");
    assert!(
        matches!(err, ConfigError::Io(_)),
        "expected Io, got {err:?}"
    );
}

/// Upstream `json_credentials_invalid_json`: a non-JSON body is a parse error.
#[tokio::test]
async fn invalid_json_is_parse_error() {
    let err = resolve_credentials(&container_returning("404: not found"), None)
        .await
        .expect_err("invalid JSON must error");
    assert!(
        matches!(err, ConfigError::Parse(_)),
        "expected Parse, got {err:?}"
    );
}

/// Upstream `json_credentials_not_json_object`: a JSON array (not an object)
/// has no credential fields, so it surfaces as a `Parse` error (missing
/// `AccessKeyId`) rather than valid credentials.
#[tokio::test]
async fn json_array_is_parse_error() {
    let err = resolve_credentials(&container_returning("[1,2,3]"), None)
        .await
        .expect_err("a JSON array has no credential fields");
    assert!(
        matches!(err, ConfigError::Parse(_)),
        "expected Parse, got {err:?}"
    );
}
