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
//! let engine = BexEngine::new(program, env_vars, SysOps::native())?;
//! ```

mod ops;
pub mod registry;

// Re-export types from sys_types for convenience
use bex_heap::builtin_types;
pub use sys_types::{
    CompletionHandle, OpError, SysOp, SysOpContext, SysOpFn, SysOpFs, SysOpHttp, SysOpLlm,
    SysOpNet, SysOpResult, SysOpSys, SysOps,
};
use sys_types::{OpErrorKind, SysOpOutput};

/// The native Tokio-based `sys_op` provider.
///
/// Implements per-module traits (`SysOpFs`, `SysOpHttp`, etc.) with clean
/// typed signatures. The generated glue handles arg extraction and error wrapping.
pub struct NativeSysOps;

// ============================================================================
// File System
// ============================================================================

impl SysOpFs for NativeSysOps {
    fn baml_fs_open(path: String) -> SysOpOutput<builtin_types::owned::FsFile> {
        SysOpOutput::async_op(async move {
            let file = tokio::fs::File::open(&path)
                .await
                .map_err(|e| OpErrorKind::Other(format!("Failed to open file '{path}': {e}")))?;

            let handle = registry::REGISTRY.register_file(file, path);
            Ok(builtin_types::owned::FsFile { _handle: handle })
        })
    }

    fn baml_fs_file_read(file: builtin_types::owned::FsFile) -> SysOpOutput<String> {
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

    fn baml_fs_file_close(file: builtin_types::owned::FsFile) -> SysOpOutput<()> {
        drop(file);
        SysOpOutput::ok(())
    }
}

// ============================================================================
// System
// ============================================================================

impl SysOpSys for NativeSysOps {
    fn baml_sys_shell(command: String) -> SysOpOutput<String> {
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
    fn baml_net_connect(addr: String) -> SysOpOutput<builtin_types::owned::NetSocket> {
        SysOpOutput::async_op(async move {
            let stream = tokio::net::TcpStream::connect(&addr)
                .await
                .map_err(|e| OpErrorKind::Other(format!("Failed to connect to '{addr}': {e}")))?;

            let handle = registry::REGISTRY.register_socket(stream, addr);
            Ok(builtin_types::owned::NetSocket { _handle: handle })
        })
    }

    fn baml_net_socket_read(socket: builtin_types::owned::NetSocket) -> SysOpOutput<String> {
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

    fn baml_net_socket_close(socket: builtin_types::owned::NetSocket) -> SysOpOutput<()> {
        drop(socket);
        SysOpOutput::ok(())
    }
}

// ============================================================================
// HTTP
// ============================================================================

impl SysOpHttp for NativeSysOps {
    fn baml_http_fetch(url: String) -> SysOpOutput<builtin_types::owned::HttpResponse> {
        let req = builtin_types::owned::HttpRequest {
            method: "GET".to_string(),
            url,
            headers: indexmap::IndexMap::new(),
            body: String::new(),
        };
        SysOpOutput::async_op(async move { ops::http::send_async(req).await })
    }

    fn baml_http_response_text(
        response: builtin_types::owned::HttpResponse,
    ) -> SysOpOutput<String> {
        SysOpOutput::async_op(async move {
            let response_mutex = registry::REGISTRY
                .get_http_response_body(response._handle.key())
                .ok_or_else(|| OpErrorKind::Other("Response handle is invalid".into()))?;

            let resp = {
                let mut guard = response_mutex.lock().await;
                guard.take().ok_or_else(|| {
                    OpErrorKind::Other("Response body has already been consumed".into())
                })?
            };

            let text = resp.text().await.map_err(|e| {
                OpErrorKind::Other(format!(
                    "Failed to read response body: {}",
                    ops::http::format_error_chain(&e)
                ))
            })?;

            Ok(text)
        })
    }

    fn baml_http_response_ok(response: builtin_types::owned::HttpResponse) -> SysOpOutput<bool> {
        SysOpOutput::ok((200..300).contains(&response.status_code))
    }

    fn baml_http_send(
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
