#![allow(clippy::doc_markdown)]

//! Smoke test proving the shared `MockIo` mock compiles and works by driving
//! `resolve_credentials` / `resolve_region` through it. Exercises every builder
//! method: env, file, file_contains, http, command.

mod common;

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use aws_config::{CommandOutput, CredentialIo, HttpResponse, resolve_credentials, resolve_region};
use common::MockIo;

#[tokio::test]
async fn mock_drives_env_credentials_and_region() {
    // env(): static credentials from the environment provider, plus region.
    let mock = MockIo::new()
        .env("AWS_ACCESS_KEY_ID", "AKIDENV")
        .env("AWS_SECRET_ACCESS_KEY", "SECRETENV")
        .env("AWS_SESSION_TOKEN", "TOKENV")
        .env("AWS_REGION", "us-west-2");

    let creds = resolve_credentials(&mock, None)
        .await
        .expect("env credentials should resolve");
    assert_eq!(creds.access_key_id, "AKIDENV");
    assert_eq!(creds.secret_access_key, "SECRETENV");
    assert_eq!(creds.session_token.as_deref(), Some("TOKENV"));

    let region = resolve_region(&mock, None).await;
    assert_eq!(region.as_deref(), Some("us-west-2"));
}

#[tokio::test]
async fn mock_drives_profile_files_and_region() {
    // file(): exact-path config + credentials files.
    let mock = MockIo::new()
        .env("HOME", "/home/test")
        .env("AWS_CONFIG_FILE", "/cfg/config")
        .env("AWS_SHARED_CREDENTIALS_FILE", "/cfg/credentials")
        .file("/cfg/config", "[default]\nregion = eu-central-1\n")
        .file(
            "/cfg/credentials",
            "[default]\naws_access_key_id = AKIDFILE\naws_secret_access_key = SECRETFILE\n",
        );

    let creds = resolve_credentials(&mock, None)
        .await
        .expect("profile credentials should resolve");
    assert_eq!(creds.access_key_id, "AKIDFILE");
    assert_eq!(creds.secret_access_key, "SECRETFILE");

    let region = resolve_region(&mock, None).await;
    assert_eq!(region.as_deref(), Some("eu-central-1"));
}

#[tokio::test]
async fn mock_drives_http_container_endpoint() {
    // http(): the container provider hits the relative URI; assert the handler
    // is invoked and call tracking works.
    let called = Arc::new(AtomicBool::new(false));
    let called_h = called.clone();

    let mock = MockIo::new()
        .env("AWS_CONTAINER_CREDENTIALS_RELATIVE_URI", "/v2/creds")
        .http(move |method, url, _headers| {
            called_h.store(true, Ordering::SeqCst);
            assert_eq!(method, "GET");
            assert_eq!(url, "http://169.254.170.2/v2/creds");
            Ok(HttpResponse {
                status: 200,
                body:
                    r#"{"AccessKeyId":"AKIDHTTP","SecretAccessKey":"SECRETHTTP","Token":"TOKHTTP","Expiration":"2099-01-01T00:00:00Z"}"#
                        .to_string(),
            })
        });

    let creds = resolve_credentials(&mock, None)
        .await
        .expect("container credentials should resolve");
    assert_eq!(creds.access_key_id, "AKIDHTTP");
    assert_eq!(creds.secret_access_key, "SECRETHTTP");

    assert!(called.load(Ordering::SeqCst), "http handler must be called");
    assert_eq!(mock.http_calls(), 1);
    assert_eq!(
        mock.last_http_url().as_deref(),
        Some("http://169.254.170.2/v2/creds")
    );
}

#[tokio::test]
async fn mock_drives_command_and_file_contains() {
    // command(): credential_process provider. file_contains(): substring match.
    let mock = MockIo::new()
        .env("HOME", "/home/test")
        .env("AWS_CONFIG_FILE", "/cfg/config")
        .file(
            "/cfg/config",
            "[default]\ncredential_process = /bin/get-creds\n",
        )
        .file_contains("token-cache", "ignored-cache-contents")
        .command(|cmd| {
            assert_eq!(cmd, "/bin/get-creds");
            Ok(CommandOutput {
                status: 0,
                stdout: r#"{"Version":1,"AccessKeyId":"AKIDCMD","SecretAccessKey":"SECRETCMD","SessionToken":"TOKCMD"}"#
                    .to_string(),
            })
        });

    let creds = resolve_credentials(&mock, None)
        .await
        .expect("credential_process should resolve");
    assert_eq!(creds.access_key_id, "AKIDCMD");
    assert_eq!(creds.secret_access_key, "SECRETCMD");
    assert_eq!(creds.session_token.as_deref(), Some("TOKCMD"));

    // file_contains: any path containing the substring returns the contents.
    let got = mock.read_file("/var/run/token-cache/abc.json").await;
    assert_eq!(got.as_deref(), Some("ignored-cache-contents"));
}
