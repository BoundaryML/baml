//! Native Tokio-based system operations provider.
//!
//! This crate provides `SysOps::native()` via an extension trait, which returns
//! a function pointer table with Tokio-based implementations for all system operations.
//!
//! # Usage
//!
//! ```ignore
//! use sys_native::SysOpsExt;
//! use bex_engine::BexEngine;
//!
//! let engine = BexEngine::new(program, SysOps::native())?;
//! ```

mod ops;
pub mod registry;

// Re-export types from sys_types for convenience
use bex_heap::builtin_types;
pub use sys_types::{
    CallId, CompletionHandle, OpError, SysOp, SysOpContext, SysOpEnv, SysOpFn, SysOpFs, SysOpHttp,
    SysOpLlm, SysOpNet, SysOpResult, SysOpSys, SysOps,
};
use sys_types::{OpErrorKind, SysOpOutput};

/// The native Tokio-based `sys_op` provider.
///
/// Implements per-module traits (`SysOpFs`, `SysOpHttp`, etc.) with clean
/// typed signatures. The generated glue handles arg extraction and error wrapping.
pub struct NativeSysOps;

impl Default for NativeSysOps {
    fn default() -> Self {
        Self
    }
}

// ============================================================================
// Environment
// ============================================================================

impl SysOpEnv for NativeSysOps {
    fn env_get(&self, _call_id: CallId, key: String) -> SysOpOutput<Option<String>> {
        match std::env::var(&key) {
            Ok(val) => SysOpOutput::ok(Some(val)),
            Err(std::env::VarError::NotPresent) => SysOpOutput::ok(None),
            Err(std::env::VarError::NotUnicode(_)) => SysOpOutput::err(OpErrorKind::Other(
                format!("Environment variable '{key}' is not valid UTF-8"),
            )),
        }
    }

    fn env_get_or_panic(&self, _call_id: CallId, key: String) -> SysOpOutput<String> {
        match std::env::var(&key) {
            Ok(val) => SysOpOutput::ok(val),
            Err(std::env::VarError::NotPresent) => SysOpOutput::err(OpErrorKind::Other(format!(
                "Environment variable '{key}' not found",
            ))),
            Err(std::env::VarError::NotUnicode(_)) => SysOpOutput::err(OpErrorKind::Other(
                format!("Environment variable '{key}' is not valid UTF-8"),
            )),
        }
    }
}

// ============================================================================
// File System
// ============================================================================

impl SysOpFs for NativeSysOps {
    fn baml_fs_open(
        &self,
        _call_id: CallId,
        path: String,
    ) -> SysOpOutput<builtin_types::owned::FsFile> {
        SysOpOutput::async_op(async move {
            let file = tokio::fs::File::open(&path)
                .await
                .map_err(|e| OpErrorKind::Other(format!("Failed to open file '{path}': {e}")))?;

            let handle = registry::REGISTRY.register_file(file, path);
            Ok(builtin_types::owned::FsFile { _handle: handle })
        })
    }

    fn baml_fs_file_read(
        &self,
        _call_id: CallId,
        file: builtin_types::owned::FsFile,
    ) -> SysOpOutput<String> {
        use tokio::io::AsyncReadExt;

        SysOpOutput::async_op(async move {
            let file_mutex = registry::REGISTRY
                .get_file(file._handle.key())
                .ok_or_else(|| {
                    OpErrorKind::Other("File handle is invalid or has been closed".into())
                })?;

            let mut f = file_mutex.lock().await;
            let mut contents = String::new();
            f.read_to_string(&mut contents)
                .await
                .map_err(|e| OpErrorKind::Other(format!("Failed to read file: {e}")))?;

            Ok(contents)
        })
    }

    fn baml_fs_file_close(
        &self,
        _call_id: CallId,
        file: builtin_types::owned::FsFile,
    ) -> SysOpOutput<()> {
        drop(file);
        SysOpOutput::ok(())
    }
}

// ============================================================================
// System
// ============================================================================

impl SysOpSys for NativeSysOps {
    fn baml_sys_panic(&self, _call_id: CallId, message: String) -> SysOpOutput<()> {
        SysOpOutput::err(OpErrorKind::Other(message))
    }

    fn baml_sys_sleep(&self, _call_id: CallId, delay_ms: i64) -> SysOpOutput<()> {
        #[allow(clippy::cast_sign_loss)]
        let millis = delay_ms.max(0) as u64;
        SysOpOutput::async_op(async move {
            tokio::time::sleep(std::time::Duration::from_millis(millis)).await;
            Ok(())
        })
    }

