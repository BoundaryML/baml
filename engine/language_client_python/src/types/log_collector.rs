use std::sync::{Arc, Mutex};

use baml_runtime::tracingv2::storage::storage::BAML_TRACER;
// use baml_types::tracing::events::{FunctionId, TraceEvent};
use pyo3::{prelude::*};
// use baml_types::tracing::events::TraceEvent;
use either::Either;
// Suppose we have a "LastRequestInfo" Python-exposed struct:
// #[pyo3::prelude::pyclass(module = "baml_py.baml_py")]
// #[derive(Clone)]
// pub struct LastRequestInfo {
//     #[pyo3(get, set)]
//     pub id: String,
//     #[pyo3(get, set)]
//     pub usage: String,
//     #[pyo3(get, set)]
//     pub raw_request: String,
//     #[pyo3(get, set)]
//     pub raw_response: Vec<String>,
//     #[pyo3(get, set)]
//     pub pre_parser: Option<String>,
// }

// Use the macro from lang_wrapper.rs to create a pyo3 class named "LogCollector"
// around our Rust `Collector`, with Arc-based clone safety.
// We also add a custom attribute "last" of type Option<LastRequestInfo>.
crate::lang_wrapper!(
    Collector,
    baml_runtime::tracingv2::storage::storage::Collector,
    clone_safe);

#[pymethods]
impl Collector {
    #[new]
    pub fn new(id: String) -> Self {
        let collector = baml_runtime::tracingv2::storage::storage::Collector::new(id);
        Self {
            // id,
            inner: Arc::new(collector),
            // last: None,
        }
    }

    /// For Python: `repr(log_collector)`
    fn __repr__(&self) -> String {
        format!(
            "<LogCollector collector_id={}>",
            self.inner.id()
        )
    }

    pub fn logs(&self) -> Vec<FunctionLog> {
        self.inner
            .function_logs()
            .iter()
            .map(|inner_function_log| FunctionLog {
                inner: Arc::new(Mutex::new(inner_function_log.clone())),
            })
            .collect()
    }

    pub fn last(&self) -> Option<FunctionLog> {
        self.inner.last_function_log().map(|inner_function_log| FunctionLog {
            inner: Arc::new(Mutex::new(inner_function_log.clone())),
        })
    }

    pub fn id(&self, function_log_id: String) -> Option<FunctionLog> {
        self.inner.function_log_by_id(&baml_types::tracing::events::FunctionId(function_log_id)).map(|inner_function_log| FunctionLog {
            inner: Arc::new(Mutex::new(inner_function_log.clone())),
        })
    }

    #[staticmethod]
    pub fn __function_span_count() -> usize {
        BAML_TRACER.lock().unwrap().function_span_count()
    }
}

crate::lang_wrapper!(
    FunctionLog,
    baml_runtime::tracingv2::storage::storage::FunctionLog,
    sync_thread_safe
);

#[pyclass]
pub struct CollectorList {
    pub inner: Arc<Vec<Collector>>,
}



#[pymethods]
impl CollectorList {
    pub fn __repr__(&self) -> String {
        format!("<CollectorList: {} collectors>", self.inner.len())
    }
}

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
        format!("<FunctionLog id={}>", self.inner.lock().unwrap().id().0)
    }

    #[getter]
    pub fn id(&self) -> String {
        self.inner.lock().unwrap().id().0
    }

    // pub fn test_data(&self) -> String {
    //     self.inner.test_data()
    // }
    #[getter]
    pub fn function_name(&self) -> String {
        self.inner.lock().unwrap().function_name()
    }

    #[getter]
    pub fn log_type(&self) -> String {
        self.inner.lock().unwrap().log_type().to_string()
    }

    #[getter]
    pub fn timing(&self) -> Timing {
        Timing { inner: self.inner.lock().unwrap().timing() }
    }

    #[getter]
    pub fn usage(&self) -> Usage {
        Usage { inner: self.inner.lock().unwrap().usage() }
    }


    #[getter]
    pub fn calls(&self) -> PyResult<Vec<Either<LLMCall, LLMStreamCall>>> {
        self.inner.lock().unwrap().calls().into_iter().map(|inner| match inner {
            baml_runtime::tracingv2::storage::storage::LLMCallKind::Basic(inner) => Either::Left(LLMCall { inner: inner.clone() }),
            baml_runtime::tracingv2::storage::storage::LLMCallKind::Stream(inner) => Either::Right(LLMStreamCall { inner: inner.clone() }),
        }).collect()
    }
    

   
}

