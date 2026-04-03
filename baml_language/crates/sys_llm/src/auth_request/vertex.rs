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

use crate::{
    BuildRequestCallbacks,
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
    callbacks: &BuildRequestCallbacks,
) -> Result<(), BuildRequestError> {
    // If an API key is provided as a query param, skip token-based auth.
    if client.options.query_params.contains_key("key") {
        return Ok(());
    }

    let vertex_opts = match &client.provider_options {
        Some(ProviderOptions::VertexAi(opts)) => Some(opts.clone()),
        _ => None,
    };

    // Resolve credentials once.
    let creds = resolve_credentials(vertex_opts.as_ref(), callbacks).await?;

    // Resolve project_id placeholder in the URL if needed.
    if request
        .url
        .contains(crate::build_request::google::VERTEX_PROJECT_ID_PLACEHOLDER)
    {
        let project_id = project_id_from_credentials(&creds, callbacks)
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

    let token = token_from_credentials(&creds, callbacks).await?;

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
    callbacks: &BuildRequestCallbacks,
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
        let json_str = read_credentials_file(creds, callbacks).await?;
        return Ok(ResolvedCredentials::ServiceAccountJson(json_str));
    }

    // 3. GOOGLE_APPLICATION_CREDENTIALS env var.
    // Inline JSON is handled here; file paths are deferred to ADC (step 5).
    if let Ok(Some(val)) = (callbacks.env_read)("GOOGLE_APPLICATION_CREDENTIALS".to_string()).await
    {
        let val = try_unwrap_quoted_json(val);
        if !val.is_empty() && serde_json::from_str::<serde_json::Value>(&val).is_ok() {
            return Ok(ResolvedCredentials::ServiceAccountJson(val));
        }
    }

    // 4. GOOGLE_APPLICATION_CREDENTIALS_CONTENT env var (BAML-specific).
    if let Ok(Some(val)) =
        (callbacks.env_read)("GOOGLE_APPLICATION_CREDENTIALS_CONTENT".to_string()).await
    {
        let val = try_unwrap_quoted_json(val);
        if !val.is_empty() && serde_json::from_str::<serde_json::Value>(&val).is_ok() {
            return Ok(ResolvedCredentials::ServiceAccountJson(val));
        }
    }

    // 5. ADC via google-cloud-auth (native only).
    #[cfg(not(target_arch = "wasm32"))]
    if native::build_from_adc(callbacks).is_ok() {
        return Ok(ResolvedCredentials::Adc);
    }

    // 6. gcloud CLI.
    if (callbacks.shell)("gcloud auth print-access-token --quiet 2>/dev/null".to_string())
        .await
        .is_ok_and(|out| !out.trim().is_empty())
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
    callbacks: &BuildRequestCallbacks,
) -> Result<String, BuildRequestError> {
    match creds {
        ResolvedCredentials::ServiceAccountJson(json_str) => {
            token_from_service_account_json(json_str, callbacks).await
        }
        ResolvedCredentials::Adc => token_from_adc(callbacks).await,
        ResolvedCredentials::GcloudCli => {
            let output = (callbacks.shell)("gcloud auth print-access-token --quiet".to_string())
                .await
                .map_err(|e| {
                    BuildRequestError::AuthorizationFailed(format!(
                        "Google Cloud: gcloud auth print-access-token failed: {e}"
                    ))
                })?;
            let token = output.trim().to_string();
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
    callbacks: &BuildRequestCallbacks,
) -> Option<String> {
    // Try the credential source itself first.
    match creds {
        ResolvedCredentials::ServiceAccountJson(json_str) => {
            if let Some(pid) = extract_project_id_from_json(json_str) {
                return Some(pid);
            }
        }
        ResolvedCredentials::GcloudCli => {
            if let Ok(output) =
                (callbacks.shell)("gcloud config get-value project 2>/dev/null".to_string()).await
            {
                let pid = output.trim().to_string();
                if !pid.is_empty() {
                    return Some(pid);
                }
            }
        }
        ResolvedCredentials::Adc => {
            // ADC was resolved by google-cloud-auth, which doesn't expose
            // project_id. Try reading the credentials file it used.
            if let Ok(Some(val)) =
                (callbacks.env_read)("GOOGLE_APPLICATION_CREDENTIALS".to_string()).await
            {
                if !val.is_empty() {
                    // Inline JSON?
                    if let Some(pid) = extract_project_id_from_json(&val) {
                        return Some(pid);
                    }
                    // File path? Read and extract.
                    if let Ok(bytes) = (callbacks.fs_read)(val).await {
                        if let Ok(contents) = std::str::from_utf8(&bytes) {
                            if let Some(pid) = extract_project_id_from_json(contents) {
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
    if let Ok(Some(val)) = (callbacks.env_read)("GOOGLE_CLOUD_PROJECT".to_string()).await {
        if !val.is_empty() && !val.starts_with('$') {
            return Some(val);
        }
    }

    // ADC config file -> quota_project_id.
    if let Some(pid) = project_id_from_adc_config(callbacks).await {
        return Some(pid);
    }

    // GCE metadata server.
    let req = HttpRequest {
        method: "GET".to_string(),
        url: "http://metadata.google.internal/computeMetadata/v1/project/project-id".to_string(),
        headers: indexmap::indexmap! {
            "Metadata-Flavor".to_string() => "Google".to_string(),
        },
        body: String::new(),
    };
    if let Ok(resp) = (callbacks.http_send)(req).await {
        if resp.status_code == 200 {
            let pid = resp.body.trim().to_string();
            if !pid.is_empty() {
                return Some(pid);
            }
        }
    }

    // gcloud CLI (if we haven't already tried it).
    if !matches!(creds, ResolvedCredentials::GcloudCli) {
        if let Ok(output) =
            (callbacks.shell)("gcloud config get-value project 2>/dev/null".to_string()).await
        {
            let pid = output.trim().to_string();
            if !pid.is_empty() {
                return Some(pid);
            }
        }
    }

    None
}

/// Read the ADC config file and extract `quota_project_id` (or `project_id`).
async fn project_id_from_adc_config(callbacks: &BuildRequestCallbacks) -> Option<String> {
    let config_dir = match (callbacks.env_read)("CLOUDSDK_CONFIG".to_string()).await {
        Ok(Some(dir)) if !dir.is_empty() => dir,
        _ => {
            let home = (callbacks.env_read)("HOME".to_string()).await.ok()??;
            format!("{home}/.config/gcloud")
        }
    };
    let adc_path = format!("{config_dir}/application_default_credentials.json");

    let bytes = (callbacks.fs_read)(adc_path).await.ok()?;
    let contents = std::str::from_utf8(&bytes).ok()?;
    let parsed: serde_json::Value = serde_json::from_str(contents).ok()?;

    parsed
        .get("quota_project_id")
        .or_else(|| parsed.get("project_id"))
        .and_then(|v| v.as_str())
        .map(String::from)
}

/// Read a credentials file via the `FsReadFn` callback.
async fn read_credentials_file(
    path: &str,
    callbacks: &BuildRequestCallbacks,
) -> Result<String, BuildRequestError> {
    let bytes = (callbacks.fs_read)(path.to_string()).await.map_err(|e| {
        BuildRequestError::AuthorizationFailed(format!(
            "Google Cloud: failed to read credentials file '{path}': {e}"
        ))
    })?;
    String::from_utf8(bytes).map_err(|e| {
        BuildRequestError::AuthorizationFailed(format!(
            "Google Cloud: credentials file '{path}' is not valid UTF-8: {e}"
        ))
    })
}

// ===========================================================================
// Native: google-cloud-auth
// ===========================================================================

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use google_cloud_auth::{credentials::Builder, io};

    use super::{BuildRequestCallbacks, BuildRequestError, HttpRequest};

    // -- IO provider adapters (callbacks -> google-cloud-auth traits) --

    pub(super) struct BexEnvProvider {
        pub env_read_fn: crate::EnvReadFn,
    }

    impl std::fmt::Debug for BexEnvProvider {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("BexEnvProvider").finish()
        }
    }

    impl std::panic::UnwindSafe for BexEnvProvider {}
    impl std::panic::RefUnwindSafe for BexEnvProvider {}

    impl io::EnvProvider for BexEnvProvider {
        fn var(&self, name: &str) -> Option<String> {
            let fut = (self.env_read_fn)(name.to_string());
            let handle = tokio::runtime::Handle::current();
            let result = std::thread::spawn(move || handle.block_on(fut))
                .join()
                .unwrap_or(Err(crate::LlmOpError::Other("thread panicked".into())));
            match result {
                Ok(Some(v)) => Some(v),
                Ok(None) | Err(_) => None,
            }
        }
    }

    pub(super) struct BexFsProvider {
        pub fs_read_fn: crate::FsReadFn,
    }

    impl std::fmt::Debug for BexFsProvider {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("BexFsProvider").finish()
        }
    }

    impl std::panic::UnwindSafe for BexFsProvider {}
    impl std::panic::RefUnwindSafe for BexFsProvider {}

    impl io::FsProvider for BexFsProvider {
        fn read_to_string(&self, path: &str) -> std::io::Result<String> {
            let fut = (self.fs_read_fn)(path.to_string());
            let handle = tokio::runtime::Handle::current();
            let result = std::thread::spawn(move || handle.block_on(fut))
                .join()
                .unwrap_or(Err(crate::LlmOpError::Other("thread panicked".into())));
            match result {
                Ok(bytes) => String::from_utf8(bytes)
                    .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e)),
                Err(_) => Err(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "file not found",
                )),
            }
        }
    }

    pub(super) struct BexHttpClientProvider {
        pub send_fn: crate::HttpSendFn,
    }

    impl std::fmt::Debug for BexHttpClientProvider {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("BexHttpClientProvider").finish()
        }
    }

    impl std::panic::UnwindSafe for BexHttpClientProvider {}
    impl std::panic::RefUnwindSafe for BexHttpClientProvider {}

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

            let baml_req = HttpRequest {
                method: method.to_string(),
                url,
                headers,
                // Safe: this adapter is only used for OAuth2 token requests (JSON/form-encoded).
                body: String::from_utf8_lossy(&request.body).into_owned(),
            };

            let resp = (self.send_fn)(baml_req)
                .await
                .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { Box::new(e) })?;

            let status = io::StatusCode::from_u16(resp.status_code)
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
                body: resp.body.into_bytes(),
            })
        }
    }

    // -- Credential builders --

    pub(super) fn build_from_service_account_json(
        json_str: &str,
        callbacks: &BuildRequestCallbacks,
    ) -> Result<google_cloud_auth::credentials::AccessTokenCredentials, BuildRequestError> {
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
            .with_http_client_provider(BexHttpClientProvider {
                send_fn: callbacks.http_send.clone(),
            });

        builder.build_access_token_credentials().map_err(|e| {
            BuildRequestError::AuthorizationFailed(format!(
                "Google Cloud: failed to build service account credentials: {e}"
            ))
        })
    }

    pub(super) fn build_from_adc(
        callbacks: &BuildRequestCallbacks,
    ) -> Result<google_cloud_auth::credentials::AccessTokenCredentials, BuildRequestError> {
        let builder = Builder::default()
            .with_scopes(["https://www.googleapis.com/auth/cloud-platform"])
            .with_http_client_provider(BexHttpClientProvider {
                send_fn: callbacks.http_send.clone(),
            })
            .with_env_provider(BexEnvProvider {
                env_read_fn: callbacks.env_read.clone(),
            })
            .with_fs_provider(BexFsProvider {
                fs_read_fn: callbacks.fs_read.clone(),
            });

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
    callbacks: &BuildRequestCallbacks,
) -> Result<String, BuildRequestError> {
    let creds = native::build_from_service_account_json(json_str, callbacks)?;
    let token = creds.access_token().await.map_err(|e| {
        BuildRequestError::AuthorizationFailed(format!(
            "Google Cloud: failed to obtain access token: {e}"
        ))
    })?;
    Ok(token.token)
}

