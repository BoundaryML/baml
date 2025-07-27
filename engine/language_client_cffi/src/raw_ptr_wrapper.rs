use std::{
    ops::Deref,
    sync::{atomic::AtomicBool, Arc},
};

use baml_runtime::tracingv2::storage::storage::{
    Collector, FunctionLog, LLMCall, LLMStreamCall, StreamTiming, Timing, Usage,
};
use baml_types::{
    tracing::events::{HTTPBody, HTTPRequest, HTTPResponse, SSEEvent},
    BamlMedia, BamlValue,
};

use crate::{
    baml::cffi::{self, CffiPointerType},
    ctypes::object_response_encode::{BamlObjectResponse, BamlObjectResponseSuccess},
};

pub struct RawPtrWrapper<T> {
    inner: Arc<T>,
    persist: AtomicBool,
}

impl<T: Clone> Clone for RawPtrWrapper<T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            // Don't persist the clone, unless we explicitly want to
            persist: AtomicBool::new(false),
        }
    }
}

impl<T: std::fmt::Debug> std::fmt::Debug for RawPtrWrapper<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.inner)
    }
}

impl<T> RawPtrWrapper<T> {
    pub fn from_raw(object: *const libc::c_void, persist: bool) -> Self {
        Self {
            inner: unsafe { Arc::from_raw(object as *const T) },
            persist: AtomicBool::new(persist),
        }
    }

