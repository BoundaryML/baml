//! Vertex AI authentication and `project_id` resolution.
//!
//! Credentials are resolved once via [`resolve_credentials`], then used for
//! both access token and `project_id` (single-source principle).
//!
//! ## Credential resolution order
//!
//! 1. `options.credentials_content` -- inline service account JSON
//! 2. `options.credentials` -- inline JSON or file path
//! 3. `GOOGLE_APPLICATION_CREDENTIALS` env var -- inline JSON (file paths
//!    are deferred to ADC on native)
//! 4. `GOOGLE_APPLICATION_CREDENTIALS_CONTENT` env var (BAML-specific)
//! 5. ADC via `google-cloud-auth` (native only -- covers ADC config file,
//!    `GOOGLE_APPLICATION_CREDENTIALS` file paths, metadata server)
//! 6. `gcloud` CLI
//!
//! ## Platform differences
//!
//! On native, service account tokens use `google-cloud-auth`; ADC and
//! `gcloud` CLI are available.
//!
//! On WASM, `google-cloud-auth` cannot be used (depends on tokio features
//! that don't compile on wasm32). Service account tokens are generated via
//! pure-Rust JWT signing (`rsa` + `sha2`). ADC and `gcloud` CLI are not
//! available; credentials must be provided explicitly or via env vars.

use std::sync::Arc;

#[cfg_attr(target_arch = "wasm32", allow(unused_imports))]
use sys_types::{
    BexExternalValue,
    runtime_io::{RuntimeIo, RuntimeIoError},
};

use crate::{
    baml_std::{HttpRequest, PrimitiveClient, ProviderOptions, VertexAiOptions},
    build_request::BuildRequestError,
};

// ---------------------------------------------------------------------------
// Public entry point (shared across native and WASM)
// ---------------------------------------------------------------------------

/// Add Google Cloud `OAuth2` auth headers to a Vertex AI request.
///
/// Also resolves `project_id` in the URL if it contains the placeholder
/// (i.e. `project_id` was not known at URL construction time).
///
/// Credentials are resolved once, then used for both token and `project_id`
/// (matching the old engine's single-source principle).
pub(crate) async fn auth_vertex(
    request: &mut HttpRequest,
    client: &PrimitiveClient,
    io: Arc<dyn RuntimeIo>,
) -> Result<(), BuildRequestError> {
    let vertex_opts = match &client.provider_options {
        Some(ProviderOptions::VertexAi(opts)) => Some(opts.clone()),
        _ => None,
    };

    // If an API key is provided as a query param, skip token-based auth
    // but still resolve the project-id placeholder in the URL.
    let api_key_auth = client.options.query_params.contains_key("key");
    let needs_project_id = request
        .url
        .contains(crate::build_request::google::VERTEX_PROJECT_ID_PLACEHOLDER);

    // With API-key auth and no project-id placeholder, no credentials needed.
    if api_key_auth && !needs_project_id {
        return Ok(());
    }

    // Resolve credentials once (needed for both project-id and token).
    let creds = resolve_credentials(vertex_opts.as_ref(), io.clone()).await?;

    // Resolve project_id placeholder in the URL if needed.
    if needs_project_id {
        let project_id = project_id_from_credentials(&creds, &*io)
            .await
            .ok_or_else(|| {
                BuildRequestError::Other(
                    "Could not resolve project_id for Vertex AI. Set options.project_id, \
                     the GOOGLE_CLOUD_PROJECT env var, or provide credentials containing \
                     a project_id."
                        .to_string(),
                )
            })?;
        request.url = request.url.replace(
            crate::build_request::google::VERTEX_PROJECT_ID_PLACEHOLDER,
            &project_id,
        );
    }

    // API-key auth doesn't need a bearer token.
    if api_key_auth {
        return Ok(());
    }

    let token = token_from_credentials(&creds, io).await?;

    request
        .headers
        .insert("authorization".to_string(), format!("Bearer {token}"));

    Ok(())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Unwrap double-quoted JSON strings (e.g. from Vercel `env pull`).
///
/// Some environments store JSON credentials as a JSON string value with escaped
/// inner quotes: `"{\"type\":\"service_account\",...}"`. This function detects
/// that and unwraps to the inner JSON object string.
fn try_unwrap_quoted_json(s: String) -> String {
    if s.starts_with('"') {
        if let Ok(serde_json::Value::String(inner)) = serde_json::from_str::<serde_json::Value>(&s)
        {
            return inner;
        }
    }
    s
}

/// Extract `project_id` from a JSON string (service account credentials).
fn extract_project_id_from_json(json_str: &str) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(json_str)
        .ok()
        .and_then(|v| v.get("project_id")?.as_str().map(String::from))
}

// ---------------------------------------------------------------------------
// Credential resolution (single source for both token and project_id)
// ---------------------------------------------------------------------------

/// The resolved credential source.
///
/// Matches the old engine's auth strategy: one source is selected, then
/// used for both token and `project_id`.
enum ResolvedCredentials {
    /// Service account JSON (inline or read from file/env var).
    ServiceAccountJson(String),
    /// ADC via `google-cloud-auth` (native only).
    /// Covers `~/.config/gcloud/application_default_credentials.json`,
    /// `GOOGLE_APPLICATION_CREDENTIALS`, and the metadata server.
    #[allow(dead_code)] // constructed only on native (behind cfg)
    Adc,
    /// gcloud CLI fallback.
    GcloudCli,
}

/// Resolve which credential source to use.
///
/// Resolution order:
/// 1. `credentials_content` option
/// 2. `credentials` option (inline JSON or file path)
/// 3. `GOOGLE_APPLICATION_CREDENTIALS` env var (inline JSON; file paths deferred to ADC)
/// 4. `GOOGLE_APPLICATION_CREDENTIALS_CONTENT` env var (BAML-specific)
/// 5. ADC via google-cloud-auth (native only)
/// 6. `gcloud` CLI
async fn resolve_credentials(
    vertex_opts: Option<&VertexAiOptions>,
    io: Arc<dyn RuntimeIo>,
) -> Result<ResolvedCredentials, BuildRequestError> {
    // 1. credentials_content: always inline JSON.
    if let Some(json_str) = vertex_opts.and_then(|o| o.credentials_content.as_ref()) {
        return Ok(ResolvedCredentials::ServiceAccountJson(json_str.clone()));
    }

    // 2. credentials: inline JSON or file path.
    if let Some(creds) = vertex_opts.and_then(|o| o.credentials.as_ref()) {
        if serde_json::from_str::<serde_json::Value>(creds).is_ok() {
            return Ok(ResolvedCredentials::ServiceAccountJson(creds.clone()));
        }
        let json_str = read_credentials_file(creds, &*io).await?;
        return Ok(ResolvedCredentials::ServiceAccountJson(json_str));
    }

    // 3. GOOGLE_APPLICATION_CREDENTIALS env var.
    // Inline JSON is handled here; file paths are deferred to ADC (step 5).
    if let Ok(Some(val)) = io
        .env_get("GOOGLE_APPLICATION_CREDENTIALS".to_string())
        .await
    {
        let val = try_unwrap_quoted_json(val);
        if !val.is_empty() && serde_json::from_str::<serde_json::Value>(&val).is_ok() {
            return Ok(ResolvedCredentials::ServiceAccountJson(val));
        }
    }

    // 4. GOOGLE_APPLICATION_CREDENTIALS_CONTENT env var (BAML-specific).
    if let Ok(Some(val)) = io
        .env_get("GOOGLE_APPLICATION_CREDENTIALS_CONTENT".to_string())
        .await
    {
        let val = try_unwrap_quoted_json(val);
        if !val.is_empty() && serde_json::from_str::<serde_json::Value>(&val).is_ok() {
            return Ok(ResolvedCredentials::ServiceAccountJson(val));
        }
    }

    // 5. ADC via google-cloud-auth (native only).
    #[cfg(not(target_arch = "wasm32"))]
    if native::build_from_adc(io.clone()).is_ok() {
        return Ok(ResolvedCredentials::Adc);
    }

    // 6. gcloud CLI.
    if io
        .sys_shell(
            "gcloud auth print-access-token --quiet 2>/dev/null".to_string(),
            None,
        )
        .await
        .is_ok_and(|out| !String::from_utf8_lossy(&out.stdout).trim().is_empty())
    {
        return Ok(ResolvedCredentials::GcloudCli);
    }

    Err(BuildRequestError::AuthorizationFailed(
        "Google Cloud: no credentials found. Set credentials/credentials_content in options, \
         GOOGLE_APPLICATION_CREDENTIALS env var, or run `gcloud auth application-default login`."
            .into(),
    ))
}

