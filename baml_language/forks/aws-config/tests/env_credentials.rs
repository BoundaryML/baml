#![allow(clippy::doc_markdown)]

//! Environment-variable credential resolution, ported from upstream
//! `aws-config` `sdk/aws-config/src/environment/credentials.rs` `mod test`.
//!
//! Upstream tests drive `EnvironmentVariableCredentialsProvider` directly; the
//! fork has no such standalone provider, so we exercise the same behavior
//! through the public `resolve_credentials` entry point with an env-only
//! `MockIo`. The env provider sits first in the chain, so when the env vars are
//! present these tests never touch the (unconfigured) profile/container/IMDS
//! stages.
//!
mod common;

use aws_config::*;
use common::MockIo;

/// Mirrors upstream `valid_no_token`: AWS_ACCESS_KEY_ID + AWS_SECRET_ACCESS_KEY
/// with no session token -> Ok, token is None.
#[tokio::test]
async fn full_creds_no_token() {
    let io = MockIo::new()
        .env("AWS_ACCESS_KEY_ID", "access")
        .env("AWS_SECRET_ACCESS_KEY", "secret");

    let creds = resolve_credentials(&io, None)
        .await
        .expect("valid credentials");

    assert_eq!(creds.access_key_id, "access");
    assert_eq!(creds.secret_access_key, "secret");
    assert_eq!(creds.session_token, None);
}

/// Mirrors upstream `valid_with_token`: adding AWS_SESSION_TOKEN sets the token.
#[tokio::test]
async fn full_creds_with_token() {
    let io = MockIo::new()
        .env("AWS_ACCESS_KEY_ID", "access")
        .env("AWS_SECRET_ACCESS_KEY", "secret")
        .env("AWS_SESSION_TOKEN", "token");

    let creds = resolve_credentials(&io, None)
        .await
        .expect("valid credentials");

    assert_eq!(creds.access_key_id, "access");
    assert_eq!(creds.secret_access_key, "secret");
    assert_eq!(creds.session_token.as_deref(), Some("token"));
}

/// Mirrors upstream `secret_key_fallback`: the legacy `SECRET_ACCESS_KEY` is
/// used when `AWS_SECRET_ACCESS_KEY` is absent.
#[tokio::test]
async fn legacy_secret_key_fallback() {
    let io = MockIo::new()
        .env("AWS_ACCESS_KEY_ID", "access")
        .env("SECRET_ACCESS_KEY", "secret")
        .env("AWS_SESSION_TOKEN", "token");

    let creds = resolve_credentials(&io, None)
        .await
        .expect("valid credentials");

    assert_eq!(creds.access_key_id, "access");
    assert_eq!(creds.secret_access_key, "secret");
    assert_eq!(creds.session_token.as_deref(), Some("token"));
}

/// Mirrors upstream `secret_key_fallback_empty`: when
/// `AWS_SECRET_ACCESS_KEY` is blank it is treated as unset and the legacy
/// `SECRET_ACCESS_KEY` fallback supplies the secret.
#[tokio::test]
async fn blank_secret_key_falls_back_to_legacy() {
    let io = MockIo::new()
        .env("AWS_ACCESS_KEY_ID", "access")
        .env("AWS_SECRET_ACCESS_KEY", " ")
        .env("SECRET_ACCESS_KEY", "secret")
        .env("AWS_SESSION_TOKEN", "token");

    let creds = resolve_credentials(&io, None)
        .await
        .expect("valid credentials");

    assert_eq!(creds.access_key_id, "access");
    assert_eq!(creds.secret_access_key, "secret");
    assert_eq!(creds.session_token.as_deref(), Some("token"));
}

/// Mirrors upstream `empty_token_env_var`: a blank `AWS_SESSION_TOKEN` is
/// treated as unset, so the resolved token is None.
#[tokio::test]
async fn blank_session_token_is_unset() {
    let io = MockIo::new()
        .env("AWS_ACCESS_KEY_ID", "access")
        .env("AWS_SECRET_ACCESS_KEY", "secret")
        .env("AWS_SESSION_TOKEN", " ");

    let creds = resolve_credentials(&io, None)
        .await
        .expect("valid credentials");

    assert_eq!(creds.access_key_id, "access");
    assert_eq!(creds.secret_access_key, "secret");
    assert_eq!(creds.session_token, None);
}

/// Mirrors upstream `empty_keys_env_vars`: a blank access key id is treated as
/// unset, so the env provider is skipped.
///
/// In the fork the chain then continues to profile/container/IMDS. With nothing
/// else configured and IMDS disabled, resolution falls through to the terminal
/// `Err(ConfigError::NoCredentials)` (the analog of upstream's
/// `CredentialsNotLoaded`). IMDS is disabled here so the default (erroring)
/// MockIo http handler is never reached.
#[tokio::test]
async fn blank_access_key_is_unset_then_chain_exhausts() {
    let io = MockIo::new()
        .env("AWS_ACCESS_KEY_ID", " ")
        .env("AWS_SECRET_ACCESS_KEY", "secret")
        // Disable IMDS so the chain falls through to NoCredentials instead of
        // hitting the default erroring http handler.
        .env("AWS_EC2_METADATA_DISABLED", "true");

    let err = resolve_credentials(&io, None)
        .await
        .expect_err("no credentials defined");

    assert!(matches!(err, ConfigError::NoCredentials(_)));
}

/// Mirrors upstream `missing`: with `AWS_ACCESS_KEY_ID` present but the secret
/// absent, the env provider is skipped and (nothing else configured, IMDS
/// disabled) resolution fails with `NoCredentials`.
#[tokio::test]
async fn missing_secret_exhausts_chain() {
    let io = MockIo::new()
        .env("AWS_ACCESS_KEY_ID", "access")
        // No AWS_SECRET_ACCESS_KEY / SECRET_ACCESS_KEY.
        .env("AWS_EC2_METADATA_DISABLED", "true");

    let err = resolve_credentials(&io, None)
        .await
        .expect_err("no credentials defined");

    assert!(matches!(err, ConfigError::NoCredentials(_)));
}