#[cfg(not(target_arch = "wasm32"))]
async fn token_from_adc(callbacks: &BuildRequestCallbacks) -> Result<String, BuildRequestError> {
    let creds = native::build_from_adc(callbacks)?;
    let token = creds.access_token().await.map_err(|e| {
        BuildRequestError::AuthorizationFailed(format!(
            "Google Cloud ADC: failed to obtain access token: {e}"
        ))
    })?;
    Ok(token.token)
}

// ===========================================================================
// WASM: manual JWT signing + OAuth2 token exchange
// ===========================================================================

// ==========================================================================
// WASM: pure-Rust JWT signing (rsa + sha2) + OAuth2 token exchange
//
// We use the RustCrypto `rsa` and `sha2` crates instead of the browser's
// `SubtleCrypto` API. This has two advantages:
//   1. It compiles and runs on any WASM host (Node, Deno, Cloudflare Workers,
//      browser) without requiring `window.crypto`.
//   2. Signing is synchronous, so we don't need `spawn_local` or a oneshot
//      channel to bridge `!Send` JS futures.
// ==========================================================================

#[cfg(target_arch = "wasm32")]
mod wasm {
    use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
    use rsa::{RsaPrivateKey, pkcs8::DecodePrivateKey, signature::SignatureEncoding};