// ---------------------------------------------------------------------------
// Token from resolved credentials
// ---------------------------------------------------------------------------

async fn token_from_credentials(
    creds: &ResolvedCredentials,
    io: Arc<dyn RuntimeIo>,
) -> Result<String, BuildRequestError> {
    match creds {
        ResolvedCredentials::ServiceAccountJson(json_str) => {
            token_from_service_account_json(json_str, io).await
        }
        ResolvedCredentials::Adc => token_from_adc(io).await,
        ResolvedCredentials::GcloudCli => {
            let output = io
                .sys_shell("gcloud auth print-access-token --quiet".to_string(), None)
                .await
                .map_err(|e| {
                    BuildRequestError::AuthorizationFailed(format!(
                        "Google Cloud: gcloud auth print-access-token failed: {e}"
                    ))
                })?;
            let token = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if token.is_empty() {
                Err(BuildRequestError::AuthorizationFailed(
                    "Google Cloud: gcloud auth print-access-token returned empty".into(),
                ))
            } else {
                Ok(token)
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Project ID from resolved credentials
// ---------------------------------------------------------------------------

/// Get `project_id` from the resolved credential source, with fallbacks.
async fn project_id_from_credentials(
    creds: &ResolvedCredentials,
    io: &dyn RuntimeIo,
) -> Option<String> {
    // Try the credential source itself first.
    match creds {
        ResolvedCredentials::ServiceAccountJson(json_str) => {
            if let Some(pid) = extract_project_id_from_json(json_str) {
                return Some(pid);
            }
        }
        ResolvedCredentials::GcloudCli => {
            if let Ok(output) = io
                .sys_shell(
                    "gcloud config get-value project 2>/dev/null".to_string(),
                    None,
                )
                .await
            {
                let pid = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !pid.is_empty() {
                    return Some(pid);
                }
            }
        }
        ResolvedCredentials::Adc => {
            // ADC was resolved by google-cloud-auth, which doesn't expose
            // project_id. Try reading the credentials file it used.
            if let Ok(Some(val)) = io
                .env_get("GOOGLE_APPLICATION_CREDENTIALS".to_string())
                .await
            {
                if !val.is_empty() {
                    // Inline JSON?
                    if let Some(pid) = extract_project_id_from_json(&val) {
                        return Some(pid);
                    }
                    // File path? Read and extract.
                    if let Ok(handle) = io
                        .fs_open(val, BexExternalValue::String("r".to_string()))
                        .await
                    {
                        if let Ok(contents) = io.fs_file_text(&handle).await {
                            if let Some(pid) = extract_project_id_from_json(&contents) {
                                return Some(pid);
                            }
                        }
                    }
                }
            }
        }
    }

    // Fallback chain for when the credential source didn't have project_id.

    // GOOGLE_CLOUD_PROJECT env var.
    if let Ok(Some(val)) = io.env_get("GOOGLE_CLOUD_PROJECT".to_string()).await {
        if !val.is_empty() && !val.starts_with('$') {
            return Some(val);
        }
    }

    // ADC config file -> quota_project_id.
    if let Some(pid) = project_id_from_adc_config(io).await {
        return Some(pid);
    }

    // GCE metadata server.
    let req = sys_types::generated::owned::http::Request {
        method: "GET".to_string(),
        url: "http://metadata.google.internal/computeMetadata/v1/project/project-id".to_string(),
        headers: indexmap::indexmap! {
            "Metadata-Flavor".to_string() => "Google".to_string(),
        },
        body: String::new(),
    };
    if let Ok(resp) = io.http_send(req).await {
        if resp.status_code == 200 {
            if let Ok(body) = io.http_response_text(&resp).await {
                let pid = body.trim().to_string();
                if !pid.is_empty() {
                    return Some(pid);
                }
            }
        }
    }

    // gcloud CLI (if we haven't already tried it).
    if !matches!(creds, ResolvedCredentials::GcloudCli) {
        if let Ok(output) = io
            .sys_shell(
                "gcloud config get-value project 2>/dev/null".to_string(),
                None,
            )
            .await
        {
            let pid = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !pid.is_empty() {
                return Some(pid);
            }
        }
    }

    None
}

/// Read the ADC config file and extract `quota_project_id` (or `project_id`).
async fn project_id_from_adc_config(io: &dyn RuntimeIo) -> Option<String> {
    let config_dir = match io.env_get("CLOUDSDK_CONFIG".to_string()).await {
        Ok(Some(dir)) if !dir.is_empty() => dir,
        _ => {
            let home = io.env_get("HOME".to_string()).await.ok()??;
            format!("{home}/.config/gcloud")
        }
    };
    let adc_path = format!("{config_dir}/application_default_credentials.json");

    let handle = io
        .fs_open(adc_path, BexExternalValue::String("r".to_string()))
        .await
        .ok()?;
    let contents = io.fs_file_text(&handle).await.ok()?;
    let parsed: serde_json::Value = serde_json::from_str(&contents).ok()?;

    parsed
        .get("quota_project_id")
        .or_else(|| parsed.get("project_id"))
        .and_then(|v| v.as_str())
        .map(String::from)
}

/// Read a credentials file via `RuntimeIo`.
async fn read_credentials_file(
    path: &str,
    io: &dyn RuntimeIo,
) -> Result<String, BuildRequestError> {
    let handle = io
        .fs_open(path.to_string(), BexExternalValue::String("r".to_string()))
        .await
        .map_err(|e| {
            BuildRequestError::AuthorizationFailed(format!(
                "Google Cloud: failed to open credentials file '{path}': {e}"
            ))
        })?;
    io.fs_file_text(&handle).await.map_err(|e| {
        BuildRequestError::AuthorizationFailed(format!(
            "Google Cloud: failed to read credentials file '{path}': {e}"
        ))
    })
}

// ===========================================================================
// Native: google-cloud-auth
// ===========================================================================

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use std::sync::Arc;

    use google_cloud_auth::{credentials::Builder, io};

    use super::{BexExternalValue, BuildRequestError, RuntimeIo, RuntimeIoError};

    // -- IO provider adapters (RuntimeIo -> google-cloud-auth traits) --

    pub(super) struct BexEnvProvider {
        pub io: Arc<dyn RuntimeIo>,
    }

    impl std::fmt::Debug for BexEnvProvider {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("BexEnvProvider").finish()
        }
    }

    impl io::EnvProvider for BexEnvProvider {
        fn var(&self, name: &str) -> Option<String> {
            let io = self.io.clone();
            let key = name.to_string();
            let handle = tokio::runtime::Handle::current();
            let result = std::thread::spawn(move || handle.block_on(io.env_get(key)))
                .join()
                .unwrap_or(Err(RuntimeIoError::Other("thread panicked".into())));
            match result {
                Ok(Some(v)) => Some(v),
                Ok(None) | Err(_) => None,
            }
        }
    }

    pub(super) struct BexFsProvider {
        pub io: Arc<dyn RuntimeIo>,
    }

    impl std::fmt::Debug for BexFsProvider {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("BexFsProvider").finish()
        }
    }

    impl io::FsProvider for BexFsProvider {
        fn read_to_string(&self, path: &str) -> std::io::Result<String> {
            let io = self.io.clone();
            let path = path.to_string();
            let handle = tokio::runtime::Handle::current();
            std::thread::spawn(move || {
                handle.block_on(async {
                    let file_handle = io
                        .fs_open(path, BexExternalValue::String("r".to_string()))
                        .await
                        .map_err(|_| {
                            std::io::Error::new(std::io::ErrorKind::NotFound, "file not found")
                        })?;
                    io.fs_file_text(&file_handle).await.map_err(|_| {
                        std::io::Error::new(std::io::ErrorKind::NotFound, "file not found")
                    })
                })
            })
            .join()
            .unwrap_or(Err(std::io::Error::other("thread panicked")))
        }
    }

    pub(super) struct BexHttpClientProvider {
        pub io: Arc<dyn RuntimeIo>,
    }

    impl std::fmt::Debug for BexHttpClientProvider {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("BexHttpClientProvider").finish()
        }
    }

    impl io::HttpClientProvider for BexHttpClientProvider {
        async fn execute(
            &self,
            request: io::HttpRequest,
        ) -> Result<io::HttpResponse, Box<dyn std::error::Error + Send + Sync>> {
            let method = match request.method {
                io::HttpMethod::Get => "GET",
                io::HttpMethod::Post => "POST",
                io::HttpMethod::Put => "PUT",
            };

            let url = if request.query_params.is_empty() {
                request.url.clone()
            } else {
                let params: Vec<String> = request
                    .query_params
                    .iter()
                    .map(|(k, v)| {
                        format!(
                            "{}={}",
                            percent_encoding::utf8_percent_encode(
                                k,
                                percent_encoding::NON_ALPHANUMERIC
                            ),
                            percent_encoding::utf8_percent_encode(
                                v,
                                percent_encoding::NON_ALPHANUMERIC
                            ),
                        )
                    })
                    .collect();
                if request.url.contains('?') {
                    format!("{}&{}", request.url, params.join("&"))
                } else {
                    format!("{}?{}", request.url, params.join("&"))
                }
            };

            let mut headers = indexmap::IndexMap::new();
            for (name, value) in &request.headers {
                headers.insert(name.clone(), value.clone());
            }

            let io_req = sys_types::generated::owned::http::Request {
                method: method.to_string(),
                url,
                headers,
                // Safe: this adapter is only used for OAuth2 token requests (JSON/form-encoded).
                body: String::from_utf8_lossy(&request.body).into_owned(),
            };

            let resp = self
                .io
                .http_send(io_req)
                .await
                .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { Box::new(e) })?;

            let resp_body = self
                .io
                .http_response_text(&resp)
                .await
                .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { Box::new(e) })?;

            let status =
                io::StatusCode::from_u16(u16::try_from(resp.status_code).unwrap_or(500))
                    .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { Box::new(e) })?;

            let mut response_headers = io::HeaderMap::new();
            for (name, value) in &resp.headers {
                if let (Ok(header_name), Ok(header_value)) = (
                    io::HeaderName::from_bytes(name.as_bytes()),
                    io::HeaderValue::from_str(value),
                ) {
                    response_headers.insert(header_name, header_value);
                }
            }

            Ok(io::HttpResponse {
                status,
                headers: response_headers,
                body: resp_body.into_bytes(),
            })
        }
    }

    // -- Credential builders --

    pub(super) fn build_from_service_account_json(
        json_str: &str,
        io: Arc<dyn RuntimeIo>,
    ) -> Result<google_cloud_auth::credentials::AccessTokenCredentials, BuildRequestError> {
        crate::ensure_rustls_crypto_provider();

        let json_value: serde_json::Value = serde_json::from_str(json_str).map_err(|e| {
            BuildRequestError::AuthorizationFailed(format!(
                "Google Cloud: failed to parse credentials JSON: {e}"
            ))
        })?;

        let builder = google_cloud_auth::credentials::service_account::Builder::new(json_value)
            .with_access_specifier(
                google_cloud_auth::credentials::service_account::AccessSpecifier::from_scopes([
                    "https://www.googleapis.com/auth/cloud-platform",
                ]),
            )
            .with_http_client_provider(BexHttpClientProvider { io });

        builder.build_access_token_credentials().map_err(|e| {
            BuildRequestError::AuthorizationFailed(format!(
                "Google Cloud: failed to build service account credentials: {e}"
            ))
        })
    }

    pub(super) fn build_from_adc(
        io: Arc<dyn RuntimeIo>,
    ) -> Result<google_cloud_auth::credentials::AccessTokenCredentials, BuildRequestError> {
        crate::ensure_rustls_crypto_provider();

        let builder = Builder::default()
            .with_scopes(["https://www.googleapis.com/auth/cloud-platform"])
            .with_http_client_provider(BexHttpClientProvider { io: io.clone() })
            .with_env_provider(BexEnvProvider { io: io.clone() })
            .with_fs_provider(BexFsProvider { io });

        builder.build_access_token_credentials().map_err(|e| {
            BuildRequestError::AuthorizationFailed(format!(
                "Google Cloud ADC: failed to build credentials: {e}"
            ))
        })
    }
}

