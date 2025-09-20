use std::{
    str::FromStr,
    sync::{Arc, Mutex},
};

use baml_runtime::tracingv2::storage::storage::BAML_TRACER;
use either::Either;
use pyo3::{
    prelude::*,
    types::{PyDict, PyList},
    IntoPyObjectExt,
};
use serde_json::Value as JsonValue;

crate::lang_wrapper!(
    Collector,
    baml_runtime::tracingv2::storage::storage::Collector,
    clone_safe
);

use super::{HTTPRequest, HTTPResponse, SSEResponse};

#[pymethods]
impl Collector {
    #[new]
    #[pyo3(signature = (name=None))]
    pub fn new(name: Option<String>) -> Self {
        let collector = baml_runtime::tracingv2::storage::storage::Collector::new(name);
        Self {
            inner: Arc::new(collector),
        }
    }

    /// Clear all tracked logs from this collector
    pub fn clear(&self) {
        self.inner.clear();
    }

    /// For Python: `repr(log_collector)`
    fn __repr__(&self) -> String {
        let logs = self.logs();
        let log_ids: Vec<String> = logs
            .iter()
            .map(|log| log.inner.lock().unwrap().id().to_string())
            .collect();
        format!(
            "LogCollector(name={}, function_log_ids=[{}])",
            self.inner.name(),
            log_ids.join(", ")
        )
    }

    #[getter]
    pub fn logs(&self) -> Vec<FunctionLog> {
        self.inner
            .function_logs()
            .iter()
            .map(|inner_function_log| FunctionLog {
                inner: Arc::new(Mutex::new(inner_function_log.clone())),
            })
            .collect()
    }

    #[getter]
    pub fn last(&self) -> Option<FunctionLog> {
        self.inner
            .last_function_log()
            .map(|inner_function_log| FunctionLog {
                inner: Arc::new(Mutex::new(inner_function_log.clone())),
            })
    }

    pub fn id(&self, function_log_id: String) -> Option<FunctionLog> {
        self.inner
            .function_log_by_id(&baml_ids::FunctionCallId::from_str(&function_log_id).ok()?)
            .map(|inner_function_log| FunctionLog {
                inner: Arc::new(Mutex::new(inner_function_log.clone())),
            })
    }

    #[getter]
    pub fn usage(&self) -> Usage {
        Usage {
            inner: self.inner.usage(),
        }
    }

    #[staticmethod]
    pub fn __function_call_count() -> usize {
        BAML_TRACER.lock().unwrap().function_call_count()
    }

    #[staticmethod]
    pub fn __print_storage() {
        let tracer = BAML_TRACER.lock().unwrap();
        println!("Storage: {tracer:#?}");
    }
}

crate::lang_wrapper!(
    FunctionLog,
    baml_runtime::tracingv2::storage::storage::FunctionLog,
    sync_thread_safe
);

#[pymethods]
impl FunctionLog {
    fn __repr__(&self) -> String {
        format!(
            "FunctionLog(id={}, function_name={}, type={}, timing={}, usage={}, calls=[{}], raw_llm_response={})",
            self.id(),
            self.function_name(),
            self.log_type(),
            self.timing().__repr__(),
            self.usage().__repr__(),
            self.calls().unwrap_or_default().into_iter().map(|call| match call {
                Either::Left(call) => call.__repr__(),
                Either::Right(call) => call.__repr__(),
            }).collect::<Vec<_>>().join(", "),
            self.raw_llm_response().unwrap_or("None".to_string())
        )
    }

    #[getter]
    pub fn id(&self) -> String {
        self.inner.lock().unwrap().id().to_string()
    }

    #[getter]
    pub fn function_name(&self) -> String {
        self.inner.lock().unwrap().function_name()
    }

    /// pyi: @property def log_type -> Literal["call", "stream"]
    #[getter]
    pub fn log_type(&self) -> String {
        self.inner.lock().unwrap().log_type()
    }

    #[getter]
    pub fn timing(&self) -> Timing {
        Timing {
            inner: self.inner.lock().unwrap().timing(),
        }
    }

    #[getter]
    pub fn usage(&self) -> Usage {
        Usage {
            inner: self.inner.lock().unwrap().usage(),
        }
    }

    /// pyi: @property def calls -> List[LLMCall | LLMStreamCall]
    #[getter]
    pub fn calls(&self) -> PyResult<Vec<Either<LLMCall, LLMStreamCall>>> {
        let calls = self.inner.lock().unwrap().calls();
        Ok(calls
            .into_iter()
            .map(|inner| match inner {
                baml_runtime::tracingv2::storage::storage::LLMCallKind::Basic(inner) => {
                    Either::Left(LLMCall {
                        inner: inner.clone(),
                    })
                }
                baml_runtime::tracingv2::storage::storage::LLMCallKind::Stream(inner) => {
                    Either::Right(LLMStreamCall {
                        inner: inner.clone(),
                    })
                }
            })
            .collect::<Vec<_>>())
    }

