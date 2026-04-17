//! New IO trait implementations for `NativeSysOps`.
//!
//! These implement the generated `IoClass*` and `IoNamespace*` traits from
//! `sys_types::io`. They coexist with the legacy `SysOp*` trait impls in
//! `lib.rs` during the transition.

use std::sync::Arc;

use bex_heap::{BexExternalValue, BexHeap};
use sys_ops::io::{self, CallId, OpErrorKind, SysOpContext, SysOpOutput, owned};

use crate::NativeSysOps;

// ============================================================================
// Environment
// ============================================================================

impl io::IoNamespaceEnv for NativeSysOps {
    fn get(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        key: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<Option<String>> {
        match std::env::var(&key) {
            Ok(val) => SysOpOutput::ok(Some(val)),
            Err(std::env::VarError::NotPresent) => SysOpOutput::ok(None),
            Err(std::env::VarError::NotUnicode(_)) => SysOpOutput::err(OpErrorKind::Other(
                format!("Environment variable '{key}' is not valid UTF-8"),
            )),
        }
    }
}

// ============================================================================
// File System
// ============================================================================

type FsFileHandle = tokio::sync::Mutex<Option<tokio::fs::File>>;

fn downcast_handle(file: &owned::fs::File) -> Result<Arc<FsFileHandle>, OpErrorKind> {
    file._handle
        .clone()
        .downcast::<FsFileHandle>()
        .map_err(|_| OpErrorKind::Other("Invalid file handle type".into()))
}

/// Error returned for operations on a closed file.
fn closed_err() -> OpErrorKind {
    OpErrorKind::Other("File is closed".into())
}

impl io::IoClassFsFile for NativeSysOps {
    fn text(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        file: owned::fs::File,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<String> {
        use tokio::io::AsyncReadExt;

        SysOpOutput::async_op(async move {
            let handle = downcast_handle(&file)?;
            let mut guard = handle.lock().await;
            let f = guard.as_mut().ok_or_else(closed_err)?;
            let mut contents = String::new();
            f.read_to_string(&mut contents)
                .await
                .map_err(|e| OpErrorKind::Other(format!("Failed to read file: {e}")))?;
            Ok(contents)
        })
    }

    fn bytes(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        file: owned::fs::File,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<Vec<u8>> {
        use tokio::io::AsyncReadExt;

        SysOpOutput::async_op(async move {
            let handle = downcast_handle(&file)?;
            let mut guard = handle.lock().await;
            let f = guard.as_mut().ok_or_else(closed_err)?;
            let mut contents = Vec::new();
            f.read_to_end(&mut contents)
                .await
                .map_err(|e| OpErrorKind::Other(format!("Failed to read file: {e}")))?;
            Ok(contents)
        })
    }

    fn read(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        file: owned::fs::File,
        n: i64,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<String> {
        SysOpOutput::async_op(async move {
            let bytes = read_up_to(&file, n).await?;
            String::from_utf8(bytes)
                .map_err(|e| OpErrorKind::Other(format!("Invalid UTF-8 in file: {e}")))
        })
    }

    fn read_bytes(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        file: owned::fs::File,
        n: i64,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<Vec<u8>> {
        SysOpOutput::async_op(async move { read_up_to(&file, n).await })
    }

    fn close(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        file: owned::fs::File,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        SysOpOutput::async_op(async move {
            let handle = downcast_handle(&file)?;
            handle.lock().await.take();
            Ok(())
        })
    }

    fn seek(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        file: owned::fs::File,
        offset: i64,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        use tokio::io::AsyncSeekExt;

        SysOpOutput::async_op(async move {
            if offset < 0 {
                return Err(OpErrorKind::Other(format!(
                    "Negative seek offset: {offset}"
                )));
            }
            let handle = downcast_handle(&file)?;
            let mut guard = handle.lock().await;
            let f = guard.as_mut().ok_or_else(closed_err)?;
            #[allow(clippy::cast_sign_loss)]
            f.seek(std::io::SeekFrom::Start(offset as u64))
                .await
                .map_err(|e| OpErrorKind::Other(format!("Failed to seek: {e}")))?;
            Ok(())
        })
    }

    fn write(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        file: owned::fs::File,
        data: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<i64> {
        SysOpOutput::async_op(async move { write_all_bytes(&file, data.into_bytes()).await })
    }

    fn write_bytes(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        file: owned::fs::File,
        data: Vec<u8>,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<i64> {
        SysOpOutput::async_op(async move { write_all_bytes(&file, data).await })
    }
}

async fn read_up_to(file: &owned::fs::File, n: i64) -> Result<Vec<u8>, OpErrorKind> {
    use tokio::io::AsyncReadExt;

    if n < 0 {
        return Err(OpErrorKind::Other(format!("Negative read length: {n}")));
    }
    let handle = downcast_handle(file)?;
    let mut guard = handle.lock().await;
    let f = guard.as_mut().ok_or_else(closed_err)?;
    let cap = usize::try_from(n)
        .map_err(|_| OpErrorKind::Other(format!("Read length out of range: {n}")))?;
    let mut buf = vec![0u8; cap];
    let mut filled = 0;
    while filled < cap {
        let read = f
            .read(&mut buf[filled..])
            .await
            .map_err(|e| OpErrorKind::Other(format!("Failed to read file: {e}")))?;
        if read == 0 {
            break;
        }
        filled += read;
    }
    buf.truncate(filled);
    Ok(buf)
}

async fn write_all_bytes(file: &owned::fs::File, data: Vec<u8>) -> Result<i64, OpErrorKind> {
    use tokio::io::AsyncWriteExt;

    let handle = downcast_handle(file)?;
    let mut guard = handle.lock().await;
    let f = guard.as_mut().ok_or_else(closed_err)?;
    #[allow(clippy::cast_possible_wrap)]
    let len = data.len() as i64;
    f.write_all(&data)
        .await
        .map_err(|e| OpErrorKind::Other(format!("Failed to write: {e}")))?;
    f.flush()
        .await
        .map_err(|e| OpErrorKind::Other(format!("Failed to write: {e}")))?;
    Ok(len)
}

impl io::IoNamespaceFs for NativeSysOps {
    fn open(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        path: String,
        mode: BexExternalValue,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<owned::fs::File> {
        SysOpOutput::async_op(async move {
            #[allow(clippy::manual_let_else)]
            let mode = match mode {
                BexExternalValue::String(s) => s,
                _ => return Err(OpErrorKind::Other("Invalid mode type".into())),
            };
            let file = match mode.as_str() {
                "r" => tokio::fs::File::open(&path).await,
                "r+" => {
                    tokio::fs::OpenOptions::new()
                        .read(true)
                        .write(true)
                        .open(&path)
                        .await
                }
                "a" => {
                    tokio::fs::OpenOptions::new()
                        .append(true)
                        .create(true)
                        .open(&path)
                        .await
                }
                "a+" => {
                    tokio::fs::OpenOptions::new()
                        .read(true)
                        .append(true)
                        .create(true)
                        .open(&path)
                        .await
                }
                _ => {
                    return Err(OpErrorKind::Other(format!(
                        "Unsupported file mode '{mode}': expected \"r\", \"r+\", \"a\", or \"a+\""
                    )));
                }
            }
            .map_err(|e| OpErrorKind::Other(format!("Failed to open file '{path}': {e}")))?;
            let handle: Arc<dyn std::any::Any + Send + Sync> =
                Arc::new(tokio::sync::Mutex::new(Some(file)));
            Ok(owned::fs::File { _handle: handle })
        })
    }

    fn exists(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        path: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<bool> {
        SysOpOutput::async_op(async move {
            tokio::fs::try_exists(&path).await.map_err(|e| {
                OpErrorKind::Other(format!("Failed to check existence of '{path}': {e}"))
            })
        })
    }

    fn remove(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        path: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        SysOpOutput::async_op(async move {
            tokio::fs::remove_file(&path)
                .await
                .map_err(|e| OpErrorKind::Other(format!("Failed to remove file '{path}': {e}")))
        })
    }

    fn size(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        path: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<i64> {
        SysOpOutput::async_op(async move {
            let metadata = tokio::fs::metadata(&path)
                .await
                .map_err(|e| OpErrorKind::Other(format!("Failed to stat '{path}': {e}")))?;
            i64::try_from(metadata.len())
                .map_err(|_| OpErrorKind::Other(format!("File '{path}' size exceeds i64::MAX")))
        })
    }

    fn write(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        path: String,
        data: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<i64> {
        SysOpOutput::async_op(async move {
            if let Some(parent) = std::path::Path::new(&path).parent() {
                if !parent.as_os_str().is_empty() {
                    tokio::fs::create_dir_all(parent).await.map_err(|e| {
                        OpErrorKind::Other(format!(
                            "Failed to create parent directories for '{path}': {e}"
                        ))
                    })?;
                }
            }
            let bytes = data.as_bytes();
            tokio::fs::write(&path, bytes)
                .await
                .map_err(|e| OpErrorKind::Other(format!("Failed to write file '{path}': {e}")))?;
            #[allow(clippy::cast_possible_wrap)]
            Ok(bytes.len() as i64)
        })
    }

    fn write_bytes(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        path: String,
        data: Vec<u8>,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<i64> {
        SysOpOutput::async_op(async move {
            if let Some(parent) = std::path::Path::new(&path).parent() {
                if !parent.as_os_str().is_empty() {
                    tokio::fs::create_dir_all(parent).await.map_err(|e| {
                        OpErrorKind::Other(format!(
                            "Failed to create parent directories for '{path}': {e}"
                        ))
                    })?;
                }
            }
            #[allow(clippy::cast_possible_wrap)]
            let len = data.len() as i64;
            tokio::fs::write(&path, &data)
                .await
                .map_err(|e| OpErrorKind::Other(format!("Failed to write file '{path}': {e}")))?;
            Ok(len)
        })
    }
}

// ============================================================================
// System
// ============================================================================

impl io::IoNamespaceSys for NativeSysOps {
    fn shell(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        command: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<String> {
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

            Ok(String::from_utf8_lossy(&output.stdout).into_owned())
        })
    }

    fn sleep(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        ms: i64,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        #[allow(clippy::cast_sign_loss)]
        let millis = ms.max(0) as u64;
        SysOpOutput::async_op(async move {
            tokio::time::sleep(std::time::Duration::from_millis(millis)).await;
            Ok(())
        })
    }
}

// ============================================================================
// Network
// ============================================================================

type NetSocketHandle = tokio::sync::Mutex<tokio::net::TcpStream>;

impl io::IoClassNetSocket for NativeSysOps {
    fn read(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        socket: owned::net::Socket,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<String> {
        use tokio::io::AsyncReadExt;

        SysOpOutput::async_op(async move {
            let handle: Arc<NetSocketHandle> = socket
                ._handle
                .downcast::<NetSocketHandle>()
                .map_err(|_| OpErrorKind::Other("Invalid socket handle type".into()))?;
            let mut stream = handle.lock().await;
            let mut buffer = vec![0u8; 4096];
            let n = stream
                .read(&mut buffer)
                .await
                .map_err(|e| OpErrorKind::Other(format!("Failed to read from socket: {e}")))?;
            Ok(String::from_utf8_lossy(&buffer[..n]).into_owned())
        })
    }

    fn close(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        _socket: owned::net::Socket,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        SysOpOutput::ok(())
    }
}

impl io::IoNamespaceNet for NativeSysOps {
    fn connect(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        addr: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<owned::net::Socket> {
        SysOpOutput::async_op(async move {
            let stream = tokio::net::TcpStream::connect(&addr)
                .await
                .map_err(|e| OpErrorKind::Other(format!("Failed to connect to '{addr}': {e}")))?;
            let handle: Arc<dyn std::any::Any + Send + Sync> =
                Arc::new(tokio::sync::Mutex::new(stream));
            Ok(owned::net::Socket { _handle: handle })
        })
    }
}

// ============================================================================
// HTTP
// ============================================================================

impl io::IoClassHttpResponse for NativeSysOps {
    #[cfg(feature = "bundle-http")]
    fn text(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        response: owned::http::Response,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<String> {
        SysOpOutput::async_op(async move {
            let body: Arc<tokio::sync::Mutex<Option<reqwest::Response>>> = response
                ._body
                .downcast::<tokio::sync::Mutex<Option<reqwest::Response>>>()
                .map_err(|_| OpErrorKind::Other("Invalid response body handle".into()))?;
            let mut guard = body.lock().await;
            let resp = guard.take().ok_or_else(|| {
                OpErrorKind::Other("Response body has already been consumed".into())
            })?;
            resp.text()
                .await
                .map_err(|e| OpErrorKind::Other(format!("Failed to read response body: {e}")))
        })
    }

    #[cfg(not(feature = "bundle-http"))]
    fn text(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        _response: owned::http::Response,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<String> {
        SysOpOutput::err(OpErrorKind::Unsupported)
    }

    #[cfg(feature = "bundle-http")]
    fn bytes(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        response: owned::http::Response,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<Vec<u8>> {
        SysOpOutput::async_op(async move {
            let body: Arc<tokio::sync::Mutex<Option<reqwest::Response>>> = response
                ._body
                .downcast::<tokio::sync::Mutex<Option<reqwest::Response>>>()
                .map_err(|_| OpErrorKind::Other("Invalid response body handle".into()))?;
            let mut guard = body.lock().await;
            let resp = guard.take().ok_or_else(|| {
                OpErrorKind::Other("Response body has already been consumed".into())
            })?;
            resp.bytes()
                .await
                .map(|b| b.to_vec())
                .map_err(|e| OpErrorKind::Other(format!("Failed to read response body: {e}")))
        })
    }

    #[cfg(not(feature = "bundle-http"))]
    fn bytes(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        _response: owned::http::Response,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<Vec<u8>> {
        SysOpOutput::err(OpErrorKind::Unsupported)
    }
}

#[cfg(feature = "bundle-http")]
fn build_io_http_response(response: reqwest::Response, url: String) -> owned::http::Response {
    let status = i64::from(response.status().as_u16());
    let headers: indexmap::IndexMap<String, String> = response
        .headers()
        .iter()
        .map(|(k, v)| (k.as_str().to_string(), v.to_str().unwrap_or("").to_string()))
        .collect();
    let body: Arc<dyn std::any::Any + Send + Sync> =
        Arc::new(tokio::sync::Mutex::new(Some(response)));
    owned::http::Response {
        status_code: status,
        headers,
        url,
        _body: body,
    }
}

impl io::IoNamespaceHttp for NativeSysOps {
    #[cfg(feature = "bundle-http")]
    fn fetch(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        url: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<owned::http::Response> {
        SysOpOutput::async_op(async move {
            let client = reqwest::Client::new();
            let response = client
                .get(&url)
                .send()
                .await
                .map_err(|e| OpErrorKind::Other(format!("HTTP fetch failed: {e}")))?;
            let final_url = response.url().to_string();
            Ok(build_io_http_response(response, final_url))
        })
    }

    #[cfg(not(feature = "bundle-http"))]
    fn fetch(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        _url: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<owned::http::Response> {
        SysOpOutput::err(OpErrorKind::Unsupported)
    }

    #[cfg(feature = "bundle-http")]
    fn send(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        request: owned::http::Request,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<owned::http::Response> {
        SysOpOutput::async_op(async move {
            let method = reqwest::Method::from_bytes(request.method.as_bytes()).map_err(|e| {
                OpErrorKind::Other(format!("Invalid HTTP method '{}': {e}", request.method))
            })?;

            let client = reqwest::Client::new();
            let mut builder = client.request(method, &request.url);

            for (k, v) in &request.headers {
                builder = builder.header(k.as_str(), v.as_str());
            }

            if !request.body.is_empty() {
                builder = builder.body(request.body);
            }

            let response = builder
                .send()
                .await
                .map_err(|e| OpErrorKind::Other(format!("HTTP send failed: {e}")))?;
            let final_url = response.url().to_string();
            Ok(build_io_http_response(response, final_url))
        })
    }

    #[cfg(not(feature = "bundle-http"))]
    fn send(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        _request: owned::http::Request,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<owned::http::Response> {
        SysOpOutput::err(OpErrorKind::Unsupported)
    }
}
