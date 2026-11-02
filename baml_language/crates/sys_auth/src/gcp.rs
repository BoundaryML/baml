//! Google Cloud `OAuth2` access tokens.
//!
//! ## Credential resolution order
//!
//! 1. `credentials_json` — an inline credential JSON document (service account,
//!    authorized user, workload identity federation, or impersonated service
//!    account: the same documents `GOOGLE_APPLICATION_CREDENTIALS` accepts). A
//!    document stored as a JSON string is unwrapped.
//! 2. Application Default Credentials — the `GOOGLE_APPLICATION_CREDENTIALS`
//!    file, the well-known ADC config file, the gcloud config file, then the GCE
//!    metadata server.
//!
//! An explicitly-passed document is used as-is: a broken value is an error,
//! never a silent cascade to ADC. Reading a credentials *file path* and the
//! `GOOGLE_APPLICATION_CREDENTIALS_CONTENT` env var are the caller's job — both
//! are plain IO, so they belong in `.baml`; only the crypto is here.
//!
//! The fork caches minted tokens process-wide (keyed by credential material +
//! scope) until shortly before expiry, so repeated calls on the same client cost
//! nothing.

use std::sync::Arc;

use google_cloud_auth::{PathSetting, TokenIo};
use sys_types::runtime_io::RuntimeIo;

use crate::AuthError;

/// `credentials_json` as the document to use: itself, or the document a JSON
/// string holds. A value that is not a document is an error naming what it
/// looks like (a path put where a document goes is the common case).
fn document(credentials_json: &str) -> Result<String, AuthError> {
    match google_cloud_auth::credential_document(credentials_json) {
        Some(document) => Ok(document.into_owned()),
        None => Err(AuthError::Access(not_a_document_message(credentials_json))),
    }
}

/// The error for a credential document that is not one, naming a path when it
/// is plainly one.
fn not_a_document_message(text: &str) -> String {
    let held = PathSetting::classify(text);
    debug_assert!(
        !matches!(held, PathSetting::Document { .. }),
        "credential_document accepts every value PathSetting calls a document"
    );
    let reason = match held {
        PathSetting::Empty => "it is empty".to_string(),
        PathSetting::Path(path) | PathSetting::QuotedPath(path) | PathSetting::PaddedPath(path) => {
            format!("it looks like the path '{path}', which goes in credentials_file")
        }
        PathSetting::Document { .. } | PathSetting::Unrecognized { .. } => format!(
            "it is not a JSON object ({} characters, not shown)",
            text.chars().count()
        ),
    };
    format!("Google Cloud: the inline credential is not a credential document: {reason}")
}

/// Mint a Google Cloud `OAuth2` access token for `scope`.
///
/// Returns the bare token; the caller builds the `Bearer` header.
pub async fn access_token(
    io: Arc<dyn RuntimeIo>,
    credentials_json: Option<String>,
    scope: &str,
) -> Result<String, AuthError> {
    let adapter = crate::bridge(io);
    match credentials_json {
        Some(json) => {
            let json = document(&json)?;
            google_cloud_auth::token_from_credentials_json(&adapter, &json, scope).await
        }
        None => google_cloud_auth::token_from_adc(&adapter, scope).await,
    }
    .map_err(|e| map_auth_error(&e))
}

/// Resolve the Google Cloud project id from `credentials_json` when it carries
/// one, else from the google-auth chain (`GOOGLE_CLOUD_PROJECT` /
/// `GCLOUD_PROJECT`, the gcloud config, the metadata server, and, for
/// Application Default Credentials only, the ADC credential files).
///
/// `Ok(None)` means "not discoverable": the caller decides whether that is
/// fatal. A source that cannot be used (a document that is not one, an
/// unreadable `GOOGLE_APPLICATION_CREDENTIALS`) is the same error
/// [`access_token`] reports, so the problem is named where it is first met
/// rather than as a missing project.
pub async fn project_id(
    io: Arc<dyn RuntimeIo>,
    credentials_json: Option<String>,
) -> Result<Option<String>, AuthError> {
    let credentials = credentials_json.as_deref().map(document).transpose()?;
    let adapter = crate::bridge(io);
    google_cloud_auth::project_id(&adapter, credentials.as_deref())
        .await
        .map_err(|e| map_auth_error(&e))
}

