//! AWS Bedrock request authorization: credential resolution + `SigV4` signing.

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
use bex_heap::builtin_types::owned::LlmPrimitiveClient;
use indexmap::IndexMap;

use super::LlmRequestAuthorizer;
use crate::build_request::{
    BuildRequestCallbacks, BuildRequestError, RawHttpRequest, get_string_option,
};

// ---------------------------------------------------------------------------
// Platform helpers
// ---------------------------------------------------------------------------

/// Platform-aware `SystemTime::now()`.
///
/// On WASM, `std::time::SystemTime::now()` panics — use `web_time` instead.
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
    use std::future::Future;

    use aws_types::os_shim_internal::{ProvideEnv, ProvideFs};

    use crate::{EnvReadFn, FsReadFn};

    pub(super) struct BexEnvProvider {
        pub env_read_fn: EnvReadFn,
    }

    impl std::fmt::Debug for BexEnvProvider {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("BexEnvProvider").finish()
        }
    }

    impl ProvideEnv for BexEnvProvider {
        fn get(&self, k: &str) -> Result<String, std::env::VarError> {
            let fut = (self.env_read_fn)(k.to_string());
            // ProvideEnv::get is sync but EnvReadFn is async (and may contain
            // real awaits). We can't block_on from inside the tokio runtime,
            // and block_in_place only works on multi-threaded runtimes. A
            // short-lived thread lets us call block_on from outside the runtime.
            // This is a truly horrible workaround but I'm not sure what else to
            // do here.
            let handle = tokio::runtime::Handle::current();
            let result = std::thread::spawn(move || handle.block_on(fut))
                .join()
                .unwrap_or(Err(crate::LlmOpError::Other("thread panicked".into())));
            match result {
                Ok(Some(v)) => Ok(v),
                Ok(None) | Err(_) => Err(std::env::VarError::NotPresent),
            }
        }
    }

    pub(super) struct BexFsProvider {
        pub fs_read_fn: FsReadFn,
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
            let fut = (self.fs_read_fn)(path.to_string_lossy().into_owned());
            Box::pin(async move {
                match fut.await {
                    Ok(v) => Ok(v),
                    Err(_) => Err(std::io::Error::new(
                        std::io::ErrorKind::NotFound,
                        "file not found",
                    )),
                }
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
    use aws_credential_types::{
        Credentials,
        provider::{self, future::ProvideCredentials},
    };

    use crate::EnvReadFn;

    /// Async credential provider that reads AWS env vars via `EnvReadFn`.
    pub(super) struct EnvCredentialProvider {
        pub env_read: EnvReadFn,
    }

    impl std::fmt::Debug for EnvCredentialProvider {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("EnvCredentialProvider").finish()
        }
    }

    impl EnvCredentialProvider {
        async fn resolve(&self) -> provider::Result {
            let access_key_id = (self.env_read)("AWS_ACCESS_KEY_ID".into())
                .await
                .ok()
                .flatten()
                .ok_or_else(|| {
                    provider::error::CredentialsError::unhandled("AWS_ACCESS_KEY_ID not set")
                })?;

            let secret_access_key = (self.env_read)("AWS_SECRET_ACCESS_KEY".into())
                .await
                .ok()
                .flatten()
                .ok_or_else(|| {
                    provider::error::CredentialsError::unhandled("AWS_SECRET_ACCESS_KEY not set")
                })?;

            let session_token = (self.env_read)("AWS_SESSION_TOKEN".into())
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
// Custom HTTP connector bridging to HttpSendFn
// ---------------------------------------------------------------------------

/// An [`aws_smithy_runtime_api::client::http::HttpConnector`] that delegates
/// all HTTP traffic to a BAML [`HttpSendFn`](crate::HttpSendFn) closure.
#[derive(Clone)]
struct BamlHttpConnector {
    send_fn: crate::HttpSendFn,
}

impl std::fmt::Debug for BamlHttpConnector {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BamlHttpConnector").finish()
    }
}

impl aws_smithy_runtime_api::client::http::HttpConnector for BamlHttpConnector {
    fn call(&self, request: smithy_http::Request) -> HttpConnectorFuture {
        let send_fn = self.send_fn.clone();
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

            let baml_req = bex_heap::builtin_types::owned::HttpRequest {
                method,
                url,
                headers,
                body,
            };

            let resp = send_fn(baml_req)
                .await
                .map_err(|e| ConnectorError::other(e.into(), None))?;

            let status = smithy_http::StatusCode::try_from(resp.status_code)
                .map_err(|e| ConnectorError::other(Box::new(e), None))?;
            let sdk_body = SdkBody::from(resp.body);
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
    send_fn: crate::HttpSendFn,
) -> aws_smithy_runtime_api::client::http::SharedHttpClient {
    use aws_smithy_runtime_api::client::http::http_client_fn;
    let connector = SharedHttpConnector::new(BamlHttpConnector { send_fn });
    http_client_fn(move |_settings, _components| connector.clone())
}

// ---------------------------------------------------------------------------
// AWS SDK config loading
// ---------------------------------------------------------------------------

/// Load the AWS SDK config with platform-specific providers.
///
/// If the client has a `profile` option, it is passed to the SDK config
/// loader to select the named profile from `~/.aws/config` and
/// `~/.aws/credentials`.
pub(crate) async fn load_aws_sdk_config(
    client: &LlmPrimitiveClient,
    http_send: &crate::HttpSendFn,
    env_read: &crate::EnvReadFn,
    #[cfg_attr(target_arch = "wasm32", allow(unused))] fs_read: &crate::FsReadFn,
) -> aws_config::SdkConfig {
    let profile = get_string_option(client, "profile");

    #[cfg(not(target_arch = "wasm32"))]
    {
        use aws_types::os_shim_internal::{Env, Fs};
        let mut loader = aws_config::defaults(aws_config::BehaviorVersion::latest())
            .http_client(baml_http_client(http_send.clone()))
            .env(Env::from_custom(native_providers::BexEnvProvider {
                env_read_fn: env_read.clone(),
            }))
            .fs(Fs::from_custom(native_providers::BexFsProvider {
                fs_read_fn: fs_read.clone(),
            }));
        if let Some(profile) = profile {
            loader = loader.profile_name(profile);
        }
        loader.load().await
    }

    #[cfg(target_arch = "wasm32")]
    {
        let _ = profile; // profiles not supported on WASM
        aws_config::defaults(aws_config::BehaviorVersion::latest())
            .sleep_impl(crate::wasm::BrowserSleep)
            .time_source(crate::wasm::BrowserTime)
            .http_client(baml_http_client(http_send.clone()))
            .credentials_provider(wasm_providers::EnvCredentialProvider {
                env_read: env_read.clone(),
            })
            .load()
            .await
    }
}

// ---------------------------------------------------------------------------
// LlmRequestAuthorizer implementation
// ---------------------------------------------------------------------------

pub(crate) struct BedrockAuth;

impl LlmRequestAuthorizer for BedrockAuth {
    async fn authorize(
        &self,
        mut request: RawHttpRequest,
        client: &LlmPrimitiveClient,
        callbacks: &BuildRequestCallbacks<'_>,
    ) -> Result<RawHttpRequest, BuildRequestError> {
        let credentials = resolve_aws_credentials(
            client,
            callbacks.http_send,
            callbacks.env_read,
            callbacks.fs_read,
        )
        .await?;

        // Extract region from the URL (already set by build_request).
        let region = extract_region_from_url(&request.url)?;

        // Sign the request with SigV4.
        let signed_headers = sign_with_credentials(
            &credentials,
            &region,
            &request.method,
            &request.url,
            &request.headers,
            request.body.as_bytes(),
        )?;
        request.headers.extend(signed_headers);

        Ok(request)
    }
}

// ---------------------------------------------------------------------------
// Credential resolution
// ---------------------------------------------------------------------------

/// Extract the AWS region from a Bedrock URL like
/// `https://bedrock-runtime.us-east-1.amazonaws.com/...`
fn extract_region_from_url(url: &str) -> Result<String, BuildRequestError> {
    url.strip_prefix("https://bedrock-runtime.")
        .and_then(|rest| rest.split('.').next())
        .map(String::from)
        .ok_or_else(|| {
            BuildRequestError::AuthorizationFailed(format!(
                "could not extract region from Bedrock URL: {url}"
            ))
        })
}

/// Resolve AWS credentials from client options or the default provider chain.
async fn resolve_aws_credentials(
    client: &LlmPrimitiveClient,
    http_send: &crate::HttpSendFn,
    env_read: &crate::EnvReadFn,
    #[cfg_attr(target_arch = "wasm32", allow(unused))] fs_read: &crate::FsReadFn,
) -> Result<Credentials, BuildRequestError> {
    if let Some(creds) = credentials_from_options(client) {
        return Ok(creds);
    }

    let sdk_config = load_aws_sdk_config(client, http_send, env_read, fs_read).await;

    let credentials_provider = sdk_config.credentials_provider().ok_or_else(|| {
        BuildRequestError::MissingOption(
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

/// Try to extract explicit credentials from client options.
fn credentials_from_options(client: &LlmPrimitiveClient) -> Option<Credentials> {
    let access_key_id = get_string_option(client, "access_key_id")?;
    let secret_access_key = get_string_option(client, "secret_access_key")?;
    let session_token = get_string_option(client, "session_token");
    Some(Credentials::new(
        access_key_id,
        secret_access_key,
        session_token,
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

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    use bex_external_types::BexExternalValue;
    use indexmap::IndexMap;

    use super::*;
    use crate::build_request::{BuildRequestCallbacks, RawHttpRequest};

    fn make_client(options: Vec<(&str, BexExternalValue)>) -> LlmPrimitiveClient {
        let mut opts = IndexMap::new();
        for (k, v) in options {
            opts.insert(k.to_string(), v);
        }
        LlmPrimitiveClient {
            name: "test-bedrock".to_string(),
            provider: "aws-bedrock".to_string(),
            default_role: "user".to_string(),
            allowed_roles: vec![
                "system".to_string(),
                "user".to_string(),
                "assistant".to_string(),
            ],
            options: opts,
        }
    }

    fn base_options() -> Vec<(&'static str, BexExternalValue)> {
        vec![
            ("region", BexExternalValue::String("us-east-1".into())),
            (
                "access_key_id",
                BexExternalValue::String("AKIAIOSFODNN7EXAMPLE".into()),
            ),
            (
                "secret_access_key",
                BexExternalValue::String("wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY".into()),
            ),
        ]
    }

    /// A minimal unsigned Bedrock request for auth tests.
    fn fake_request() -> RawHttpRequest {
        let mut headers = IndexMap::new();
        headers.insert("content-type".to_string(), "application/json".to_string());
        RawHttpRequest {
            method: "POST".to_string(),
            url: "https://bedrock-runtime.us-east-1.amazonaws.com/model/some-model/converse"
                .to_string(),
            headers,
            body: r#"{"messages":[]}"#.to_string(),
        }
    }

    fn mock_http_send(
        call_count: Arc<AtomicUsize>,
        status: u16,
        body: &'static str,
    ) -> crate::HttpSendFn {
        Arc::new(move |_req| {
            call_count.fetch_add(1, Ordering::SeqCst);
            let body = body.to_string();
            Box::pin(async move {
                Ok(crate::HttpSendResponse {
                    status_code: status,
                    headers: IndexMap::new(),
                    body,
                })
            })
        })
    }

    #[tokio::test]
    async fn sigv4_headers_present() {
        let client = make_client(base_options());
        let (h, e, f) = crate::build_request::stub_callbacks();
        let callbacks = BuildRequestCallbacks {
            http_send: &h,
            env_read: &e,
            fs_read: &f,
        };
        let result = BedrockAuth
            .authorize(fake_request(), &client, &callbacks)
            .await
            .unwrap();
        assert!(result.headers.contains_key("authorization"));
        assert!(result.headers.contains_key("x-amz-date"));
    }

    #[tokio::test]
    async fn fails_without_explicit_credentials() {
        let call_count = Arc::new(AtomicUsize::new(0));
        let send_fn = mock_http_send(call_count, 404, "");
        let (e, f) = crate::build_request::noop_env_fs_callbacks();
        let client = make_client(vec![]);
        let callbacks = BuildRequestCallbacks {
            http_send: &send_fn,
            env_read: &e,
            fs_read: &f,
        };
        let result = BedrockAuth
            .authorize(fake_request(), &client, &callbacks)
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn http_send_invoked_during_credential_resolution() {
        let call_count = Arc::new(AtomicUsize::new(0));
        let send_fn = mock_http_send(call_count.clone(), 404, "");
        let (e, f) = crate::build_request::noop_env_fs_callbacks();
        let client = make_client(vec![]);
        let callbacks = BuildRequestCallbacks {
            http_send: &send_fn,
            env_read: &e,
            fs_read: &f,
        };
        let _result = BedrockAuth
            .authorize(fake_request(), &client, &callbacks)
            .await;
        assert!(call_count.load(Ordering::SeqCst) > 0);
    }

    #[tokio::test]
    async fn http_send_not_invoked_with_explicit_credentials() {
        let call_count = Arc::new(AtomicUsize::new(0));
        let send_fn = mock_http_send(call_count.clone(), 200, "");
        let client = make_client(base_options());
        let (_, e, f) = crate::build_request::stub_callbacks();
        let callbacks = BuildRequestCallbacks {
            http_send: &send_fn,
            env_read: &e,
            fs_read: &f,
        };
        let result = BedrockAuth
            .authorize(fake_request(), &client, &callbacks)
            .await;
        assert!(result.is_ok());
        assert_eq!(call_count.load(Ordering::SeqCst), 0);
    }
}
