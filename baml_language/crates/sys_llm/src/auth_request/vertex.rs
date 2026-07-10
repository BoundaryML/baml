//! Vertex AI authentication and `project_id` resolution.
//!
//! Credentials are resolved once via [`resolve_credentials`], then used for
//! both access token and `project_id` (single-source principle).
//!
//! ## Credential resolution order
//!
//! 1. `options.credentials` -- a credential JSON **file path** (service
//!    account, authorized user, workload identity federation, or impersonated
//!    service account -- the same documents `GOOGLE_APPLICATION_CREDENTIALS`
//!    accepts)
//! 2. `options.credentials_content` -- an inline credential document as a
//!    `json` object (or a pre-serialized JSON string)
//! 3. Application Default Credentials -- the `GOOGLE_APPLICATION_CREDENTIALS`
//!    file, the well-known ADC config file, then the GCE metadata server
//!
//! An explicitly-set option is used as-is: a broken value is an error, never
//! a silent cascade to the next source.
//!
//! This mirrors `google-auth` (Python/Node), with `credentials_content` as the
//! one BAML extra for inline credentials. Deliberately NOT supported:
//! inline JSON in `credentials` or in env vars (`GOOGLE_APPLICATION_CREDENTIALS`
//! is a file path, period) and `gcloud` CLI shell-outs.
//!
//! All token minting runs through the slim `google-cloud-auth` fork, whose IO
//! is routed through BAML's [`RuntimeIo`] by [`BamlTokenIo`]. Signing is pure
//! Rust (`rsa` + `sha2`), so a single code path works on native and wasm, and
//! the fork caches tokens process-wide until shortly before expiry.

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
    let creds = resolve_credentials(vertex_opts.as_ref())?;

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

/// Extract `project_id` from a JSON string (service account credentials).
fn extract_project_id_from_json(json_str: &str) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(json_str)
        .ok()
        .and_then(|v| v.get("project_id")?.as_str().map(String::from))
}

/// Serialize a `credentials_content` value (a BAML `json` value) to JSON text.
///
/// A string value is treated as pre-serialized JSON text and passed through;
/// any other JSON shape (typically a `json` object) is serialized. `null`
/// means unset (`Ok(None)`); a value with no JSON representation is an error.
fn credentials_content_to_json_string(
    value: &BexExternalValue,
) -> Result<Option<String>, BuildRequestError> {
    match value {
        BexExternalValue::Null => Ok(None),
        BexExternalValue::String(s) => Ok(Some(s.as_str().to_string())),
        other => crate::build_request::bex_value_to_json(other)
            .map(|v| Some(v.to_string()))
            .ok_or_else(|| {
                BuildRequestError::AuthorizationFailed(
                    "Google Cloud: credentials_content must be a JSON credential object"
                        .to_string(),
                )
            }),
    }
}

// ---------------------------------------------------------------------------
// Credential resolution (single source for both token and project_id)
// ---------------------------------------------------------------------------

/// The resolved credential source.
///
/// One source is selected, then used for both token and `project_id`.
enum ResolvedCredentials {
    /// `options.credentials` -- a credential JSON file path.
    CredentialsFile(String),
    /// `options.credentials_content` -- inline credential JSON, serialized.
    CredentialsJson(String),
    /// Application Default Credentials -- `GOOGLE_APPLICATION_CREDENTIALS`,
    /// the well-known ADC config file, or the GCE metadata server.
    Adc,
}

