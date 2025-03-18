use std::sync::Arc;
use std::sync::Mutex;

use crate::Result;
use baml_runtime::tracingv2::storage::storage::BAML_TRACER;
use magnus::scan_args::get_kwargs;
use magnus::{
    class, function, method, scan_args::scan_args, try_convert::TryConvertOwned, Error,
    IntoValueFromNative, Module, Object, RArray, RModule, Ruby, Value,
};

use super::{
    request::{HTTPBody, HTTPRequest},
    response::HTTPResponse,
};

crate::lang_wrapper!(
    Collector,
    "Baml::Ffi::Collector",
    baml_runtime::tracingv2::storage::storage::Collector,
    clone_safe
);

unsafe impl TryConvertOwned for &Collector {}

crate::lang_wrapper!(
    Usage,
    "Baml::Ffi::Usage",
    baml_runtime::tracingv2::storage::storage::Usage,
    clone_safe
);

crate::lang_wrapper!(
    Timing,
    "Baml::Ffi::Timing",
    baml_runtime::tracingv2::storage::storage::Timing,
    clone_safe
);

crate::lang_wrapper!(
    StreamTiming,
    "Baml::Ffi::StreamTiming",
    baml_runtime::tracingv2::storage::storage::StreamTiming,
    clone_safe
);

crate::lang_wrapper!(
    FunctionLog,
    "Baml::Ffi::FunctionLog",
    baml_runtime::tracingv2::storage::storage::FunctionLog,
    sync_thread_safe
);

crate::lang_wrapper!(
    LLMCall,
    "Baml::Ffi::LLMCall",
    baml_runtime::tracingv2::storage::storage::LLMCall,
    clone_safe
);

unsafe impl IntoValueFromNative for LLMCall {}

unsafe impl IntoValueFromNative for LLMStreamCall {}
crate::lang_wrapper!(
    LLMStreamCall,
    "Baml::Ffi::LLMStreamCall",
    baml_runtime::tracingv2::storage::storage::LLMStreamCall,
    clone_safe
);

unsafe impl TryConvertOwned for &FunctionLog {}

impl Collector {
    pub fn new(args: &[Value]) -> Result<Self> {
        let args = scan_args::<(), (), (), (), _, ()>(args)?;
        let kwargs = get_kwargs::<_, (), (Option<String>,), ()>(args.keywords, &[], &["name"])?;

        let name = kwargs.optional.0;
        let collector = baml_runtime::tracingv2::storage::storage::Collector::new(name);
        Ok(Self {
            inner: Arc::new(collector),
        })
    }

    pub fn logs(&self) -> RArray {
        let function_logs = self.inner.function_logs();
        function_logs
            .iter()
            .map(|inner_function_log| FunctionLog {
                inner: Arc::new(Mutex::new(inner_function_log.clone())),
            })
            .collect()
    }

    pub fn last(&self) -> Option<FunctionLog> {
        self.inner
            .last_function_log()
            .map(|inner_function_log| FunctionLog {
                inner: Arc::new(Mutex::new(inner_function_log.clone())),
            })
    }

    pub fn id(&self, function_log_id: String) -> Option<FunctionLog> {
        self.inner
            .function_log_by_id(&baml_types::tracing::events::FunctionId(function_log_id))
            .map(|inner_function_log| FunctionLog {
                inner: Arc::new(Mutex::new(inner_function_log.clone())),
            })
    }

    pub fn usage(&self) -> Usage {
        Usage {
            inner: self.inner.usage().into(),
        }
    }

    pub fn to_s(&self) -> String {
        let logs = self.inner.function_logs();
        let log_ids: Vec<String> = logs.iter().map(|log| log.id().0.clone()).collect();
        format!(
            "LogCollector(name={}, function_log_ids=[{}])",
            self.inner.name(),
            log_ids.join(", ")
        )
    }

    pub fn __function_span_count() -> u32 {
        let span_count = BAML_TRACER.lock().unwrap().function_span_count();
        span_count as u32
    }