    #[getter]
    pub fn raw_llm_response(&self) -> Option<String> {
        // Modify as needed to locate or parse the "raw_llm_response"
        let mut guarded = self.inner.lock().unwrap();
        // Example: If it stores somewhere in the struct
        guarded.raw_llm_response()
    }

    /// pyi: @property def metadata -> Dict[str, Any]
    /// We expose a (String -> PyObject) map or similar.
    #[getter]
    pub fn metadata<'py>(&self, py: Python<'py>) -> PyResult<PyObject> {
        // Construct a python dict with relevant metadata
        let meta = self.inner.lock().unwrap().metadata();
        let dict = PyDict::new(py);
        for (k, v) in meta.iter() {
            // Convert each value to a PyObject as appropriate
            dict.set_item(k, serde_value_to_py(py, v)?)?;
        }
        Ok(dict.into())
    }

    /// pyi: @property def selected_call -> Optional[Union[LLMCall, LLMStreamCall]]
    /// Suppose if there's exactly one call with `selected=true`, we return it:
    #[getter]
    pub fn selected_call(&self) -> Option<Either<LLMCall, LLMStreamCall>> {
        let calls = self.inner.lock().unwrap().calls();
        calls.into_iter().find_map(|call| match call {
            baml_runtime::tracingv2::storage::storage::LLMCallKind::Basic(inner) => {
                if inner.selected {
                    Some(Either::Left(LLMCall { inner }))
                } else {
                    None
                }
            }
            baml_runtime::tracingv2::storage::storage::LLMCallKind::Stream(inner) => {
                if inner.llm_call.selected {
                    Some(Either::Right(LLMStreamCall { inner }))
                } else {
                    None
                }
            }
        })
    }
}

crate::lang_wrapper!(Timing, baml_runtime::tracingv2::storage::storage::Timing);

crate::lang_wrapper!(
    StreamTiming,
    baml_runtime::tracingv2::storage::storage::StreamTiming
);

crate::lang_wrapper!(Usage, baml_runtime::tracingv2::storage::storage::Usage);

crate::lang_wrapper!(LLMCall, baml_runtime::tracingv2::storage::storage::LLMCall);

crate::lang_wrapper!(
    LLMStreamCall,
    baml_runtime::tracingv2::storage::storage::LLMStreamCall
);

#[pymethods]
impl LLMCall {
    #[getter]
    pub fn selected(&self) -> bool {
        self.inner.selected
    }

    #[getter]
    pub fn http_request(&self) -> Option<HTTPRequest> {
        self.inner
            .request
            .clone()
            .map(|req| HTTPRequest { inner: req })
    }

    #[getter]
    pub fn http_response(&self) -> Option<HTTPResponse> {
        self.inner
            .response
            .clone()
            .map(|resp| HTTPResponse { inner: resp })
    }

    #[getter]
    pub fn usage(&self) -> Option<Usage> {
        self.inner.usage.clone().map(|u| Usage { inner: u })
    }

    #[getter]
    pub fn timing(&self) -> Timing {
        Timing {
            inner: self.inner.timing.clone(),
        }
    }

    #[getter]
    pub fn provider(&self) -> String {
        self.inner.provider.clone()
    }

    #[getter]
    pub fn client_name(&self) -> String {
        self.inner.client_name.clone()
    }

    pub fn __repr__(&self) -> String {
        format!(
            "LLMCall(provider={}, client_name={}, selected={}, usage={}, timing={}, http_request={}, http_response={})>",
            self.provider(),
            self.client_name(),
            self.selected(),
            self.usage().map_or("None".to_string(), |u| u.__repr__()),
            self.timing().__repr__(),
            self.http_request().map_or("None".to_string(), |req| req.__repr__()),
            self.http_response().map_or("None".to_string(), |resp| resp.__repr__())
        )
    }
}

#[pymethods]
impl LLMStreamCall {
    /// If we want a separate __repr__ / __str__, we can define it:
    pub fn __repr__(&self) -> String {
        format!(
            "LLMStreamCall(provider={}, client_name={}, selected={}, usage={}, timing={}, http_request={}, http_response={})",
            self.provider(),
            self.client_name(),
            self.selected(),
            self.usage().map_or("None".to_string(), |u| u.__repr__()),
            self.timing().__repr__(),
            self.http_request().map_or("None".to_string(), |req| req.__repr__()),
            self.http_response().map_or("None".to_string(), |resp| resp.__repr__())
        )
    }