crate::lang_wrapper!(Timing, baml_runtime::tracingv2::storage::storage::Timing);

crate::lang_wrapper!(Usage, baml_runtime::tracingv2::storage::storage::Usage);

// crate::lang_wrapper!(
//     LLMCallKind,
//     baml_runtime::tracingv2::storage::storage::LLMCallKind
// );

crate::lang_wrapper!(LLMCall, baml_runtime::tracingv2::storage::storage::LLMCall);

crate::lang_wrapper!(LLMStreamCall, baml_runtime::tracingv2::storage::storage::LLMStreamCall);

// TODO: remove unwraps
#[pymethods]
impl LLMCall {
    // pub fn new(inner: baml_runtime::tracingv2::storage::storage::LLMCall) -> Self {
    //     Self { inner }
    // }

    pub fn selected(&self) -> bool {
        self.inner.selected
    }

    pub fn provider(&self) -> String {
        self.inner.provider.clone()
    }

    pub fn client_name(&self) -> String {
        self.inner.client_name.clone()
    }

    pub fn response(&self) -> Option<HTTPResponse> {
        self.inner.response.clone().map(|inner| HTTPResponse {
            inner,
        })
    }

    pub fn request(&self) -> Option<HTTPRequest> {
        self.inner.request.clone().map(|inner| HTTPRequest {
            inner,
        })
    }

    pub fn usage(&self) -> Option<Usage> {
        if let Some(inner) = self.inner.usage.clone() {
            Some(Usage {
                inner,
            })
        } else {
            None
        }
    }

    pub fn timing(&self) -> Timing {
        Timing {
            inner: self.inner.timing.clone(),
        }
    }

    // TODO: the request_id ? And / Or span_id ?
    pub fn __repr__(&self) -> String {
        format!("<LLMCall: provider={}, client_name={}, selected={}, response={:?}, request={:?}, usage={:?}, timing={:?}>", 
            self.provider(), 
            self.client_name(), 
            self.selected(), 
            self.response().map_or("None".to_string(), |inner| inner.__repr__()), 
            self.request().map_or("None".to_string(), |inner| inner.__repr__()), 
            self.usage().map_or("None".to_string(), |inner| inner.__repr__()), 
            self.timing().__repr__())
    }
}

crate::lang_wrapper!(HTTPRequest, baml_types::tracing::events::HTTPRequest, clone_safe);

#[pymethods]
impl HTTPRequest {
    pub fn __repr__(&self) -> String {
        format!("<HTTPRequest: url={}, method={}, headers={:?}, body={:?}>", self.inner.url, self.inner.method, self.inner.headers, self.inner.body)
    }
}

crate::lang_wrapper!(HTTPResponse, baml_types::tracing::events::HTTPResponse, clone_safe);

// TODO: print each of these as actual json pretty strings or python dicts
#[pymethods]
impl HTTPResponse {
    pub fn __repr__(&self) -> String {
        format!("<HTTPResponse: status={}, headers={:?}, body={:?}>", self.inner.status, self.inner.headers, self.inner.body)
    }
}
#[pymethods]
impl Usage {
    pub fn __repr__(&self) -> String {
        format!("<Usage: input_tokens={}, output_tokens={}>", self.inner.input_tokens, self.inner.output_tokens)
    }
}

#[pymethods]
impl Timing {
    pub fn __repr__(&self) -> String {
        format!("<Timing: start_time_utc_ms={}, duration_ms={}, time_to_first_parsed_ms={}>", self.inner.start_time_utc_ms, self.inner.duration_ms, self.inner.time_to_first_parsed_ms)
    }
}



// impl Drop for FunctionLog {
//     fn drop(&mut self) {
//         BAML_TRACER
//             .blocking_lock()
//             .dec_function_id(&FunctionId(self.id.clone()));
//     }
// }
