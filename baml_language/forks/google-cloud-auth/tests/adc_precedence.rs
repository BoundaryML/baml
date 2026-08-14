#![allow(clippy::doc_markdown)]

//! ADC source-precedence guard — the GCP analog of the AWS provider-chain
//! ordering test. `token_from_adc` must consult sources in this order:
//!
//!     GOOGLE_APPLICATION_CREDENTIALS  >  well-known config file  >  GCE metadata
//!
//! Each source is wired with a credential that mints a DISTINCT access token;
//! sources are removed from the top so exactly one can answer. The returned
//! token tells us which source won. Treat an unexplained change here as a
//! credential-safety regression in the ADC discovery order.

mod common;

use common::{MockIo, authorized_user_json, token_response};
use forked_google_cloud_auth::{AuthError, CLOUD_PLATFORM_SCOPE, HttpResponse, token_from_adc};

const TOKEN_URI: &str = "https://oauth2.googleapis.com/token";
const WELL_KNOWN: &str = "/home/dev/.config/gcloud/application_default_credentials.json";

/// One handler that answers the `OAuth2` token endpoint (keyed on which
/// `refresh_token` is present) and the GCE metadata server, each with its own
/// distinct access token.
fn router(
    method: &str,
    url: &str,
    _h: &[(String, String)],
    body: &str,
) -> Result<HttpResponse, AuthError> {
    if method == "POST" && url == TOKEN_URI {
        if body.contains("refresh_token=RTGAC") {
            return Ok(HttpResponse {
                status: 200,
                body: token_response("ya29.gac"),
            });
        }
        if body.contains("refresh_token=RTWELLKNOWN") {
            return Ok(HttpResponse {
                status: 200,
                body: token_response("ya29.wellknown"),
            });
        }
    }
    if method == "GET" && url.contains("metadata.google.internal") {
        return Ok(HttpResponse {
            status: 200,
            body: token_response("ya29.metadata"),
        });
    }
    Err(AuthError::Io(format!(
        "unexpected request {method} {url} body={body}"
    )))
}

fn gac() -> (String, String) {
    (
        "GOOGLE_APPLICATION_CREDENTIALS".into(),
        "/gac/adc.json".into(),
    )
}

#[tokio::test]
async fn gac_beats_well_known_and_metadata() {
    let io = MockIo::new()
        .env("GOOGLE_APPLICATION_CREDENTIALS", "/gac/adc.json")
        .file(
            "/gac/adc.json",
            &authorized_user_json("cid", "RTGAC", TOKEN_URI),
        )
        .env("HOME", "/home/dev")
        .file(
            WELL_KNOWN,
            &authorized_user_json("cid", "RTWELLKNOWN", TOKEN_URI),
        )
        .http(router);
    let token = token_from_adc(&io, CLOUD_PLATFORM_SCOPE).await.unwrap();
    assert_eq!(
        token, "ya29.gac",
        "GOOGLE_APPLICATION_CREDENTIALS must take precedence"
    );
}

#[tokio::test]
async fn well_known_beats_metadata() {
    // No GAC env -> well-known file wins over metadata.
    let io = MockIo::new()
        .env("HOME", "/home/dev")
        .file(
            WELL_KNOWN,
            &authorized_user_json("cid", "RTWELLKNOWN", TOKEN_URI),
        )
        .http(router);
    let token = token_from_adc(&io, CLOUD_PLATFORM_SCOPE).await.unwrap();
    assert_eq!(
        token, "ya29.wellknown",
        "well-known ADC file must beat metadata"
    );
}

#[tokio::test]
async fn metadata_is_last_resort() {
    // Nothing on disk -> GCE metadata server.
    let io = MockIo::new().env("HOME", "/home/dev").http(router);
    let token = token_from_adc(&io, CLOUD_PLATFORM_SCOPE).await.unwrap();
    assert_eq!(
        token, "ya29.metadata",
        "metadata server is the final fallback"
    );
}

#[tokio::test]
async fn metadata_failure_surfaces_no_credentials() {
    // No disk creds and a metadata server that refuses -> NoCredentials.
    // Uses a distinct scope: metadata tokens are cached process-wide keyed by
    // scope, and `metadata_is_last_resort` mints one under the default scope.
    let io = MockIo::new()
        .env("HOME", "/home/dev")
        .http(|_m, _u, _h, _b| {
            Ok(HttpResponse {
                status: 404,
                body: String::new(),
            })
        });
    let scope = "https://www.googleapis.com/auth/userinfo.email";
    let err = token_from_adc(&io, scope).await.unwrap_err();
    assert!(matches!(err, AuthError::NoCredentials(_)), "got {err:?}");
    let _ = gac(); // silence unused helper on some toolchains
}