#[cfg(not(target_arch = "wasm32"))]
async fn token_from_service_account_json(
    json_str: &str,
    io: Arc<dyn RuntimeIo>,
) -> Result<String, BuildRequestError> {
    let creds = native::build_from_service_account_json(json_str, io)?;
    let token = creds.access_token().await.map_err(|e| {
        BuildRequestError::AuthorizationFailed(format!(
            "Google Cloud: failed to obtain access token: {e}"
        ))
    })?;
    Ok(token.token)
}

#[cfg(not(target_arch = "wasm32"))]
async fn token_from_adc(io: Arc<dyn RuntimeIo>) -> Result<String, BuildRequestError> {
    let creds = native::build_from_adc(io)?;
    let token = creds.access_token().await.map_err(|e| {
        BuildRequestError::AuthorizationFailed(format!(
            "Google Cloud ADC: failed to obtain access token: {e}"
        ))
    })?;
    Ok(token.token)
}

// ===========================================================================
// WASM: pure-Rust JWT signing (rsa + sha2) + OAuth2 token exchange
//
// Uses RustCrypto crates instead of SubtleCrypto so it works on any WASM
// host without `window.crypto`, and signing is synchronous.
// ===========================================================================

#[cfg(target_arch = "wasm32")]
mod wasm {
    use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
    use rsa::{RsaPrivateKey, pkcs8::DecodePrivateKey, signature::SignatureEncoding};

