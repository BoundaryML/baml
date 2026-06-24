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
//!    are deferred to ADC)
//! 4. `GOOGLE_APPLICATION_CREDENTIALS_CONTENT` env var (BAML-specific)
//! 5. Application Default Credentials -- ADC config file,
//!    `GOOGLE_APPLICATION_CREDENTIALS` file paths, or the GCE metadata server
//! 6. `gcloud` CLI
//!
//! All token minting (service-account RS256 JWT, ADC authorized-user refresh,
//! GCE metadata) runs through the slim `google-cloud-auth` fork, whose IO is
//! routed through BAML's [`RuntimeIo`] by [`BamlTokenIo`]. Signing is pure Rust
//! (`rsa` + `sha2`), so a single code path works on native and wasm; only the
//! `gcloud` CLI fallback is effectively native-only (it shells out).

use std::sync::Arc;

use indexmap::IndexMap;
use sys_types::{BexExternalValue, runtime_io::RuntimeIo};

use crate::{
    baml_std::{HttpRequest, PrimitiveClient, ProviderOptions, VertexAiOptions},
    build_request::BuildRequestError,
};

// ---------------------------------------------------------------------------
// Public entry point
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
// TokenIo adapter over RuntimeIo
// ---------------------------------------------------------------------------

/// Bridges the `google-cloud-auth` [`google_cloud_auth::TokenIo`] trait to
/// BAML's [`RuntimeIo`] so token resolution stays inside BAML's sandbox.
struct BamlTokenIo {
    io: Arc<dyn RuntimeIo>,
}

#[async_trait::async_trait]
impl google_cloud_auth::TokenIo for BamlTokenIo {
    async fn env(&self, key: &str) -> Option<String> {
        self.io.env_get(key.to_string()).await.ok().flatten()
    }

    async fn read_file(&self, path: &str) -> Option<String> {
        let handle = self
            .io
            .fs_open(path.to_string(), BexExternalValue::String("r".into()))
            .await
            .ok()?;
        self.io.fs_file_text(&handle).await.ok()
    }

    async fn http(
        &self,
        method: &str,
        url: &str,
        headers: &[(String, String)],
        body: &str,
    ) -> Result<google_cloud_auth::HttpResponse, google_cloud_auth::AuthError> {
        let mut header_map = IndexMap::new();
        for (k, v) in headers {
            header_map.insert(k.clone(), v.clone());
        }
        let request = sys_types::generated::owned::http::Request {
            method: method.to_string(),
            url: url.to_string(),
            headers: header_map,
            body: body.to_string(),
        };
        let resp = self
            .io
            // Unbounded, as before: `0n` -> no deadline.
            .http__send(request, std::sync::Arc::new(num_bigint::BigInt::from(0i64)))
            .await
            .map_err(|e| google_cloud_auth::AuthError::Io(e.to_string()))?;
        let resp_body = self
            .io
            .http_response_text(&resp)
            .await
            .map_err(|e| google_cloud_auth::AuthError::Io(e.to_string()))?;
        Ok(google_cloud_auth::HttpResponse {
            status: u16::try_from(resp.status_code).unwrap_or(0),
            body: resp_body,
        })
    }
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
    /// Application Default Credentials -- ADC config file,
    /// `GOOGLE_APPLICATION_CREDENTIALS`, or the GCE metadata server.
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
/// 5. Application Default Credentials
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

    // 5. Application Default Credentials (config file or GCE metadata).
    {
        let adapter = BamlTokenIo { io: io.clone() };
        if google_cloud_auth::adc_available(&adapter).await {
            return Ok(ResolvedCredentials::Adc);
        }
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

    // 7. ADC metadata fallback. `adc_available` intentionally avoids network
    // IO, so no-file metadata environments (GCE/Cloud Run/GKE) reach this point
    // and are resolved by `token_from_adc` rather than failing before metadata
    // can be queried.
    Ok(ResolvedCredentials::Adc)
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
            let adapter = BamlTokenIo { io };
            google_cloud_auth::token_from_service_account_json(
                &adapter,
                json_str,
                google_cloud_auth::CLOUD_PLATFORM_SCOPE,
            )
            .await
            .map_err(|e| BuildRequestError::AuthorizationFailed(e.to_string()))
        }
        ResolvedCredentials::Adc => {
            let adapter = BamlTokenIo { io };
            google_cloud_auth::token_from_adc(&adapter, google_cloud_auth::CLOUD_PLATFORM_SCOPE)
                .await
                .map_err(|e| BuildRequestError::AuthorizationFailed(e.to_string()))
        }
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
            // ADC doesn't expose project_id directly. Try reading the
            // credentials file the GOOGLE_APPLICATION_CREDENTIALS env points to.
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
                    if let Ok(handle) = io.fs_open(val, BexExternalValue::String("r".into())).await
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
    if let Ok(resp) = io
        .http__send(req, std::sync::Arc::new(num_bigint::BigInt::from(0i64)))
        .await
    {
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
        .fs_open(adc_path, BexExternalValue::String("r".into()))
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
        .fs_open(path.to_string(), BexExternalValue::String("r".into()))
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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use std::{future::Future, pin::Pin, sync::Arc};

    use indexmap::IndexMap;
    use sys_types::runtime_io::RuntimeIoError;

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
        fn http__send(
            &self,
            _request: sys_types::generated::owned::http::Request,
            _timeout_nanos: std::sync::Arc<num_bigint::BigInt>,
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
        fn http__send(
            &self,
            _request: sys_types::generated::owned::http::Request,
            _timeout_nanos: std::sync::Arc<num_bigint::BigInt>,
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
        fn http__send(
            &self,
            _request: sys_types::generated::owned::http::Request,
            _timeout_nanos: std::sync::Arc<num_bigint::BigInt>,
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
                        raw: bex_external_types::BexExternalValue::String(path.into()),
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
                bex_external_types::BexExternalValue::String(s) => s.to_string(),
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
        fn http__send(
            &self,
            _request: sys_types::generated::owned::http::Request,
            _timeout_nanos: std::sync::Arc<num_bigint::BigInt>,
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
    /// by the ADC flow (authorized-user refresh grant).
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
