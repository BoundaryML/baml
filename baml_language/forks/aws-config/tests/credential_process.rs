#![allow(clippy::doc_markdown)]

//! Integration coverage for the `credential_process` profile provider, ported
//! from upstream `aws-config`'s `src/credential_process.rs` test module
//! (`test_credential_process`, `test_credential_process_no_expiry`) and
//! adjacent parser error cases.
//!
//! The fork's slim resolver reaches `credential_process` via the shared-profile
//! provider, so each test wires a `[profile cp]` config section with
//! `credential_process = some-cmd` and drives it with `profile_override = "cp"`.
//! The mock `command` handler stands in for the external process, returning the
//! JSON document on stdout. All IO is mocked; nothing touches the real
//! environment, filesystem, or network.

mod common;

use aws_config::*;
use common::MockIo;

/// Build a `MockIo` whose active profile (`cp`) runs `credential_process`, with
/// the given command handler producing the process output.
fn mock_with_process<F>(handler: F) -> MockIo
where
    F: Fn(&str) -> Result<CommandOutput, ConfigError> + Send + Sync + 'static,
{
    MockIo::new()
        .env("HOME", "/home/test")
        .env("AWS_CONFIG_FILE", "/cfg/config")
        .file(
            "/cfg/config",
            "[profile cp]\ncredential_process = get-creds\n",
        )
        .command(handler)
}

/// Mirrors upstream `test_credential_process`: a `Version == 1` document with
/// `AccessKeyId` / `SecretAccessKey` / `SessionToken` yields those credentials.
/// (Expiration/AccountId are not modeled by the fork's `Credentials`, so only
/// the three core fields are asserted.)
#[tokio::test]
async fn version_1_with_session_token_yields_credentials() {
    let mock = mock_with_process(|cmd| {
        assert_eq!(cmd, "get-creds");
        Ok(CommandOutput {
            status: 0,
            stdout: r#"{"Version":1,"AccessKeyId":"ASIARTESTID","SecretAccessKey":"TESTSECRETKEY","SessionToken":"TESTSESSIONTOKEN"}"#
                .to_string(),
        })
    });

    let creds = resolve_credentials(&mock, Some("cp"))
        .await
        .expect("valid creds");
    assert_eq!(creds.access_key_id, "ASIARTESTID");
    assert_eq!(creds.secret_access_key, "TESTSECRETKEY");
    assert_eq!(creds.session_token.as_deref(), Some("TESTSESSIONTOKEN"));
}

/// Mirrors upstream `test_credential_process_no_expiry`: a `Version == 1`
/// document with only `AccessKeyId` / `SecretAccessKey` resolves, and the
/// session token is absent.
#[tokio::test]
async fn version_1_without_session_token_yields_credentials() {
    let mock = mock_with_process(|_cmd| {
        Ok(CommandOutput {
            status: 0,
            stdout:
                r#"{"Version":1,"AccessKeyId":"ASIARTESTID","SecretAccessKey":"TESTSECRETKEY"}"#
                    .to_string(),
        })
    });

    let creds = resolve_credentials(&mock, Some("cp"))
        .await
        .expect("valid creds");
    assert_eq!(creds.access_key_id, "ASIARTESTID");
    assert_eq!(creds.secret_access_key, "TESTSECRETKEY");
    assert_eq!(creds.session_token, None);
}

/// `credential_process` accepts `SessionToken`, not the container/IMDS `Token`
/// spelling. This matches upstream's dedicated
/// `parse_credential_process_json_credentials` parser.
#[tokio::test]
async fn token_field_is_not_a_credential_process_session_token() {
    let mock = mock_with_process(|_cmd| {
        Ok(CommandOutput {
            status: 0,
            stdout: r#"{"Version":1,"AccessKeyId":"ASIARTEST","SecretAccessKey":"xjtest","Token":"IQote///test"}"#
                .to_string(),
        })
    });

    let creds = resolve_credentials(&mock, Some("cp"))
        .await
        .expect("valid creds");
    assert_eq!(creds.access_key_id, "ASIARTEST");
    assert_eq!(creds.secret_access_key, "xjtest");
    assert_eq!(creds.session_token, None);
}

