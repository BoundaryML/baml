//! napi wrappers for the Collector and its view types.
//! Mirrors bridge_python/src/types/collector.rs.

use std::{collections::HashMap, sync::Arc};

use bridge_ctypes::external_to_outbound;
use napi::bindgen_prelude::*;
use napi_derive::napi;
use prost::Message;

#[napi]
pub struct Collector {
    inner: Arc<bex_events::Collector>,
}

impl Collector {
    pub fn inner_arc(&self) -> Arc<bex_events::Collector> {
        Arc::clone(&self.inner)
    }
}

#[napi]
impl Collector {
    #[napi(constructor)]
    pub fn new(name: Option<String>) -> Self {
        Self {
            inner: Arc::new(bex_events::Collector::new(
                name.unwrap_or_else(|| "default".to_string()),
            )),
        }
    }

    #[napi(getter)]
    pub fn name(&self) -> &str {
        self.inner.name()
    }

    #[napi(getter)]
    pub fn logs(&self) -> Vec<FunctionLog> {
        self.inner
            .logs()
            .into_iter()
            .map(FunctionLog::from)
            .collect()
    }

    #[napi(getter)]
    pub fn last(&self) -> Option<FunctionLog> {
        self.inner.last().map(FunctionLog::from)
    }

    #[napi(getter)]
    pub fn usage(&self) -> Usage {
        Usage::from(self.inner.usage())
    }

    #[napi]
    pub fn clear(&self) -> u32 {
        self.inner.clear() as u32
    }

    #[napi]
    pub fn id(&self, function_log_id: String) -> Option<FunctionLog> {
        self.inner.id(&function_log_id).map(FunctionLog::from)
    }
}

#[napi]
pub struct FunctionLog {
    inner: bex_events::FunctionLog,
}

impl From<bex_events::FunctionLog> for FunctionLog {
    fn from(inner: bex_events::FunctionLog) -> Self {
        Self { inner }
    }
}

#[napi]
impl FunctionLog {
    #[napi(getter)]
    pub fn id(&self) -> String {
        self.inner.id.to_string()
    }

    #[napi(getter, js_name = "functionName")]
    pub fn function_name(&self) -> &str {
        &self.inner.function_name
    }

    #[napi(getter)]
    pub fn timing(&self) -> Timing {
        Timing::from(self.inner.timing.clone())
    }

    #[napi(getter)]
    pub fn usage(&self) -> Usage {
        Usage::from(self.inner.usage.clone())
    }

    #[napi(getter)]
    pub fn calls(&self) -> Vec<LLMCall> {
        self.inner
            .calls
            .iter()
            .cloned()
            .map(LLMCall::from)
            .collect()
    }

    #[napi(getter)]
    pub fn tags(&self) -> HashMap<String, String> {
        self.inner.tags.clone()
    }

    #[napi(getter)]
    pub fn result(&self) -> Result<Option<Buffer>> {
        let handle_options = bridge_ctypes::CffiHandleTableOptions::for_in_process();
        self.inner
            .result
            .as_ref()
            .map(|val| {
                let baml_val = external_to_outbound(val, &handle_options).map_err(|e| {
                    napi::Error::from_reason(format!(
                        "FunctionLog.result: failed to convert value: {e}"
                    ))
                })?;
                Ok(Buffer::from(baml_val.encode_to_vec()))
            })
            .transpose()
    }
}

#[napi]
pub struct Timing {
    inner: bex_events::Timing,
}

impl From<bex_events::Timing> for Timing {
    fn from(inner: bex_events::Timing) -> Self {
        Self { inner }
    }
}

#[napi]
impl Timing {
    #[napi(getter, js_name = "startTimeUtcMs")]
    pub fn start_time_utc_ms(&self) -> i64 {
        self.inner.start_time_utc_ms
    }

    #[napi(getter, js_name = "durationMs")]
    pub fn duration_ms(&self) -> Option<i64> {
        self.inner.duration_ms
    }
}

#[napi]
pub struct Usage {
    inner: bex_events::Usage,
}

impl From<bex_events::Usage> for Usage {
    fn from(inner: bex_events::Usage) -> Self {
        Self { inner }
    }
}

#[napi]
impl Usage {
    #[napi(getter, js_name = "inputTokens")]
    pub fn input_tokens(&self) -> Option<i64> {
        self.inner.input_tokens
    }

    #[napi(getter, js_name = "outputTokens")]
    pub fn output_tokens(&self) -> Option<i64> {
        self.inner.output_tokens
    }

    #[napi(getter, js_name = "cachedInputTokens")]
    pub fn cached_input_tokens(&self) -> Option<i64> {
        self.inner.cached_input_tokens
    }
}

#[napi]
pub struct LLMCall {
    inner: bex_events::LLMCall,
}

impl From<bex_events::LLMCall> for LLMCall {
    fn from(inner: bex_events::LLMCall) -> Self {
        Self { inner }
    }
}

#[napi]
impl LLMCall {
    #[napi(getter, js_name = "functionName")]
    pub fn function_name(&self) -> &str {
        &self.inner.function_name
    }

    #[napi(getter)]
    pub fn provider(&self) -> Option<&str> {
        self.inner.provider.as_deref()
    }

    #[napi(getter)]
    pub fn timing(&self) -> Timing {
        Timing::from(self.inner.timing.clone())
    }

    #[napi(getter)]
    pub fn usage(&self) -> Usage {
        Usage::from(self.inner.usage.clone())
    }
}