    pub fn __print_storage() {
        let tracer = BAML_TRACER.lock().unwrap();
        println!("Storage: {:#?}", tracer);
    }

    pub fn define_in_ruby(module: &RModule) -> Result<()> {
        let cls = module.define_class("Collector", class::object())?;

        cls.define_singleton_method("new", function!(Collector::new, -1))?;
        cls.define_method("logs", method!(Collector::logs, 0))?;
        cls.define_method("last", method!(Collector::last, 0))?;
        cls.define_method("id", method!(Collector::id, 1))?;
        cls.define_method("usage", method!(Collector::usage, 0))?;
        cls.define_method("to_s", method!(Collector::to_s, 0))?;
        cls.define_singleton_method(
            "__function_span_count",
            function!(Collector::__function_span_count, 0),
        )?;
        cls.define_singleton_method("__print_storage", function!(Collector::__print_storage, 0))?;

        Ok(())
    }
}

impl FunctionLog {
    pub fn to_s(&self) -> String {
        // Acquire the lock once and extract all needed data
        let mut guard = self.inner.lock().unwrap();
        let id = guard.id().0.clone();
        let function_name = guard.function_name();
        let log_type = guard.log_type().to_string();
        let timing_data = guard.timing().clone();
        let usage_data = guard.usage().clone();
        let calls_data = guard.calls();
        let raw_llm_response = guard.raw_llm_response().unwrap_or("null".to_string());
        // Release the lock by dropping the guard
        drop(guard);

        // Now process calls without holding the lock
        let calls_str = calls_data
            .into_iter()
            .map(|call| match call {
                baml_runtime::tracingv2::storage::storage::LLMCallKind::Basic(inner) => {
                    let llm_call = LLMCall {
                        inner: Arc::new(inner.clone()),
                    };
                    llm_call.to_s()
                }
                baml_runtime::tracingv2::storage::storage::LLMCallKind::Stream(inner) => {
                    let stream_call = LLMStreamCall {
                        inner: Arc::new(inner.clone()),
                    };
                    stream_call.to_s()
                }
            })
            .collect::<Vec<_>>()
            .join(", ");

        // Create timing and usage objects from cloned data
        let timing = Timing {
            inner: timing_data.into(),
        };
        let usage = Usage {
            inner: usage_data.into(),
        };

        // Format the string with all the extracted data
        format!(
            "FunctionLog(id={}, function_name={}, type={}, timing={}, usage={}, calls=[{}], raw_llm_response={})",
            id,
            function_name,
            log_type,
            timing.to_s(),
            usage.to_s(),
            calls_str,
            raw_llm_response
        )
    }

    pub fn id(&self) -> String {
        self.inner.lock().unwrap().id().0.clone()
    }

    pub fn function_name(&self) -> String {
        self.inner.lock().unwrap().function_name()
    }

    pub fn log_type(&self) -> String {
        self.inner.lock().unwrap().log_type().to_string()
    }

    pub fn timing(&self) -> Timing {
        Timing {
            inner: self.inner.lock().unwrap().timing().into(),
        }
    }

    pub fn usage(&self) -> Usage {
        Usage {
            inner: self.inner.lock().unwrap().usage().into(),
        }
    }

    pub fn calls(&self) -> RArray {
        let calls = self.inner.lock().unwrap().calls();
        let array = RArray::new();

        for call in calls {
            match call {
                baml_runtime::tracingv2::storage::storage::LLMCallKind::Basic(inner) => {
                    let llm_call = LLMCall {
                        inner: Arc::new(inner.clone()),
                    };
                    array.push(llm_call).unwrap();
                }
                baml_runtime::tracingv2::storage::storage::LLMCallKind::Stream(inner) => {
                    let stream_call = LLMStreamCall {
                        inner: Arc::new(inner.clone()),
                    };
                    array.push(stream_call).unwrap();
                }
            }
        }

        array
    }

    pub fn raw_llm_response(&self) -> Option<String> {
        self.inner.lock().unwrap().raw_llm_response()
    }

