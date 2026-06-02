#![allow(clippy::doc_markdown)]

//! Provider-chain precedence + coverage guard.
//!
//! The per-provider test files prove each credential source parses correctly in
//! isolation. THIS file is the safety net for the *ordering* of the default
//! provider chain — the invariant most likely to be silently broken by a future
//! refactor of `resolve_credentials`. The documented order is:
//!
//!     environment  >  shared profile  >  ECS/container  >  EC2 `IMDSv2`
//!
//! Each layer below is wired with a credential bearing a DISTINCT access key id,
//! then layers are removed from the top so that exactly one provider can answer.
//! The returned access key id tells us which provider won.
//!
//! If you change the chain, these assertions must be updated deliberately — that
//! is the point. Treat an unexplained change here as a credential-safety regression.

mod common;

use common::MockIo;

const AK_ENV: &str = "AKIA_ENV_0000000000";
const AK_PROFILE: &str = "AKIA_PROFILE_000000";
const AK_CONTAINER: &str = "AKIA_CONTAINER_0000";
const AK_IMDS: &str = "AKIA_IMDS_0000000000";

const CONTAINER_URI: &str = "http://169.254.170.2/v2/credentials/abc";

fn creds_json(access_key: &str) -> String {
    serde_json::json!({
        "AccessKeyId": access_key,
        "SecretAccessKey": "secret-for-".to_string() + access_key,
        "Token": "token-for-".to_string() + access_key,
        "Expiration": "2099-01-01T00:00:00Z",
    })
    .to_string()
}

/// One HTTP handler that can answer BOTH the container endpoint and the full
/// `IMDSv2` dance, each with its own distinct credential. Whichever provider the
/// chain consults first is the one whose creds come back.
fn container_and_imds_http(
    method: &str,
    url: &str,
    _headers: &[(String, String)],
) -> Result<aws_config::HttpResponse, aws_config::ConfigError> {
    // Container endpoint.
    if url.starts_with(CONTAINER_URI) {
        return Ok(aws_config::HttpResponse {
            status: 200,
            body: creds_json(AK_CONTAINER),
        });
    }
    // IMDSv2: token, then role name, then role credentials.
    if url.ends_with("/latest/api/token") && method == "PUT" {
        return Ok(aws_config::HttpResponse {
            status: 200,
            body: "imds-token".into(),
        });
    }
    if url.ends_with("/iam/security-credentials/") {
        return Ok(aws_config::HttpResponse {
            status: 200,
            body: "imds-role".into(),
        });
    }
    if url.ends_with("/iam/security-credentials/imds-role") {
        return Ok(aws_config::HttpResponse {
            status: 200,
            body: creds_json(AK_IMDS),
        });
    }
    Err(aws_config::ConfigError::Io(format!("unexpected url {url}")))
}

fn with_env(io: MockIo) -> MockIo {
    io.env("AWS_ACCESS_KEY_ID", AK_ENV)
        .env("AWS_SECRET_ACCESS_KEY", "secret-for-env")
        .env("AWS_SESSION_TOKEN", "token-for-env")
}

fn with_profile(io: MockIo) -> MockIo {
    io.env("AWS_SHARED_CREDENTIALS_FILE", "/fake/aws/credentials").file(
        "/fake/aws/credentials",
        &format!(
            "[default]\naws_access_key_id = {AK_PROFILE}\naws_secret_access_key = secret-for-profile\n"
        ),
    )
}

fn with_container(io: MockIo) -> MockIo {
    io.env("AWS_CONTAINER_CREDENTIALS_FULL_URI", CONTAINER_URI)
}

/// Full stack present → environment wins.
#[tokio::test]
async fn env_beats_everything() {
    let io = with_container(with_profile(with_env(MockIo::new()))).http(container_and_imds_http);
    let creds = aws_config::resolve_credentials(&io, None).await.unwrap();
    assert_eq!(
        creds.access_key_id, AK_ENV,
        "environment must take top priority"
    );
    // Environment short-circuits before any HTTP provider is consulted.
    assert_eq!(
        io.http_calls(),
        0,
        "env provider must not touch the network"
    );
}

/// No environment creds → shared profile wins over container + IMDS.
#[tokio::test]
async fn profile_beats_container_and_imds() {
    let io = with_container(with_profile(MockIo::new())).http(container_and_imds_http);
    let creds = aws_config::resolve_credentials(&io, None).await.unwrap();
    assert_eq!(
        creds.access_key_id, AK_PROFILE,
        "profile must beat container/IMDS"
    );
    assert_eq!(
        io.http_calls(),
        0,
        "profile static creds must not touch the network"
    );
}

/// No env, no profile → container endpoint wins over IMDS.
#[tokio::test]
async fn container_beats_imds() {
    let io = with_container(MockIo::new()).http(container_and_imds_http);
    let creds = aws_config::resolve_credentials(&io, None).await.unwrap();
    assert_eq!(
        creds.access_key_id, AK_CONTAINER,
        "container must beat IMDS"
    );
}

/// Only IMDS available → `IMDSv2` is the last resort and still works.
#[tokio::test]
async fn imds_is_last_resort() {
    let io = MockIo::new().http(container_and_imds_http);
    let creds = aws_config::resolve_credentials(&io, None).await.unwrap();
    assert_eq!(
        creds.access_key_id, AK_IMDS,
        "IMDS must answer when nothing else can"
    );
    assert!(
        creds.session_token.is_some(),
        "IMDS role creds carry a session token"
    );
}

/// Coverage guard: with NO provider able to answer, resolution reaches IMDS as
/// the last resort (so a dropped provider is visible).
#[tokio::test]
async fn empty_chain_attempts_imds_last() {
    // No env/profile/container, and an HTTP handler that refuses everything so
    // the IMDS token probe fails immediately rather than hanging.
    let io = MockIo::new().http(|_, url, _| {
        assert!(
            url.ends_with("/latest/api/token"),
            "first no-provider HTTP probe should be IMDS token fetch, got {url}"
        );
        Err(aws_config::ConfigError::Io(format!("blocked {url}")))
    });
    let err = aws_config::resolve_credentials(&io, None)
        .await
        .unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("blocked"), "unexpected error: {msg}");
    assert_eq!(io.http_calls(), 1, "IMDS token probe should be attempted");
}