    fn baml_sys_shell(&self, _call_id: CallId, command: String) -> SysOpOutput<String> {
        SysOpOutput::async_op(async move {
            let output = tokio::process::Command::new("sh")
                .arg("-c")
                .arg(&command)
                .output()
                .await
                .map_err(|e| {
                    OpErrorKind::Other(format!("Failed to execute command '{command}': {e}"))
                })?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                let code = output.status.code().unwrap_or(-1);
                return Err(OpErrorKind::Other(format!(
                    "Command '{}' failed with exit code {}: {}",
                    command,
                    code,
                    stderr.trim()
                )));
            }

            let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
            Ok(stdout)
        })
    }
}

// ============================================================================
// Network
// ============================================================================

impl SysOpNet for NativeSysOps {
    fn baml_net_connect(
        &self,
        _call_id: CallId,
        addr: String,
    ) -> SysOpOutput<builtin_types::owned::NetSocket> {
        SysOpOutput::async_op(async move {
            let stream = tokio::net::TcpStream::connect(&addr)
                .await
                .map_err(|e| OpErrorKind::Other(format!("Failed to connect to '{addr}': {e}")))?;

            let handle = registry::REGISTRY.register_socket(stream, addr);
            Ok(builtin_types::owned::NetSocket { _handle: handle })
        })
    }

    fn baml_net_socket_read(
        &self,
        _call_id: CallId,
        socket: builtin_types::owned::NetSocket,
    ) -> SysOpOutput<String> {
        use tokio::io::AsyncReadExt;

        SysOpOutput::async_op(async move {
            let stream_mutex = registry::REGISTRY
                .get_socket(socket._handle.key())
                .ok_or_else(|| {
                    OpErrorKind::Other("Socket handle is invalid or has been closed".into())
                })?;

            let mut stream = stream_mutex.lock().await;
            let mut buffer = vec![0u8; 4096];
            let n = stream
                .read(&mut buffer)
                .await
                .map_err(|e| OpErrorKind::Other(format!("Failed to read from socket: {e}")))?;

            let contents = String::from_utf8_lossy(&buffer[..n]).into_owned();
            Ok(contents)
        })
    }

    fn baml_net_socket_close(
        &self,
        _call_id: CallId,
        socket: builtin_types::owned::NetSocket,
    ) -> SysOpOutput<()> {
        drop(socket);
        SysOpOutput::ok(())
    }
}

// ============================================================================
// HTTP
// ============================================================================

impl SysOpHttp for NativeSysOps {
    fn baml_http_response_ok(
        &self,
        _call_id: CallId,
        response: builtin_types::owned::HttpResponse,
    ) -> SysOpOutput<bool> {
        SysOpOutput::ok((200..300).contains(&response.status_code))
    }

    #[cfg(feature = "bundle-http")]
    fn baml_http_fetch(
        &self,
        _call_id: CallId,
        url: String,
    ) -> SysOpOutput<builtin_types::owned::HttpResponse> {
        let req = builtin_types::owned::HttpRequest {
            method: "GET".to_string(),
            url,
            headers: indexmap::IndexMap::new(),
            body: String::new(),
        };
        SysOpOutput::async_op(async move { ops::http::send_async(req).await })
    }

    #[cfg(feature = "bundle-http")]
    fn baml_http_response_text(
        &self,
        _call_id: CallId,
        response: builtin_types::owned::HttpResponse,
    ) -> SysOpOutput<String> {
        SysOpOutput::async_op(async move {
            let body_mutex = registry::REGISTRY
                .get_http_response_body(response._handle.key())
                .ok_or_else(|| OpErrorKind::Other("Response handle is invalid".into()))?;

            let mut guard = body_mutex.lock().await;
            match &mut *guard {
                registry::ResponseBody::Real(opt) => {
                    let resp = opt.take().ok_or_else(|| {
                        OpErrorKind::Other("Response body has already been consumed".into())
                    })?;
                    let text = resp.text().await.map_err(|e| {
                        OpErrorKind::Other(format!(
                            "Failed to read response body: {}",
                            ops::http::format_error_chain(&e)
                        ))
                    })?;
                    Ok(text)
                }
                registry::ResponseBody::Error(opt) => {
                    let msg = opt.take().ok_or_else(|| {
                        OpErrorKind::Other("Error response body has already been consumed".into())
                    })?;
                    Ok(msg)
                }
            }
        })
    }

    #[cfg(feature = "bundle-http")]
    fn baml_http_send(
        &self,
        _call_id: CallId,
        request: builtin_types::owned::HttpRequest,
    ) -> SysOpOutput<builtin_types::owned::HttpResponse> {
        SysOpOutput::async_op(async move { ops::http::send_async(request).await })
    }
}

// ============================================================================
// Extension trait
// ============================================================================

/// Extension trait to add `native()` constructor to `SysOps`.
pub trait SysOpsExt {
    /// Create a `SysOps` table with native Tokio-based implementations.
    fn native() -> Self;
}

impl SysOpsExt for SysOps {
    fn native() -> Self {
        SysOps::from_impl::<NativeSysOps>()
    }
}
