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
//! 2. `options.credentials_content` -- an inline credential JSON string
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
//! Rust (`rsa` + `sha2`), so a single code path works on native and wasm. The
//! fork caches tokens process-wide until shortly before expiry and also
//! resolves `project_id` and the quota project (`x-goog-user-project`).

use std::sync::Arc;

use google_cloud_auth::TokenIo;
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
/// Also resolves the `location` and `project_id` placeholders in the URL if
/// present (i.e. they were not known at URL construction time): `location`
/// from the `GOOGLE_CLOUD_LOCATION` env var, `project_id` from the credential
/// / `GOOGLE_CLOUD_PROJECT` chain.
///
/// Credentials are resolved once, then used for both token and `project_id`
/// (matching the old engine's single-source principle).
pub(crate) async fn auth_vertex(
    request: &mut HttpRequest,
    client: &PrimitiveClient,
    io: Arc<dyn RuntimeIo>,
) -> Result<(), BuildRequestError> {
    let vertex_opts = client
        .provider_options
        .as_ref()
        .and_then(ProviderOptions::vertex_ai);

    // If an API key is provided as a query param, skip token-based auth
    // but still resolve the location / project-id placeholders in the URL.
    let api_key_auth = client.options.query_params.contains_key("key");
    let needs_location = request
        .url
        .contains(crate::build_request::google::VERTEX_LOCATION_PLACEHOLDER);
    let needs_project_id = request
        .url
        .contains(crate::build_request::google::VERTEX_PROJECT_ID_PLACEHOLDER);

    // With API-key auth and no placeholders, no credentials needed.
    if api_key_auth && !needs_project_id && !needs_location {
        return Ok(());
    }

    let adapter = BamlTokenIo { io: io.clone() };

    // Resolve the location placeholder from GOOGLE_CLOUD_LOCATION if needed
    // (options.location was unset at URL construction time).
    if needs_location {
        let location = match adapter.env("GOOGLE_CLOUD_LOCATION").await {
            Some(v) => v.trim().to_string(),
            // Enterprise mode defaults an unset location to the global endpoint,
            // matching google-genai; plain Vertex still requires an explicit one.
            None if crate::build_request::google_use_enterprise(client, &*io).await => {
                "global".to_string()
            }
            None => {
                return Err(BuildRequestError::Other(
                    "Could not resolve location for Vertex AI. Set options.location \
                     (e.g. us-central1) or the GOOGLE_CLOUD_LOCATION env var."
                        .to_string(),
                ));
            }
        };
        // "global" uses the region-less endpoint host.
        if location == "global" {
            request.url = request.url.replace(
                &format!(
                    "{}-aiplatform.googleapis.com",
                    crate::build_request::google::VERTEX_LOCATION_PLACEHOLDER
                ),
                "aiplatform.googleapis.com",
            );
        }
        request.url = request.url.replace(
            crate::build_request::google::VERTEX_LOCATION_PLACEHOLDER,
            &location,
        );
    }

    // Resolve credentials once (needed for both project-id and token).
    let creds = resolve_credentials(vertex_opts.as_ref());

    // Resolve project_id placeholder in the URL if needed.
    if needs_project_id {
        let project_id = project_id_from_credentials(&creds, &adapter)
            .await?
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

    let token = token_from_credentials(&creds, &adapter, &*io).await?;

    request
        .headers
        .insert("authorization".to_string(), format!("Bearer {token}"));

    // google-auth parity (`Credentials.apply`): attribute quota/billing to the
    // configured quota project when the credentials carry one.
    if !request.headers.contains_key("x-goog-user-project") {
        if let Some(quota_project) = quota_project_from_credentials(&creds, &adapter).await {
            request
                .headers
                .insert("x-goog-user-project".to_string(), quota_project);
        }
    }

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
impl TokenIo for BamlTokenIo {
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
/// cascading to the next source.
fn resolve_credentials(vertex_opts: Option<&VertexAiOptions>) -> ResolvedCredentials {
    if let Some(path) = vertex_opts.and_then(|o| o.credentials.as_ref()) {
        return ResolvedCredentials::CredentialsFile(path.clone());
    }
    if let Some(content) = vertex_opts.and_then(|o| o.credentials_content.as_ref()) {
        return ResolvedCredentials::CredentialsJson(content.clone());
    }
    ResolvedCredentials::Adc
}

// ---------------------------------------------------------------------------
// Token from resolved credentials
// ---------------------------------------------------------------------------

async fn token_from_credentials(
    creds: &ResolvedCredentials,
    adapter: &BamlTokenIo,
    io: &dyn RuntimeIo,
) -> Result<String, BuildRequestError> {
    match creds {
        ResolvedCredentials::CredentialsJson(json_str) => {
            google_cloud_auth::token_from_credentials_json(
                adapter,
                json_str,
                google_cloud_auth::CLOUD_PLATFORM_SCOPE,
            )
            .await
            .map_err(|e| BuildRequestError::AuthorizationFailed(e.to_string()))
        }
        ResolvedCredentials::CredentialsFile(path) => {
            let json_str = read_credentials_file(path, io).await?;
            google_cloud_auth::token_from_credentials_json(
                adapter,
                &json_str,
                google_cloud_auth::CLOUD_PLATFORM_SCOPE,
            )
            .await
            .map_err(|e| BuildRequestError::AuthorizationFailed(e.to_string()))
        }
        ResolvedCredentials::Adc => {
            google_cloud_auth::token_from_adc(adapter, google_cloud_auth::CLOUD_PLATFORM_SCOPE)
                .await
                .map_err(|e| BuildRequestError::AuthorizationFailed(e.to_string()))
        }
    }
}

// ---------------------------------------------------------------------------
// Project ID / quota project from resolved credentials
// ---------------------------------------------------------------------------

/// Get `project_id` from the resolved credential source, falling back to the
/// fork's google-auth-style chain (env vars, credential files, the gcloud
/// config file, the GCE metadata server).
///
/// An unreadable `options.credentials` file is an error, not a fallthrough:
/// on paths that only need the project id (e.g. express-mode API-key auth),
/// silently continuing to the ADC chain would mask the misconfiguration.
async fn project_id_from_credentials(
    creds: &ResolvedCredentials,
    adapter: &BamlTokenIo,
) -> Result<Option<String>, BuildRequestError> {
    match creds {
        ResolvedCredentials::CredentialsJson(json_str) => {
            if let Some(pid) = google_cloud_auth::project_id_from_json(json_str) {
                return Ok(Some(pid));
            }
        }
        ResolvedCredentials::CredentialsFile(path) => {
            let contents = read_credentials_file(path, &*adapter.io).await?;
            if let Some(pid) = google_cloud_auth::project_id_from_json(&contents) {
                return Ok(Some(pid));
            }
        }
        ResolvedCredentials::Adc => {}
    }
    Ok(google_cloud_auth::project_id(adapter).await)
}

/// Get the quota project for the resolved credential source
/// (`GOOGLE_CLOUD_QUOTA_PROJECT` always wins, matching google-auth).
async fn quota_project_from_credentials(
    creds: &ResolvedCredentials,
    adapter: &BamlTokenIo,
) -> Option<String> {
    match creds {
        ResolvedCredentials::CredentialsJson(json_str) => {
            // Set-but-empty is honored, matching the fork: a misconfigured
            // env var should be visible, not silently skipped.
            if let Some(val) = adapter.env("GOOGLE_CLOUD_QUOTA_PROJECT").await {
                return Some(val.trim().to_string());
            }
            google_cloud_auth::quota_project_id_from_json(json_str)
        }
        ResolvedCredentials::CredentialsFile(path) => {
            // Set-but-empty is honored, matching the fork: a misconfigured
            // env var should be visible, not silently skipped.
            if let Some(val) = adapter.env("GOOGLE_CLOUD_QUOTA_PROJECT").await {
                return Some(val.trim().to_string());
            }
            let contents = adapter.read_file(path).await?;
            google_cloud_auth::quota_project_id_from_json(&contents)
        }
        ResolvedCredentials::Adc => google_cloud_auth::quota_project_id(adapter).await,
    }
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

    /// NOTE: generates a fresh RSA key per call, so every test's credential
    /// material is unique and the fork's process-wide token cache can never
    /// serve one test's token to another.
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
    async fn credentials_content_inline_json_string() {
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
    async fn credentials_file_beats_content_and_never_cascades() {
        // `credentials` wins over `credentials_content`, and an explicitly-set
        // (but unreadable) file errors instead of cascading to the inline JSON.
        let client = make_client_with_vertex_opts(VertexAiOptions {
            credentials: Some("/unreadable/service-account.json".to_string()),
            credentials_content: Some(test_service_account_json()),
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
        // A plain service account has no quota project.
        assert!(!req.headers.contains_key("x-goog-user-project"));
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
            "refresh_token": "vertex-adc-refresh-token",
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

    /// google-auth parity: ADC credentials carrying a quota project must
    /// stamp `x-goog-user-project` (`Credentials.apply`).
    #[tokio::test]
    async fn quota_project_header_set_from_adc_file() {
        let adc_json = serde_json::json!({
            "client_id": "quota-client-id",
            "client_secret": "quota-client-secret",
            "refresh_token": "vertex-quota-refresh-token",
            "type": "authorized_user",
            "quota_project_id": "my-quota-project",
            "token_uri": "https://fake-oauth.example.com/token",
        })
        .to_string();

        let io = AdcIo {
            http_call_count: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            env_vars: std::collections::HashMap::from([(
                "GOOGLE_APPLICATION_CREDENTIALS".to_string(),
                "/fake/quota-adc.json".to_string(),
            )]),
            files: std::collections::HashMap::from([(
                "/fake/quota-adc.json".to_string(),
                adc_json,
            )]),
            token_body: serde_json::json!({
                "access_token": "ya29.quota-token",
                "token_type": "Bearer",
                "expires_in": 3600,
            })
            .to_string(),
        };

        let client = make_client("vertex-ai");
        let mut req = fake_request();
        auth_vertex(&mut req, &client, Arc::new(io)).await.unwrap();

        assert_eq!(
            req.headers.get("x-goog-user-project").unwrap(),
            "my-quota-project",
        );
    }

    /// An unreadable `options.credentials` file must fail project-id
    /// resolution too — on the express-mode (API-key) path no token is ever
    /// minted, so falling through to the env/ADC project chain would silently
    /// mask the broken file.
    #[tokio::test]
    async fn broken_credentials_file_fails_project_resolution_not_masked() {
        use bex_external_types::AsBexExternalValue;
        let client = PrimitiveClient::new(
            "test-google".to_string(),
            "vertex-ai".to_string(),
            PrimitiveClientOptions {
                model: Some("gemini-pro".to_string()),
                query_params: indexmap::IndexMap::from([(
                    "key".to_string(),
                    "my-api-key".to_string(),
                )]),
                provider_options: VertexAiOptions {
                    credentials: Some("/missing/creds.json".to_string()),
                    credentials_content: None,
                    location: None,
                    project_id: None,
                }
                .into_bex_external_value(),
                ..Default::default()
            },
        )
        .unwrap();

        // GOOGLE_CLOUD_PROJECT is set: the fallback COULD resolve a project,
        // which is exactly the masking this test forbids.
        let io = AdcIo {
            http_call_count: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            env_vars: std::collections::HashMap::from([
                (
                    "GOOGLE_CLOUD_PROJECT".to_string(),
                    "env-project".to_string(),
                ),
                (
                    "GOOGLE_CLOUD_LOCATION".to_string(),
                    "us-central1".to_string(),
                ),
            ]),
            files: std::collections::HashMap::new(),
            token_body: String::new(),
        };

        let mut req = fake_request();
        req.url = format!(
            "https://us-central1-aiplatform.googleapis.com/v1/projects/{}/locations/us-central1/publishers/google/models/gemini-pro:generateContent",
            crate::build_request::google::VERTEX_PROJECT_ID_PLACEHOLDER
        );
        let err = auth_vertex(&mut req, &client, Arc::new(io))
            .await
            .unwrap_err();
        let msg = format!("{err:?}");
        assert!(
            msg.contains("failed to open credentials file"),
            "broken credentials must not be masked by the project fallback: {msg}"
        );
    }

    /// The location placeholder is filled from `GOOGLE_CLOUD_LOCATION` when
    /// `options.location` was not set at URL construction time.
    #[tokio::test]
    async fn location_placeholder_resolved_from_env() {
        let adc_json = serde_json::json!({
            "client_id": "loc-client-id",
            "client_secret": "loc-client-secret",
            "refresh_token": "vertex-location-refresh-token",
            "type": "authorized_user",
            "token_uri": "https://fake-oauth.example.com/token",
        })
        .to_string();

        let io = AdcIo {
            http_call_count: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            env_vars: std::collections::HashMap::from([
                (
                    "GOOGLE_APPLICATION_CREDENTIALS".to_string(),
                    "/fake/loc-adc.json".to_string(),
                ),
                (
                    "GOOGLE_CLOUD_LOCATION".to_string(),
                    "europe-west4".to_string(),
                ),
            ]),
            files: std::collections::HashMap::from([("/fake/loc-adc.json".to_string(), adc_json)]),
            token_body: serde_json::json!({
                "access_token": "ya29.location-token",
                "token_type": "Bearer",
                "expires_in": 3600,
            })
            .to_string(),
        };

        let client = make_client("vertex-ai");
        let mut req = fake_request();
        req.url = format!(
            "https://{p}-aiplatform.googleapis.com/v1/projects/test/locations/{p}/publishers/google/models/gemini-pro:generateContent",
            p = crate::build_request::google::VERTEX_LOCATION_PLACEHOLDER
        );
        auth_vertex(&mut req, &client, Arc::new(io)).await.unwrap();

        assert_eq!(
            req.url,
            "https://europe-west4-aiplatform.googleapis.com/v1/projects/test/locations/europe-west4/publishers/google/models/gemini-pro:generateContent",
        );
    }

    #[tokio::test]
    async fn missing_location_is_an_actionable_error() {
        let client = make_client("vertex-ai");
        let mut req = fake_request();
        req.url = format!(
            "https://{p}-aiplatform.googleapis.com/v1/projects/test/locations/{p}/publishers/google/models/gemini-pro:generateContent",
            p = crate::build_request::google::VERTEX_LOCATION_PLACEHOLDER
        );
        let err = auth_vertex(&mut req, &client, Arc::new(NoCredsIo))
            .await
            .unwrap_err();
        let msg = format!("{err:?}");
        assert!(
            msg.contains("Could not resolve location") && msg.contains("GOOGLE_CLOUD_LOCATION"),
            "got: {msg}"
        );
    }

    /// The URL placeholder is filled from the google-auth project chain
    /// (here: the `GOOGLE_CLOUD_PROJECT` env var).
    #[tokio::test]
    async fn project_id_placeholder_resolved_from_env() {
        let adc_json = serde_json::json!({
            "client_id": "proj-client-id",
            "client_secret": "proj-client-secret",
            "refresh_token": "vertex-project-refresh-token",
            "type": "authorized_user",
            "token_uri": "https://fake-oauth.example.com/token",
        })
        .to_string();

        let io = AdcIo {
            http_call_count: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            env_vars: std::collections::HashMap::from([
                (
                    "GOOGLE_APPLICATION_CREDENTIALS".to_string(),
                    "/fake/proj-adc.json".to_string(),
                ),
                (
                    "GOOGLE_CLOUD_PROJECT".to_string(),
                    "env-project".to_string(),
                ),
            ]),
            files: std::collections::HashMap::from([("/fake/proj-adc.json".to_string(), adc_json)]),
            token_body: serde_json::json!({
                "access_token": "ya29.project-token",
                "token_type": "Bearer",
                "expires_in": 3600,
            })
            .to_string(),
        };

        let client = make_client("vertex-ai");
        let mut req = fake_request();
        req.url = format!(
            "https://us-central1-aiplatform.googleapis.com/v1/projects/{}/locations/us-central1/publishers/google/models/gemini-pro:generateContent",
            crate::build_request::google::VERTEX_PROJECT_ID_PLACEHOLDER
        );
        auth_vertex(&mut req, &client, Arc::new(io)).await.unwrap();

        assert!(
            req.url.contains("/projects/env-project/"),
            "placeholder must be replaced, got: {}",
            req.url
        );
    }
}
