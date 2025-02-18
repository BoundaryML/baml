use std::sync::Arc;

use baml_runtime::tracingv2::storage::storage::Collector;
// use baml_types::tracing::events::{FunctionId, TraceEvent};
use pyo3::prelude::*;
// use baml_types::tracing::events::TraceEvent;

// Suppose we have a "LastRequestInfo" Python-exposed struct:
#[pyo3::prelude::pyclass(module = "baml_py.baml_py")]
#[derive(Clone)]
pub struct LastRequestInfo {
    #[pyo3(get, set)]
    pub id: String,
    #[pyo3(get, set)]
    pub usage: String,
    #[pyo3(get, set)]
    pub raw_request: String,
    #[pyo3(get, set)]
    pub raw_response: Vec<String>,
    #[pyo3(get, set)]
    pub pre_parser: Option<String>,
}

// Use the macro from lang_wrapper.rs to create a pyo3 class named "LogCollector"
// around our Rust `Collector`, with Arc-based clone safety.
// We also add a custom attribute "last" of type Option<LastRequestInfo>.
crate::lang_wrapper!(
    LogCollector,
    Collector,
    clone_safe,
    last: Option<LastRequestInfo> = None
);

// TODO: next up -- pass the collector thru baml options.
#[pymethods]
impl LogCollector {
    #[new]
    pub fn new(id: String) -> Self {
        let collector = Collector::new(id);
        // BAML_TRACER.blocking_lock().register_collector(collector);
        Self {
            // id,
            inner: Arc::new(collector),
            last: None,
        }
    }

    /// For Python: `repr(log_collector)`
    fn __repr__(&self) -> String {
        format!(
            "<LogCollector collector_id={}, has_last={}>",
            self.inner.id(),
            self.last.is_some()
        )
    }

    // TODO: just get event ids
    pub fn events(&self) -> Vec<FunctionLog> {
        self.inner
            .function_logs()
            .iter()
            .map(|inner_function_log| FunctionLog {
                inner: inner_function_log.clone(),
            })
            .collect()
    }
}

// TODO: do the decrementing in the Rust object, so we dont have to replicate this logic in Typescript as well. Just call like "clean" or something.

// #[pyclass(module = "baml_py.baml_py")]
// pub struct FunctionLog {
//     // id: String,
//     // inner: Arc<Mutex<Vec<Arc<TraceEvent>>>>,
// }

crate::lang_wrapper!(
    FunctionLog,
    baml_runtime::tracingv2::storage::storage::FunctionLog,
    clone_safe
);

#[pymethods]
impl FunctionLog {
    // #[new]
    // pub fn new(id: String) -> Self {
    //     BAML_TRACER
    //         .blocking_lock()
    //         .inc_function_id(&FunctionId(id.clone()));
    //     Self { id }
    // }

    // pub fn id(&self) -> String {
    //     self.id.clone()
    // }

    // pub fn usage(&self) -> String {
    //     // "usage".to_string()
    //     // self.usage.clone()
    //     BAML_TRACER
    //         .blocking_lock()
    //         .get_events(&FunctionId(self.id.clone()))
    //         .iter()
    //         .last()
    //         .unwrap()
    //         // TODO this panics in async context ?
    //         .blocking_lock()
    //         .last()
    //         .unwrap()
    //         .event_id
    //         .0
    //         .to_string()
    // }
    fn __repr__(&self) -> String {
        format!("<FunctionLog id={}>", self.inner.id().0)
    }
}

// impl Drop for FunctionLog {
//     fn drop(&mut self) {
//         BAML_TRACER
//             .blocking_lock()
//             .dec_function_id(&FunctionId(self.id.clone()));
//     }
// }
