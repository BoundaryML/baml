//! Bridges the forks' IO traits to BAML's [`RuntimeIo`].
//!
//! One adapter implements both `google_cloud_auth::TokenIo` and
//! `aws_config::CredentialIo`: they need the same three capabilities (env,
//! file read, HTTP), differing only in whether the request carries a body.
//! Routing them through `RuntimeIo` keeps credential resolution inside BAML's
//! sandbox — the host decides what env vars and paths are visible.

use std::sync::Arc;

use async_trait::async_trait;
use indexmap::IndexMap;
use sys_types::{BexExternalValue, runtime_io::RuntimeIo};

pub(crate) struct BamlAuthIo {
    pub(crate) io: Arc<dyn RuntimeIo>,
}

impl BamlAuthIo {
    async fn env_var(&self, key: &str) -> Option<String> {
        self.io.env_get(key.to_string()).await.ok().flatten()
    }

    async fn file_text(&self, path: &str) -> Option<String> {
        let handle = self
            .io
            .fs_open(path.to_string(), BexExternalValue::String("r".into()))
            .await
            .ok()?;
        self.io.fs_file_text(&handle).await.ok()
    }

    /// One HTTP round trip through the runtime, returning `(status, body)`.
    async fn request(
        &self,
        method: &str,
        url: &str,
        headers: &[(String, String)],
        body: String,
    ) -> Result<(u16, String), String> {
        let mut header_map = IndexMap::new();
        for (k, v) in headers {
            header_map.insert(k.clone(), v.clone());
        }
        let request = sys_types::generated::owned::http::Request {
            method: method.to_string(),
            url: url.to_string(),
            headers: header_map,
            body,
        };
        let resp = self
            .io
            // Unbounded: `0n` -> no deadline. Credential endpoints (token,
            // IMDS, container) are already fast-failing.
            .http__send(request, Arc::new(num_bigint::BigInt::from(0i64)))
            .await
            .map_err(|e| e.to_string())?;
        let text = self
            .io
            .http_response_text(&resp)
            .await
            .map_err(|e| e.to_string())?;
        Ok((u16::try_from(resp.status_code).unwrap_or(0), text))
    }
}

#[async_trait]
impl google_cloud_auth::TokenIo for BamlAuthIo {
    async fn env(&self, key: &str) -> Option<String> {
        self.env_var(key).await
    }

    async fn read_file(&self, path: &str) -> Option<String> {
        self.file_text(path).await
    }

    async fn http(
        &self,
        method: &str,
        url: &str,
        headers: &[(String, String)],
        body: &str,
    ) -> Result<google_cloud_auth::HttpResponse, google_cloud_auth::AuthError> {
        let (status, body) = self
            .request(method, url, headers, body.to_string())
            .await
            .map_err(google_cloud_auth::AuthError::Io)?;
        Ok(google_cloud_auth::HttpResponse { status, body })
    }
}

#[async_trait]
impl aws_config::CredentialIo for BamlAuthIo {
    async fn env(&self, key: &str) -> Option<String> {
        self.env_var(key).await
    }

    async fn read_file(&self, path: &str) -> Option<String> {
        self.file_text(path).await
    }

    async fn http(
        &self,
        method: &str,
        url: &str,
        headers: &[(String, String)],
    ) -> Result<aws_config::HttpResponse, aws_config::ConfigError> {
        let (status, body) = self
            .request(method, url, headers, String::new())
            .await
            .map_err(aws_config::ConfigError::Io)?;
        Ok(aws_config::HttpResponse { status, body })
    }

    async fn run_command(
        &self,
        command: &str,
    ) -> Result<aws_config::CommandOutput, aws_config::ConfigError> {
        // `credential_process` has no analogue in `RuntimeIo`; it runs as a
        // native subprocess, and is simply unavailable on wasm.
        #[cfg(not(target_arch = "wasm32"))]
        {
            let output = std::process::Command::new("sh")
                .arg("-c")
                .arg(command)
                .output()
                .map_err(|e| {
                    aws_config::ConfigError::Io(format!("failed to spawn credential_process: {e}"))
                })?;
            Ok(aws_config::CommandOutput {
                status: output.status.code().unwrap_or(-1),
                stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            })
        }
        #[cfg(target_arch = "wasm32")]
        {
            let _ = command;
            Err(aws_config::ConfigError::Io(
                "credential_process is not supported on wasm".into(),
            ))
        }
    }
}
