use std::sync::{Arc, Mutex};

use bex_project::{BexExternalValue, Handle, HostValueArc};
use indexmap::{IndexMap, indexmap};
use num_traits::ToPrimitive as _;
use sys_ops::io::{self, IoClassHttpResponse, IoNamespaceFs, IoNamespaceHttp};
use sys_types::{
    BexHeap, CallId, OpErrorBody, SysOp, SysOpContext, SysOpOutput, VmBamlError, VmInternalError,
    VmRustFnError,
};

use crate::host_value::WasmHost;

const UNSUPPORTED: &str = "Operation not supported by the Web runtime";

pub(crate) struct WebHttp {
    host: Arc<WasmHost>,
    fetch: Arc<HostValueArc>,
}

impl WebHttp {
    pub(crate) fn new(host: Arc<WasmHost>, fetch: Arc<HostValueArc>) -> Self {
        Self { host, fetch }
    }

    fn send(
        &self,
        operation: SysOp,
        request: io::owned::http::Request,
        timeout_nanos: &num_bigint::BigInt,
    ) -> SysOpOutput<io::owned::http::Response> {
        if timeout_nanos.sign() == num_bigint::Sign::Minus {
            return SysOpOutput::err(VmBamlError::InvalidArgument {
                message: "HTTP timeout must be non-negative".to_string(),
            });
        }

        let headers = request
            .headers
            .into_iter()
            .map(|(name, value)| {
                array(vec![
                    BexExternalValue::String(name.into()),
                    BexExternalValue::String(value.into()),
                ])
            })
            .collect();
        let request_value = map(indexmap! {
            "method".to_string() => BexExternalValue::String(request.method.into()),
            "url".to_string() => BexExternalValue::String(request.url.into()),
            "headers".to_string() => array(headers),
            "body".to_string() => BexExternalValue::Uint8Array(request.body.into_bytes()),
            "timeoutNanos".to_string() => BexExternalValue::Bigint(timeout_nanos.clone()),
        });
        let duration_ms = timeout_duration_ms(timeout_nanos);
        map_output(
            self.host.call_registered_callable(
                operation,
                self.fetch.as_ref(),
                &[request_value],
                &IndexMap::new(),
            ),
            move |value| parse_fetch_result(value, duration_ms),
        )
    }
}

#[derive(Debug)]
struct BufferedResponseBody(Mutex<Option<Vec<u8>>>);

fn take_response_body(response: &io::owned::http::Response) -> Result<Vec<u8>, VmRustFnError> {
    let body = response
        ._body
        .downcast_ref::<BufferedResponseBody>()
        .ok_or_else(|| VmInternalError::BridgeFailure {
            message: "Web HTTP response contains an invalid body resource".to_string(),
        })?;
    body.0
        .lock()
        .map_err(|_| VmInternalError::BridgeFailure {
            message: "Web HTTP response body lock was poisoned".to_string(),
        })?
        .take()
        .ok_or_else(|| {
            VmBamlError::Io {
                message: "response body has already been consumed".to_string(),
            }
            .into()
        })
}

