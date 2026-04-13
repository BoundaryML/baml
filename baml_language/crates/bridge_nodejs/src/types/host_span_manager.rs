//! HostSpanManager — thin napi wrapper delegating to `bridge_cffi::host_spans`.
//! Mirrors bridge_python/src/types/host_span_manager.rs.

use std::collections::HashMap;

use napi_derive::napi;

#[napi]
pub struct HostSpanManager {
    inner: bridge_cffi::host_spans::HostSpanManager,
}

impl HostSpanManager {
    pub fn new_internal() -> Self {
        let sink = bridge_cffi::engine::get_event_sink();
        Self {
            inner: bridge_cffi::host_spans::HostSpanManager::new(sink),
        }
    }

    pub fn host_span_context(&self) -> Option<bex_events::HostSpanContext> {
        self.inner.host_span_context()
    }
}

#[napi]
impl HostSpanManager {
    #[napi(constructor)]
    pub fn new() -> Self {
        Self::new_internal()
    }

    #[napi]
    pub fn enter(&mut self, name: String, args: serde_json::Value) -> napi::Result<()> {
        self.inner.enter(name, args);
        Ok(())
    }

    #[napi(js_name = "exitOk")]
    pub fn exit_ok(&mut self) {
        self.inner.exit_ok();
    }

    #[napi(js_name = "exitError")]
    pub fn exit_error(&mut self, error_message: String) {
        self.inner.exit_error(error_message);
    }

    #[napi(js_name = "upsertTags")]
    pub fn upsert_tags(&mut self, tags: HashMap<String, String>) {
        self.inner.upsert_tags(tags);
    }

    #[napi(js_name = "deepClone")]
    pub fn deep_clone(&self) -> Self {
        Self {
            inner: self.inner.deep_clone(),
        }
    }

    #[napi(js_name = "contextDepth")]
    pub fn context_depth(&self) -> u32 {
        self.inner.context_depth() as u32
    }
}
