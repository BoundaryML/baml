//! Google Cloud `OAuth2` access tokens.
//!
//! ## Credential resolution order
//!
//! 1. `credentials_json` — an inline credential JSON document (service account,
//!    authorized user, workload identity federation, or impersonated service
//!    account: the same documents `GOOGLE_APPLICATION_CREDENTIALS` accepts).
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

use google_cloud_auth::TokenIo;
use sys_types::runtime_io::RuntimeIo;

use crate::AuthError;

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
        Some(json) => google_cloud_auth::token_from_credentials_json(&adapter, &json, scope).await,
        None => google_cloud_auth::token_from_adc(&adapter, scope).await,
    }
    .map_err(map_auth_error)
}

/// Resolve the Google Cloud project id from `credentials_json` when it carries
/// one, else from the google-auth chain (`GOOGLE_CLOUD_PROJECT` /
/// `GCLOUD_PROJECT`, the ADC and gcloud config files, the metadata server).
///
/// `None` means "not discoverable" — the caller decides whether that is fatal.
pub async fn project_id(
    io: Arc<dyn RuntimeIo>,
    credentials_json: Option<String>,
) -> Option<String> {
    if let Some(json) = &credentials_json {
        if let Some(pid) = google_cloud_auth::project_id_from_json(json) {
            return Some(pid);
        }
    }
    let adapter = crate::bridge(io);
    google_cloud_auth::project_id(&adapter).await
}

/// Resolve the quota/billing project (`x-goog-user-project`), matching
/// google-auth's `Credentials.apply`: `GOOGLE_CLOUD_QUOTA_PROJECT` always wins,
/// then the credential document's `quota_project_id`.
///
/// A set-but-empty env var is honored, matching the fork: a misconfigured
/// variable should be visible rather than silently skipped.
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
            google_cloud_auth::quota_project_id_from_json(&json)
        }
        None => google_cloud_auth::quota_project_id(&adapter).await,
    }
}

/// Transport failures stay retry-safe (`Io`); everything else is a credential
/// problem the caller cannot retry its way out of.
fn map_auth_error(err: google_cloud_auth::AuthError) -> AuthError {
    match err {
        google_cloud_auth::AuthError::Io(m) => AuthError::Io(format!("Google Cloud auth: {m}")),
        other => AuthError::Access(format!("Google Cloud auth: {other}")),
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
            project_id(io, Some(json)).await.as_deref(),
            Some("doc-project")
        );
    }

    /// Without a project in the document, the google-auth env chain answers.
    #[tokio::test]
    async fn project_id_falls_back_to_the_env_chain() {
        let io = StubIo::new()
            .env("GOOGLE_CLOUD_PROJECT", "env-project")
            .arc();
        assert_eq!(project_id(io, None).await.as_deref(), Some("env-project"));
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