    use super::{BuildRequestError, RuntimeIo};

    #[derive(serde::Deserialize)]
    pub(super) struct ServiceAccount {
        pub token_uri: String,
        pub client_email: String,
        pub private_key: String,
        #[allow(dead_code)]
        pub private_key_id: Option<String>,
    }

    /// Parse service account JSON, sign a JWT with `rsa`/`sha2`, and exchange
    /// it for an access token via `RuntimeIo`.
    pub(super) async fn service_account_token(
        json_str: &str,
        io: &dyn RuntimeIo,
    ) -> Result<String, BuildRequestError> {
        let sa: ServiceAccount = serde_json::from_str(json_str).map_err(|e| {
            BuildRequestError::AuthorizationFailed(format!(
                "Google Cloud: failed to parse service account JSON: {e}"
            ))
        })?;

        let jwt = sign_jwt(&sa)?;
        exchange_jwt_for_token(&sa.token_uri, &jwt, io).await
    }

    /// Sign a JWT using RSASSA-PKCS1-v1_5 with SHA-256 (pure Rust).
    #[allow(clippy::items_after_statements)]
    pub(super) fn sign_jwt(sa: &ServiceAccount) -> Result<String, BuildRequestError> {
        #[allow(clippy::cast_possible_wrap)]
        let now = web_time::SystemTime::now()
            .duration_since(web_time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        let header = serde_json::json!({
            "alg": "RS256",
            "typ": "JWT",
        });
        let claims = serde_json::json!({
            "iss": sa.client_email,
            "scope": "https://www.googleapis.com/auth/cloud-platform",
            "aud": sa.token_uri,
            "iat": now,
            "exp": now + 3600,
        });

        let header_b64 = URL_SAFE_NO_PAD.encode(header.to_string());
        let claims_b64 = URL_SAFE_NO_PAD.encode(claims.to_string());
        let signing_input = format!("{header_b64}.{claims_b64}");

        // Parse PEM -> DER -> RsaPrivateKey.
        let private_key = RsaPrivateKey::from_pkcs8_pem(&sa.private_key).map_err(|e| {
            BuildRequestError::AuthorizationFailed(format!(
                "Google Cloud: failed to parse PKCS8 private key: {e}"
            ))
        })?;

        let signing_key = rsa::pkcs1v15::SigningKey::<sha2::Sha256>::new(private_key);

        use rsa::signature::Signer;
        let signature = signing_key.sign(signing_input.as_bytes());
        let sig_b64 = URL_SAFE_NO_PAD.encode(signature.to_bytes());

        Ok(format!("{signing_input}.{sig_b64}"))
    }

    /// Exchange a signed JWT for an access token via the token URI.
    pub(super) async fn exchange_jwt_for_token(
        token_uri: &str,
        jwt: &str,
        io: &dyn RuntimeIo,
    ) -> Result<String, BuildRequestError> {
        let body = format!(
            "grant_type={}&assertion={}",
            percent_encoding::utf8_percent_encode(
                "urn:ietf:params:oauth:grant-type:jwt-bearer",
                percent_encoding::NON_ALPHANUMERIC,
            ),
            percent_encoding::utf8_percent_encode(jwt, percent_encoding::NON_ALPHANUMERIC),
        );

        let mut headers = indexmap::IndexMap::new();
        headers.insert(
            "content-type".to_string(),
            "application/x-www-form-urlencoded".to_string(),
        );

        let req = sys_types::generated::owned::http::Request {
            method: "POST".to_string(),
            url: token_uri.to_string(),
            headers,
            body,
        };

        let resp = io.http_send(req).await.map_err(|e| {
            BuildRequestError::AuthorizationFailed(format!(
                "Google Cloud: token exchange HTTP request failed: {e}"
            ))
        })?;

        let resp_body = io.http_response_text(&resp).await.map_err(|e| {
            BuildRequestError::AuthorizationFailed(format!(
                "Google Cloud: failed to read token exchange response body: {e}"
            ))
        })?;

        if resp.status_code < 200 || resp.status_code >= 300 {
            return Err(BuildRequestError::AuthorizationFailed(format!(
                "Google Cloud: token exchange returned status {}: {}",
                resp.status_code, resp_body,
            )));
        }

        let token_resp: serde_json::Value = serde_json::from_str(&resp_body).map_err(|e| {
            BuildRequestError::AuthorizationFailed(format!(
                "Google Cloud: failed to parse token exchange response: {e}"
            ))
        })?;

        token_resp
            .get("access_token")
            .and_then(|v| v.as_str())
            .map(String::from)
            .ok_or_else(|| {
                BuildRequestError::AuthorizationFailed(
                    "Google Cloud: token exchange response missing 'access_token'".into(),
                )
            })
    }

    #[cfg(test)]
    mod tests {
        use std::{future::Future, pin::Pin, sync::Arc};

        use sys_types::runtime_io::{RuntimeIo, RuntimeIoError};
        use wasm_bindgen_test::wasm_bindgen_test;

        use super::*;

        /// A mock RuntimeIo that returns a configurable token response body.
        struct MockTokenIo {
            status_code: i64,
            body: String,
        }

        impl MockTokenIo {
            fn success() -> Self {
                Self {
                    status_code: 200,
                    body: serde_json::json!({
                        "access_token": "ya29.wasm-test",
                        "token_type": "Bearer",
                        "expires_in": 3600,
                    })
                    .to_string(),
                }
            }
        }

        impl RuntimeIo for MockTokenIo {
            fn http_send(
                &self,
                _request: sys_types::generated::owned::http::Request,
            ) -> Pin<
                Box<
                    dyn Future<
                            Output = Result<
                                sys_types::runtime_io::HttpResponseHandle,
                                RuntimeIoError,
                            >,
                        > + Send
                        + '_,
                >,
            > {
                let status_code = self.status_code;
                Box::pin(async move {
                    Ok(sys_types::runtime_io::HttpResponseHandle {
                        raw: bex_external_types::BexExternalValue::Null,
                        status_code,
                        headers: indexmap::IndexMap::new(),
                        url: String::new(),
                    })
                })
            }

            fn http_response_text(
                &self,
                _: &sys_types::runtime_io::HttpResponseHandle,
            ) -> Pin<Box<dyn Future<Output = Result<String, RuntimeIoError>> + Send + '_>>
            {
                let body = self.body.clone();
                Box::pin(async move { Ok(body) })
            }

            fn env_get(
                &self,
                _key: String,
            ) -> Pin<Box<dyn Future<Output = Result<Option<String>, RuntimeIoError>> + Send + '_>>
            {
                Box::pin(async { Ok(None) })
            }
        }

        /// A mock RuntimeIo that captures HTTP requests and returns a configurable response.
        struct CapturingIo {
            captured: Arc<std::sync::Mutex<Option<sys_types::generated::owned::http::Request>>>,
            status_code: i64,
            body: String,
        }

        impl RuntimeIo for CapturingIo {
            fn http_send(
                &self,
                request: sys_types::generated::owned::http::Request,
            ) -> Pin<
                Box<
                    dyn Future<
                            Output = Result<
                                sys_types::runtime_io::HttpResponseHandle,
                                RuntimeIoError,
                            >,
                        > + Send
                        + '_,
                >,
            > {
                *self.captured.lock().unwrap() = Some(request);
                let status_code = self.status_code;
                Box::pin(async move {
                    Ok(sys_types::runtime_io::HttpResponseHandle {
                        raw: bex_external_types::BexExternalValue::Null,
                        status_code,
                        headers: indexmap::IndexMap::new(),
                        url: String::new(),
                    })
                })
            }

            fn http_response_text(
                &self,
                _: &sys_types::runtime_io::HttpResponseHandle,
            ) -> Pin<Box<dyn Future<Output = Result<String, RuntimeIoError>> + Send + '_>>
            {
                let body = self.body.clone();
                Box::pin(async move { Ok(body) })
            }

            fn env_get(
                &self,
                _key: String,
            ) -> Pin<Box<dyn Future<Output = Result<Option<String>, RuntimeIoError>> + Send + '_>>
            {
                Box::pin(async { Ok(None) })
            }
        }

        fn gen_test_private_key_pem() -> String {
            use rsa::pkcs8::EncodePrivateKey;
            let key = RsaPrivateKey::new(&mut rsa::rand_core::OsRng, 2048).unwrap();
            key.to_pkcs8_pem(rsa::pkcs8::LineEnding::LF)
                .unwrap()
                .to_string()
        }

        fn test_sa_json() -> String {
            let pem = gen_test_private_key_pem();
            serde_json::json!({
                "type": "service_account",
                "project_id": "test-project",
                "private_key_id": "key-id-123",
                "private_key": pem,
                "client_email": "test@test-project.iam.gserviceaccount.com",
                "client_id": "123456789",
                "auth_uri": "https://accounts.google.com/o/oauth2/auth",
                "token_uri": "https://oauth2.googleapis.com/token",
            })
            .to_string()
        }

        #[wasm_bindgen_test]
        fn sign_jwt_produces_valid_three_part_token() {
            let sa: ServiceAccount = serde_json::from_str(&test_sa_json()).unwrap();
            let jwt = sign_jwt(&sa).unwrap();
            let parts: Vec<&str> = jwt.split('.').collect();
            assert_eq!(parts.len(), 3, "JWT should have header.claims.sig");
        }

        #[wasm_bindgen_test]
        fn sign_jwt_header_is_rs256() {
            use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};

            let sa: ServiceAccount = serde_json::from_str(&test_sa_json()).unwrap();
            let jwt = sign_jwt(&sa).unwrap();
            let header_b64 = jwt.split('.').next().unwrap();
            let header: serde_json::Value =
                serde_json::from_slice(&URL_SAFE_NO_PAD.decode(header_b64).unwrap()).unwrap();
            assert_eq!(header["alg"], "RS256");
            assert_eq!(header["typ"], "JWT");
        }

        #[wasm_bindgen_test]
        fn sign_jwt_claims_have_expected_fields() {
            use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};

            let sa: ServiceAccount = serde_json::from_str(&test_sa_json()).unwrap();
            let jwt = sign_jwt(&sa).unwrap();
            let claims_b64 = jwt.split('.').nth(1).unwrap();
            let claims: serde_json::Value =
                serde_json::from_slice(&URL_SAFE_NO_PAD.decode(claims_b64).unwrap()).unwrap();
            assert_eq!(claims["iss"], "test@test-project.iam.gserviceaccount.com");
            assert_eq!(
                claims["scope"],
                "https://www.googleapis.com/auth/cloud-platform"
            );
            assert_eq!(claims["aud"], "https://oauth2.googleapis.com/token");
            assert!(claims["iat"].is_number());
            assert!(claims["exp"].is_number());
        }

