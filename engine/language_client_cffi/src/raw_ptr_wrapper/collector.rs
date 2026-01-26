use std::str::FromStr;

use baml_cffi_macros::export_baml_fn;
use baml_runtime::tracingv2::storage::storage::{
    FunctionLog, LLMCall, LLMStreamCall, StreamTiming, Timing, Usage,
};
use baml_types::{
    tracing::events::{HTTPBody, HTTPRequest, HTTPResponse, SSEEvent},
    BamlValue,
};
use tokio_util::either;

use super::{BamlObjectResponse, BamlObjectResponseSuccess, CallMethod};
use crate::raw_ptr_wrapper::{
    CollectorWrapper, FunctionLogWrapper, HTTPBodyWrapper, HTTPRequestWrapper, HTTPResponseWrapper,
    LLMCallWrapper, LLMStreamCallWrapper, SSEEventWrapper, StreamTimingWrapper, TimingWrapper,
    UsageWrapper,
};

#[export_baml_fn]
impl CollectorWrapper {
    #[export_baml_fn]
    fn usage(&self) -> Usage {
        self.inner.usage()
    }

    #[export_baml_fn]
    fn name(&self) -> BamlValue {
        BamlValue::String(self.inner.name())
    }

    #[export_baml_fn]
    fn logs(&self) -> Vec<FunctionLog> {
        self.inner.function_logs()
    }

    #[export_baml_fn]
    fn last(&self) -> Option<FunctionLog> {
        self.inner.last_function_log()
    }

    #[export_baml_fn]
    fn clear(&self) -> BamlValue {
        BamlValue::Int(self.inner.clear() as i64)
    }

    #[export_baml_fn]
    fn id(&self, function_id: &str) -> Result<FunctionLog, String> {
        // Parse the function_id string to FunctionCallId
        let function_id = match baml_ids::FunctionCallId::from_str(function_id) {
            Ok(id) => id,
            Err(e) => return Err(format!("Invalid id: {e}")),
        };

        let function_log = self
            .inner
            .function_log_by_id(&function_id)
            .ok_or_else(|| format!("ID not found: {function_id}"))?;
        Ok(function_log)
    }
}

#[export_baml_fn]
impl UsageWrapper {
    #[export_baml_fn]
    fn input_tokens(&self) -> i64 {
        self.input_tokens.unwrap_or_default()
    }

    #[export_baml_fn]
    fn output_tokens(&self) -> i64 {
        self.output_tokens.unwrap_or_default()
    }

    #[export_baml_fn]
    fn cached_input_tokens(&self) -> Option<i64> {
        self.cached_input_tokens
    }
}

#[export_baml_fn]
impl FunctionLogWrapper {
    #[export_baml_fn]
    fn id(&self) -> String {
        self.inner.id().to_string()
    }

    #[export_baml_fn]
    fn function_name(&self) -> String {
        let mut log_clone = (self.as_ref()).clone();
        log_clone.function_name()
    }

    #[export_baml_fn]
    fn log_type(&self) -> String {
        let mut log_clone = (self.as_ref()).clone();
        log_clone.log_type()
    }

    #[export_baml_fn]
    fn timing(&self) -> Timing {
        let mut log_clone = (self.as_ref()).clone();
        log_clone.timing()
    }

    #[export_baml_fn]
    fn usage(&self) -> Usage {
        let mut log_clone = (self.as_ref()).clone();
        log_clone.usage()
    }

    #[export_baml_fn]
    fn raw_llm_response(&self) -> Option<String> {
        let mut log_clone = (self.as_ref()).clone();
        log_clone.raw_llm_response()
    }

    #[export_baml_fn]
    fn calls(&self) -> Vec<either::Either<LLMCall, LLMStreamCall>> {
        let mut log_clone = (self.as_ref()).clone();
        let calls = log_clone.calls();
        calls
            .into_iter()
            .map(|call| match call {
                baml_runtime::tracingv2::storage::storage::LLMCallKind::Basic(call) => {
                    either::Either::Left(call)
                }
                baml_runtime::tracingv2::storage::storage::LLMCallKind::Stream(llmstream_call) => {
                    either::Either::Right(llmstream_call)
                }
            })
            .collect()
    }

    #[export_baml_fn]
    fn metadata(&self) -> BamlValue {
        let mut log_clone = (self.as_ref()).clone();
        let metadata = log_clone.metadata();
        BamlValue::Map(
            metadata
                .iter()
                .map(|(k, v)| {
                    (
                        k.clone(),
                        baml_types::BamlValue::try_from(v.clone()).unwrap(),
                    )
                })
                .collect(),
        )
    }

    #[export_baml_fn]
    fn tags(&self) -> BamlValue {
        let mut log_clone = (self.as_ref()).clone();
        let metadata = log_clone.tags();
        BamlValue::Map(
            metadata
                .iter()
                .map(|(k, v)| {
                    (
                        k.clone(),
                        baml_types::BamlValue::try_from(v.clone()).unwrap(),
                    )
                })
                .collect(),
        )
    }

    #[export_baml_fn]
    fn selected_call(&self) -> Option<either::Either<LLMCall, LLMStreamCall>> {
        let mut log_clone = (self.as_ref()).clone();
        let calls = log_clone.calls();

        // Find the selected call (where selected = true)
        for call in calls {
            match call {
                baml_runtime::tracingv2::storage::storage::LLMCallKind::Basic(c) => {
                    if c.selected {
                        return Some(either::Either::Left(c));
                    }
                }
                baml_runtime::tracingv2::storage::storage::LLMCallKind::Stream(c) => {
                    if c.llm_call.selected {
                        return Some(either::Either::Right(c));
                    }
                }
            };
        }

        // No selected call found
        None
    }
}