    pub fn selected_call(ruby: &Ruby, rb_self: &Self) -> Option<Value> {
        let calls = rb_self.inner.lock().unwrap().calls();
        calls.into_iter().find_map(|call| match call {
            baml_runtime::tracingv2::storage::storage::LLMCallKind::Basic(inner) => {
                if inner.selected {
                    Some(
                        LLMCall {
                            inner: Arc::new(inner.clone()),
                        }
                        .to_value(ruby)
                        .unwrap(),
                    )
                } else {
                    None
                }
            }
            baml_runtime::tracingv2::storage::storage::LLMCallKind::Stream(inner) => {
                if inner.selected {
                    let stream_call = LLMStreamCall {
                        inner: Arc::new(inner.clone()),
                    };
                    Some(stream_call.to_value(ruby).unwrap())
                } else {
                    None
                }
            }
        })
    }

    pub fn define_in_ruby(module: &RModule) -> Result<()> {
        let cls = module.define_class("FunctionLog", class::object())?;

        cls.define_method("to_s", method!(FunctionLog::to_s, 0))?;
        cls.define_method("id", method!(FunctionLog::id, 0))?;
        cls.define_method("function_name", method!(FunctionLog::function_name, 0))?;
        cls.define_method("log_type", method!(FunctionLog::log_type, 0))?;
        cls.define_method("timing", method!(FunctionLog::timing, 0))?;
        cls.define_method("usage", method!(FunctionLog::usage, 0))?;
        cls.define_method("calls", method!(FunctionLog::calls, 0))?;
        cls.define_method(
            "raw_llm_response",
            method!(FunctionLog::raw_llm_response, 0),
        )?;
        cls.define_method("selected_call", method!(FunctionLog::selected_call, 0))?;

        Ok(())
    }
}

impl Timing {
    pub fn to_s(&self) -> String {
        format!(
            "Timing(start_time_utc_ms={}, duration_ms={}, time_to_first_parsed_ms={})",
            self.inner.start_time_utc_ms,
            self.inner
                .duration_ms
                .map_or("null".to_string(), |v| v.to_string()),
            self.inner
                .time_to_first_parsed_ms
                .map_or("null".to_string(), |v| v.to_string())
        )
    }

    pub fn start_time_utc_ms(&self) -> i64 {
        self.inner.start_time_utc_ms
    }

    pub fn duration_ms(&self) -> Option<i64> {
        self.inner.duration_ms
    }

    pub fn time_to_first_parsed_ms(&self) -> Option<i64> {
        self.inner.time_to_first_parsed_ms
    }

    pub fn define_in_ruby(module: &RModule) -> Result<()> {
        let cls = module.define_class("Timing", class::object())?;

        cls.define_method("to_s", method!(Timing::to_s, 0))?;
        cls.define_method("start_time_utc_ms", method!(Timing::start_time_utc_ms, 0))?;
        cls.define_method("duration_ms", method!(Timing::duration_ms, 0))?;
        cls.define_method(
            "time_to_first_parsed_ms",
            method!(Timing::time_to_first_parsed_ms, 0),
        )?;

        Ok(())
    }

    pub fn into_value(self, ruby: &Ruby) -> crate::Result<Value> {
        serde_magnus::serialize(&self.inner)
            .map_err(|e| Error::new(ruby.exception_runtime_error(), format!("{:?}", e)))
    }
}

impl StreamTiming {
    pub fn to_s(&self) -> String {
        format!(
            "StreamTiming(start_time_utc_ms={}, duration_ms={}, time_to_first_parsed_ms={}, time_to_first_token_ms={})",
            self.inner.start_time_utc_ms,
            self.inner
                .duration_ms
                .map_or("null".to_string(), |v| v.to_string()),
            self.inner
                .time_to_first_parsed_ms
                .map_or("null".to_string(), |v| v.to_string()),
            self.inner
                .time_to_first_token_ms
                .map_or("null".to_string(), |v| v.to_string())
        )
    }

    pub fn start_time_utc_ms(&self) -> i64 {
        self.inner.start_time_utc_ms
    }