impl IoClassHttpResponse for WebHttp {
    fn text(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        response: io::owned::http::Response,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<String> {
        match take_response_body(&response).and_then(|bytes| {
            String::from_utf8(bytes).map_err(|error| {
                VmBamlError::Io {
                    message: format!("HTTP response body is not valid UTF-8: {error}"),
                }
                .into()
            })
        }) {
            Ok(text) => SysOpOutput::ok(text),
            Err(error) => SysOpOutput::err(error),
        }
    }

    fn bytes(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        response: io::owned::http::Response,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<Vec<u8>> {
        match take_response_body(&response) {
            Ok(bytes) => SysOpOutput::ok(bytes),
            Err(error) => SysOpOutput::err(error),
        }
    }

    fn new(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        _status_code: i64,
        _headers: IndexMap<String, String>,
        _body: Vec<u8>,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<io::owned::http::Response> {
        unsupported()
    }

    fn new_streaming(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        _status_code: i64,
        _headers: IndexMap<String, String>,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<io::owned::http::Response> {
        unsupported()
    }

    fn write(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        _response: io::owned::http::Response,
        _data: Vec<u8>,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        unsupported()
    }

    fn end(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        _response: io::owned::http::Response,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        unsupported()
    }
}

impl io::IoClassHttpTlsConfig for WebHttp {
    fn _new(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        _cert_pem: Vec<u8>,
        _key_pem: Vec<u8>,
        _allow_tls1_2: bool,
        _handshake_timeout_nanos: Arc<num_bigint::BigInt>,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<io::owned::http::TlsConfig> {
        unsupported()
    }
}

impl io::IoClassHttpServer for WebHttp {
    fn bind(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        _addr: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<io::owned::http::Server> {
        unsupported()
    }

    fn _serve(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        _server: io::owned::http::Server,
        _handler: Handle,
        _tls_config: Option<io::owned::http::TlsConfig>,
        _allow_http1: bool,
        _allow_http2: bool,
        _max_body_size: i64,
        _max_connections: i64,
        _header_read_timeout_nanos: Arc<num_bigint::BigInt>,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        unsupported()
    }
}

impl io::IoClassHttpSseStream for WebHttp {
    fn next(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        _stream: io::owned::http::SseStream,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<Option<String>> {
        unsupported()
    }

    fn close(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        _stream: io::owned::http::SseStream,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        unsupported()
    }
}

impl IoNamespaceHttp for WebHttp {
    fn _fetch(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        url: String,
        timeout_nanos: Arc<num_bigint::BigInt>,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<io::owned::http::Response> {
        self.send(
            SysOp::BamlHttpFetch,
            io::owned::http::Request {
                method: "GET".to_string(),
                url,
                headers: IndexMap::new(),
                body: String::new(),
            },
            timeout_nanos.as_ref(),
        )
    }

    fn _send(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        request: io::owned::http::Request,
        timeout_nanos: Arc<num_bigint::BigInt>,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<io::owned::http::Response> {
        self.send(SysOp::BamlHttpSend, request, timeout_nanos.as_ref())
    }

    fn fetch_sse(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        _request: io::owned::http::Request,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<io::owned::http::SseStream> {
        unsupported()
    }
}

pub(crate) struct WebFs {
    host: Arc<WasmHost>,
    read_file_sync: Arc<HostValueArc>,
}

impl WebFs {
    pub(crate) fn new(host: Arc<WasmHost>, read_file_sync: Arc<HostValueArc>) -> Self {
        Self {
            host,
            read_file_sync,
        }
    }
}

impl io::IoClassFsFile for WebFs {
    fn text(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _f: io::owned::fs::File,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<String> {
        unsupported()
    }
    fn bytes(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _f: io::owned::fs::File,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<Vec<u8>> {
        unsupported()
    }
    fn read(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _f: io::owned::fs::File,
        _n: i64,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<String> {
        unsupported()
    }
    fn read_bytes(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _f: io::owned::fs::File,
        _n: i64,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<Vec<u8>> {
        unsupported()
    }
    fn close(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _f: io::owned::fs::File,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        unsupported()
    }
    fn seek_from(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _f: io::owned::fs::File,
        _whence: BexExternalValue,
        _offset: i64,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<i64> {
        unsupported()
    }
    fn write(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _f: io::owned::fs::File,
        _data: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<i64> {
        unsupported()
    }
    fn write_bytes(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _f: io::owned::fs::File,
        _data: Vec<u8>,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<i64> {
        unsupported()
    }
}

impl IoNamespaceFs for WebFs {
    fn open(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _path: String,
        _mode: BexExternalValue,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<io::owned::fs::File> {
        unsupported()
    }
    fn exists(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _path: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<bool> {
        unsupported()
    }
    fn remove(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _path: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        unsupported()
    }
    fn remove_dir(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _path: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        unsupported()
    }
    fn remove_dir_all(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _path: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        unsupported()
    }
    fn size(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _path: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<i64> {
        unsupported()
    }

    fn read(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        path: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<String> {
        map_output(
            self.host.call_registered_callable(
                SysOp::BamlFsRead,
                self.read_file_sync.as_ref(),
                &[BexExternalValue::String(path.into())],
                &IndexMap::new(),
            ),
            parse_read_file_result,
        )
    }

    fn write(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _path: String,
        _content: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<i64> {
        unsupported()
    }
    fn write_bytes(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _path: String,
        _content: Vec<u8>,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<i64> {
        unsupported()
    }
    fn read_dir(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _path: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<Vec<io::owned::fs::DirEntry>> {
        unsupported()
    }
    fn mkdir(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _path: String,
        _options: io::owned::fs::MkdirOptions,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        unsupported()
    }

    // These two declare `throws root.errors.Io`, which cannot carry the
    // `Unsupported` that `unsupported()` builds — an off-contract error escapes
    // every typed `catch` arm the caller can write — so the browser's lack of a
    // permission model and of symbolic links is reported as `Io`.
    fn chmod(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _path: String,
        _mode: i64,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        SysOpOutput::err(VmBamlError::Io {
            message: "File permissions are not supported in the browser".to_string(),
        })
    }

    fn symlink(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _target: String,
        _path: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        SysOpOutput::err(VmBamlError::Io {
            message: "Symbolic links are not supported in the browser".to_string(),
        })
    }
}

fn parse_fetch_result(
    value: BexExternalValue,
    duration_ms: Option<i64>,
) -> Result<io::owned::http::Response, VmRustFnError> {
    let mut result = expect_map(value, "fetch result")?;
    match take_string(&mut result, "kind", "fetch result")?.as_str() {
        "ok" => {
            let status_code = take_int(&mut result, "statusCode", "fetch result")?;
            if !(100..=999).contains(&status_code) {
                return Err(bridge_failure(format!(
                    "fetch result statusCode must be between 100 and 999, got {status_code}"
                )));
            }
            let url = take_string(&mut result, "url", "fetch result")?;
            let headers = take_headers(&mut result)?;
            let body = take_bytes(&mut result, "body", "fetch result")?;
            Ok(io::owned::http::Response {
                status_code,
                headers,
                url,
                _body: Arc::new(BufferedResponseBody(Mutex::new(Some(body)))),
            })
        }
        "io" => Err(VmBamlError::Io {
            message: take_string(&mut result, "message", "fetch error")?,
        }
        .into()),
        "timeout" => Err(VmBamlError::Timeout {
            message: take_string(&mut result, "message", "fetch timeout")?,
            duration_ms,
        }
        .into()),
        kind => Err(bridge_failure(format!(
            "fetch result has unknown kind {kind:?}"
        ))),
    }
}

fn parse_read_file_result(value: BexExternalValue) -> Result<String, VmRustFnError> {
    let mut result = expect_map(value, "readFileSync result")?;
    match take_string(&mut result, "kind", "readFileSync result")?.as_str() {
        "ok" => {
            let bytes = take_bytes(&mut result, "bytes", "readFileSync result")?;
            String::from_utf8(bytes).map_err(|error| {
                VmBamlError::ParseError {
                    message: format!("file is not valid UTF-8: {error}"),
                }
                .into()
            })
        }
        "io" => Err(VmBamlError::Io {
            message: take_string(&mut result, "message", "readFileSync error")?,
        }
        .into()),
        "unavailable" => Err(VmBamlError::Unsupported {
            message: take_string(&mut result, "message", "readFileSync unavailable")?,
        }
        .into()),
        kind => Err(bridge_failure(format!(
            "readFileSync result has unknown kind {kind:?}"
        ))),
    }
}

fn take_headers(
    result: &mut IndexMap<String, BexExternalValue>,
) -> Result<IndexMap<String, String>, VmRustFnError> {
    let value = result
        .swap_remove("headers")
        .ok_or_else(|| bridge_failure("fetch result is missing headers"))?;
    let BexExternalValue::Array { items, .. } = value else {
        return Err(bridge_failure("fetch result headers must be an array"));
    };
    items
        .into_iter()
        .map(|pair| {
            let BexExternalValue::Array { mut items, .. } = pair else {
                return Err(bridge_failure("fetch result header must be a pair"));
            };
            if items.len() != 2 {
                return Err(bridge_failure("fetch result header must contain two items"));
            }
            let value = expect_string(items.pop().expect("length checked"), "header value")?;
            let name = expect_string(items.pop().expect("length checked"), "header name")?;
            Ok((name, value))
        })
        .collect()
}

fn expect_map(
    value: BexExternalValue,
    context: &str,
) -> Result<IndexMap<String, BexExternalValue>, VmRustFnError> {
    match value {
        BexExternalValue::Map { entries, .. } => Ok(entries),
        other => Err(bridge_failure(format!(
            "{context} must be a map, got {}",
            other.type_name()
        ))),
    }
}

fn take_string(
    values: &mut IndexMap<String, BexExternalValue>,
    key: &str,
    context: &str,
) -> Result<String, VmRustFnError> {
    let value = values
        .swap_remove(key)
        .ok_or_else(|| bridge_failure(format!("{context} is missing {key}")))?;
    expect_string(value, key)
}

fn expect_string(value: BexExternalValue, context: &str) -> Result<String, VmRustFnError> {
    match value {
        BexExternalValue::String(value) => Ok(value.to_string()),
        other => Err(bridge_failure(format!(
            "{context} must be a string, got {}",
            other.type_name()
        ))),
    }
}

fn take_int(
    values: &mut IndexMap<String, BexExternalValue>,
    key: &str,
    context: &str,
) -> Result<i64, VmRustFnError> {
    match values
        .swap_remove(key)
        .ok_or_else(|| bridge_failure(format!("{context} is missing {key}")))?
    {
        BexExternalValue::Int(value) => Ok(value),
        other => Err(bridge_failure(format!(
            "{context}.{key} must be an integer, got {}",
            other.type_name()
        ))),
    }
}

fn take_bytes(
    values: &mut IndexMap<String, BexExternalValue>,
    key: &str,
    context: &str,
) -> Result<Vec<u8>, VmRustFnError> {
    match values
        .swap_remove(key)
        .ok_or_else(|| bridge_failure(format!("{context} is missing {key}")))?
    {
        BexExternalValue::Uint8Array(value) => Ok(value),
        other => Err(bridge_failure(format!(
            "{context}.{key} must be bytes, got {}",
            other.type_name()
        ))),
    }
}

fn timeout_duration_ms(timeout_nanos: &num_bigint::BigInt) -> Option<i64> {
    if timeout_nanos.sign() == num_bigint::Sign::NoSign {
        return None;
    }
    ((timeout_nanos + num_bigint::BigInt::from(999_999_u64))
        / num_bigint::BigInt::from(1_000_000_u64))
    .to_i64()
}

fn map(entries: IndexMap<String, BexExternalValue>) -> BexExternalValue {
    BexExternalValue::Map {
        key_type: baml_type::RuntimeTy::string(),
        value_type: baml_type::RuntimeTy::unknown(),
        entries,
    }
}

fn array(items: Vec<BexExternalValue>) -> BexExternalValue {
    BexExternalValue::Array {
        element_type: baml_type::RuntimeTy::unknown(),
        items,
    }
}

fn map_output<T, U>(
    output: SysOpOutput<T>,
    map: impl FnOnce(T) -> Result<U, VmRustFnError> + Send + 'static,
) -> SysOpOutput<U>
where
    T: Send + 'static,
    U: Send + 'static,
{
    match output {
        SysOpOutput::Ready(Ok(value)) => SysOpOutput::Ready(map(value)),
        SysOpOutput::Ready(Err(error)) => SysOpOutput::Ready(Err(error)),
        SysOpOutput::Async(future) => SysOpOutput::Async(Box::pin(async move {
            let value = future.await?;
            map(value).map_err(OpErrorBody::from)
        })),
    }
}

fn unsupported<T>() -> SysOpOutput<T> {
    SysOpOutput::err(VmBamlError::Unsupported {
        message: UNSUPPORTED.to_string(),
    })
}

fn bridge_failure(message: impl Into<String>) -> VmRustFnError {
    VmInternalError::BridgeFailure {
        message: message.into(),
    }
    .into()
}
