#![allow(clippy::doc_markdown)]

//! Static-credential profile resolution, ported from the upstream `aws-config`
//! profile credential tests (`aws-config/src/profile/credentials.rs`,
//! `aws-config/src/profile.rs`, `aws-config/src/profile/profile_file.rs`).
//!
//! Only the behavioral cases that apply to our slim resolver are mirrored;
//! upstream cases that depend on Smithy machinery or providers we did not port
//! (assume-role, web-identity, etc.) are skipped. All IO goes through the
//! shared `MockIo` — fully offline and deterministic.

mod common;

use aws_config::*;
use common::MockIo;

/// Mirrors upstream "`profile_static_creds`" / the default `[default]` section in
/// the shared *credentials* file producing static keys with no config file.
#[tokio::test]
async fn default_profile_from_credentials_file() {
    let mock = MockIo::new()
        .env("AWS_SHARED_CREDENTIALS_FILE", "/cfg/credentials")
        .file(
            "/cfg/credentials",
            "[default]\n\
             aws_access_key_id = AKID_DEFAULT\n\
             aws_secret_access_key = SECRET_DEFAULT\n",
        );

    let creds = resolve_credentials(&mock, None)
        .await
        .expect("default profile static creds should resolve");
    assert_eq!(creds.access_key_id, "AKID_DEFAULT");
    assert_eq!(creds.secret_access_key, "SECRET_DEFAULT");
    assert_eq!(creds.session_token, None);
}

/// Mirrors upstream "`retrieve_from_profile`" with an explicit profile name: the
/// resolver selects a named (non-default) profile passed via the explicit
/// `profile` option (our `profile_override`).
#[tokio::test]
async fn named_profile_via_override() {
    let mock = MockIo::new()
        .env("AWS_SHARED_CREDENTIALS_FILE", "/cfg/credentials")
        .file(
            "/cfg/credentials",
            "[default]\n\
             aws_access_key_id = AKID_DEFAULT\n\
             aws_secret_access_key = SECRET_DEFAULT\n\
             \n\
             [work]\n\
             aws_access_key_id = AKID_WORK\n\
             aws_secret_access_key = SECRET_WORK\n",
        );

    let creds = resolve_credentials(&mock, Some("work"))
        .await
        .expect("named profile via override should resolve");
    assert_eq!(creds.access_key_id, "AKID_WORK");
    assert_eq!(creds.secret_access_key, "SECRET_WORK");
}

/// Mirrors upstream profile-selection via the `AWS_PROFILE` environment
/// variable (`profile.rs` env-based active-profile selection).
#[tokio::test]
async fn named_profile_via_aws_profile_env() {
    let mock = MockIo::new()
        .env("AWS_PROFILE", "work")
        .env("AWS_SHARED_CREDENTIALS_FILE", "/cfg/credentials")
        .file(
            "/cfg/credentials",
            "[default]\n\
             aws_access_key_id = AKID_DEFAULT\n\
             aws_secret_access_key = SECRET_DEFAULT\n\
             \n\
             [work]\n\
             aws_access_key_id = AKID_WORK\n\
             aws_secret_access_key = SECRET_WORK\n",
        );

    let creds = resolve_credentials(&mock, None)
        .await
        .expect("named profile via AWS_PROFILE should resolve");
    assert_eq!(creds.access_key_id, "AKID_WORK");
    assert_eq!(creds.secret_access_key, "SECRET_WORK");
}

/// Mirrors upstream `profile_file.rs`: in the *config* file a non-default
/// profile must be written as `[profile NAME]` (not bare `[NAME]`) to be
/// recognized.
#[tokio::test]
async fn config_file_named_profile_uses_profile_prefix() {
    let mock = MockIo::new()
        .env("AWS_PROFILE", "work")
        .env("AWS_CONFIG_FILE", "/cfg/config")
        .file(
            "/cfg/config",
            "[default]\n\
             aws_access_key_id = AKID_DEFAULT\n\
             aws_secret_access_key = SECRET_DEFAULT\n\
             \n\
             [profile work]\n\
             aws_access_key_id = AKID_WORK\n\
             aws_secret_access_key = SECRET_WORK\n",
        );

    let creds = resolve_credentials(&mock, Some("work"))
        .await
        .expect("[profile work] in config file should resolve");
    assert_eq!(creds.access_key_id, "AKID_WORK");
    assert_eq!(creds.secret_access_key, "SECRET_WORK");
}