    #[getter]
    pub fn http_request(&self) -> Option<HTTPRequest> {
        self.inner
            .llm_call
            .request
            .clone()
            .map(|req| HTTPRequest { inner: req })
    }

    #[getter]
    pub fn http_response(&self) -> Option<HTTPResponse> {
        self.inner
            .llm_call
            .response
            .clone()
            .map(|resp| HTTPResponse { inner: resp })
    }

    // TODO: use python subclassing
    #[getter]
    pub fn provider(&self) -> String {
        self.inner.llm_call.provider.clone()
    }

    #[getter]
    pub fn client_name(&self) -> String {
        self.inner.llm_call.client_name.clone()
    }

    #[getter]
    pub fn selected(&self) -> bool {
        self.inner.llm_call.selected
    }

    #[getter]
    pub fn usage(&self) -> Option<Usage> {
        self.inner
            .llm_call
            .usage
            .clone()
            .map(|u| Usage { inner: u })
    }

    #[getter]
    pub fn timing(&self) -> StreamTiming {
        StreamTiming {
            inner: self.inner.timing.clone(),
        }
    }

    pub fn sse_responses(&self) -> Option<Vec<SSEResponse>> {
        self.inner.sse_chunks.as_ref().map(|sse_chunks| {
            sse_chunks
                .event
                .iter()
                .map(|event| SSEResponse {
                    inner: event.clone(),
                })
                .collect()
        })
    }
}

pub(crate) fn serde_value_to_py(py: Python<'_>, value: &JsonValue) -> PyResult<PyObject> {
    match value {
        JsonValue::Null => Ok(py.None()),
        JsonValue::Bool(b) => b.into_py_any(py),
        JsonValue::Number(num) => {
            if let Some(i) = num.as_i64() {
                i.into_py_any(py)
            } else if let Some(f) = num.as_f64() {
                f.into_py_any(py)
            } else {
                Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(
                    "Could not convert number to i64 or f64",
                ))
            }
        }
        JsonValue::String(s) => s.into_py_any(py),
        JsonValue::Array(arr) => {
            let pylist = PyList::empty(py);
            for elem in arr {
                pylist.append(serde_value_to_py(py, elem)?)?;
            }
            Ok(pylist.into_any().unbind())
        }
        JsonValue::Object(obj) => {
            let pydict = PyDict::new(py);
            for (k, v) in obj {
                pydict.set_item(k, serde_value_to_py(py, v)?)?;
            }
            Ok(pydict.into_any().unbind())
        }
    }
}

#[pymethods]
impl Usage {
    pub fn __repr__(&self) -> String {
        format!(
            "Usage(input_tokens={}, output_tokens={}, cached_input_tokens={})",
            self.inner
                .input_tokens
                .map_or_else(|| "None".to_string(), |v| v.to_string()),
            self.inner
                .output_tokens
                .map_or_else(|| "None".to_string(), |v| v.to_string()),
            self.inner
                .cached_input_tokens
                .map_or_else(|| "None".to_string(), |v| v.to_string())
        )
    }

    #[getter]
    pub fn input_tokens(&self) -> Option<i64> {
        self.inner.input_tokens
    }

    #[getter]
    pub fn output_tokens(&self) -> Option<i64> {
        self.inner.output_tokens
    }

    #[getter]
    pub fn cached_input_tokens(&self) -> Option<i64> {
        self.inner.cached_input_tokens
    }
}

#[pymethods]
impl Timing {
    pub fn __repr__(&self) -> String {
        format!(
            "Timing(start_time_utc_ms={}, duration_ms={})",
            self.inner.start_time_utc_ms,
            self.inner
                .duration_ms
                .map_or("None".to_string(), |v| v.to_string()),
        )
    }

    #[getter]
    pub fn start_time_utc_ms(&self) -> i64 {
        self.inner.start_time_utc_ms
    }

    #[getter]
    pub fn duration_ms(&self) -> Option<i64> {
        self.inner.duration_ms
    }
}

#[pymethods]
impl StreamTiming {
    pub fn __repr__(&self) -> String {
        format!(
            "StreamTiming(start_time_utc_ms={}, duration_ms={})",
            self.inner.start_time_utc_ms,
            self.inner
                .duration_ms
                .map_or("None".to_string(), |v| v.to_string()),
        )
    }

    #[getter]
    pub fn start_time_utc_ms(&self) -> i64 {
        self.inner.start_time_utc_ms
    }

    #[getter]
    pub fn duration_ms(&self) -> Option<i64> {
        self.inner.duration_ms
    }
}