        #[wasm_bindgen_test]
        fn sign_jwt_signature_verifies() {
            use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
            use rsa::signature::Verifier;

            let sa: ServiceAccount = serde_json::from_str(&test_sa_json()).unwrap();
            let jwt = sign_jwt(&sa).unwrap();
            let parts: Vec<&str> = jwt.split('.').collect();
            let signing_input = format!("{}.{}", parts[0], parts[1]);
            let sig_bytes = URL_SAFE_NO_PAD.decode(parts[2]).unwrap();

            let private_key = RsaPrivateKey::from_pkcs8_pem(&sa.private_key).unwrap();
            let public_key = private_key.to_public_key();
            let verifying_key = rsa::pkcs1v15::VerifyingKey::<sha2::Sha256>::new(public_key);
            let signature = rsa::pkcs1v15::Signature::try_from(sig_bytes.as_slice()).unwrap();
            verifying_key
                .verify(signing_input.as_bytes(), &signature)
                .expect("JWT signature should verify");
        }

        #[wasm_bindgen_test]
        fn sign_jwt_rejects_invalid_pem() {
            let sa = ServiceAccount {
                token_uri: "https://oauth2.googleapis.com/token".into(),
                client_email: "test@test.iam.gserviceaccount.com".into(),
                private_key: "not-a-real-pem".into(),
                private_key_id: None,
            };
            assert!(sign_jwt(&sa).is_err());
        }

        #[wasm_bindgen_test]
        async fn exchange_jwt_rejects_non_200() {
            let mock_io = MockTokenIo {
                status_code: 401,
                body: r#"{"error": "invalid_grant"}"#.to_string(),
            };
            let result = exchange_jwt_for_token(
                "https://oauth2.googleapis.com/token",
                "fake.jwt.here",
                &mock_io,
            )
            .await;
            let err = result.unwrap_err().to_string();
            assert!(err.contains("401"), "should mention status: {err}");
        }

        #[wasm_bindgen_test]
        async fn exchange_jwt_rejects_missing_access_token() {
            let mock_io = MockTokenIo {
                status_code: 200,
                body: r#"{"token_type": "Bearer"}"#.to_string(),
            };
            let result = exchange_jwt_for_token(
                "https://oauth2.googleapis.com/token",
                "fake.jwt.here",
                &mock_io,
            )
            .await;
            let err = result.unwrap_err().to_string();
            assert!(
                err.contains("access_token"),
                "should mention missing field: {err}"
            );
        }