/// Upstream's `credential_process` parser matches field names
/// case-insensitively.
#[tokio::test]
async fn field_names_are_case_insensitive() {
    let mock = mock_with_process(|_cmd| {
        Ok(CommandOutput {
            status: 0,
            stdout: r#"{"version":1,"accesskeyid":"AKIDCI","secretaccesskey":"SECRETCI","sessiontoken":"TOKCI"}"#
                .to_string(),
        })
    });

    let creds = resolve_credentials(&mock, Some("cp"))
        .await
        .expect("valid creds");
    assert_eq!(creds.access_key_id, "AKIDCI");
    assert_eq!(creds.secret_access_key, "SECRETCI");
    assert_eq!(creds.session_token.as_deref(), Some("TOKCI"));
}

/// Mirrors upstream's missing-`Version` rejection (`InvalidJsonCredentials`
/// branch `version => None`): without a `Version` field the fork returns a
/// `ConfigError::Parse`.
#[tokio::test]
async fn missing_version_is_parse_error() {
    let mock = mock_with_process(|_cmd| {
        Ok(CommandOutput {
            status: 0,
            stdout: r#"{"AccessKeyId":"AKID","SecretAccessKey":"SECRET"}"#.to_string(),
        })
    });

    let err = resolve_credentials(&mock, Some("cp"))
        .await
        .expect_err("missing Version must error");
    assert!(
        matches!(err, ConfigError::Parse(_)),
        "expected Parse, got {err:?}"
    );
}

/// Mirrors upstream's unknown-version rejection (`Some(version)` branch of
/// `parse_credential_process_json_credentials`): a `Version` other than 1
/// yields a `ConfigError::Parse`.
#[tokio::test]
async fn wrong_version_is_parse_error() {
    let mock = mock_with_process(|_cmd| {
        Ok(CommandOutput {
            status: 0,
            stdout: r#"{"Version":2,"AccessKeyId":"AKID","SecretAccessKey":"SECRET"}"#.to_string(),
        })
    });

    let err = resolve_credentials(&mock, Some("cp"))
        .await
        .expect_err("Version != 1 must error");
    assert!(
        matches!(err, ConfigError::Parse(_)),
        "expected Parse, got {err:?}"
    );
}

/// Mirrors upstream's non-zero exit handling (`if !output.status.success()`
/// branch returning a provider error): a non-zero command exit status maps to
/// `ConfigError::Io`.
#[tokio::test]
async fn non_zero_exit_status_is_io_error() {
    let mock = mock_with_process(|_cmd| {
        Ok(CommandOutput {
            status: 1,
            stdout: String::new(),
        })
    });

    let err = resolve_credentials(&mock, Some("cp"))
        .await
        .expect_err("non-zero exit must error");
    assert!(
        matches!(err, ConfigError::Io(_)),
        "expected Io, got {err:?}"
    );
}

/// Confirms the parser tolerates surrounding whitespace/newlines in the process
/// output (upstream parses via a streaming JSON tokenizer that ignores leading
/// whitespace; the fork trims before `serde_json::from_str`). Echo-style
/// commands commonly emit a trailing newline.
#[tokio::test]
async fn whitespace_around_json_is_tolerated() {
    let mock = mock_with_process(|_cmd| {
        Ok(CommandOutput {
            status: 0,
            stdout: "\n\t  {\"Version\":1,\"AccessKeyId\":\"AKIDWS\",\"SecretAccessKey\":\"SECRETWS\",\"SessionToken\":\"TOKWS\"}  \n"
                .to_string(),
        })
    });

    let creds = resolve_credentials(&mock, Some("cp"))
        .await
        .expect("valid creds despite surrounding whitespace");
    assert_eq!(creds.access_key_id, "AKIDWS");
    assert_eq!(creds.secret_access_key, "SECRETWS");
    assert_eq!(creds.session_token.as_deref(), Some("TOKWS"));
}