/// Mirrors upstream "merge across config and credentials files": when both
/// files define the same profile, the *credentials* file wins on a per-key
/// basis while keys only present in the config file are still merged in.
#[tokio::test]
async fn credentials_file_overrides_config_per_key() {
    let mock = MockIo::new()
        .env("AWS_CONFIG_FILE", "/cfg/config")
        .env("AWS_SHARED_CREDENTIALS_FILE", "/cfg/credentials")
        // Config file: provides a key and a region.
        .file(
            "/cfg/config",
            "[default]\n\
             aws_access_key_id = AKID_FROM_CONFIG\n\
             aws_secret_access_key = SECRET_FROM_CONFIG\n\
             region = us-east-1\n",
        )
        // Credentials file: overrides the access key id only.
        .file(
            "/cfg/credentials",
            "[default]\n\
             aws_access_key_id = AKID_FROM_CREDS\n",
        );

    let creds = resolve_credentials(&mock, None)
        .await
        .expect("merged static creds should resolve");
    // access_key_id overridden by the credentials file...
    assert_eq!(creds.access_key_id, "AKID_FROM_CREDS");
    // ...secret only present in the config file, so it is merged in.
    assert_eq!(creds.secret_access_key, "SECRET_FROM_CONFIG");

    // region key (config-only) still resolves.
    let region = resolve_region(&mock, None).await;
    assert_eq!(region.as_deref(), Some("us-east-1"));
}

/// Mirrors upstream "`profile with session token`": `aws_session_token` is
/// carried through alongside the static access/secret keys.
#[tokio::test]
async fn session_token_carried_through() {
    let mock = MockIo::new()
        .env("AWS_SHARED_CREDENTIALS_FILE", "/cfg/credentials")
        .file(
            "/cfg/credentials",
            "[default]\n\
             aws_access_key_id = AKID_ST\n\
             aws_secret_access_key = SECRET_ST\n\
             aws_session_token = SESSION_TOKEN_ST\n",
        );

    let creds = resolve_credentials(&mock, None)
        .await
        .expect("static creds with session token should resolve");
    assert_eq!(creds.access_key_id, "AKID_ST");
    assert_eq!(creds.secret_access_key, "SECRET_ST");
    assert_eq!(creds.session_token.as_deref(), Some("SESSION_TOKEN_ST"));
}

/// Mirrors upstream `profile_file.rs` "bare section in config is not a
/// profile": a bare `[work]` in the *config* file is not a valid profile (only
/// `[profile NAME]` / `[default]` are), so resolution finds no credentials.
#[tokio::test]
async fn bare_section_in_config_file_is_not_a_profile() {
    let mock = MockIo::new()
        .env("AWS_PROFILE", "work")
        .env("AWS_CONFIG_FILE", "/cfg/config")
        // Disable the downstream IMDS provider so that, with the profile
        // correctly skipped, the chain ends in NoCredentials rather than an IO
        // error from an unreachable metadata endpoint. No container URI is set,
        // so that provider yields nothing.
        .env("AWS_EC2_METADATA_DISABLED", "true")
        .file(
            "/cfg/config",
            // Bare [work] — invalid in the config file.
            "[work]\n\
             aws_access_key_id = AKID_WORK\n\
             aws_secret_access_key = SECRET_WORK\n",
        );

    let err = resolve_credentials(&mock, Some("work"))
        .await
        .expect_err("bare [work] in config file must not be a valid profile");
    assert!(
        matches!(err, ConfigError::NoCredentials(_)),
        "expected NoCredentials, got {err:?}"
    );
}

/// Companion to the case above: the same bare `[NAME]` *is* a valid profile in
/// the credentials file (where bare section names are the profiles), so it
/// resolves. Mirrors the credentials-file half of upstream profile parsing.
#[tokio::test]
async fn bare_section_in_credentials_file_is_a_profile() {
    let mock = MockIo::new()
        .env("AWS_PROFILE", "work")
        .env("AWS_SHARED_CREDENTIALS_FILE", "/cfg/credentials")
        .file(
            "/cfg/credentials",
            "[work]\n\
             aws_access_key_id = AKID_WORK\n\
             aws_secret_access_key = SECRET_WORK\n",
        );

    let creds = resolve_credentials(&mock, Some("work"))
        .await
        .expect("bare [work] in credentials file should resolve");
    assert_eq!(creds.access_key_id, "AKID_WORK");
    assert_eq!(creds.secret_access_key, "SECRET_WORK");
}
