//! AWS Bedrock request authorization: credential resolution + `SigV4` signing.

use std::sync::Arc;
#[allow(clippy::disallowed_types)]
use std::time::SystemTime;

use aws_credential_types::{Credentials, provider::ProvideCredentials};
use aws_sigv4::{
    http_request::{SignableBody, SignableRequest, SigningSettings, sign},
    sign::v4,
};
use aws_smithy_runtime_api::{
    client::{
        http::{HttpConnectorFuture, SharedHttpConnector},
        result::ConnectorError,
    },
    http as smithy_http,
};
use aws_smithy_types::body::SdkBody;
use indexmap::IndexMap;
#[cfg_attr(target_arch = "wasm32", allow(unused_imports))]
use sys_types::{
    BexExternalValue,
    runtime_io::{RuntimeIo, RuntimeIoError},
};

use crate::{
    baml_std::{BedrockOptions, HttpRequest, PrimitiveClient, ProviderOptions},
    build_request::BuildRequestError,
};

// ---------------------------------------------------------------------------
// Platform helpers
// ---------------------------------------------------------------------------

/// Platform-aware `SystemTime::now()`.
///
/// On WASM, `std::time::SystemTime::now()` panics -- use `web_time` instead.
#[allow(clippy::disallowed_types)]
fn now() -> SystemTime {
    #[cfg(not(target_arch = "wasm32"))]
    {
        SystemTime::now()
    }
    #[cfg(target_arch = "wasm32")]
    {
        use aws_smithy_async::time::TimeSource;
        crate::wasm::BrowserTime.now()
    }
}

// ---------------------------------------------------------------------------
// Native: sync env/fs providers for AWS SDK config loading
// ---------------------------------------------------------------------------

#[cfg(not(target_arch = "wasm32"))]
mod native_providers {
    use std::{future::Future, sync::Arc};

    use aws_types::os_shim_internal::{ProvideEnv, ProvideFs};

    use super::{BexExternalValue, RuntimeIo};

    pub(super) struct BexEnvProvider {
        pub io: Arc<dyn RuntimeIo>,
    }

    impl std::fmt::Debug for BexEnvProvider {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("BexEnvProvider").finish()
        }
    }

    impl ProvideEnv for BexEnvProvider {
        fn get(&self, k: &str) -> Result<String, std::env::VarError> {
            let io = self.io.clone();
            let key = k.to_string();
            // ProvideEnv::get is sync but RuntimeIo::env_get is async (and may
            // contain real awaits). We can't block_on from inside the tokio
            // runtime, and block_in_place only works on multi-threaded runtimes.
            // A short-lived thread lets us call block_on from outside the
            // runtime.
            let handle = tokio::runtime::Handle::current();
            let result = std::thread::spawn(move || handle.block_on(io.env_get(key)))
                .join()
                .unwrap_or(Err(super::RuntimeIoError::Other("thread panicked".into())));
            match result {
                Ok(Some(v)) => Ok(v),
                Ok(None) | Err(_) => Err(std::env::VarError::NotPresent),
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

    impl ProvideFs for BexFsProvider {
        fn read_to_end(
            &self,
            path: &std::path::Path,
        ) -> std::pin::Pin<Box<dyn Future<Output = std::io::Result<Vec<u8>>> + Send + '_>> {
            let io = self.io.clone();
            let path_str = path.to_string_lossy().into_owned();
            Box::pin(async move {
                let file_handle = io
                    .fs_open(path_str, BexExternalValue::String("r".to_string()))
                    .await
                    .map_err(|_| {
                        std::io::Error::new(std::io::ErrorKind::NotFound, "file not found")
                    })?;
                let contents = io.fs_file_text(&file_handle).await.map_err(|_| {
                    std::io::Error::new(std::io::ErrorKind::NotFound, "file not found")
                })?;
                Ok(contents.into_bytes())
            })
        }

        fn write(
            &self,
            _path: &std::path::Path,
            _contents: &[u8],
        ) -> std::pin::Pin<Box<dyn Future<Output = std::io::Result<()>> + Send + '_>> {
            Box::pin(async { Err(std::io::Error::other("not implemented")) })
        }
    }
}

