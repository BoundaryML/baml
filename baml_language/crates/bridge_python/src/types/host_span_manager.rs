//! HostSpanManager — thin PyO3 wrapper delegating to `bridge_cffi::host_spans`.

use std::collections::HashMap;

use pyo3::{
    prelude::*,
    types::{PyDict, PyList},
};

/// Manages host-side span tracking for `@trace` in Python.
///
/// This is a thin PyO3 wrapper around `bridge_cffi::host_spans::HostSpanManager`.
/// All core logic (span stack, event emission) lives in bridge_cffi.
#[pyclass]
pub struct HostSpanManager {
    inner: bridge_cffi::host_spans::HostSpanManager,
}

impl HostSpanManager {
    pub fn new() -> Self {
        Self {
            inner: bridge_cffi::host_spans::HostSpanManager::new(),
        }
    }

    /// Get the current host span context for passing to `call_function`.
    ///
    /// Returns `None` if there are no active host spans.
    pub fn host_span_context(&self) -> Option<bex_events::HostSpanContext> {
        self.inner.host_span_context()
    }
}

#[pymethods]
impl HostSpanManager {
    #[new]
    fn py_new() -> Self {
        Self::new()
    }

    /// Enter a new host-language span (`@trace` function start).
    #[pyo3(signature = (name, args))]
    fn enter(&mut self, py: Python<'_>, name: String, args: PyObject) -> PyResult<()> {
        let args_json = py_to_json(args.bind(py));
        self.inner.enter(name, args_json);
        Ok(())
    }

    /// Exit the current span successfully.
    fn exit_ok(&mut self) {
        self.inner.exit_ok();
    }

    /// Exit the current span with an error.
    fn exit_error(&mut self, error_message: String) {
        self.inner.exit_error(error_message);
    }

    /// Merge tags into the current span and emit a `SetTags` event.
    fn upsert_tags(&mut self, tags: HashMap<String, String>) {
        self.inner.upsert_tags(tags);
    }

    /// Deep clone for async context forking.
    fn deep_clone(&self) -> Self {
        Self {
            inner: self.inner.deep_clone(),
        }
    }

    /// Number of active spans (call depth).
    fn context_depth(&self) -> usize {
        self.inner.context_depth()
    }
}

// ────────────────────────────────── Helpers ─────────────────────────────────

/// Recursively convert a Python object to a `serde_json::Value`.
fn py_to_json(obj: &Bound<'_, PyAny>) -> serde_json::Value {
    if obj.is_none() {
        return serde_json::Value::Null;
    }
    // bool before int (bool is a subclass of int in Python)
    if let Ok(b) = obj.extract::<bool>() {
        return serde_json::Value::Bool(b);
    }
    if let Ok(i) = obj.extract::<i64>() {
        return serde_json::json!(i);
    }
    if let Ok(f) = obj.extract::<f64>() {
        return serde_json::json!(f);
    }
    if let Ok(s) = obj.extract::<String>() {
        return serde_json::Value::String(s);
    }
    if let Ok(list) = obj.downcast::<PyList>() {
        let items: Vec<serde_json::Value> = list.iter().map(|item| py_to_json(&item)).collect();
        return serde_json::Value::Array(items);
    }
    if let Ok(dict) = obj.downcast::<PyDict>() {
        let mut map = serde_json::Map::new();
        for (k, v) in dict.iter() {
            let key = k
                .extract::<String>()
                .unwrap_or_else(|_| k.str().map(|s| s.to_string()).unwrap_or_default());
            map.insert(key, py_to_json(&v));
        }
        return serde_json::Value::Object(map);
    }
    // Fallback: repr
    match obj.repr() {
        Ok(s) => serde_json::Value::String(s.to_string()),
        Err(_) => serde_json::Value::String("<unprintable>".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_starts_with_zero_depth() {
        let mgr = HostSpanManager::new();
        assert_eq!(mgr.inner.context_depth(), 0);
    }

    #[test]
    fn host_span_context_none_when_empty() {
        let mgr = HostSpanManager::new();
        assert!(mgr.host_span_context().is_none());
    }
}