        #[wasm_bindgen_test]
        async fn exchange_jwt_sends_correct_request() {
            let captured = Arc::new(std::sync::Mutex::new(None));
            let mock_io = CapturingIo {
                captured: captured.clone(),
                status_code: 200,
                body: serde_json::json!({
                    "access_token": "ya29.test",
                    "token_type": "Bearer",
                    "expires_in": 3600,
                })
                .to_string(),
            };

            let token = exchange_jwt_for_token(
                "https://oauth2.googleapis.com/token",
                "my.test.jwt",
                &mock_io,
            )
            .await
            .unwrap();
            assert_eq!(token, "ya29.test");

            let req = captured.lock().unwrap().take().unwrap();
            assert_eq!(req.method, "POST");
            assert_eq!(req.url, "https://oauth2.googleapis.com/token");
            assert_eq!(
                req.headers.get("content-type").unwrap(),
                "application/x-www-form-urlencoded"
            );
            assert!(req.body.contains("grant_type="), "body: {}", req.body);
            assert!(req.body.contains("assertion="), "body: {}", req.body);
        }

        #[wasm_bindgen_test]
        async fn service_account_token_full_flow() {
            let captured = Arc::new(std::sync::Mutex::new(None));
            let mock_io = CapturingIo {
                captured: captured.clone(),
                status_code: 200,
                body: serde_json::json!({
                    "access_token": "ya29.wasm-full-flow",
                    "token_type": "Bearer",
                    "expires_in": 3600,
                })
                .to_string(),
            };

            let token = service_account_token(&test_sa_json(), &mock_io)
                .await
                .unwrap();
            assert_eq!(token, "ya29.wasm-full-flow");

            // Verify the HTTP request contained a real signed JWT
            let req = captured.lock().unwrap().take().unwrap();
            assert_eq!(req.method, "POST");

            // Extract the assertion (JWT) from the URL-encoded body
            let assertion = req
                .body
                .split('&')
                .find(|p| p.starts_with("assertion="))
                .unwrap()
                .strip_prefix("assertion=")
                .unwrap();
            let jwt = percent_encoding::percent_decode_str(assertion)
                .decode_utf8()
                .unwrap();
            let parts: Vec<&str> = jwt.split('.').collect();
            assert_eq!(parts.len(), 3, "assertion should be a 3-part JWT");
        }

        #[wasm_bindgen_test]
        async fn service_account_token_rejects_bad_json() {
            let mock_io = MockTokenIo::success();
            let result = service_account_token("not json", &mock_io).await;
            assert!(result.is_err());
        }
    }
}

#[cfg(target_arch = "wasm32")]
async fn token_from_service_account_json(
    json_str: &str,
    io: Arc<dyn RuntimeIo>,
) -> Result<String, BuildRequestError> {
    wasm::service_account_token(json_str, &*io).await
}