/// Resolve the quota/billing project (`x-goog-user-project`), matching
/// google-auth's `Credentials.apply`: `GOOGLE_CLOUD_QUOTA_PROJECT` always wins,
/// then the credential document's `quota_project_id`.
///
/// Best-effort: a source that cannot be used is [`access_token`]'s error to
/// report. A set-but-empty env var is honored, matching the fork: a
/// misconfigured variable should be visible rather than silently skipped.
pub async fn quota_project_id(
    io: Arc<dyn RuntimeIo>,
    credentials_json: Option<String>,
) -> Option<String> {
    let adapter = crate::bridge(io);
    match credentials_json {
        Some(json) => {
            if let Some(val) = adapter.env("GOOGLE_CLOUD_QUOTA_PROJECT").await {
                return Some(val.trim().to_string());
            }
            google_cloud_auth::quota_project_id_from_json(&document(&json).ok()?)
        }
        None => google_cloud_auth::quota_project_id(&adapter).await,
    }
}

/// Transport failures stay retry-safe (`Io`); everything else is a credential
/// problem the caller cannot retry its way out of. A document put in
/// `GOOGLE_APPLICATION_CREDENTIALS` is told where documents go.
fn map_auth_error(err: &google_cloud_auth::AuthError) -> AuthError {
    match err {
        google_cloud_auth::AuthError::Io(_) => AuthError::Io(err.to_string()),
        google_cloud_auth::AuthError::UnusableCredentialPath { held, .. } => {
            let hint = match held {
                PathSetting::Document { .. } => {
                    "; pass a document through GOOGLE_APPLICATION_CREDENTIALS_CONTENT or the \
                     client's credentials_json"
                }
                PathSetting::Empty
                | PathSetting::Path(_)
                | PathSetting::QuotedPath(_)
                | PathSetting::PaddedPath(_)
                | PathSetting::Unrecognized { .. } => "",
            };
            AuthError::Access(format!("{err}{hint}"))
        }
        google_cloud_auth::AuthError::Parse(_)
        | google_cloud_auth::AuthError::Signing(_)
        | google_cloud_auth::AuthError::TokenEndpoint(_)
        | google_cloud_auth::AuthError::NoCredentials(_)
        | google_cloud_auth::AuthError::Unsupported(_) => AuthError::Access(err.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{StubIo, authorized_user_json, token_response};

    /// An explicitly-passed credential document is used as-is: a broken one
    /// errors instead of cascading to the (here, working) ADC chain.
    #[tokio::test]
    async fn explicit_credentials_never_cascade_to_adc() {
        let io = StubIo::new()
            .env("GOOGLE_APPLICATION_CREDENTIALS", "/fake/adc.json")
            .file("/fake/adc.json", &authorized_user_json("cascade-adc"))
            .http(200, &token_response("ya29.from-adc"))
            .arc();

        let err = access_token(io, Some("{ not json".to_string()), "scope")
            .await
            .unwrap_err();
        assert!(
            matches!(err, AuthError::Access(_)),
            "a broken explicit credential must not fall back to ADC: {err:?}"
        );
    }

    /// The explicit document is what gets minted, through the fork's full
    /// credential-type dispatch (here an `authorized_user` refresh grant).
    #[tokio::test]
    async fn explicit_credentials_json_is_minted() {
        let io = StubIo::new()
            .http(200, &token_response("ya29.from-explicit"))
            .arc();
        let token = access_token(io, Some(authorized_user_json("explicit-mint")), "scope")
            .await
            .unwrap();
        assert_eq!(token, "ya29.from-explicit");
    }

    /// Nothing discoverable anywhere: an unretryable credential error, not an
    /// empty token.
    #[tokio::test]
    async fn no_credentials_is_an_access_error() {
        let err = access_token(StubIo::new().arc(), None, "scope")
            .await
            .unwrap_err();
        assert!(matches!(err, AuthError::Access(_)), "{err:?}");
    }

    /// A credential document put in `GOOGLE_APPLICATION_CREDENTIALS` (which
    /// takes a path) must not reach the error the caller logs.
    #[tokio::test]
    async fn inline_document_in_gac_stays_out_of_the_error() {
        let document = authorized_user_json("inline-gac");
        let io = StubIo::new()
            .env("GOOGLE_APPLICATION_CREDENTIALS", &document)
            .arc();
        let err = access_token(io, None, "scope").await.unwrap_err();
        let text = err.to_string();
        assert!(matches!(err, AuthError::Access(_)), "{text}");
        for secret in ["inline-gac-client-secret", "inline-gac-refresh-token"] {
            assert!(!text.contains(secret), "error leaks {secret:?}: {text}");
        }
        assert!(
            text.contains("GOOGLE_APPLICATION_CREDENTIALS_CONTENT")
                && text.contains("credentials_json"),
            "the error says where a document goes: {text}"
        );
        assert!(
            !text.contains("Google Cloud auth: Google Cloud"),
            "one prefix: {text}"
        );
    }

    /// A path put where a document goes is named as one, with where it goes.
    #[tokio::test]
    async fn a_path_in_the_document_slot_says_where_paths_go() {
        let err = access_token(StubIo::new().arc(), Some("/creds/key.json".into()), "scope")
            .await
            .unwrap_err();
        let text = err.to_string();
        assert!(
            text.contains(
                "it looks like the path '/creds/key.json', which goes in credentials_file"
            ),
            "{text}"
        );
    }

    /// With no explicit document, the ADC chain runs through the injected IO.
    #[tokio::test]
    async fn adc_chain_uses_injected_io() {
        let io = StubIo::new()
            .env("GOOGLE_APPLICATION_CREDENTIALS", "/fake/adc.json")
            .file("/fake/adc.json", &authorized_user_json("adc-chain"))
            .http(200, &token_response("ya29.adc-token"))
            .arc();
        let token = access_token(io, None, "scope").await.unwrap();
        assert_eq!(token, "ya29.adc-token");
    }

    /// The credential document's own `project_id` wins over the env chain.
    #[tokio::test]
    async fn project_id_prefers_the_credential_document() {
        let io = StubIo::new()
            .env("GOOGLE_CLOUD_PROJECT", "env-project")
            .arc();
        let json = serde_json::json!({
            "type": "authorized_user",
            "project_id": "doc-project",
            "client_id": "cid",
            "client_secret": "secret",
            "refresh_token": "refresh",
        })
        .to_string();
        assert_eq!(
            project_id(io, Some(json)).await.unwrap().as_deref(),
            Some("doc-project")
        );
    }

    /// Without a project in the document, the google-auth env chain answers.
    #[tokio::test]
    async fn project_id_falls_back_to_the_env_chain() {
        let io = StubIo::new()
            .env("GOOGLE_CLOUD_PROJECT", "env-project")
            .arc();
        assert_eq!(
            project_id(io, None).await.unwrap().as_deref(),
            Some("env-project")
        );
    }

    #[tokio::test]
    async fn quota_project_env_wins_over_the_document() {
        let io = StubIo::new()
            .env("GOOGLE_CLOUD_QUOTA_PROJECT", "env-quota")
            .arc();
        let json = serde_json::json!({
            "type": "authorized_user",
            "quota_project_id": "doc-quota",
        })
        .to_string();
        assert_eq!(
            quota_project_id(io, Some(json)).await.as_deref(),
            Some("env-quota")
        );
    }

    #[tokio::test]
    async fn quota_project_falls_back_to_the_document() {
        let json = serde_json::json!({
            "type": "authorized_user",
            "quota_project_id": "doc-quota",
        })
        .to_string();
        assert_eq!(
            quota_project_id(StubIo::new().arc(), Some(json))
                .await
                .as_deref(),
            Some("doc-quota")
        );
    }
}