    pub fn duration_ms(&self) -> Option<i64> {
        self.inner.duration_ms
    }

    pub fn time_to_first_parsed_ms(&self) -> Option<i64> {
        self.inner.time_to_first_parsed_ms
    }

    pub fn time_to_first_token_ms(&self) -> Option<i64> {
        self.inner.time_to_first_token_ms
    }

    pub fn define_in_ruby(module: &RModule) -> Result<()> {
        let cls = module.define_class("StreamTiming", class::object())?;

        cls.define_method("to_s", method!(StreamTiming::to_s, 0))?;
        cls.define_method(
            "start_time_utc_ms",
            method!(StreamTiming::start_time_utc_ms, 0),
        )?;
        cls.define_method("duration_ms", method!(StreamTiming::duration_ms, 0))?;
        cls.define_method(
            "time_to_first_parsed_ms",
            method!(StreamTiming::time_to_first_parsed_ms, 0),
        )?;
        cls.define_method(
            "time_to_first_token_ms",
            method!(StreamTiming::time_to_first_token_ms, 0),
        )?;

        Ok(())
    }
}

impl Usage {
    pub fn to_s(&self) -> String {
        format!(
            "Usage(input_tokens={}, output_tokens={})",
            self.inner
                .input_tokens
                .map_or_else(|| "null".to_string(), |v| v.to_string()),
            self.inner
                .output_tokens
                .map_or_else(|| "null".to_string(), |v| v.to_string())
        )
    }

    pub fn input_tokens(&self) -> Option<i64> {
        self.inner.input_tokens
    }

    pub fn output_tokens(&self) -> Option<i64> {
        self.inner.output_tokens
    }

    pub fn define_in_ruby(module: &RModule) -> Result<()> {
        let cls = module.define_class("Usage", class::object())?;

        cls.define_method("to_s", method!(Usage::to_s, 0))?;
        cls.define_method("input_tokens", method!(Usage::input_tokens, 0))?;
        cls.define_method("output_tokens", method!(Usage::output_tokens, 0))?;

        Ok(())
    }
}

struct SerializationError {
    position: Vec<String>,
    message: String,
}

unsafe impl TryConvertOwned for &LLMCall {}

impl LLMCall {
    pub fn selected(&self) -> bool {
        self.inner.selected
    }

    pub fn http_request(&self) -> Option<HTTPRequest> {
        self.inner
            .request
            .clone()
            .map(|req| HTTPRequest { inner: req })
    }

    pub fn http_response(&self) -> Option<HTTPResponse> {
        self.inner
            .response
            .clone()
            .map(|resp| HTTPResponse { inner: resp })
    }

    pub fn usage(&self) -> Option<Usage> {
        self.inner.usage.clone().map(|u| Usage { inner: u.into() })
    }

    pub fn timing(&self) -> Timing {
        Timing {
            inner: self.inner.timing.clone().into(),
        }
    }

    pub fn provider(&self) -> String {
        self.inner.provider.clone()
    }

    pub fn client_name(&self) -> String {
        self.inner.client_name.clone()
    }

    pub fn to_s(&self) -> String {
        format!(
            "LLMCall(provider={}, client_name={}, selected={}, usage={}, timing={}, http_request={}, http_response={})",
            self.inner.provider,
            self.inner.client_name,
            self.inner.selected,
            self.inner.usage.as_ref().map_or("null".to_string(), |u| format!("{:?}", u)),
            format!("{:?}", self.inner.timing),
            self.inner.request.as_ref().map_or("null".to_string(), |req| format!("{:?}", req)),
            self.inner.response.as_ref().map_or("null".to_string(), |resp| format!("{:?}", resp))
        )
    }

    pub fn to_value(self, ruby: &Ruby) -> crate::Result<Value> {
        serde_magnus::serialize(&self.inner)
            .map_err(|e| Error::new(ruby.exception_runtime_error(), format!("{:?}", e)))
    }