#[export_baml_fn]
impl TimingWrapper {
    #[export_baml_fn]
    fn start_time_utc_ms(&self) -> i64 {
        self.start_time_utc_ms
    }

    #[export_baml_fn]
    fn duration_ms(&self) -> Option<i64> {
        self.duration_ms
    }
}

#[export_baml_fn]
impl StreamTimingWrapper {
    #[export_baml_fn]
    fn start_time_utc_ms(&self) -> BamlValue {
        BamlValue::Int(self.start_time_utc_ms)
    }

    #[export_baml_fn]
    fn duration_ms(&self) -> BamlValue {
        BamlValue::Int(self.duration_ms.unwrap_or_default())
    }
}

#[export_baml_fn]
impl LLMCallWrapper {
    #[export_baml_fn]
    fn client_name(&self) -> String {
        self.inner.client_name.clone()
    }

    #[export_baml_fn]
    fn provider(&self) -> String {
        self.inner.provider.clone()
    }

    #[export_baml_fn]
    fn selected(&self) -> bool {
        self.inner.selected
    }

    #[export_baml_fn]
    fn timing(&self) -> Timing {
        self.inner.timing.clone()
    }

    #[export_baml_fn]
    fn usage(&self) -> Option<Usage> {
        self.inner.usage.clone()
    }

    #[export_baml_fn]
    fn http_request_id(&self) -> Option<String> {
        self.inner.request.as_ref().map(|req| req.id().to_string())
    }

    #[export_baml_fn]
    fn http_request(&self) -> Option<std::sync::Arc<HTTPRequest>> {
        self.inner.request.clone()
    }

    #[export_baml_fn]
    fn http_response(&self) -> Option<std::sync::Arc<HTTPResponse>> {
        self.inner.response.clone()
    }
}

#[export_baml_fn]
impl LLMStreamCallWrapper {
    #[export_baml_fn]
    fn client_name(&self) -> String {
        self.inner.llm_call.client_name.clone()
    }

    #[export_baml_fn]
    fn provider(&self) -> String {
        self.inner.llm_call.provider.clone()
    }

    #[export_baml_fn]
    fn selected(&self) -> bool {
        self.inner.llm_call.selected
    }

    #[export_baml_fn]
    fn timing(&self) -> StreamTiming {
        self.inner.timing.clone()
    }

    #[export_baml_fn]
    fn usage(&self) -> Option<Usage> {
        self.inner.llm_call.usage.clone()
    }

    #[export_baml_fn]
    fn http_request(&self) -> Option<std::sync::Arc<HTTPRequest>> {
        self.inner.llm_call.request.clone()
    }

    #[export_baml_fn]
    fn http_response(&self) -> Option<std::sync::Arc<HTTPResponse>> {
        self.inner.llm_call.response.clone()
    }

    #[export_baml_fn]
    fn http_request_id(&self) -> Option<String> {
        self.inner
            .llm_call
            .request
            .as_ref()
            .map(|req| req.id().to_string())
    }

    #[export_baml_fn]
    fn sse_chunks(&self) -> Option<Vec<std::sync::Arc<SSEEvent>>> {
        self.inner
            .sse_chunks
            .as_ref()
            .map(|chunks| chunks.event.to_vec())
    }
}

#[export_baml_fn]
impl HTTPRequestWrapper {
    #[export_baml_fn]
    fn id(&self) -> String {
        self.inner.id().to_string()
    }

    #[export_baml_fn]
    fn url(&self) -> String {
        self.inner.url().to_string()
    }

    #[export_baml_fn]
    fn method(&self) -> String {
        self.inner.method().to_string()
    }

    #[export_baml_fn]
    fn headers(&self) -> BamlValue {
        let headers = self.inner.headers();
        BamlValue::Map(
            headers
                .iter()
                .map(|(k, v)| (k.clone(), BamlValue::String(v.clone())))
                .collect(),
        )
    }

    #[export_baml_fn]
    fn body(&self) -> HTTPBody {
        self.inner.body().clone()
    }
}

#[export_baml_fn]
impl HTTPResponseWrapper {
    #[export_baml_fn]
    fn id(&self) -> String {
        self.inner.request_id.to_string()
    }

    #[export_baml_fn]
    fn status(&self) -> i64 {
        self.inner.status as i64
    }

    #[export_baml_fn]
    fn headers(&self) -> BamlValue {
        match self.inner.headers() {
            Some(headers) => BamlValue::Map(
                headers
                    .iter()
                    .map(|(k, v)| (k.clone(), BamlValue::String(v.clone())))
                    .collect(),
            ),
            None => BamlValue::Null,
        }
    }

    #[export_baml_fn]
    fn body(&self) -> std::sync::Arc<HTTPBody> {
        self.inner.body.clone()
    }
}

#[export_baml_fn]
impl HTTPBodyWrapper {
    #[export_baml_fn]
    fn text(&self) -> Result<BamlValue, String> {
        match self.inner.text() {
            Ok(text) => Ok(BamlValue::String(text.to_string())),
            Err(e) => Err(e.to_string()),
        }
    }

    #[export_baml_fn]
    fn json(&self) -> BamlValue {
        match self.inner.json() {
            Ok(json_value) => BamlValue::try_from(json_value).unwrap(),
            Err(_) => BamlValue::Null,
        }
    }
}

#[export_baml_fn]
impl SSEEventWrapper {
    #[export_baml_fn]
    fn text(&self) -> BamlValue {
        BamlValue::String(self.inner.data.clone())
    }

    #[export_baml_fn]
    fn json(&self) -> BamlValue {
        match serde_json::from_str::<BamlValue>(&self.inner.data) {
            Ok(json_value) => json_value,
            Err(_) => BamlValue::Null,
        }
    }
}
