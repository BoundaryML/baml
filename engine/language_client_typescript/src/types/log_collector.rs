use std::{
    str::FromStr,
    sync::{Arc, Mutex},
};

use baml_runtime::tracingv2::storage::storage::BAML_TRACER;
use napi::{
    bindgen_prelude::*, Env, JsBoolean, JsNull, JsNumber, JsObject, JsString, JsUndefined,
    JsUnknown, Result,
};
use napi_derive::napi;
use serde_json::Value as JsonValue;

use super::{request::HTTPRequest, response::HTTPResponse};

crate::lang_wrapper!(
    Collector,
    baml_runtime::tracingv2::storage::storage::Collector,
    clone_safe
);

#[napi]
impl Collector {
    #[napi(constructor)]
    pub fn new(name: Option<String>) -> Self {
        let collector = baml_runtime::tracingv2::storage::storage::Collector::new(name);
        Self {
            inner: Arc::new(collector),
        }
    }

    #[napi(getter)]
    pub fn logs(&self) -> Vec<FunctionLog> {
        self.inner
            .function_logs()
            .iter()
            .map(|inner_function_log| FunctionLog {
                inner: Arc::new(Mutex::new(inner_function_log.clone())),
            })
            .collect()
    }

    #[napi(getter)]
    pub fn last(&self) -> Option<FunctionLog> {
        self.inner
            .last_function_log()
            .map(|inner_function_log| FunctionLog {
                inner: Arc::new(Mutex::new(inner_function_log.clone())),
            })
    }

    #[napi]
    pub fn id(&self, function_log_id: String) -> Option<FunctionLog> {
        self.inner
            .function_log_by_id(&baml_ids::FunctionCallId::from_str(&function_log_id).ok()?)
            .map(|inner_function_log| FunctionLog {
                inner: Arc::new(Mutex::new(inner_function_log.clone())),
            })
    }

    #[napi(getter)]
    pub fn usage(&self) -> Usage {
        Usage {
            inner: self.inner.usage(),
        }
    }

