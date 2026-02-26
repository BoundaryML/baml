//! PyO3 wrappers for the Collector and its view types.

use std::{collections::HashMap, sync::Arc};

use bridge_ctypes::external_to_baml_value;
use prost::Message;
use pyo3::prelude::*;

/// Python-facing Collector that tracks BAML function call logs.
///
/// Usage:
/// ```python
/// from baml_py import Collector
/// collector = Collector("my_collector")
/// result = await b.MyFunction("input", baml_options={"collector": collector})
/// print(collector.logs)
/// print(collector.usage)
/// ```
#[pyclass(subclass)]
pub struct Collector {
    inner: Arc<bex_events::Collector>,
}

impl Collector {
    /// Get a clone of the Arc for passing across async boundaries.
    pub fn inner_arc(&self) -> Arc<bex_events::Collector> {
        Arc::clone(&self.inner)
    }
}

#[pymethods]
impl Collector {
    #[new]
    #[pyo3(signature = (name=None))]
    fn new(name: Option<String>) -> Self {
        Self {
            inner: Arc::new(bex_events::Collector::new(
                name.unwrap_or_else(|| "default".to_string()),
            )),
        }
    }

    /// The collector's name.
    #[getter]
    fn name(&self) -> &str {
        self.inner.name()
    }

    /// All function logs tracked by this collector, in insertion order.
    #[getter]
    fn logs(&self) -> Vec<FunctionLog> {
        self.inner
            .logs()
            .into_iter()
            .map(FunctionLog::from)
            .collect()
    }

    /// The most recent function log, or None if empty.
    #[getter]
    fn last(&self) -> Option<FunctionLog> {
        self.inner.last().map(FunctionLog::from)
    }

    /// Aggregate token usage across all tracked calls.
    #[getter]
    fn usage(&self) -> Usage {
        Usage::from(self.inner.usage())
    }

    /// Clear all tracked logs and release event store references.
    /// Returns the number of logs that were cleared.
    fn clear(&self) -> usize {
        self.inner.clear()
    }

    /// Look up a function log by its span ID string.
    fn id(&self, function_log_id: String) -> Option<FunctionLog> {
        self.inner.id(&function_log_id).map(FunctionLog::from)
    }

    fn __repr__(&self) -> String {
        format!(
            "Collector(name='{}', logs={})",
            self.inner.name(),
            self.inner.logs().len()
        )
    }
}

/// Read-only view of a single BAML function invocation.
#[pyclass]
#[derive(Clone)]
pub struct FunctionLog {
    inner: bex_events::FunctionLog,
}

impl From<bex_events::FunctionLog> for FunctionLog {
    fn from(inner: bex_events::FunctionLog) -> Self {
        Self { inner }
    }
}

#[pymethods]
impl FunctionLog {
    /// The span ID for this function invocation.
    #[getter]
    fn id(&self) -> String {
        self.inner.id.to_string()
    }

    /// The BAML function name.
    #[getter]
    fn function_name(&self) -> &str {
        &self.inner.function_name
    }

    /// Timing information (start time, duration).
    #[getter]
    fn timing(&self) -> Timing {
        Timing::from(self.inner.timing.clone())
    }

    /// Token usage for this function invocation.
    #[getter]
    fn usage(&self) -> Usage {
        Usage::from(self.inner.usage.clone())
    }

    /// Child LLM calls made during this function invocation.
    #[getter]
    fn calls(&self) -> Vec<LLMCall> {
        self.inner
            .calls
            .iter()
            .cloned()
            .map(LLMCall::from)
            .collect()
    }

    /// Tags (metadata) attached to this invocation.
    #[getter]
    fn tags(&self) -> HashMap<String, String> {
        self.inner.tags.clone()
    }

    /// The result value as protobuf-encoded bytes, or None if not yet complete.
    #[getter]
    fn result(&self) -> Option<Vec<u8>> {
        let handle_options = bridge_ctypes::HandleTableOptions::for_in_process();
        self.inner.result.as_ref().and_then(|val| {
            match external_to_baml_value(val, &handle_options) {
                Ok(baml_val) => Some(baml_val.encode_to_vec()),
                Err(e) => {
                    log::warn!("FunctionLog.result: failed to convert value: {e}");
                    None
                }
            }
        })
    }

    fn __repr__(&self) -> String {
        format!(
            "FunctionLog(function_name='{}', id='{}')",
            self.inner.function_name, self.inner.id,
        )
    }
}

/// Timing information for a span.
#[pyclass]
#[derive(Clone)]
pub struct Timing {
    inner: bex_events::Timing,
}

impl From<bex_events::Timing> for Timing {
    fn from(inner: bex_events::Timing) -> Self {
        Self { inner }
    }
}

#[pymethods]
impl Timing {
    /// Start time as UTC milliseconds since epoch.
    #[getter]
    fn start_time_utc_ms(&self) -> i64 {
        self.inner.start_time_utc_ms
    }

    /// Duration in milliseconds, or None if not yet complete.
    #[getter]
    fn duration_ms(&self) -> Option<i64> {
        self.inner.duration_ms
    }

    fn __repr__(&self) -> String {
        format!(
            "Timing(start_time_utc_ms={}, duration_ms={:?})",
            self.inner.start_time_utc_ms, self.inner.duration_ms,
        )
    }
}

/// Token usage from LLM calls.
#[pyclass]
#[derive(Clone)]
pub struct Usage {
    inner: bex_events::Usage,
}

impl From<bex_events::Usage> for Usage {
    fn from(inner: bex_events::Usage) -> Self {
        Self { inner }
    }
}

#[pymethods]
impl Usage {
    /// Number of input tokens, or None if not reported.
    #[getter]
    fn input_tokens(&self) -> Option<i64> {
        self.inner.input_tokens
    }

    /// Number of output tokens, or None if not reported.
    #[getter]
    fn output_tokens(&self) -> Option<i64> {
        self.inner.output_tokens
    }

    /// Number of cached input tokens, or None if not reported.
    #[getter]
    fn cached_input_tokens(&self) -> Option<i64> {
        self.inner.cached_input_tokens
    }

    fn __repr__(&self) -> String {
        format!(
            "Usage(input_tokens={:?}, output_tokens={:?}, cached_input_tokens={:?})",
            self.inner.input_tokens, self.inner.output_tokens, self.inner.cached_input_tokens,
        )
    }
}

/// A single LLM call within a function invocation.
#[pyclass]
#[derive(Clone)]
pub struct LLMCall {
    inner: bex_events::LLMCall,
}

impl From<bex_events::LLMCall> for LLMCall {
    fn from(inner: bex_events::LLMCall) -> Self {
        Self { inner }
    }
}

#[pymethods]
impl LLMCall {
    /// The LLM function name.
    #[getter]
    fn function_name(&self) -> &str {
        &self.inner.function_name
    }

    /// The provider name, if known.
    #[getter]
    fn provider(&self) -> Option<&str> {
        self.inner.provider.as_deref()
    }

    /// Timing information for this LLM call.
    #[getter]
    fn timing(&self) -> Timing {
        Timing::from(self.inner.timing.clone())
    }

    /// Token usage for this LLM call.
    #[getter]
    fn usage(&self) -> Usage {
        Usage::from(self.inner.usage.clone())
    }

    fn __repr__(&self) -> String {
        format!(
            "LLMCall(function_name='{}', provider={:?})",
            self.inner.function_name, self.inner.provider,
        )
    }
}