    pub fn destroy(self) {
        self.persist
            .store(false, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn from_object(object: T) -> Self {
        Self {
            inner: Arc::new(object),
            persist: AtomicBool::new(true),
        }
    }

    pub fn from_arc(object: Arc<T>) -> Self {
        Self {
            inner: object,
            persist: AtomicBool::new(true),
        }
    }

    pub fn pointer(&self) -> CffiPointerType {
        CffiPointerType {
            pointer: Arc::into_raw(self.inner.clone()) as i64,
        }
    }
}

impl<T> Deref for RawPtrWrapper<T> {
    type Target = Arc<T>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<T> Drop for RawPtrWrapper<T> {
    fn drop(&mut self) {
        if self.persist.load(std::sync::atomic::Ordering::Relaxed) {
            let _ = Arc::into_raw(self.inner.clone());
        }
    }
}

pub type CollectorWrapper = RawPtrWrapper<Collector>;
pub type UsageWrapper = RawPtrWrapper<Usage>;
pub type FunctionLogWrapper = RawPtrWrapper<FunctionLog>;
pub type TimingWrapper = RawPtrWrapper<Timing>;
pub type StreamTimingWrapper = RawPtrWrapper<StreamTiming>;
pub type LLMCallWrapper = RawPtrWrapper<LLMCall>;
pub type LLMStreamCallWrapper = RawPtrWrapper<LLMStreamCall>;
pub type HTTPRequestWrapper = RawPtrWrapper<HTTPRequest>;
pub type HTTPResponseWrapper = RawPtrWrapper<HTTPResponse>;
pub type HTTPBodyWrapper = RawPtrWrapper<HTTPBody>;
pub type SSEEventWrapper = RawPtrWrapper<SSEEvent>;
pub type MediaWrapper = RawPtrWrapper<BamlMedia>;

#[derive(Debug, Clone)]
pub enum RawPtrType {
    Collector(CollectorWrapper),
    Usage(UsageWrapper),
    FunctionLog(FunctionLogWrapper),
    Timing(TimingWrapper),
    StreamTiming(StreamTimingWrapper),
    LLMCall(LLMCallWrapper),
    LLMStreamCall(LLMStreamCallWrapper),
    HTTPRequest(HTTPRequestWrapper),
    HTTPResponse(HTTPResponseWrapper),
    HTTPBody(HTTPBodyWrapper),
    SSEEvent(SSEEventWrapper),
    Media(MediaWrapper),
}

fn create_media_object(
    media_type: baml_types::BamlMediaType,
    kwargs: &baml_types::BamlMap<String, baml_types::BamlValue>,
) -> Result<MediaWrapper, String> {
    let mime_type = kwargs
        .get("mime_type")
        .and_then(|n| n.as_str())
        .map(|n| n.to_string());
    let url = kwargs
        .get("url")
        .and_then(|n| n.as_str())
        .map(|n| n.to_string());
    let base64 = kwargs
        .get("base64")
        .and_then(|n| n.as_str())
        .map(|n| n.to_string());
    let media = match (url, base64) {
        (Some(url), None) => BamlMedia::url(media_type, url, mime_type),
        (None, Some(base64)) => BamlMedia::base64(media_type, base64, mime_type),
        (Some(_), Some(_)) => {
            return Err("Only one of url or base64 can be provided".to_string());
        }
        (None, None) => {
            return Err("Must provide either url or base64".to_string());
        }
    };

    Ok(MediaWrapper::from_object(media))
}

impl RawPtrType {
    pub fn new_from(
        object: cffi::CffiObjectType,
        kwargs: &baml_types::BamlMap<String, baml_types::BamlValue>,
    ) -> BamlObjectResponse {
        match object {
            cffi::CffiObjectType::ObjectCollector => {
                let name = kwargs
                    .get("name")
                    .and_then(|n| n.as_str())
                    .map(|n| n.to_string());
                Ok(BamlObjectResponseSuccess::new_object(
                    RawPtrType::Collector(CollectorWrapper::from_object(Collector::new(name))),
                ))
            }
            cffi::CffiObjectType::ObjectMediaImage => {
                let media = create_media_object(baml_types::BamlMediaType::Image, kwargs)?;
                Ok(BamlObjectResponseSuccess::new_object(RawPtrType::Media(
                    media,
                )))
            }
            cffi::CffiObjectType::ObjectMediaAudio => {
                let media = create_media_object(baml_types::BamlMediaType::Audio, kwargs)?;
                Ok(BamlObjectResponseSuccess::new_object(RawPtrType::Media(
                    media,
                )))
            }
            cffi::CffiObjectType::ObjectMediaPdf => {
                let media = create_media_object(baml_types::BamlMediaType::Pdf, kwargs)?;
                Ok(BamlObjectResponseSuccess::new_object(RawPtrType::Media(
                    media,
                )))
            }
            cffi::CffiObjectType::ObjectMediaVideo => {
                let media = create_media_object(baml_types::BamlMediaType::Video, kwargs)?;
                Ok(BamlObjectResponseSuccess::new_object(RawPtrType::Media(
                    media,
                )))
            }
            _ => Err(format!(
                "Cannot create object of type {}",
                object.as_str_name()
            )),
        }
    }
}

macro_rules! impl_from_for_raw_ptr_type {
    ($type:ty, $variant:ident, $wrapper:ident) => {
        impl From<$type> for RawPtrType {
            fn from(value: $type) -> Self {
                RawPtrType::$variant($wrapper::from_object(value))
            }
        }

        impl From<Arc<$type>> for RawPtrType {
            fn from(value: Arc<$type>) -> Self {
                RawPtrType::$variant($wrapper::from_arc(value))
            }
        }
    };
}

impl_from_for_raw_ptr_type!(Timing, Timing, TimingWrapper);
impl_from_for_raw_ptr_type!(StreamTiming, StreamTiming, StreamTimingWrapper);
impl_from_for_raw_ptr_type!(LLMCall, LLMCall, LLMCallWrapper);
impl_from_for_raw_ptr_type!(LLMStreamCall, LLMStreamCall, LLMStreamCallWrapper);
impl_from_for_raw_ptr_type!(HTTPRequest, HTTPRequest, HTTPRequestWrapper);
impl_from_for_raw_ptr_type!(HTTPResponse, HTTPResponse, HTTPResponseWrapper);
impl_from_for_raw_ptr_type!(HTTPBody, HTTPBody, HTTPBodyWrapper);
impl_from_for_raw_ptr_type!(SSEEvent, SSEEvent, SSEEventWrapper);
impl_from_for_raw_ptr_type!(Usage, Usage, UsageWrapper);
impl_from_for_raw_ptr_type!(FunctionLog, FunctionLog, FunctionLogWrapper);
impl_from_for_raw_ptr_type!(Collector, Collector, CollectorWrapper);

impl RawPtrType {
    pub fn name(&self) -> &str {
        match self {
            RawPtrType::Collector(_) => "Collector",
            RawPtrType::Usage(_) => "Usage",
            RawPtrType::FunctionLog(_) => "FunctionLog",
            RawPtrType::Timing(_) => "Timing",
            RawPtrType::StreamTiming(_) => "StreamTiming",
            RawPtrType::LLMCall(_) => "LLMCall",
            RawPtrType::LLMStreamCall(_) => "LLMStreamCall",
            RawPtrType::HTTPRequest(_) => "HTTPRequest",
            RawPtrType::HTTPResponse(_) => "HTTPResponse",
            RawPtrType::HTTPBody(_) => "HTTPBody",
            RawPtrType::SSEEvent(_) => "SSEEvent",
            RawPtrType::Media(m) => match m.media_type {
                baml_types::BamlMediaType::Image => "Image",
                baml_types::BamlMediaType::Audio => "Audio",
                baml_types::BamlMediaType::Pdf => "PDF",
                baml_types::BamlMediaType::Video => "Video",
            },
        }
    }
}

pub trait CallMethod {
    fn call_method(
        &self,
        method_name: &str,
        _kwargs: &baml_types::BamlMap<String, baml_types::BamlValue>,
    ) -> BamlObjectResponse;
}

impl CallMethod for RawPtrType {
    fn call_method(
        &self,
        method_name: &str,
        kwargs: &baml_types::BamlMap<String, baml_types::BamlValue>,
    ) -> BamlObjectResponse {
        match self {
            RawPtrType::Collector(collector) => collector.call_method(method_name, kwargs),
            RawPtrType::Usage(usage) => usage.call_method(method_name, kwargs),
            RawPtrType::FunctionLog(function_log) => function_log.call_method(method_name, kwargs),
            RawPtrType::Timing(timing) => timing.call_method(method_name, kwargs),
            RawPtrType::StreamTiming(stream_timing) => {
                stream_timing.call_method(method_name, kwargs)
            }
            RawPtrType::LLMCall(llm_call) => llm_call.call_method(method_name, kwargs),
            RawPtrType::LLMStreamCall(llm_stream_call) => {
                llm_stream_call.call_method(method_name, kwargs)
            }
            RawPtrType::HTTPRequest(http_request) => http_request.call_method(method_name, kwargs),
            RawPtrType::HTTPResponse(http_response) => {
                http_response.call_method(method_name, kwargs)
            }
            RawPtrType::HTTPBody(http_body) => http_body.call_method(method_name, kwargs),
            RawPtrType::SSEEvent(sse_event) => sse_event.call_method(method_name, kwargs),
            RawPtrType::Media(media) => media.call_method(method_name, kwargs),
        }
    }
}

impl CallMethod for CollectorWrapper {
    fn call_method(
        &self,
        method_name: &str,
        kwargs: &baml_types::BamlMap<String, baml_types::BamlValue>,
    ) -> BamlObjectResponse {
        match method_name {
            "~destructor" => {
                self.clone().destroy();
                Ok(BamlObjectResponseSuccess::new_value(BamlValue::Null))
            }
            "usage" => {
                let usage = self.usage();
                Ok(BamlObjectResponseSuccess::new_object(usage.into()))
            }
            "name" => {
                let name = self.name();
                Ok(BamlObjectResponseSuccess::new_value(BamlValue::String(
                    name,
                )))
            }
            "logs" => {
                let logs = self.function_logs();
                Ok(BamlObjectResponseSuccess::new_objects(
                    logs.into_iter().map(|log| log.into()).collect(),
                ))
            }
            "last" => match self.last_function_log() {
                Some(log) => Ok(BamlObjectResponseSuccess::new_object(log.into())),
                None => Ok(BamlObjectResponseSuccess::new_value(BamlValue::Null)),
            },
            "clear" => {
                let count = self.clear();
                Ok(BamlObjectResponseSuccess::new_value(BamlValue::Int(
                    count as i64,
                )))
            }
            "id" => {
                let _function_id = kwargs
                    .get("function_id")
                    .and_then(|v| v.as_str().map(|s| s.to_string()))
                    .ok_or_else(|| "id lookup requires function_id parameter".to_string())?;

                // Parse the function_id string to FunctionCallId
                // For now, we'll just return null as we need to handle FunctionCallId parsing
                Ok(BamlObjectResponseSuccess::new_value(BamlValue::Null))
            }
            _ => Err(format!(
                "Failed to call function: \"{method_name}\" on object type: Collector"
            )),
        }
    }
}

impl CallMethod for UsageWrapper {
    fn call_method(
        &self,
        method_name: &str,
        _kwargs: &baml_types::BamlMap<String, baml_types::BamlValue>,
    ) -> BamlObjectResponse {
        match method_name {
            "~destructor" => {
                self.clone().destroy();
                Ok(BamlObjectResponseSuccess::new_value(BamlValue::Null))
            }
            "input_tokens" => Ok(BamlObjectResponseSuccess::new_value(BamlValue::Int(
                self.input_tokens.unwrap_or_default(),
            ))),
            "output_tokens" => Ok(BamlObjectResponseSuccess::new_value(BamlValue::Int(
                self.output_tokens.unwrap_or_default(),
            ))),
            _ => Err(format!(
                "Failed to call function: \"{method_name}\" on object type: Usage"
            )),
        }
    }
}

impl CallMethod for FunctionLogWrapper {
    fn call_method(
        &self,
        method_name: &str,
        _kwargs: &baml_types::BamlMap<String, baml_types::BamlValue>,
    ) -> BamlObjectResponse {
        match method_name {
            "~destructor" => {
                self.clone().destroy();
                Ok(BamlObjectResponseSuccess::new_value(BamlValue::Null))
            }
            "id" => {
                let id = self.id().to_string();
                Ok(BamlObjectResponseSuccess::new_value(BamlValue::String(id)))
            }
            "function_name" => {
                // Create a mutable clone from the Arc
                let mut log_clone = (self.as_ref()).clone();
                let name = log_clone.function_name();
                Ok(BamlObjectResponseSuccess::new_value(BamlValue::String(
                    name,
                )))
            }
            "log_type" => {
                let mut log_clone = (self.as_ref()).clone();
                let log_type = log_clone.log_type();
                Ok(BamlObjectResponseSuccess::new_value(BamlValue::String(
                    log_type,
                )))
            }
            "timing" => {
                let mut log_clone = (self.as_ref()).clone();
                let timing = log_clone.timing();
                Ok(BamlObjectResponseSuccess::new_object(timing.into()))
            }
            "usage" => {
                let mut log_clone = (self.as_ref()).clone();
                let usage = log_clone.usage();
                Ok(BamlObjectResponseSuccess::new_object(usage.into()))
            }
            "raw_llm_response" => {
                let mut log_clone = (self.as_ref()).clone();
                match log_clone.raw_llm_response() {
                    Some(response) => Ok(BamlObjectResponseSuccess::new_value(BamlValue::String(
                        response,
                    ))),
                    None => Ok(BamlObjectResponseSuccess::new_value(BamlValue::Null)),
                }
            }
            "calls" => {
                let mut log_clone = (self.as_ref()).clone();
                let calls = log_clone.calls();
                Ok(BamlObjectResponseSuccess::new_objects(
                    calls
                        .into_iter()
                        .map(|call| match call {
                            baml_runtime::tracingv2::storage::storage::LLMCallKind::Basic(call) => {
                                call.into()
                            }
                            baml_runtime::tracingv2::storage::storage::LLMCallKind::Stream(
                                llmstream_call,
                            ) => llmstream_call.into(),
                        })
                        .collect(),
                ))
            }
            "metadata" => {
                let mut log_clone = (self.as_ref()).clone();
                let metadata = log_clone.metadata();
                Ok(BamlObjectResponseSuccess::new_value(BamlValue::Map(
                    metadata
                        .iter()
                        .map(|(k, v)| {
                            (
                                k.clone(),
                                baml_types::BamlValue::try_from(v.clone()).unwrap(),
                            )
                        })
                        .collect(),
                )))
            }
            "selected_call" => {
                let mut log_clone = (self.as_ref()).clone();
                let calls = log_clone.calls();

                // Find the selected call (where selected = true)
                for call in calls {
                    match call {
                        baml_runtime::tracingv2::storage::storage::LLMCallKind::Basic(c) => {
                            if c.selected {
                                return Ok(BamlObjectResponseSuccess::new_object(c.into()));
                            }
                        }
                        baml_runtime::tracingv2::storage::storage::LLMCallKind::Stream(c) => {
                            if c.llm_call.selected {
                                return Ok(BamlObjectResponseSuccess::new_object(c.into()));
                            }
                        }
                    };
                }

                // No selected call found
                Ok(BamlObjectResponseSuccess::new_value(BamlValue::Null))
            }
            _ => Err(format!(
                "Failed to call function: \"{method_name}\" on object type: FunctionLog"
            )),
        }
    }
}

impl CallMethod for TimingWrapper {
    fn call_method(
        &self,
        method_name: &str,
        _kwargs: &baml_types::BamlMap<String, baml_types::BamlValue>,
    ) -> BamlObjectResponse {
        match method_name {
            "~destructor" => {
                self.clone().destroy();
                Ok(BamlObjectResponseSuccess::new_value(BamlValue::Null))
            }
            "start_time_utc_ms" => Ok(BamlObjectResponseSuccess::new_value(BamlValue::Int(
                self.start_time_utc_ms,
            ))),
            "duration_ms" => Ok(BamlObjectResponseSuccess::new_value(BamlValue::Int(
                self.duration_ms.unwrap_or_default(),
            ))),
            _ => Err(format!(
                "Failed to call function: \"{method_name}\" on object type: Timing"
            )),
        }
    }
}

impl CallMethod for StreamTimingWrapper {
    fn call_method(
        &self,
        method_name: &str,
        _kwargs: &baml_types::BamlMap<String, baml_types::BamlValue>,
    ) -> BamlObjectResponse {
        match method_name {
            "~destructor" => {
                self.clone().destroy();
                Ok(BamlObjectResponseSuccess::new_value(BamlValue::Null))
            }
            "start_time_utc_ms" => Ok(BamlObjectResponseSuccess::new_value(BamlValue::Int(
                self.start_time_utc_ms,
            ))),
            "duration_ms" => Ok(BamlObjectResponseSuccess::new_value(BamlValue::Int(
                self.duration_ms.unwrap_or_default(),
            ))),
            _ => Err(format!(
                "Failed to call function: \"{method_name}\" on object type: StreamTiming"
            )),
        }
    }
}

impl CallMethod for LLMCallWrapper {
    fn call_method(
        &self,
        method_name: &str,
        _kwargs: &baml_types::BamlMap<String, baml_types::BamlValue>,
    ) -> BamlObjectResponse {
        match method_name {
            "~destructor" => {
                self.clone().destroy();
                Ok(BamlObjectResponseSuccess::new_value(BamlValue::Null))
            }
            "client_name" => Ok(BamlObjectResponseSuccess::new_value(BamlValue::String(
                self.client_name.clone(),
            ))),
            "provider" => Ok(BamlObjectResponseSuccess::new_value(BamlValue::String(
                self.provider.clone(),
            ))),
            "selected" => Ok(BamlObjectResponseSuccess::new_value(BamlValue::Bool(
                self.selected,
            ))),
            "timing" => Ok(BamlObjectResponseSuccess::new_object(
                self.timing.clone().into(),
            )),
            "usage" => {
                if let Some(usage) = self.usage.clone() {
                    Ok(BamlObjectResponseSuccess::new_object(usage.into()))
                } else {
                    Ok(BamlObjectResponseSuccess::new_value(BamlValue::Null))
                }
            }
            "http_request_id" => {
                if let Some(req) = self.request.as_ref() {
                    Ok(BamlObjectResponseSuccess::new_value(BamlValue::String(
                        req.id().to_string(),
                    )))
                } else {
                    Ok(BamlObjectResponseSuccess::new_value(BamlValue::Null))
                }
            }
            "http_request" => {
                if let Some(request) = self.request.clone() {
                    Ok(BamlObjectResponseSuccess::new_object(request.into()))
                } else {
                    Ok(BamlObjectResponseSuccess::new_value(BamlValue::Null))
                }
            }
            "http_response" => {
                if let Some(response) = self.response.clone() {
                    Ok(BamlObjectResponseSuccess::new_object(response.into()))
                } else {
                    Ok(BamlObjectResponseSuccess::new_value(BamlValue::Null))
                }
            }
            _ => Err(format!(
                "Failed to call function: \"{method_name}\" on object type: LLMCall"
            )),
        }
    }
}

impl CallMethod for LLMStreamCallWrapper {
    fn call_method(
        &self,
        method_name: &str,
        _kwargs: &baml_types::BamlMap<String, baml_types::BamlValue>,
    ) -> BamlObjectResponse {
        match method_name {
            "~destructor" => {
                self.clone().destroy();
                Ok(BamlObjectResponseSuccess::new_value(BamlValue::Null))
            }
            "client_name" => Ok(BamlObjectResponseSuccess::new_value(BamlValue::String(
                self.llm_call.client_name.clone(),
            ))),
            "provider" => Ok(BamlObjectResponseSuccess::new_value(BamlValue::String(
                self.llm_call.provider.clone(),
            ))),
            "selected" => Ok(BamlObjectResponseSuccess::new_value(BamlValue::Bool(
                self.llm_call.selected,
            ))),
            "timing" => Ok(BamlObjectResponseSuccess::new_object(
                self.timing.clone().into(),
            )),
            "usage" => {
                if let Some(usage) = self.llm_call.usage.clone() {
                    Ok(BamlObjectResponseSuccess::new_object(usage.into()))
                } else {
                    Ok(BamlObjectResponseSuccess::new_value(BamlValue::Null))
                }
            }
            "http_request" => {
                if let Some(request) = self.llm_call.request.clone() {
                    Ok(BamlObjectResponseSuccess::new_object(request.into()))
                } else {
                    Ok(BamlObjectResponseSuccess::new_value(BamlValue::Null))
                }
            }
            "http_response" => {
                if let Some(response) = self.llm_call.response.clone() {
                    Ok(BamlObjectResponseSuccess::new_object(response.into()))
                } else {
                    Ok(BamlObjectResponseSuccess::new_value(BamlValue::Null))
                }
            }
            "http_request_id" => {
                if let Some(req) = self.llm_call.request.as_ref() {
                    Ok(BamlObjectResponseSuccess::new_value(BamlValue::String(
                        req.id().to_string(),
                    )))
                } else {
                    Ok(BamlObjectResponseSuccess::new_value(BamlValue::Null))
                }
            }
            "sse_chunks" => {
                if let Some(chunks) = &self.sse_chunks {
                    Ok(BamlObjectResponseSuccess::new_objects(
                        chunks
                            .event
                            .iter()
                            .map(|chunk| chunk.clone().into())
                            .collect(),
                    ))
                } else {
                    Ok(BamlObjectResponseSuccess::new_value(BamlValue::Null))
                }
            }
            _ => Err(format!(
                "Failed to call function: \"{method_name}\" on object type: LLMStreamCall"
            )),
        }
    }
}

impl CallMethod for HTTPRequestWrapper {
    fn call_method(
        &self,
        method_name: &str,
        _kwargs: &baml_types::BamlMap<String, baml_types::BamlValue>,
    ) -> BamlObjectResponse {
        match method_name {
            "~destructor" => {
                self.clone().destroy();
                Ok(BamlObjectResponseSuccess::new_value(BamlValue::Null))
            }
            "id" => {
                let id = self.id().to_string();
                Ok(BamlObjectResponseSuccess::new_value(BamlValue::String(id)))
            }
            "url" => Ok(BamlObjectResponseSuccess::new_value(BamlValue::String(
                self.url().to_string(),
            ))),
            "method" => Ok(BamlObjectResponseSuccess::new_value(BamlValue::String(
                self.method().to_string(),
            ))),
            "headers" => {
                let headers = self.headers();
                Ok(BamlObjectResponseSuccess::new_value(BamlValue::Map(
                    headers
                        .iter()
                        .map(|(k, v)| (k.clone(), BamlValue::String(v.clone())))
                        .collect(),
                )))
            }
            "body" => Ok(BamlObjectResponseSuccess::new_object(
                self.body().clone().into(),
            )),
            _ => Err(format!(
                "Failed to call function: \"{method_name}\" on object type: HTTPRequest"
            )),
        }
    }
}

impl CallMethod for HTTPResponseWrapper {
    fn call_method(
        &self,
        method_name: &str,
        _kwargs: &baml_types::BamlMap<String, baml_types::BamlValue>,
    ) -> BamlObjectResponse {
        match method_name {
            "~destructor" => {
                self.clone().destroy();
                Ok(BamlObjectResponseSuccess::new_value(BamlValue::Null))
            }
            "id" => {
                let id = self.inner.request_id.to_string();
                Ok(BamlObjectResponseSuccess::new_value(BamlValue::String(id)))
            }
            "status" => Ok(BamlObjectResponseSuccess::new_value(BamlValue::Int(
                self.status as i64,
            ))),
            "headers" => {
                if let Some(headers) = self.headers() {
                    Ok(BamlObjectResponseSuccess::new_value(BamlValue::Map(
                        headers
                            .iter()
                            .map(|(k, v)| (k.clone(), BamlValue::String(v.clone())))
                            .collect(),
                    )))
                } else {
                    Ok(BamlObjectResponseSuccess::new_value(BamlValue::Null))
                }
            }
            "body" => Ok(BamlObjectResponseSuccess::new_object(
                self.body.clone().into(),
            )),
            _ => Err(format!(
                "Failed to call function: \"{method_name}\" on object type: HTTPResponse"
            )),
        }
    }
}

impl CallMethod for HTTPBodyWrapper {
    fn call_method(
        &self,
        method_name: &str,
        _kwargs: &baml_types::BamlMap<String, baml_types::BamlValue>,
    ) -> BamlObjectResponse {
        match method_name {
            "~destructor" => {
                self.clone().destroy();
                Ok(BamlObjectResponseSuccess::new_value(BamlValue::Null))
            }
            "text" => match self.text() {
                Ok(text) => Ok(BamlObjectResponseSuccess::new_value(BamlValue::String(
                    text.to_string(),
                ))),
                Err(_) => Ok(BamlObjectResponseSuccess::new_value(BamlValue::Null)),
            },
            "json" => match self.json() {
                Ok(json_value) => Ok(BamlObjectResponseSuccess::new_value(
                    BamlValue::try_from(json_value).unwrap(),
                )),
                Err(_) => Ok(BamlObjectResponseSuccess::new_value(BamlValue::Null)),
            },
            _ => Err(format!(
                "Failed to call function: \"{method_name}\" on object type: HTTPBody"
            )),
        }
    }
}

impl CallMethod for SSEEventWrapper {
    fn call_method(
        &self,
        method_name: &str,
        _kwargs: &baml_types::BamlMap<String, baml_types::BamlValue>,
    ) -> BamlObjectResponse {
        match method_name {
            "~destructor" => {
                self.clone().destroy();
                Ok(BamlObjectResponseSuccess::new_value(BamlValue::Null))
            }
            "text" => Ok(BamlObjectResponseSuccess::new_value(BamlValue::String(
                self.data.clone(),
            ))),
            "json" => match serde_json::from_str::<BamlValue>(&self.data) {
                Ok(json_value) => Ok(BamlObjectResponseSuccess::new_value(json_value)),
                Err(_) => Ok(BamlObjectResponseSuccess::new_value(BamlValue::Null)),
            },
            _ => Err(format!(
                "Failed to call function: \"{method_name}\" on object type: SSEEvent"
            )),
        }
    }
}

impl CallMethod for MediaWrapper {
    fn call_method(
        &self,
        method_name: &str,
        _kwargs: &baml_types::BamlMap<String, baml_types::BamlValue>,
    ) -> BamlObjectResponse {
        match method_name {
            "~destructor" => {
                self.clone().destroy();
                Ok(BamlObjectResponseSuccess::new_value(BamlValue::Null))
            }
            "media_type" => Ok(BamlObjectResponseSuccess::new_value(BamlValue::Enum(
                "MediaType".into(),
                self.media_type.to_string(),
            ))),
            "mime_type" => Ok(BamlObjectResponseSuccess::new_value(
                self.mime_type.as_ref().map_or_else(
                    || BamlValue::Null,
                    |mime_type| BamlValue::String(mime_type.to_string()),
                ),
            )),
            "as_url" => match &self.content {
                baml_types::BamlMediaContent::Url(media_url) => Ok(
                    BamlObjectResponseSuccess::new_value(BamlValue::String(media_url.url.clone())),
                ),
                baml_types::BamlMediaContent::File(_) | baml_types::BamlMediaContent::Base64(_) => {
                    Ok(BamlObjectResponseSuccess::new_value(BamlValue::Null))
                }
            },
            "as_base64" => match &self.content {
                baml_types::BamlMediaContent::Base64(media_base64) => {
                    Ok(BamlObjectResponseSuccess::new_value(BamlValue::String(
                        media_base64.base64.clone(),
                    )))
                }
                baml_types::BamlMediaContent::File(_) | baml_types::BamlMediaContent::Url(_) => {
                    Ok(BamlObjectResponseSuccess::new_value(BamlValue::Null))
                }
            },
            _ => Err(format!(
                "Failed to call function: \"{method_name}\" on object type: Media"
            )),
        }
    }
}
