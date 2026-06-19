//! AWS Bedrock request authorization: credential resolution + `SigV4` signing.
//!
//! Credentials and region are resolved via the slim `aws-config` fork, whose
//! IO is routed through BAML's [`RuntimeIo`] by [`BamlCredentialIo`]. Signing
//! uses the slim `aws-sigv4` fork. No Smithy runtime is involved.

use std::sync::Arc;

use async_trait::async_trait;
use aws_config::{CommandOutput, ConfigError, CredentialIo, Credentials, HttpResponse};
use indexmap::IndexMap;
use sys_types::{BexExternalValue, runtime_io::RuntimeIo};
use web_time::SystemTime;

use crate::{
    baml_std::{BedrockOptions, HttpRequest, PrimitiveClient, ProviderOptions},
    build_request::BuildRequestError,
};

// ---------------------------------------------------------------------------
// Platform helpers
// ---------------------------------------------------------------------------

fn now() -> SystemTime {
    SystemTime::now()
}

// ---------------------------------------------------------------------------
// CredentialIo adapter over RuntimeIo
// ---------------------------------------------------------------------------

/// Bridges the `aws-config` [`CredentialIo`] trait to BAML's [`RuntimeIo`].
///
/// Environment, file, and HTTP access all go through the runtime so credential
/// resolution stays inside BAML's sandbox. `credential_process` execution uses
/// a native subprocess (it has no analogue in `RuntimeIo`).
struct BamlCredentialIo {
    io: Arc<dyn RuntimeIo>,
}

#[async_trait]
impl CredentialIo for BamlCredentialIo {
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
    ) -> Result<HttpResponse, ConfigError> {
        let mut header_map = IndexMap::new();
        for (k, v) in headers {
            header_map.insert(k.clone(), v.clone());
        }
        let request = sys_types::generated::owned::http::Request {
            method: method.to_string(),
            url: url.to_string(),
            headers: header_map,
            body: String::new(),
        };
        let resp = self
            .io
            // Unbounded, as before: `0n` -> no deadline.
            .http__send(request, std::sync::Arc::new(num_bigint::BigInt::from(0i64)))
            .await
            .map_err(|e| ConfigError::Io(e.to_string()))?;
        let body = self
            .io
            .http_response_text(&resp)
            .await
            .map_err(|e| ConfigError::Io(e.to_string()))?;
        Ok(HttpResponse {
            status: u16::try_from(resp.status_code).unwrap_or(0),
            body,
        })
    }

    async fn run_command(&self, command: &str) -> Result<CommandOutput, ConfigError> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let output = std::process::Command::new("sh")
                .arg("-c")
                .arg(command)
                .output()
                .map_err(|e| ConfigError::Io(format!("failed to spawn credential_process: {e}")))?;
            Ok(CommandOutput {
                status: output.status.code().unwrap_or(-1),
                stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            })
        }
        #[cfg(target_arch = "wasm32")]
        {
            let _ = command;
            Err(ConfigError::Io(
                "credential_process is not supported on wasm".into(),
            ))
        }
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

    let header_pairs: Vec<(&str, &str)> = request
        .headers
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();

    let signed = aws_sigv4::sign_request(
        &request.method,
        &request.url,
        &header_pairs,
        request.body.as_bytes(),
        &credentials,
        &region,
        "bedrock",
        now(),
    )
    .map_err(|e| BuildRequestError::AuthorizationFailed(format!("SigV4 signing: {e}")))?;

    for (name, value) in signed {
        request.headers.insert(name, value);
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Credential + region resolution
// ---------------------------------------------------------------------------

/// Resolve the AWS region from explicit options or the default provider chain.
pub(crate) async fn resolve_region(
    opts: &BedrockOptions,
    io: Arc<dyn RuntimeIo>,
) -> Result<String, BuildRequestError> {
    if let Some(region) = &opts.region {
        return Ok(region.clone());
    }

    let adapter = BamlCredentialIo { io };
    aws_config::resolve_region(&adapter, opts.profile.as_deref())
        .await
        .ok_or_else(|| {
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
    let adapter = BamlCredentialIo { io };
    aws_config::resolve_credentials(&adapter, opts.profile.as_deref())
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
        access_key_id.clone(),
        secret_access_key.clone(),
        opts.session_token.clone(),
    ))
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
    use sys_types::runtime_io::RuntimeIoError;

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
    /// and region, which are resolved through the AWS provider chain and
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

    /// Confirms that the fs IO is used by the AWS provider chain.
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

    /// Confirms that the http IO is used when the provider chain falls back
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