/// Resolve which credential source to use: the explicit `credentials` option
/// (a file path), then `credentials_content` (inline JSON), else Application
/// Default Credentials.
///
/// An explicitly-set option is used as-is -- a broken value errors instead of
/// cascading to the next source. (`credentials_content null` is the unset
/// spelling of the `json | null` type, not an explicit source.)
fn resolve_credentials(
    vertex_opts: Option<&VertexAiOptions>,
) -> Result<ResolvedCredentials, BuildRequestError> {
    if let Some(path) = vertex_opts.and_then(|o| o.credentials.as_ref()) {
        return Ok(ResolvedCredentials::CredentialsFile(path.clone()));
    }
    if let Some(content) = vertex_opts.and_then(|o| o.credentials_content.as_ref()) {
        if let Some(json_str) = credentials_content_to_json_string(content)? {
            return Ok(ResolvedCredentials::CredentialsJson(json_str));
        }
    }
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
        ResolvedCredentials::CredentialsJson(json_str) => {
            let adapter = BamlTokenIo { io };
            google_cloud_auth::token_from_credentials_json(
                &adapter,
                json_str,
                google_cloud_auth::CLOUD_PLATFORM_SCOPE,
            )
            .await
            .map_err(|e| BuildRequestError::AuthorizationFailed(e.to_string()))
        }
        ResolvedCredentials::CredentialsFile(path) => {
            let json_str = read_credentials_file(path, &*io).await?;
            let adapter = BamlTokenIo { io };
            google_cloud_auth::token_from_credentials_json(
                &adapter,
                &json_str,
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
        ResolvedCredentials::CredentialsJson(json_str) => {
            if let Some(pid) = extract_project_id_from_json(json_str) {
                return Some(pid);
            }
        }
        ResolvedCredentials::CredentialsFile(path) => {
            if let Ok(contents) = read_credentials_file(path, io).await {
                if let Some(pid) = extract_project_id_from_json(&contents) {
                    return Some(pid);
                }
            }
        }
        ResolvedCredentials::Adc => {
            // ADC doesn't expose project_id directly. Try reading the
            // credentials file the GOOGLE_APPLICATION_CREDENTIALS env points to.
            if let Ok(Some(path)) = io
                .env_get("GOOGLE_APPLICATION_CREDENTIALS".to_string())
                .await
            {
                if !path.is_empty() {
                    if let Ok(handle) =
                        io.fs_open(path, BexExternalValue::String("r".into())).await
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

    /// Build a `json`-typed `credentials_content` value: a BAML map/object
    /// mirroring the given JSON object (all test SA fields are strings).
    fn json_object_to_bex(json_str: &str) -> BexExternalValue {
        use bex_external_types::RuntimeTy;
        let value: serde_json::Value = serde_json::from_str(json_str).unwrap();
        let mut entries = indexmap::IndexMap::new();
        for (k, v) in value.as_object().unwrap() {
            entries.insert(
                k.clone(),
                BexExternalValue::String(v.as_str().unwrap().into()),
            );
        }
        BexExternalValue::Map {
            key_type: RuntimeTy::unknown(),
            value_type: RuntimeTy::unknown(),
            entries,
        }
    }

    #[tokio::test]
    async fn credentials_content_inline_json_object() {
        let client = make_client_with_vertex_opts(VertexAiOptions {
            credentials_content: Some(json_object_to_bex(&test_service_account_json())),
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
    async fn credentials_content_preserialized_json_string() {
        // A `json` string value is treated as pre-serialized JSON text.
        let client = make_client_with_vertex_opts(VertexAiOptions {
            credentials_content: Some(BexExternalValue::String(
                test_service_account_json().into(),
            )),
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
    async fn credentials_file_beats_content_and_never_cascades() {
        // `credentials` wins over `credentials_content`, and an explicitly-set
        // (but unreadable) file errors instead of cascading to the inline JSON.
        let client = make_client_with_vertex_opts(VertexAiOptions {
            credentials: Some("/unreadable/service-account.json".to_string()),
            credentials_content: Some(json_object_to_bex(&test_service_account_json())),
            location: None,
            project_id: None,
        });
        let io = StubIo::with_token("ya29.should-not-mint");
        let mut req = fake_request();
        let err = auth_vertex(&mut req, &client, Arc::new(io))
            .await
            .unwrap_err();
        let msg = format!("{err:?}");
        assert!(
            msg.contains("failed to open credentials file"),
            "must fail on the file, not fall back to credentials_content: {msg}"
        );
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
    async fn credentials_file_dispatches_all_adc_types_not_just_service_accounts() {
        // An `authorized_user` document through options.credentials proves the
        // consumer routes through the fork's full type dispatch.
        let user_json = serde_json::json!({
            "type": "authorized_user",
            "client_id": "vertex-file-cid",
            "client_secret": "vertex-file-secret",
            "refresh_token": "vertex-file-refresh",
        })
        .to_string();
        let client = make_client_with_vertex_opts(VertexAiOptions {
            credentials: Some("/fake/authorized-user.json".to_string()),
            credentials_content: None,
            location: None,
            project_id: None,
        });
        let io = FsIo {
            expected_path: "/fake/authorized-user.json".to_string(),
            file_contents: user_json,
            token_body: serde_json::json!({
                "access_token": "ya29.from-authorized-user",
                "token_type": "Bearer",
                "expires_in": 3600,
            })
            .to_string(),
        };
        let mut req = fake_request();
        auth_vertex(&mut req, &client, Arc::new(io)).await.unwrap();
        assert_eq!(
            req.headers.get("authorization").unwrap(),
            "Bearer ya29.from-authorized-user",
        );
    }

    #[tokio::test]
    async fn credentials_option_rejects_inline_json() {
        // Inline JSON is deliberately unsupported: `credentials` is a file
        // path, exactly like GOOGLE_APPLICATION_CREDENTIALS in google-auth.
        let client = make_client_with_vertex_opts(VertexAiOptions {
            credentials: Some(test_service_account_json()),
            credentials_content: None,
            location: None,
            project_id: None,
        });
        let io = StubIo::with_token("ya29.should-not-mint");
        let mut req = fake_request();
        let err = auth_vertex(&mut req, &client, Arc::new(io))
            .await
            .unwrap_err();
        let msg = format!("{err:?}");
        assert!(
            msg.contains("failed to open credentials file"),
            "inline JSON must be treated as an (unreadable) path, got: {msg}"
        );
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