// ---------------------------------------------------------------------------
// WASM: async credential provider for AWS SDK config loading
// ---------------------------------------------------------------------------

#[cfg(target_arch = "wasm32")]
mod wasm_providers {
    use std::sync::Arc;

    use aws_credential_types::{
        Credentials,
        provider::{self, future::ProvideCredentials},
    };

    use super::RuntimeIo;

    /// Async credential provider that reads AWS env vars via `RuntimeIo`.
    pub(super) struct EnvCredentialProvider {
        pub io: Arc<dyn RuntimeIo>,
    }

    impl std::fmt::Debug for EnvCredentialProvider {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("EnvCredentialProvider").finish()
        }
    }

    impl EnvCredentialProvider {
        async fn resolve(&self) -> provider::Result {
            let access_key_id = self
                .io
                .env_get("AWS_ACCESS_KEY_ID".into())
                .await
                .ok()
                .flatten()
                .ok_or_else(|| {
                    provider::error::CredentialsError::unhandled("AWS_ACCESS_KEY_ID not set")
                })?;

            let secret_access_key = self
                .io
                .env_get("AWS_SECRET_ACCESS_KEY".into())
                .await
                .ok()
                .flatten()
                .ok_or_else(|| {
                    provider::error::CredentialsError::unhandled("AWS_SECRET_ACCESS_KEY not set")
                })?;

            let session_token = self
                .io
                .env_get("AWS_SESSION_TOKEN".into())
                .await
                .ok()
                .flatten();

            Ok(Credentials::new(
                access_key_id,
                secret_access_key,
                session_token,
                None,
                "baml-bedrock-wasm",
            ))
        }
    }

    impl aws_credential_types::provider::ProvideCredentials for EnvCredentialProvider {
        fn provide_credentials<'a>(&'a self) -> ProvideCredentials<'a>
        where
            Self: 'a,
        {
            ProvideCredentials::new(self.resolve())
        }
    }
}

// ---------------------------------------------------------------------------
// Custom HTTP connector bridging to RuntimeIo
// ---------------------------------------------------------------------------

/// An [`aws_smithy_runtime_api::client::http::HttpConnector`] that delegates
/// all HTTP traffic to a BAML [`RuntimeIo`] implementation.
#[derive(Clone)]
struct BamlHttpConnector {
    io: Arc<dyn RuntimeIo>,
}

// The AWS SDK HttpConnector trait requires `UnwindSafe + RefUnwindSafe`.
impl std::fmt::Debug for BamlHttpConnector {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BamlHttpConnector").finish()
    }
}

impl aws_smithy_runtime_api::client::http::HttpConnector for BamlHttpConnector {
    fn call(&self, request: smithy_http::Request) -> HttpConnectorFuture {
        let io = self.io.clone();
        HttpConnectorFuture::new(async move {
            let method = request.method().to_string();
            let url = request.uri().to_string();
            let mut headers = IndexMap::new();
            for (name, value) in request.headers() {
                headers.insert(name.to_string(), value.to_string());
            }
            let body = request
                .body()
                .bytes()
                .map(|b| String::from_utf8_lossy(b).into_owned())
                .unwrap_or_default();

            let io_req = sys_types::generated::owned::http::Request {
                method,
                url,
                headers,
                body,
            };

            let resp = io
                .http_send(io_req)
                .await
                .map_err(|e| ConnectorError::other(Box::new(e), None))?;

            let resp_body = io
                .http_response_text(&resp)
                .await
                .map_err(|e| ConnectorError::other(Box::new(e), None))?;

            let status =
                smithy_http::StatusCode::try_from(u16::try_from(resp.status_code).unwrap_or(500))
                    .map_err(|e| ConnectorError::other(Box::new(e), None))?;
            let sdk_body = SdkBody::from(resp_body);
            let mut aws_resp = smithy_http::Response::new(status, sdk_body);
            for (name, value) in resp.headers {
                aws_resp
                    .headers_mut()
                    .try_insert(name, value)
                    .map_err(|e| ConnectorError::other(e.into(), None))?;
            }

            Ok(aws_resp)
        })
    }
}

