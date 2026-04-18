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
// IO (stdin input)
// ============================================================================

impl io::IoNamespaceIo for NativeSysOps {
    fn input(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        prompt: Option<String>,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<String> {
        SysOpOutput::async_op(async move {
            use tokio::io::{AsyncBufReadExt, AsyncWriteExt};

            if let Some(p) = prompt {
                let mut stdout = tokio::io::stdout();
                stdout
                    .write_all(p.as_bytes())
                    .await
                    .map_err(|e| OpErrorKind::Other(format!("Failed to write prompt: {e}")))?;
                stdout
                    .flush()
                    .await
                    .map_err(|e| OpErrorKind::Other(format!("Failed to flush stdout: {e}")))?;
            }
            let mut line = String::new();
            tokio::io::BufReader::new(tokio::io::stdin())
                .read_line(&mut line)
                .await
                .map_err(|e| OpErrorKind::Other(format!("Failed to read stdin: {e}")))?;
            // Trim trailing newline (returns "" on EOF when read_line returns 0 bytes)
            if line.ends_with('\n') {
                line.pop();
            }
            if line.ends_with('\r') {
                line.pop();
            }
            Ok(line)
        })
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

    fn seek_from(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        file: owned::fs::File,
        whence: BexExternalValue,
        offset: i64,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<i64> {
        use tokio::io::AsyncSeekExt;

        SysOpOutput::async_op(async move {
            let BexExternalValue::String(whence) = whence else {
                return Err(OpErrorKind::Other("Invalid whence type".into()));
            };
            let from = match whence.as_str() {
                "start" => {
                    let off = u64::try_from(offset).map_err(|_| {
                        OpErrorKind::Other(format!(
                            "Negative offset with whence=\"start\": {offset}"
                        ))
                    })?;
                    std::io::SeekFrom::Start(off)
                }
                "current" => std::io::SeekFrom::Current(offset),
                "end" => std::io::SeekFrom::End(offset),
                _ => {
                    return Err(OpErrorKind::Other(format!(
                        "Unsupported whence '{whence}': expected \"start\", \"current\", or \"end\""
                    )));
                }
            };
            let handle = downcast_handle(&file)?;
            let mut guard = handle.lock().await;
            let f = guard.as_mut().ok_or_else(closed_err)?;
            let pos = f
                .seek(from)
                .await
                .map_err(|e| OpErrorKind::Other(format!("Failed to seek: {e}")))?;
            i64::try_from(pos)
                .map_err(|_| OpErrorKind::Other(format!("Seek position out of range: {pos}")))
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

    let cap =
        u64::try_from(n).map_err(|_| OpErrorKind::Other(format!("Negative read length: {n}")))?;
    let handle = downcast_handle(file)?;
    let mut guard = handle.lock().await;
    let f = guard.as_mut().ok_or_else(closed_err)?;
    let mut buf = Vec::new();
    f.take(cap)
        .read_to_end(&mut buf)
        .await
        .map_err(|e| OpErrorKind::Other(format!("Failed to read file: {e}")))?;
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
            let BexExternalValue::String(mode) = mode else {
                return Err(OpErrorKind::Other("Invalid mode type".into()));
            };
            // Modes that create the file also auto-create missing parent dirs,
            // matching Bun's `Bun.write` behavior.
            let creates = matches!(mode.as_str(), "w" | "w+" | "a" | "a+");
            if creates {
                if let Some(parent) = std::path::Path::new(&path).parent() {
                    if !parent.as_os_str().is_empty() {
                        tokio::fs::create_dir_all(parent).await.map_err(|e| {
                            OpErrorKind::Other(format!(
                                "Failed to create parent directories for '{path}': {e}"
                            ))
                        })?;
                    }
                }
            }
            let file = match mode.as_str() {
                "r" => tokio::fs::File::open(&path).await,
                "r+" => {
                    tokio::fs::OpenOptions::new()
                        .read(true)
                        .write(true)
                        .open(&path)
                        .await
                }
                "w" => {
                    tokio::fs::OpenOptions::new()
                        .write(true)
                        .create(true)
                        .truncate(true)
                        .open(&path)
                        .await
                }
                "w+" => {
                    tokio::fs::OpenOptions::new()
                        .read(true)
                        .write(true)
                        .create(true)
                        .truncate(true)
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
                        "Unsupported file mode '{mode}': expected \"r\", \"r+\", \"w\", \"w+\", \"a\", or \"a+\""
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

    fn read(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        path: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<String> {
        SysOpOutput::async_op(async move {
            tokio::fs::read_to_string(&path)
                .await
                .map_err(|e| OpErrorKind::Other(format!("Failed to read file '{path}': {e}")))
        })
    }

    fn write(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        path: String,
        content: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<i64> {
        SysOpOutput::async_op(async move { write_path(&path, content.as_bytes()).await })
    }

    fn write_bytes(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        path: String,
        content: Vec<u8>,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<i64> {
        SysOpOutput::async_op(async move { write_path(&path, &content).await })
    }
}

// Auto-creates missing parent dirs, matching Bun's `Bun.write` behavior.
async fn write_path(path: &str, data: &[u8]) -> Result<i64, OpErrorKind> {
    if let Some(parent) = std::path::Path::new(path).parent() {
        if !parent.as_os_str().is_empty() {
            tokio::fs::create_dir_all(parent).await.map_err(|e| {
                OpErrorKind::Other(format!(
                    "Failed to create parent directories for '{path}': {e}"
                ))
            })?;
        }
    }
    tokio::fs::write(path, data)
        .await
        .map_err(|e| OpErrorKind::Other(format!("Failed to write file '{path}': {e}")))?;
    i64::try_from(data.len())
        .map_err(|_| OpErrorKind::Other(format!("Write size {} exceeds i64::MAX", data.len())))
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

impl io::IoClassHttpSseStream for NativeSysOps {
    #[cfg(feature = "bundle-http")]
    fn next(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        sse_stream: owned::http::SseStream,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<Option<String>> {
        use std::sync::atomic::Ordering;

        SysOpOutput::async_op(async move {
            let handle = sse_stream
                ._handle
                .downcast::<bex_resource_types::ResourceHandle>()
                .map_err(|_| OpErrorKind::Other("Invalid SSE stream handle type".into()))?;

            let (buffer, notify, closed) =
                crate::registry::REGISTRY
                    .get_sse_stream(handle.key())
                    .ok_or_else(|| OpErrorKind::Other("SSE stream handle is invalid".into()))?;

            loop {
                let notified = notify.notified();
                {
                    let mut buf = buffer.lock().await;
                    if closed.load(Ordering::Acquire) {
                        buf.done = true;
                        buf.error = None;
                        return Ok(None);
                    }
                    if !buf.events.is_empty() {
                        let events: Vec<serde_json::Value> = std::mem::take(&mut buf.events)
                            .into_iter()
                            .map(|e| {
                                serde_json::json!({
                                    "event": e.event,
                                    "data": e.data,
                                    "id": e.id,
                                })
                            })
                            .collect();
                        return Ok(Some(serde_json::to_string(&events).map_err(|e| {
                            OpErrorKind::Other(format!("Failed to serialize SSE events: {e}"))
                        })?));
                    }
                    if let Some(err) = buf.error.take() {
                        return Err(OpErrorKind::Other(err));
                    }
                    if buf.done {
                        return Ok(None);
                    }
                }
                notified.await;
            }
        })
    }

    #[cfg(not(feature = "bundle-http"))]
    fn next(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        _sse_stream: owned::http::SseStream,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<Option<String>> {
        SysOpOutput::err(OpErrorKind::Unsupported)
    }

    fn close(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        _sse_stream: owned::http::SseStream,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        // Dropping the owned SseStream drops its ResourceHandle, which
        // triggers cleanup via ResourceRegistryRef::remove.
        SysOpOutput::ok(())
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

    #[cfg(feature = "bundle-http")]
    fn fetch_sse(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        request: owned::http::Request,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<owned::http::SseStream> {
        use std::sync::atomic::{AtomicBool, Ordering};

        use futures::StreamExt;
        use sys_types::sse::SseParser;
        use tokio::sync::{Mutex as TokioMutex, Notify};

        use crate::registry::{REGISTRY, SseBuffer};

        SysOpOutput::async_op(async move {
            let method = reqwest::Method::from_bytes(request.method.as_bytes()).map_err(|e| {
                OpErrorKind::Other(format!("Invalid HTTP method '{}': {e}", request.method))
            })?;

            let client = reqwest::Client::new();
            let mut builder = client.request(method, &request.url);

            for (key, value) in &request.headers {
                builder = builder.header(key.as_str(), value.as_str());
            }

            if !request.body.is_empty() {
                builder = builder.body(request.body.clone());
            }

            let response = builder
                .send()
                .await
                .map_err(|e| OpErrorKind::Other(format!("SSE connection failed: {e}")))?;

            if !response.status().is_success() {
                let status = response.status().as_u16();
                let body = response
                    .text()
                    .await
                    .unwrap_or_else(|_| "<could not read body>".to_string());
                return Err(OpErrorKind::Other(format!(
                    "SSE request failed with status {status}: {body}"
                )));
            }

            let url = response.url().to_string();

            let buffer = Arc::new(TokioMutex::new(SseBuffer {
                events: Vec::new(),
                done: false,
                error: None,
            }));
            let closed = Arc::new(AtomicBool::new(false));
            let notify = Arc::new(Notify::new());

            let buf_clone = buffer.clone();
            let closed_clone = closed.clone();
            let notify_clone = notify.clone();
            let consumer = tokio::spawn(async move {
                struct SseDropGuard {
                    buffer: Arc<TokioMutex<SseBuffer>>,
                    closed: Arc<AtomicBool>,
                    notify: Arc<Notify>,
                    completed: bool,
                }

                impl Drop for SseDropGuard {
                    fn drop(&mut self) {
                        if !self.completed {
                            if let Ok(mut buf) = self.buffer.try_lock() {
                                if !buf.done {
                                    if !self.closed.load(Ordering::Acquire) {
                                        buf.error = Some("SSE stream task was cancelled".into());
                                    }
                                    buf.done = true;
                                }
                            }
                            self.notify.notify_waiters();
                        }
                    }
                }

                let mut guard = SseDropGuard {
                    buffer: buf_clone.clone(),
                    closed: closed_clone.clone(),
                    notify: notify_clone.clone(),
                    completed: false,
                };

                let mut parser = SseParser::new();
                let mut byte_stream = response.bytes_stream();

                while let Some(chunk_result) = byte_stream.next().await {
                    match chunk_result {
                        Ok(bytes) => {
                            let events = parser.feed(&bytes);
                            if !events.is_empty() {
                                let mut buf = buf_clone.lock().await;
                                buf.events.extend(events);
                                notify_clone.notify_waiters();
                            }
                        }
                        Err(e) => {
                            let mut buf = buf_clone.lock().await;
                            buf.error = Some(format!("SSE stream error: {e}"));
                            buf.done = true;
                            notify_clone.notify_waiters();
                            guard.completed = true;
                            return;
                        }
                    }
                }

                // Stream ended cleanly — flush any event buffered without a
                // trailing blank line (some servers omit the final delimiter).
                let final_events = parser.finish();
                let mut buf = buf_clone.lock().await;
                if !final_events.is_empty() {
                    buf.events.extend(final_events);
                }
                buf.done = true;
                notify_clone.notify_waiters();
                guard.completed = true;
            });

            let handle = REGISTRY.register_sse_stream(
                buffer,
                closed,
                notify,
                consumer.abort_handle(),
                url.clone(),
            );
            let handle: Arc<dyn std::any::Any + Send + Sync> = Arc::new(handle);
            Ok(owned::http::SseStream {
                url,
                _handle: handle,
            })
        })
    }

    #[cfg(not(feature = "bundle-http"))]
    fn fetch_sse(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        _request: owned::http::Request,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<owned::http::SseStream> {
        SysOpOutput::err(OpErrorKind::Unsupported)
    }
}