    pub fn define_in_ruby(module: &RModule) -> Result<()> {
        let cls = module.define_class("LLMCall", class::object())?;

        cls.define_method("selected", method!(LLMCall::selected, 0))?;
        cls.define_method("http_request", method!(LLMCall::http_request, 0))?;
        cls.define_method("http_response", method!(LLMCall::http_response, 0))?;
        cls.define_method("usage", method!(LLMCall::usage, 0))?;
        cls.define_method("timing", method!(LLMCall::timing, 0))?;
        cls.define_method("provider", method!(LLMCall::provider, 0))?;
        cls.define_method("client_name", method!(LLMCall::client_name, 0))?;
        cls.define_method("to_s", method!(LLMCall::to_s, 0))?;

        Ok(())
    }
}

// TODO: remove?
unsafe impl TryConvertOwned for &LLMStreamCall {}

impl LLMStreamCall {
    pub fn to_s(&self) -> String {
        format!(
            "LLMStreamCall(provider={}, client_name={}, selected={}, usage={}, timing={}, http_request={}, http_response={})",
            self.inner.provider,
            self.inner.client_name,
            self.inner.selected,
            self.inner.usage.as_ref().map_or("null".to_string(), |u| format!("{:?}", u)),
            format!("{:?}", self.inner.timing),
            self.inner.request.as_ref().map_or("null".to_string(), |req| format!("{:?}", req)),
            self.inner.response.as_ref().map_or("null".to_string(), |resp| format!("{:?}", resp))
        )
    }

    pub fn http_request(&self) -> Option<HTTPRequest> {
        self.inner
            .request
            .clone()
            .map(|req| HTTPRequest { inner: req })
    }

    pub fn http_response(&self) -> Option<HTTPResponse> {
        self.inner.response.clone().map(|resp| HTTPResponse {
            inner: resp.clone(),
        })
    }

    pub fn provider(&self) -> String {
        self.inner.provider.clone()
    }

    pub fn client_name(&self) -> String {
        self.inner.client_name.clone()
    }

    pub fn selected(&self) -> bool {
        self.inner.selected
    }

    pub fn usage(&self) -> Option<Usage> {
        self.inner.usage.clone().map(|u| Usage { inner: u.into() })
    }

    pub fn timing(&self) -> StreamTiming {
        StreamTiming {
            inner: self.inner.timing.clone().into(),
        }
    }

    pub fn to_value(self, ruby: &Ruby) -> crate::Result<Value> {
        // Serialize to Ruby value - handle errors gracefully
        serde_magnus::serialize(&self.inner)
            .map_err(|e| Error::new(ruby.exception_runtime_error(), format!("{:?}", e)))
    }

    pub fn define_in_ruby(module: &RModule) -> Result<()> {
        let cls = module.define_class("LLMStreamCall", class::object())?;

        cls.define_method("to_s", method!(LLMStreamCall::to_s, 0))?;
        cls.define_method("http_request", method!(LLMStreamCall::http_request, 0))?;
        cls.define_method("http_response", method!(LLMStreamCall::http_response, 0))?;
        cls.define_method("provider", method!(LLMStreamCall::provider, 0))?;
        cls.define_method("client_name", method!(LLMStreamCall::client_name, 0))?;
        cls.define_method("selected", method!(LLMStreamCall::selected, 0))?;
        cls.define_method("usage", method!(LLMStreamCall::usage, 0))?;
        cls.define_method("timing", method!(LLMStreamCall::timing, 0))?;

        Ok(())
    }
}

pub fn define_all_in_ruby(module: &RModule) -> Result<()> {
    Collector::define_in_ruby(module)?;
    FunctionLog::define_in_ruby(module)?;
    Timing::define_in_ruby(module)?;
    StreamTiming::define_in_ruby(module)?;
    Usage::define_in_ruby(module)?;
    LLMCall::define_in_ruby(module)?;
    LLMStreamCall::define_in_ruby(module)?;
    HTTPRequest::define_in_ruby(module)?;
    HTTPResponse::define_in_ruby(module)?;
    HTTPBody::define_in_ruby(module)?;

    Ok(())
}