#[cfg(target_arch = "wasm32")]
#[allow(clippy::unused_async)] // must be async to match native signature
async fn token_from_adc(_io: Arc<dyn RuntimeIo>) -> Result<String, BuildRequestError> {
    Err(BuildRequestError::AuthorizationFailed(
        "Google Cloud ADC is not supported on WASM. \
         Provide explicit credentials via 'credentials' or 'credentials_content'."
            .into(),
    ))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use std::{future::Future, pin::Pin, sync::Arc};

    use indexmap::IndexMap;

    use super::*;
    use crate::baml_std::PrimitiveClientOptions;

    /// A stub `RuntimeIo` that returns sensible defaults for Vertex-relevant
    /// operations and `Unsupported` for everything else.
    struct StubIo {
        token_body: String,
    }

    impl StubIo {
        fn with_token(token: &str) -> Self {
            Self {
                token_body: serde_json::json!({
                    "access_token": token,
                    "token_type": "Bearer",
                    "expires_in": 3600,
                })
                .to_string(),
            }
        }
    }

    impl RuntimeIo for StubIo {
        fn http_send(
            &self,
            _request: sys_types::generated::owned::http::Request,
        ) -> Pin<
            Box<
                dyn Future<
                        Output = Result<sys_types::runtime_io::HttpResponseHandle, RuntimeIoError>,
                    > + Send
                    + '_,
            >,
        > {
            Box::pin(async {
                Ok(sys_types::runtime_io::HttpResponseHandle {
                    raw: bex_external_types::BexExternalValue::Null,
                    status_code: 200,
                    headers: IndexMap::new(),
                    url: String::new(),
                })
            })
        }

        fn http_response_text(
            &self,
            _: &sys_types::runtime_io::HttpResponseHandle,
        ) -> Pin<Box<dyn Future<Output = Result<String, RuntimeIoError>> + Send + '_>> {
            let body = self.token_body.clone();
            Box::pin(async move { Ok(body) })
        }

        fn env_get(
            &self,
            _key: String,
        ) -> Pin<Box<dyn Future<Output = Result<Option<String>, RuntimeIoError>> + Send + '_>>
        {
            Box::pin(async { Ok(None) })
        }

        fn fs_open(
            &self,
            _path: String,
            _mode: BexExternalValue,
        ) -> Pin<
            Box<
                dyn Future<Output = Result<sys_types::runtime_io::FsFileHandle, RuntimeIoError>>
                    + Send
                    + '_,
            >,
        > {
            Box::pin(async { Err(RuntimeIoError::Other("not found".into())) })
        }

        fn fs_file_text(
            &self,
            _: &sys_types::runtime_io::FsFileHandle,
        ) -> Pin<Box<dyn Future<Output = Result<String, RuntimeIoError>> + Send + '_>> {
            Box::pin(async { Err(RuntimeIoError::Other("not found".into())) })
        }

        fn sys_shell(
            &self,
            _: String,
            _options: Option<sys_types::generated::owned::sys::ProcessOptions>,
        ) -> Pin<
            Box<
                dyn Future<
                        Output = Result<
                            sys_types::generated::owned::sys::ShellOutput,
                            RuntimeIoError,
                        >,
                    > + Send
                    + '_,
            >,
        > {
            Box::pin(async { Err(RuntimeIoError::Other("unsupported".into())) })
        }
    }

    /// A mock `RuntimeIo` that serves credentials from a file and returns a
    /// configurable token response.
    struct FsIo {
        expected_path: String,
        file_contents: String,
        token_body: String,
    }

    impl RuntimeIo for FsIo {
        fn http_send(
            &self,
            _request: sys_types::generated::owned::http::Request,
        ) -> Pin<
            Box<
                dyn Future<
                        Output = Result<sys_types::runtime_io::HttpResponseHandle, RuntimeIoError>,
                    > + Send
                    + '_,
            >,
        > {
            Box::pin(async {
                Ok(sys_types::runtime_io::HttpResponseHandle {
                    raw: bex_external_types::BexExternalValue::Null,
                    status_code: 200,
                    headers: IndexMap::new(),
                    url: String::new(),
                })
            })
        }

        fn http_response_text(
            &self,
            _: &sys_types::runtime_io::HttpResponseHandle,
        ) -> Pin<Box<dyn Future<Output = Result<String, RuntimeIoError>> + Send + '_>> {
            let body = self.token_body.clone();
            Box::pin(async move { Ok(body) })
        }

        fn env_get(
            &self,
            _key: String,
        ) -> Pin<Box<dyn Future<Output = Result<Option<String>, RuntimeIoError>> + Send + '_>>
        {
            Box::pin(async { Ok(None) })
        }

        fn fs_open(
            &self,
            path: String,
            _mode: BexExternalValue,
        ) -> Pin<
            Box<
                dyn Future<Output = Result<sys_types::runtime_io::FsFileHandle, RuntimeIoError>>
                    + Send
                    + '_,
            >,
        > {
            let expected = self.expected_path.clone();
            Box::pin(async move {
                if path == expected {
                    Ok(sys_types::runtime_io::FsFileHandle {
                        raw: bex_external_types::BexExternalValue::Null,
                    })
                } else {
                    Err(RuntimeIoError::Other("not found".into()))
                }
            })
        }

        fn fs_file_text(
            &self,
            _: &sys_types::runtime_io::FsFileHandle,
        ) -> Pin<Box<dyn Future<Output = Result<String, RuntimeIoError>> + Send + '_>> {
            let contents = self.file_contents.clone();
            Box::pin(async move { Ok(contents) })
        }

        fn sys_shell(
            &self,
            _: String,
            _options: Option<sys_types::generated::owned::sys::ProcessOptions>,
        ) -> Pin<
            Box<
                dyn Future<
                        Output = Result<
                            sys_types::generated::owned::sys::ShellOutput,
                            RuntimeIoError,
                        >,
                    > + Send
                    + '_,
            >,
        > {
            Box::pin(async { Err(RuntimeIoError::Other("unsupported".into())) })
        }
    }

    /// A mock `RuntimeIo` for testing ADC with env vars pointing to credential files.
    struct AdcIo {
        http_call_count: Arc<std::sync::atomic::AtomicUsize>,
        env_vars: std::collections::HashMap<String, String>,
        files: std::collections::HashMap<String, String>,
        token_body: String,
    }

    impl RuntimeIo for AdcIo {
        fn http_send(
            &self,
            _request: sys_types::generated::owned::http::Request,
        ) -> Pin<
            Box<
                dyn Future<
                        Output = Result<sys_types::runtime_io::HttpResponseHandle, RuntimeIoError>,
                    > + Send
                    + '_,
            >,
        > {
            self.http_call_count
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Box::pin(async {
                Ok(sys_types::runtime_io::HttpResponseHandle {
                    raw: bex_external_types::BexExternalValue::Null,
                    status_code: 200,
                    headers: IndexMap::new(),
                    url: String::new(),
                })
            })
        }

        fn http_response_text(
            &self,
            _: &sys_types::runtime_io::HttpResponseHandle,
        ) -> Pin<Box<dyn Future<Output = Result<String, RuntimeIoError>> + Send + '_>> {
            let body = self.token_body.clone();
            Box::pin(async move { Ok(body) })
        }

        fn env_get(
            &self,
            key: String,
        ) -> Pin<Box<dyn Future<Output = Result<Option<String>, RuntimeIoError>> + Send + '_>>
        {
            let val = self.env_vars.get(&key).cloned();
            Box::pin(async move { Ok(val) })
        }

        fn fs_open(
            &self,
            path: String,
            _mode: BexExternalValue,
        ) -> Pin<
            Box<
                dyn Future<Output = Result<sys_types::runtime_io::FsFileHandle, RuntimeIoError>>
                    + Send
                    + '_,
            >,
        > {
            let exists = self.files.contains_key(&path);
            Box::pin(async move {
                if exists {
                    Ok(sys_types::runtime_io::FsFileHandle {
                        raw: bex_external_types::BexExternalValue::String(path),
                    })
                } else {
                    Err(RuntimeIoError::Other("not found".into()))
                }
            })
        }

        fn fs_file_text(
            &self,
            handle: &sys_types::runtime_io::FsFileHandle,
        ) -> Pin<Box<dyn Future<Output = Result<String, RuntimeIoError>> + Send + '_>> {
            // Extract the path from the handle's raw value.
            let path = match &handle.raw {
                bex_external_types::BexExternalValue::String(s) => s.clone(),
                _ => return Box::pin(async { Err(RuntimeIoError::Other("bad handle".into())) }),
            };
            let contents = self.files.get(&path).cloned();
            Box::pin(
                async move { contents.ok_or_else(|| RuntimeIoError::Other("not found".into())) },
            )
        }

        fn sys_shell(
            &self,
            _: String,
            _options: Option<sys_types::generated::owned::sys::ProcessOptions>,
        ) -> Pin<
            Box<
                dyn Future<
                        Output = Result<
                            sys_types::generated::owned::sys::ShellOutput,
                            RuntimeIoError,
                        >,
                    > + Send
                    + '_,
            >,
        > {
            Box::pin(async { Err(RuntimeIoError::Other("unsupported".into())) })
        }
    }

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    fn make_client(provider: &str) -> PrimitiveClient {
        PrimitiveClient::new(
            "test-google".to_string(),
            provider.to_string(),
            PrimitiveClientOptions {
                model: Some("gemini-pro".to_string()),
                ..Default::default()
            },
        )
        .unwrap()
    }

    fn fake_request() -> HttpRequest {
        HttpRequest {
            method: "POST".to_string(),
            url: "https://us-central1-aiplatform.googleapis.com/v1/projects/test/locations/us-central1/publishers/google/models/gemini-pro:generateContent".to_string(),
            headers: indexmap::IndexMap::new(),
            body: r#"{"contents":[]}"#.to_string(),
        }
    }

    fn gen_test_private_key_pem() -> String {
        use rsa::{RsaPrivateKey, pkcs8::EncodePrivateKey};
        let key = RsaPrivateKey::new(&mut rsa::rand_core::OsRng, 2048).unwrap();
        key.to_pkcs8_pem(rsa::pkcs8::LineEnding::LF)
            .unwrap()
            .to_string()
    }

    fn test_service_account_json() -> String {
        let pem = gen_test_private_key_pem();
        serde_json::json!({
            "type": "service_account",
            "project_id": "test-project",
            "private_key_id": "key-id-123",
            "private_key": pem,
            "client_email": "test@test-project.iam.gserviceaccount.com",
            "client_id": "123456789",
            "auth_uri": "https://accounts.google.com/o/oauth2/auth",
            "token_uri": "https://oauth2.googleapis.com/token",
        })
        .to_string()
    }

    fn make_client_with_vertex_opts(opts: VertexAiOptions) -> PrimitiveClient {
        use bex_external_types::AsBexExternalValue;
        PrimitiveClient::new(
            "test-google".to_string(),
            "vertex-ai".to_string(),
            PrimitiveClientOptions {
                model: Some("gemini-pro".to_string()),
                provider_options: opts.into_bex_external_value(),
                ..Default::default()
            },
        )
        .unwrap()
    }

    fn assert_bearer_token(req: &HttpRequest) {
        let auth = req
            .headers
            .get("authorization")
            .expect("missing authorization header");
        assert!(
            auth.starts_with("Bearer "),
            "expected Bearer token, got: {auth}"
        );
    }

    // -----------------------------------------------------------------------
    // Tests
    // -----------------------------------------------------------------------

    /// A mock `RuntimeIo` where all operations fail, simulating an environment
    /// with no credentials at all.
    struct NoCredsIo;

    impl RuntimeIo for NoCredsIo {
        fn http_send(
            &self,
            _request: sys_types::generated::owned::http::Request,
        ) -> Pin<
            Box<
                dyn Future<
                        Output = Result<sys_types::runtime_io::HttpResponseHandle, RuntimeIoError>,
                    > + Send
                    + '_,
            >,
        > {
            Box::pin(async {
                Ok(sys_types::runtime_io::HttpResponseHandle {
                    raw: bex_external_types::BexExternalValue::Null,
                    status_code: 404,
                    headers: IndexMap::new(),
                    url: String::new(),
                })
            })
        }

        fn http_response_text(
            &self,
            _: &sys_types::runtime_io::HttpResponseHandle,
        ) -> Pin<Box<dyn Future<Output = Result<String, RuntimeIoError>> + Send + '_>> {
            Box::pin(async { Ok(String::new()) })
        }

        fn env_get(
            &self,
            _key: String,
        ) -> Pin<Box<dyn Future<Output = Result<Option<String>, RuntimeIoError>> + Send + '_>>
        {
            Box::pin(async { Ok(None) })
        }

        fn fs_open(
            &self,
            _path: String,
            _mode: BexExternalValue,
        ) -> Pin<
            Box<
                dyn Future<Output = Result<sys_types::runtime_io::FsFileHandle, RuntimeIoError>>
                    + Send
                    + '_,
            >,
        > {
            Box::pin(async { Err(RuntimeIoError::Other("not found".into())) })
        }

        fn fs_file_text(
            &self,
            _: &sys_types::runtime_io::FsFileHandle,
        ) -> Pin<Box<dyn Future<Output = Result<String, RuntimeIoError>> + Send + '_>> {
            Box::pin(async { Err(RuntimeIoError::Other("not found".into())) })
        }

        fn sys_shell(
            &self,
            _: String,
            _options: Option<sys_types::generated::owned::sys::ProcessOptions>,
        ) -> Pin<
            Box<
                dyn Future<
                        Output = Result<
                            sys_types::generated::owned::sys::ShellOutput,
                            RuntimeIoError,
                        >,
                    > + Send
                    + '_,
            >,
        > {
            Box::pin(async { Err(RuntimeIoError::Other("unsupported".into())) })
        }
    }

    #[tokio::test]
    async fn fails_without_credentials() {
        let io = NoCredsIo;
        let client = make_client("vertex-ai");
        let mut req = fake_request();
        let result = auth_vertex(&mut req, &client, Arc::new(io)).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn credentials_content_inline_json() {
        let client = make_client_with_vertex_opts(VertexAiOptions {
            credentials_content: Some(test_service_account_json()),
            credentials: None,
            location: None,
            project_id: None,
        });
        let io = StubIo::with_token("ya29.from-service-account");
        let mut req = fake_request();
        auth_vertex(&mut req, &client, Arc::new(io)).await.unwrap();
        assert_bearer_token(&req);
    }

    #[tokio::test]
    async fn credentials_inline_json() {
        let client = make_client_with_vertex_opts(VertexAiOptions {
            credentials: Some(test_service_account_json()),
            credentials_content: None,
            location: None,
            project_id: None,
        });
        let io = StubIo::with_token("ya29.from-service-account");
        let mut req = fake_request();
        auth_vertex(&mut req, &client, Arc::new(io)).await.unwrap();
        assert_bearer_token(&req);
    }

    #[tokio::test]
    async fn credentials_file_path() {
        let sa_json = test_service_account_json();
        let client = make_client_with_vertex_opts(VertexAiOptions {
            credentials: Some("/fake/service-account.json".to_string()),
            credentials_content: None,
            location: None,
            project_id: None,
        });
        let io = FsIo {
            expected_path: "/fake/service-account.json".to_string(),
            file_contents: sa_json,
            token_body: serde_json::json!({
                "access_token": "ya29.from-service-account",
                "token_type": "Bearer",
                "expires_in": 3600,
            })
            .to_string(),
        };
        let mut req = fake_request();
        auth_vertex(&mut req, &client, Arc::new(io)).await.unwrap();
        assert_bearer_token(&req);
    }

    #[tokio::test]
    async fn credentials_content_takes_precedence() {
        let client = make_client_with_vertex_opts(VertexAiOptions {
            credentials_content: Some(test_service_account_json()),
            credentials: Some("/should/not/be/read.json".to_string()),
            location: None,
            project_id: None,
        });
        let io = StubIo::with_token("ya29.from-service-account");
        let mut req = fake_request();
        auth_vertex(&mut req, &client, Arc::new(io)).await.unwrap();
        assert_bearer_token(&req);
    }

    #[tokio::test]
    async fn skips_auth_when_key_query_param_present() {
        let client = PrimitiveClient::new(
            "test-google".to_string(),
            "vertex-ai".to_string(),
            PrimitiveClientOptions {
                model: Some("gemini-pro".to_string()),
                query_params: indexmap::IndexMap::from([(
                    "key".to_string(),
                    "my-api-key".to_string(),
                )]),
                ..Default::default()
            },
        )
        .unwrap();
        let io = StubIo::with_token("");
        let mut req = fake_request();
        auth_vertex(&mut req, &client, Arc::new(io)).await.unwrap();
        assert!(!req.headers.contains_key("authorization"));
    }

    /// Confirms that the injected env/fs/http providers are actually used
    /// by the google-cloud-auth ADC flow.
    #[tokio::test]
    async fn adc_with_injected_providers() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        let http_call_count = Arc::new(AtomicUsize::new(0));

        let adc_json = serde_json::json!({
            "client_id": "test-client-id",
            "client_secret": "test-client-secret",
            "refresh_token": "test-refresh-token",
            "type": "authorized_user",
            "token_uri": "https://fake-oauth.example.com/token",
        })
        .to_string();

        let io = AdcIo {
            http_call_count: http_call_count.clone(),
            env_vars: std::collections::HashMap::from([(
                "GOOGLE_APPLICATION_CREDENTIALS".to_string(),
                "/fake/adc.json".to_string(),
            )]),
            files: std::collections::HashMap::from([("/fake/adc.json".to_string(), adc_json)]),
            token_body: serde_json::json!({
                "access_token": "ya29.fake-test-token",
                "token_type": "Bearer",
                "expires_in": 3600,
            })
            .to_string(),
        };

        let client = make_client("vertex-ai");
        let mut req = fake_request();
        auth_vertex(&mut req, &client, Arc::new(io)).await.unwrap();

        assert!(http_call_count.load(Ordering::SeqCst) > 0);
        assert_eq!(
            req.headers.get("authorization").unwrap(),
            "Bearer ya29.fake-test-token",
        );
    }
}