    #[napi]
    pub fn to_string(&self) -> String {
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

    #[napi(js_name = "__functionSpanCount")]
    pub fn function_call_count() -> u32 {
        let span_count = BAML_TRACER.lock().unwrap().function_call_count();
        span_count as u32
    }

    #[napi(js_name = "__printStorage")]
    pub fn print_storage() {
        let tracer = BAML_TRACER.lock().unwrap();
        println!("Storage: {tracer:#?}");
    }
}

crate::lang_wrapper!(
    FunctionLog,
    baml_runtime::tracingv2::storage::storage::FunctionLog,
    sync_thread_safe
);

#[napi]
impl FunctionLog {
    #[napi]
    pub fn to_string(&self) -> String {
        let mut inner = self.inner.lock().unwrap();
        let calls_str = inner
            .calls()
            .into_iter()
            .map(|call| match call {
                baml_runtime::tracingv2::storage::storage::LLMCallKind::Basic(inner) => {
                    let llm_call = LLMCall {
                        inner: inner.clone(),
                    };
                    llm_call.to_string()
                }
                baml_runtime::tracingv2::storage::storage::LLMCallKind::Stream(inner) => {
                    let stream_call = LLMStreamCall {
                        inner: inner.clone(),
                    };
                    stream_call.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join(", ");

        format!(
            "FunctionLog(id={}, function_name={}, type={}, timing={}, usage={}, calls=[{}], raw_llm_response={})",
            inner.id(),
            inner.function_name(),
            inner.log_type(),
            Timing { inner: inner.timing() }.to_string(),
            Usage { inner: inner.usage() }.to_string(),
            calls_str,
            inner.raw_llm_response().unwrap_or("null".to_string())
        )
    }

    #[napi(getter)]
    pub fn id(&self) -> String {
        self.inner.lock().unwrap().id().to_string()
    }

    #[napi(getter)]
    pub fn function_name(&self) -> String {
        self.inner.lock().unwrap().function_name()
    }

    #[napi(getter)]
    pub fn log_type(&self) -> String {
        self.inner.lock().unwrap().log_type().to_string()
    }

    #[napi(getter)]
    pub fn timing(&self) -> Timing {
        Timing {
            inner: self.inner.lock().unwrap().timing(),
        }
    }

    #[napi(getter)]
    pub fn usage(&self) -> Usage {
        Usage {
            inner: self.inner.lock().unwrap().usage(),
        }
    }

    #[napi(getter, ts_return_type = "(LLMCall | LLMStreamCall)[]")]
    pub fn calls(&self, env: Env) -> Result<Array> {
        let calls = self.inner.lock().unwrap().calls();
        let mut js_array = env.create_array(calls.len() as u32)?;

        for (i, call) in calls.into_iter().enumerate() {
            match call {
                baml_runtime::tracingv2::storage::storage::LLMCallKind::Basic(inner) => {
                    let llm_call = LLMCall {
                        inner: inner.clone(),
                    };
                    let js_value = llm_call.into_instance(env)?;
                    js_array.set(i as u32, js_value)?;
                }
                baml_runtime::tracingv2::storage::storage::LLMCallKind::Stream(inner) => {
                    let stream_call = LLMStreamCall {
                        inner: inner.clone(),
                    };
                    let js_value = LLMStreamCall::into_instance(stream_call, env)?;
                    js_array.set(i as u32, js_value)?;
                }
            };
        }

        Ok(js_array)
    }

    #[napi(getter)]
    pub fn raw_llm_response(&self) -> Option<String> {
        let mut guarded = self.inner.lock().unwrap();
        guarded.raw_llm_response()
    }

    #[napi(getter)]
    pub fn selected_call(&self, env: Env) -> Result<JsUnknown> {
        let calls = self.inner.lock().unwrap().calls();
        let found = calls.into_iter().find_map(|call| match call {
            baml_runtime::tracingv2::storage::storage::LLMCallKind::Basic(inner) => {
                if inner.selected {
                    Some(
                        baml_runtime::tracingv2::storage::storage::LLMCallKind::Basic(
                            inner.clone(),
                        ),
                    )
                } else {
                    None
                }
            }
            baml_runtime::tracingv2::storage::storage::LLMCallKind::Stream(inner) => {
                if inner.selected {
                    Some(
                        baml_runtime::tracingv2::storage::storage::LLMCallKind::Stream(
                            inner.clone(),
                        ),
                    )
                } else {
                    None
                }
            }
        });

        match found {
            Some(call) => match call {
                baml_runtime::tracingv2::storage::storage::LLMCallKind::Basic(inner) => {
                    let llm_call = LLMCall { inner };
                    Ok(env.create_external(llm_call, None)?.into_unknown())
                }
                baml_runtime::tracingv2::storage::storage::LLMCallKind::Stream(inner) => {
                    let stream_call = LLMStreamCall { inner };
                    Ok(env.create_external(stream_call, None)?.into_unknown())
                }
            },
            None => Ok(env.get_null()?.into_unknown()),
        }
    }
}

crate::lang_wrapper!(Timing, baml_runtime::tracingv2::storage::storage::Timing);

#[napi]
impl Timing {
    #[napi]
    pub fn to_string(&self) -> String {
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

    #[napi(getter)]
    pub fn start_time_utc_ms(&self) -> i64 {
        self.inner.start_time_utc_ms
    }

    #[napi(getter)]
    pub fn duration_ms(&self) -> Option<i64> {
        self.inner.duration_ms
    }

    #[napi(getter)]
    pub fn time_to_first_parsed_ms(&self) -> Option<i64> {
        self.inner.time_to_first_parsed_ms
    }
}

crate::lang_wrapper!(
    StreamTiming,
    baml_runtime::tracingv2::storage::storage::StreamTiming
);

#[napi]
impl StreamTiming {
    #[napi]
    pub fn to_string(&self) -> String {
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

    #[napi(getter)]
    pub fn start_time_utc_ms(&self) -> i64 {
        self.inner.start_time_utc_ms
    }

    #[napi(getter)]
    pub fn duration_ms(&self) -> Option<i64> {
        self.inner.duration_ms
    }

    #[napi(getter)]
    pub fn time_to_first_parsed_ms(&self) -> Option<i64> {
        self.inner.time_to_first_parsed_ms
    }

    #[napi(getter)]
    pub fn time_to_first_token_ms(&self) -> Option<i64> {
        self.inner.time_to_first_token_ms
    }
}

crate::lang_wrapper!(Usage, baml_runtime::tracingv2::storage::storage::Usage);

#[napi]
impl Usage {
    #[napi]
    pub fn to_string(&self) -> String {
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

    #[napi(getter)]
    pub fn input_tokens(&self) -> Option<i64> {
        self.inner.input_tokens
    }

    #[napi(getter)]
    pub fn output_tokens(&self) -> Option<i64> {
        self.inner.output_tokens
    }
}

crate::lang_wrapper!(LLMCall, baml_runtime::tracingv2::storage::storage::LLMCall);

#[napi]
impl LLMCall {
    #[napi(getter)]
    pub fn selected(&self) -> bool {
        self.inner.selected
    }

    #[napi(getter)]
    pub fn http_request(&self) -> Option<HTTPRequest> {
        self.inner
            .request
            .clone()
            .map(|req| HTTPRequest { inner: req })
    }

    #[napi(getter)]
    pub fn http_response(&self) -> Option<HTTPResponse> {
        self.inner
            .response
            .clone()
            .map(|resp| HTTPResponse { inner: resp })
    }

    #[napi(getter)]
    pub fn usage(&self) -> Option<Usage> {
        self.inner.usage.clone().map(|u| Usage { inner: u })
    }

    #[napi(getter)]
    pub fn timing(&self) -> Timing {
        Timing {
            inner: self.inner.timing.clone(),
        }
    }

    #[napi(getter)]
    pub fn provider(&self) -> String {
        self.inner.provider.clone()
    }

    #[napi(getter)]
    pub fn client_name(&self) -> String {
        self.inner.client_name.clone()
    }

    #[napi]
    pub fn to_string(&self) -> String {
        format!(
            "LLMCall(provider={}, client_name={}, selected={}, usage={}, timing={}, http_request={}, http_response={})",
            self.provider(),
            self.client_name(),
            self.selected(),
            self.usage().map_or("null".to_string(), |u| u.to_string()),
            self.timing().to_string(),
            self.http_request().map_or("null".to_string(), |req| req.to_string()),
            self.http_response().map_or("null".to_string(), |resp| resp.to_string())
        )
    }

    #[napi(js_name = "toString")]
    pub fn js_to_string(&self) -> String {
        self.to_string()
    }
}

crate::lang_wrapper!(
    LLMStreamCall,
    baml_runtime::tracingv2::storage::storage::LLMStreamCall
);

#[napi]
impl LLMStreamCall {
    #[napi]
    pub fn to_string(&self) -> String {
        format!(
            "LLMStreamCall(provider={}, client_name={}, selected={}, usage={}, timing={}, http_request={}, http_response={})",
            self.provider(),
            self.client_name(),
            self.selected(),
            self.usage().map_or("null".to_string(), |u| u.to_string()),
            self.timing().to_string(),
            self.http_request().map_or("null".to_string(), |req| req.to_string()),
            self.http_response().map_or("null".to_string(), |resp| resp.to_string())
        )
    }

    #[napi(getter)]
    pub fn http_request(&self) -> Option<HTTPRequest> {
        self.inner
            .request
            .clone()
            .map(|req| HTTPRequest { inner: req })
    }

    #[napi(getter)]
    pub fn http_response(&self) -> Option<HTTPResponse> {
        self.inner
            .response
            .clone()
            .map(|resp| HTTPResponse { inner: resp })
    }

    #[napi(getter)]
    pub fn provider(&self) -> String {
        self.inner.provider.clone()
    }

    #[napi(getter)]
    pub fn client_name(&self) -> String {
        self.inner.client_name.clone()
    }

    #[napi(getter)]
    pub fn selected(&self) -> bool {
        self.inner.selected
    }

    #[napi(getter)]
    pub fn usage(&self) -> Option<Usage> {
        self.inner.usage.clone().map(|u| Usage { inner: u })
    }

    #[napi(getter)]
    pub fn timing(&self) -> StreamTiming {
        StreamTiming {
            inner: self.inner.timing.clone(),
        }
    }

    #[napi(js_name = "toString")]
    pub fn js_to_string(&self) -> String {
        self.to_string()
    }
}

pub fn serde_value_to_js(env: Env, value: &JsonValue) -> Result<JsUnknown> {
    match value {
        JsonValue::Null => Ok(env.get_null()?.into_unknown()),
        JsonValue::Bool(b) => Ok(env.get_boolean(*b)?.into_unknown()),
        JsonValue::Number(num) => {
            if let Some(i) = num.as_i64() {
                Ok(env.create_int64(i)?.into_unknown())
            } else if let Some(f) = num.as_f64() {
                Ok(env.create_double(f)?.into_unknown())
            } else {
                Err(Error::from_reason("Could not convert number to i64 or f64"))
            }
        }
        JsonValue::String(s) => Ok(env.create_string(s)?.into_unknown()),
        JsonValue::Array(arr) => {
            let mut js_array = env.create_array_with_length(arr.len())?;
            for (i, elem) in arr.iter().enumerate() {
                let js_value = serde_value_to_js(env, elem)?;
                js_array.set_element(i as u32, js_value)?;
            }
            Ok(js_array.into_unknown())
        }
        JsonValue::Object(obj) => {
            let mut js_obj = env.create_object()?;
            for (k, v) in obj {
                let js_value = serde_value_to_js(env, v)?;
                js_obj.set_named_property(k, js_value)?;
            }
            Ok(js_obj.into_unknown())
        }
    }
}