fn baml_http_client(
    io: Arc<dyn RuntimeIo>,
) -> aws_smithy_runtime_api::client::http::SharedHttpClient {
    use aws_smithy_runtime_api::client::http::http_client_fn;
    let connector = SharedHttpConnector::new(BamlHttpConnector { io });
    http_client_fn(move |_settings, _components| connector.clone())
}

// ---------------------------------------------------------------------------
// AWS SDK config loading
// ---------------------------------------------------------------------------

/// Load the AWS SDK config using the provided `RuntimeIo` for IO.
///
/// Accepts `Arc<dyn RuntimeIo>` because the AWS SDK config loader stores
/// provider objects that capture the IO handle (they need owned handles).
async fn load_aws_sdk_config(
    #[cfg_attr(target_arch = "wasm32", allow(unused))] bedrock_opts: &BedrockOptions,
    io: Arc<dyn RuntimeIo>,
) -> aws_config::SdkConfig {
    #[cfg(not(target_arch = "wasm32"))]
    {
        use aws_types::os_shim_internal::{Env, Fs};
        let mut loader = aws_config::defaults(aws_config::BehaviorVersion::latest())
            .http_client(baml_http_client(io.clone()))
            .env(Env::from_custom(native_providers::BexEnvProvider {
                io: io.clone(),
            }))
            .fs(Fs::from_custom(native_providers::BexFsProvider {
                io: io.clone(),
            }));
        if let Some(profile) = &bedrock_opts.profile {
            loader = loader.profile_name(profile);
        }
        loader.load().await
    }

    #[cfg(target_arch = "wasm32")]
    {
        aws_config::defaults(aws_config::BehaviorVersion::latest())
            .sleep_impl(crate::wasm::BrowserSleep)
            .time_source(crate::wasm::BrowserTime)
            .http_client(baml_http_client(io.clone()))
            .credentials_provider(wasm_providers::EnvCredentialProvider { io: io.clone() })
            .load()
            .await
    }
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Add `SigV4` auth headers to a Bedrock request.
pub(crate) async fn auth_bedrock(
    request: &mut HttpRequest,
    client: &PrimitiveClient,
    io: Arc<dyn RuntimeIo>,
) -> Result<(), BuildRequestError> {
    let bedrock_opts = match &client.provider_options {
        Some(ProviderOptions::Bedrock(opts)) => opts.clone(),
        _ => BedrockOptions::default(),
    };

    let credentials = resolve_credentials(&bedrock_opts, io.clone()).await?;
    let region = resolve_region(&bedrock_opts, io).await?;

    let signed_headers = sign_with_credentials(
        &credentials,
        &region,
        &request.method,
        &request.url,
        &request.headers,
        request.body.as_bytes(),
    )?;
    request.headers.extend(signed_headers);

    Ok(())
}

// ---------------------------------------------------------------------------
// Credential resolution
// ---------------------------------------------------------------------------

/// Resolve the AWS region from explicit options or the default provider chain.
pub(crate) async fn resolve_region(
    opts: &BedrockOptions,
    io: Arc<dyn RuntimeIo>,
) -> Result<String, BuildRequestError> {
    if let Some(region) = &opts.region {
        return Ok(region.clone());
    }

    let sdk_config = load_aws_sdk_config(opts, io).await;
    sdk_config.region().map(ToString::to_string).ok_or_else(|| {
        BuildRequestError::AuthorizationFailed(
            "AWS region not found in default provider chain".into(),
        )
    })
}

/// Resolve AWS credentials from explicit options or the default provider chain.
async fn resolve_credentials(
    opts: &BedrockOptions,
    io: Arc<dyn RuntimeIo>,
) -> Result<Credentials, BuildRequestError> {
    // Prefer explicit credentials from client options.
    if let Some(creds) = credentials_from_options(opts) {
        return Ok(creds);
    }

    // Fall back to the AWS provider chain via RuntimeIo.
    let sdk_config = load_aws_sdk_config(opts, io).await;
    let credentials_provider = sdk_config.credentials_provider().ok_or_else(|| {
        BuildRequestError::AuthorizationFailed(
            "AWS credentials provider not found in default provider chain".into(),
        )
    })?;
    credentials_provider
        .provide_credentials()
        .await
        .map_err(|e| {
            BuildRequestError::AuthorizationFailed(format!(
                "failed to load credentials from default provider chain: {e}"
            ))
        })
}

/// Try to build credentials from explicit client options.
fn credentials_from_options(opts: &BedrockOptions) -> Option<Credentials> {
    let access_key_id = opts.access_key_id.as_ref()?;
    let secret_access_key = opts.secret_access_key.as_ref()?;
    Some(Credentials::new(
        access_key_id,
        secret_access_key,
        opts.session_token.clone(),
        None,
        "baml-bedrock",
    ))
}

// ---------------------------------------------------------------------------
// SigV4 signing
// ---------------------------------------------------------------------------

/// Sign the request with `SigV4` given resolved credentials and region.
fn sign_with_credentials(
    credentials: &Credentials,
    region: &str,
    method: &str,
    url: &str,
    existing_headers: &IndexMap<String, String>,
    body: &[u8],
) -> Result<IndexMap<String, String>, BuildRequestError> {
    let identity = credentials.clone().into();

    let signing_settings = SigningSettings::default();
    let signing_params = v4::SigningParams::builder()
        .identity(&identity)
        .region(region)
        .name("bedrock")
        .time(now())
        .settings(signing_settings)
        .build()
        .map_err(|e| BuildRequestError::AuthorizationFailed(format!("SigV4 params: {e}")))?
        .into();

    let header_pairs: Vec<(&str, &str)> = existing_headers
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();

    let signable = SignableRequest::new(
        method,
        url,
        header_pairs.into_iter(),
        SignableBody::Bytes(body),
    )
    .map_err(|e| BuildRequestError::AuthorizationFailed(format!("SigV4 signable request: {e}")))?;

    let (instructions, _signature) = sign(signable, &signing_params)
        .map_err(|e| BuildRequestError::AuthorizationFailed(format!("SigV4 signing: {e}")))?
        .into_parts();

    let mut signed_headers = IndexMap::new();
    for (name, value) in instructions.headers() {
        signed_headers.insert(name.to_string(), value.to_string());
    }

    Ok(signed_headers)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use std::{
        future::Future,
        panic::{RefUnwindSafe, UnwindSafe},
        pin::Pin,
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
    };

    use bex_external_types::AsBexExternalValue;

    use super::*;
    use crate::baml_std::PrimitiveClientOptions;

    fn make_client(opts: BedrockOptions) -> PrimitiveClient {
        PrimitiveClient::new(
            "test-bedrock".to_string(),
            "aws-bedrock".to_string(),
            PrimitiveClientOptions {
                model: Some("test-model".to_string()),
                provider_options: opts.into_bex_external_value(),
                ..Default::default()
            },
        )
        .unwrap()
    }

    fn base_bedrock_opts() -> BedrockOptions {
        BedrockOptions {
            region: Some("us-east-1".to_string()),
            access_key_id: Some("AKIAIOSFODNN7EXAMPLE".to_string()),
            secret_access_key: Some("wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY".to_string()),
            ..Default::default()
        }
    }

    fn fake_bedrock_request() -> HttpRequest {
        let mut headers = IndexMap::new();
        headers.insert("content-type".to_string(), "application/json".to_string());
        HttpRequest {
            method: "POST".to_string(),
            url: "https://bedrock-runtime.us-east-1.amazonaws.com/model/some-model/converse"
                .to_string(),
            headers,
            body: r#"{"messages":[]}"#.to_string(),
        }
    }

    /// A stub `RuntimeIo` that returns sensible defaults for Bedrock-relevant
    /// operations and `Unsupported` for everything else.
    struct StubIo;

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
    }

    fn stub_io() -> Arc<dyn RuntimeIo> {
        Arc::new(StubIo)
    }

    /// A mock `RuntimeIo` that tracks HTTP calls and returns configurable responses.
    struct MockHttpIo {
        call_count: Arc<AtomicUsize>,
        status: u16,
        body: &'static str,
    }

    impl RuntimeIo for MockHttpIo {
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
            self.call_count.fetch_add(1, Ordering::SeqCst);
            let status = i64::from(self.status);
            Box::pin(async move {
                Ok(sys_types::runtime_io::HttpResponseHandle {
                    raw: bex_external_types::BexExternalValue::Null,
                    status_code: status,
                    headers: IndexMap::new(),
                    url: String::new(),
                })
            })
        }

        fn http_response_text(
            &self,
            _: &sys_types::runtime_io::HttpResponseHandle,
        ) -> Pin<Box<dyn Future<Output = Result<String, RuntimeIoError>> + Send + '_>> {
            let body = self.body.to_string();
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
    }

    fn mock_http_io(
        call_count: Arc<AtomicUsize>,
        status: u16,
        body: &'static str,
    ) -> Arc<dyn RuntimeIo> {
        Arc::new(MockHttpIo {
            call_count,
            status,
            body,
        })
    }

    /// A mock `RuntimeIo` with custom `env_get` behavior.
    struct EnvIo<F: Fn(String) -> Option<String> + Send + Sync> {
        env_fn: F,
    }

    impl<F: Fn(String) -> Option<String> + Send + Sync + UnwindSafe + RefUnwindSafe> RuntimeIo
        for EnvIo<F>
    {
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
            Box::pin(async { Ok(String::new()) })
        }

        fn env_get(
            &self,
            key: String,
        ) -> Pin<Box<dyn Future<Output = Result<Option<String>, RuntimeIoError>> + Send + '_>>
        {
            let result = (self.env_fn)(key);
            Box::pin(async move { Ok(result) })
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
    }

    /// A mock `RuntimeIo` with custom env + fs behavior that stores file
    /// contents and tracks the last opened path.
    struct EnvFsContentIo<E>
    where
        E: Fn(String) -> Option<String> + Send + Sync,
    {
        env_fn: E,
        fs_contents: std::collections::HashMap<String, String>,
        last_opened: std::sync::Mutex<Option<String>>,
    }

    impl<E> RuntimeIo for EnvFsContentIo<E>
    where
        E: Fn(String) -> Option<String> + Send + Sync + UnwindSafe + RefUnwindSafe,
    {
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
            Box::pin(async { Ok(String::new()) })
        }

        fn env_get(
            &self,
            key: String,
        ) -> Pin<Box<dyn Future<Output = Result<Option<String>, RuntimeIoError>> + Send + '_>>
        {
            let result = (self.env_fn)(key);
            Box::pin(async move { Ok(result) })
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
            if self.fs_contents.contains_key(&path) {
                *self.last_opened.lock().unwrap() = Some(path);
                Box::pin(async {
                    Ok(sys_types::runtime_io::FsFileHandle {
                        raw: bex_external_types::BexExternalValue::Null,
                    })
                })
            } else {
                Box::pin(async { Err(RuntimeIoError::Other("not found".into())) })
            }
        }

        fn fs_file_text(
            &self,
            _: &sys_types::runtime_io::FsFileHandle,
        ) -> Pin<Box<dyn Future<Output = Result<String, RuntimeIoError>> + Send + '_>> {
            let path = self.last_opened.lock().unwrap().clone();
            let content = path.and_then(|p| self.fs_contents.get(&p).cloned());
            Box::pin(
                async move { content.ok_or_else(|| RuntimeIoError::Other("not found".into())) },
            )
        }
    }

    /// A mock `RuntimeIo` with custom env + http behavior.
    struct EnvHttpIo<E>
    where
        E: Fn(String) -> Option<String> + Send + Sync,
    {
        env_fn: E,
        http_call_count: Arc<AtomicUsize>,
        http_status: u16,
        http_body: String,
    }

    impl<E> RuntimeIo for EnvHttpIo<E>
    where
        E: Fn(String) -> Option<String> + Send + Sync + UnwindSafe + RefUnwindSafe,
    {
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
            self.http_call_count.fetch_add(1, Ordering::SeqCst);
            let status = i64::from(self.http_status);
            Box::pin(async move {
                Ok(sys_types::runtime_io::HttpResponseHandle {
                    raw: bex_external_types::BexExternalValue::Null,
                    status_code: status,
                    headers: IndexMap::new(),
                    url: String::new(),
                })
            })
        }

        fn http_response_text(
            &self,
            _: &sys_types::runtime_io::HttpResponseHandle,
        ) -> Pin<Box<dyn Future<Output = Result<String, RuntimeIoError>> + Send + '_>> {
            let body = self.http_body.clone();
            Box::pin(async move { Ok(body) })
        }

        fn env_get(
            &self,
            key: String,
        ) -> Pin<Box<dyn Future<Output = Result<Option<String>, RuntimeIoError>> + Send + '_>>
        {
            let result = (self.env_fn)(key);
            Box::pin(async move { Ok(result) })
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
    }

    // -----------------------------------------------------------------------
    // Tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn sigv4_headers_present() {
        let client = make_client(base_bedrock_opts());
        let mut req = fake_bedrock_request();
        auth_bedrock(
            &mut req,
            &client,
            Arc::new(sys_types::runtime_io::NoopRuntimeIo),
        )
        .await
        .unwrap();
        assert!(req.headers.contains_key("authorization"));
        assert!(req.headers.contains_key("x-amz-date"));
    }

    #[tokio::test]
    async fn explicit_credentials_used_without_io() {
        let client = make_client(base_bedrock_opts());
        let mut req = fake_bedrock_request();
        let result = auth_bedrock(
            &mut req,
            &client,
            Arc::new(sys_types::runtime_io::NoopRuntimeIo),
        )
        .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn fails_without_explicit_credentials_uses_io() {
        let call_count = Arc::new(AtomicUsize::new(0));
        let io = mock_http_io(call_count.clone(), 404, "");
        let client = make_client(BedrockOptions {
            region: Some("us-east-1".to_string()),
            ..Default::default()
        });
        let mut req = fake_bedrock_request();
        let result = auth_bedrock(&mut req, &client, io.clone()).await;
        assert!(result.is_err());
        // The IO was invoked during credential resolution.
        assert!(call_count.load(Ordering::SeqCst) > 0);
    }

    #[tokio::test]
    async fn http_send_not_invoked_with_explicit_credentials() {
        let call_count = Arc::new(AtomicUsize::new(0));
        let io = mock_http_io(call_count.clone(), 200, "");
        let client = make_client(base_bedrock_opts());
        let mut req = fake_bedrock_request();
        let result = auth_bedrock(&mut req, &client, io.clone()).await;
        assert!(result.is_ok());
        assert_eq!(call_count.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn resolve_region_from_explicit_option() {
        let opts = BedrockOptions {
            region: Some("eu-west-1".into()),
            ..Default::default()
        };
        let region = resolve_region(&opts, Arc::new(sys_types::runtime_io::NoopRuntimeIo))
            .await
            .unwrap();
        assert_eq!(region, "eu-west-1");
    }

    #[tokio::test]
    async fn resolve_region_from_env_via_io() {
        let io: Arc<dyn RuntimeIo> = Arc::new(EnvIo {
            env_fn: |key| match key.as_str() {
                "AWS_REGION" => Some("ap-southeast-1".into()),
                _ => None,
            },
        });
        let opts = BedrockOptions::default();
        let region = resolve_region(&opts, io.clone()).await.unwrap();
        assert_eq!(region, "ap-southeast-1");
    }

    #[tokio::test]
    async fn resolve_region_missing_with_empty_env() {
        let io = stub_io();
        let opts = BedrockOptions::default();
        let result = resolve_region(&opts, io.clone()).await;
        assert!(result.is_err());
    }

    /// Confirms the full injection flow: env IO provides AWS credentials
    /// and region, which are resolved through the AWS SDK config loader and
    /// used to produce valid `SigV4` headers on the request.
    #[tokio::test]
    async fn sigv4_headers_from_env_via_io() {
        let io: Arc<dyn RuntimeIo> = Arc::new(EnvIo {
            env_fn: |key| match key.as_str() {
                "AWS_ACCESS_KEY_ID" => Some("AKIAIOSFODNN7EXAMPLE".to_string()),
                "AWS_SECRET_ACCESS_KEY" => {
                    Some("wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY".to_string())
                }
                "AWS_REGION" => Some("us-east-1".to_string()),
                _ => None,
            },
        });
        let client = make_client(BedrockOptions::default());
        let mut req = fake_bedrock_request();
        auth_bedrock(&mut req, &client, io.clone()).await.unwrap();
        assert!(req.headers.contains_key("authorization"));
        assert!(req.headers.contains_key("x-amz-date"));
    }

    /// Confirms that the fs IO is used by the AWS SDK config loader.
    #[tokio::test]
    async fn sigv4_headers_from_credentials_file_via_fs_io() {
        let credentials_file = "\
[default]
aws_access_key_id = AKIAIOSFODNN7EXAMPLE
aws_secret_access_key = wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY
";
        let mut fs_contents = std::collections::HashMap::new();
        fs_contents.insert(
            "/fake/aws/credentials".to_string(),
            credentials_file.to_string(),
        );
        let io: Arc<dyn RuntimeIo> = Arc::new(EnvFsContentIo {
            env_fn: |key| match key.as_str() {
                "AWS_SHARED_CREDENTIALS_FILE" => Some("/fake/aws/credentials".to_string()),
                "AWS_REGION" => Some("us-east-1".to_string()),
                _ => None,
            },
            fs_contents,
            last_opened: std::sync::Mutex::new(None),
        });
        let client = make_client(BedrockOptions::default());
        let mut req = fake_bedrock_request();
        auth_bedrock(&mut req, &client, io.clone()).await.unwrap();
        assert!(req.headers.contains_key("authorization"));
        assert!(req.headers.contains_key("x-amz-date"));
    }

    /// Confirms that the http IO is used when the AWS SDK falls back
    /// to the container credentials provider.
    #[tokio::test]
    async fn sigv4_headers_from_container_credentials_via_http_io() {
        let http_call_count = Arc::new(AtomicUsize::new(0));
        let io: Arc<dyn RuntimeIo> = Arc::new(EnvHttpIo {
            env_fn: |key| match key.as_str() {
                "AWS_CONTAINER_CREDENTIALS_FULL_URI" => {
                    Some("http://169.254.170.23/creds".to_string())
                }
                "AWS_REGION" => Some("us-east-1".to_string()),
                _ => None,
            },
            http_call_count: http_call_count.clone(),
            http_status: 200,
            http_body: serde_json::json!({
                "AccessKeyId": "AKIAIOSFODNN7EXAMPLE",
                "SecretAccessKey": "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY",
                "Token": "FwoGZXIvYXdzEBYaDH...",
                "Expiration": "2099-01-01T00:00:00Z",
            })
            .to_string(),
        });
        let client = make_client(BedrockOptions::default());
        let mut req = fake_bedrock_request();
        auth_bedrock(&mut req, &client, io.clone()).await.unwrap();
        assert!(http_call_count.load(Ordering::SeqCst) > 0);
        assert!(req.headers.contains_key("authorization"));
        assert!(req.headers.contains_key("x-amz-date"));
    }

    #[test]
    fn credentials_from_options_complete() {
        let opts = base_bedrock_opts();
        assert!(credentials_from_options(&opts).is_some());
    }

    #[test]
    fn credentials_from_options_missing_secret() {
        let opts = BedrockOptions {
            access_key_id: Some("AKID".to_string()),
            ..Default::default()
        };
        assert!(credentials_from_options(&opts).is_none());
    }

    #[test]
    fn credentials_from_options_empty() {
        assert!(credentials_from_options(&BedrockOptions::default()).is_none());
    }
}