    use super::{BuildRequestCallbacks, BuildRequestError, HttpRequest};

    #[derive(serde::Deserialize)]
    pub(super) struct ServiceAccount {
        pub token_uri: String,
        pub client_email: String,
        pub private_key: String,
        #[allow(dead_code)]
        pub private_key_id: Option<String>,
    }

    /// Parse service account JSON, sign a JWT with `rsa`/`sha2`, and exchange
    /// it for an access token via `HttpSendFn`.
    pub(super) async fn service_account_token(
        json_str: &str,
        callbacks: &BuildRequestCallbacks,
    ) -> Result<String, BuildRequestError> {
        let sa: ServiceAccount = serde_json::from_str(json_str).map_err(|e| {
            BuildRequestError::AuthorizationFailed(format!(
                "Google Cloud: failed to parse service account JSON: {e}"
            ))
        })?;

        let jwt = sign_jwt(&sa)?;
        exchange_jwt_for_token(&sa.token_uri, &jwt, callbacks).await
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
        callbacks: &BuildRequestCallbacks,
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

        let req = HttpRequest {
            method: "POST".to_string(),
            url: token_uri.to_string(),
            headers,
            body,
        };

        let resp = (callbacks.http_send)(req).await.map_err(|e| {
            BuildRequestError::AuthorizationFailed(format!(
                "Google Cloud: token exchange HTTP request failed: {e}"
            ))
        })?;

        if resp.status_code < 200 || resp.status_code >= 300 {
            return Err(BuildRequestError::AuthorizationFailed(format!(
                "Google Cloud: token exchange returned status {}: {}",
                resp.status_code, resp.body,
            )));
        }

        let token_resp: serde_json::Value = serde_json::from_str(&resp.body).map_err(|e| {
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
        use std::sync::Arc;

        use wasm_bindgen_test::wasm_bindgen_test;

        use super::*;

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

        fn mock_callbacks(http_send: crate::HttpSendFn) -> BuildRequestCallbacks {
            BuildRequestCallbacks {
                http_send,
                env_read: Arc::new(|_key| Box::pin(async { Ok(None) })),
                fs_read: Arc::new(|_path| {
                    Box::pin(async { Err(crate::LlmOpError::Other("not found".into())) })
                }),
                shell: Arc::new(|_cmd| {
                    Box::pin(async { Err(crate::LlmOpError::Other("unsupported".into())) })
                }),
            }
        }

        fn mock_token_http() -> crate::HttpSendFn {
            Arc::new(|_req| {
                Box::pin(async {
                    Ok(crate::HttpSendResponse {
                        status_code: 200,
                        headers: indexmap::IndexMap::new(),
                        body: serde_json::json!({
                            "access_token": "ya29.wasm-test",
                            "token_type": "Bearer",
                            "expires_in": 3600,
                        })
                        .to_string(),
                    })
                })
            })
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
            let cb = mock_callbacks(Arc::new(|_req| {
                Box::pin(async {
                    Ok(crate::HttpSendResponse {
                        status_code: 401,
                        headers: indexmap::IndexMap::new(),
                        body: r#"{"error": "invalid_grant"}"#.to_string(),
                    })
                })
            }));
            let result =
                exchange_jwt_for_token("https://oauth2.googleapis.com/token", "fake.jwt.here", &cb)
                    .await;
            let err = result.unwrap_err().to_string();
            assert!(err.contains("401"), "should mention status: {err}");
        }

        #[wasm_bindgen_test]
        async fn exchange_jwt_rejects_missing_access_token() {
            let cb = mock_callbacks(Arc::new(|_req| {
                Box::pin(async {
                    Ok(crate::HttpSendResponse {
                        status_code: 200,
                        headers: indexmap::IndexMap::new(),
                        body: r#"{"token_type": "Bearer"}"#.to_string(),
                    })
                })
            }));
            let result =
                exchange_jwt_for_token("https://oauth2.googleapis.com/token", "fake.jwt.here", &cb)
                    .await;
            let err = result.unwrap_err().to_string();
            assert!(
                err.contains("access_token"),
                "should mention missing field: {err}"
            );
        }

        #[wasm_bindgen_test]
        async fn exchange_jwt_sends_correct_request() {
            use std::sync::Mutex;

            let captured = Arc::new(Mutex::new(None));
            let captured_clone = captured.clone();
            let cb = mock_callbacks(Arc::new(move |req| {
                *captured_clone.lock().unwrap() = Some(req.clone());
                Box::pin(async {
                    Ok(crate::HttpSendResponse {
                        status_code: 200,
                        headers: indexmap::IndexMap::new(),
                        body: serde_json::json!({
                            "access_token": "ya29.test",
                            "token_type": "Bearer",
                            "expires_in": 3600,
                        })
                        .to_string(),
                    })
                })
            }));

            let token =
                exchange_jwt_for_token("https://oauth2.googleapis.com/token", "my.test.jwt", &cb)
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
            use std::sync::Mutex;

            let captured = Arc::new(Mutex::new(None));
            let captured_clone = captured.clone();
            let cb = mock_callbacks(Arc::new(move |req| {
                *captured_clone.lock().unwrap() = Some(req.clone());
                Box::pin(async {
                    Ok(crate::HttpSendResponse {
                        status_code: 200,
                        headers: indexmap::IndexMap::new(),
                        body: serde_json::json!({
                            "access_token": "ya29.wasm-full-flow",
                            "token_type": "Bearer",
                            "expires_in": 3600,
                        })
                        .to_string(),
                    })
                })
            }));

            let token = service_account_token(&test_sa_json(), &cb).await.unwrap();
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
            let cb = mock_callbacks(mock_token_http());
            let result = service_account_token("not json", &cb).await;
            assert!(result.is_err());
        }
    }
}

#[cfg(target_arch = "wasm32")]
async fn token_from_service_account_json(
    json_str: &str,
    callbacks: &BuildRequestCallbacks,
) -> Result<String, BuildRequestError> {
    wasm::service_account_token(json_str, callbacks).await
}

#[cfg(target_arch = "wasm32")]
#[allow(clippy::unused_async)] // must be async to match native signature
async fn token_from_adc(_callbacks: &BuildRequestCallbacks) -> Result<String, BuildRequestError> {
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
    use std::sync::Arc;

    use super::*;
    use crate::baml_std::PrimitiveClientOptions;

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

    #[tokio::test]
    async fn fails_without_credentials() {
        let callbacks = BuildRequestCallbacks {
            http_send: Arc::new(|_req| {
                Box::pin(async {
                    Ok(crate::HttpSendResponse {
                        status_code: 404,
                        headers: indexmap::IndexMap::new(),
                        body: String::new(),
                    })
                })
            }),
            env_read: Arc::new(|_key| Box::pin(async { Ok(None) })),
            fs_read: Arc::new(|_path| {
                Box::pin(async { Err(crate::LlmOpError::Other("not found".into())) })
            }),
            shell: Arc::new(|_cmd| {
                Box::pin(async { Err(crate::LlmOpError::Other("unsupported".into())) })
            }),
        };
        let client = make_client("vertex-ai");
        let mut req = fake_request();
        let result = auth_vertex(&mut req, &client, &callbacks).await;
        assert!(result.is_err());
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

    fn mock_token_http() -> crate::HttpSendFn {
        Arc::new(|_req| {
            Box::pin(async {
                Ok(crate::HttpSendResponse {
                    status_code: 200,
                    headers: indexmap::IndexMap::new(),
                    body: serde_json::json!({
                        "access_token": "ya29.from-service-account",
                        "token_type": "Bearer",
                        "expires_in": 3600,
                    })
                    .to_string(),
                })
            })
        })
    }

    fn stub_callbacks_with_http(http_send: crate::HttpSendFn) -> BuildRequestCallbacks {
        BuildRequestCallbacks {
            http_send,
            env_read: Arc::new(|_key| Box::pin(async { Ok(None) })),
            fs_read: Arc::new(|_path| {
                Box::pin(async { Err(crate::LlmOpError::Other("not found".into())) })
            }),
            shell: Arc::new(|_cmd| {
                Box::pin(async { Err(crate::LlmOpError::Other("unsupported".into())) })
            }),
        }
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

    #[tokio::test]
    async fn credentials_content_inline_json() {
        let client = make_client_with_vertex_opts(VertexAiOptions {
            credentials_content: Some(test_service_account_json()),
            credentials: None,
            location: None,
            project_id: None,
        });
        let callbacks = stub_callbacks_with_http(mock_token_http());
        let mut req = fake_request();
        auth_vertex(&mut req, &client, &callbacks).await.unwrap();
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
        let callbacks = stub_callbacks_with_http(mock_token_http());
        let mut req = fake_request();
        auth_vertex(&mut req, &client, &callbacks).await.unwrap();
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
        let callbacks = BuildRequestCallbacks {
            http_send: mock_token_http(),
            env_read: Arc::new(|_key| Box::pin(async { Ok(None) })),
            fs_read: Arc::new(move |path| {
                let sa_json = sa_json.clone();
                Box::pin(async move {
                    if path == "/fake/service-account.json" {
                        Ok(sa_json.into_bytes())
                    } else {
                        Err(crate::LlmOpError::Other("not found".into()))
                    }
                })
            }),
            shell: Arc::new(|_cmd| {
                Box::pin(async { Err(crate::LlmOpError::Other("unsupported".into())) })
            }),
        };
        let mut req = fake_request();
        auth_vertex(&mut req, &client, &callbacks).await.unwrap();
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
        let callbacks = stub_callbacks_with_http(mock_token_http());
        let mut req = fake_request();
        auth_vertex(&mut req, &client, &callbacks).await.unwrap();
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
        let callbacks = BuildRequestCallbacks {
            http_send: Arc::new(|_req| {
                Box::pin(async {
                    panic!("http_send should not be called when key query param is set");
                })
            }),
            env_read: Arc::new(|_key| Box::pin(async { Ok(None) })),
            fs_read: Arc::new(|_path| {
                Box::pin(async { Err(crate::LlmOpError::Other("not found".into())) })
            }),
            shell: Arc::new(|_cmd| {
                Box::pin(async { Err(crate::LlmOpError::Other("unsupported".into())) })
            }),
        };
        let mut req = fake_request();
        auth_vertex(&mut req, &client, &callbacks).await.unwrap();
        assert!(!req.headers.contains_key("authorization"));
    }

    /// Confirms that the injected env/fs/http providers are actually used
    /// by the google-cloud-auth ADC flow.
    #[tokio::test]
    async fn adc_with_injected_providers() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        let http_call_count = Arc::new(AtomicUsize::new(0));
        let http_call_count_clone = http_call_count.clone();

        let adc_json = serde_json::json!({
            "client_id": "test-client-id",
            "client_secret": "test-client-secret",
            "refresh_token": "test-refresh-token",
            "type": "authorized_user",
            "token_uri": "https://fake-oauth.example.com/token",
        })
        .to_string();

        let callbacks = BuildRequestCallbacks {
            http_send: Arc::new(move |_req| {
                http_call_count_clone.fetch_add(1, Ordering::SeqCst);
                Box::pin(async {
                    Ok(crate::HttpSendResponse {
                        status_code: 200,
                        headers: indexmap::IndexMap::new(),
                        body: serde_json::json!({
                            "access_token": "ya29.fake-test-token",
                            "token_type": "Bearer",
                            "expires_in": 3600,
                        })
                        .to_string(),
                    })
                })
            }),
            env_read: Arc::new(|key| {
                Box::pin(async move {
                    match key.as_str() {
                        "GOOGLE_APPLICATION_CREDENTIALS" => Ok(Some("/fake/adc.json".to_string())),
                        _ => Ok(None),
                    }
                })
            }),
            fs_read: Arc::new(move |path| {
                let adc_json = adc_json.clone();
                Box::pin(async move {
                    if path == "/fake/adc.json" {
                        Ok(adc_json.into_bytes())
                    } else {
                        Err(crate::LlmOpError::Other("not found".into()))
                    }
                })
            }),
            shell: Arc::new(|_cmd| {
                Box::pin(async { Err(crate::LlmOpError::Other("unsupported".into())) })
            }),
        };

        let client = make_client("vertex-ai");
        let mut req = fake_request();
        auth_vertex(&mut req, &client, &callbacks).await.unwrap();

        assert!(http_call_count.load(Ordering::SeqCst) > 0);
        assert_eq!(
            req.headers.get("authorization").unwrap(),
            "Bearer ya29.fake-test-token",
        );
    }
}
