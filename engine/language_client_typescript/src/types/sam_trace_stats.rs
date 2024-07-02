use napi_derive::napi;
use std::sync;
use std::sync::Arc;
use std::sync::Mutex;

#[derive(internal_baml_derive::FfiWrapper)]
struct SamTraceStats {
    // #[ffi]
    inner: Arc<Mutex<baml_runtime::TraceStats>>,
}
