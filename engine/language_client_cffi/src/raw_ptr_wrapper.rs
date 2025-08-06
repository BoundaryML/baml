pub mod collector;
pub mod media;
pub mod type_builder;

use std::{
    ops::Deref,
    sync::{atomic::AtomicBool, Arc},
};

use baml_cffi_macros::{define_raw_ptr_types, export_baml_new_fn};
use baml_runtime::{
    tracingv2::storage::storage::{
        Collector, FunctionLog, LLMCall, LLMStreamCall, StreamTiming, Timing, Usage,
    },
    BamlRuntime,
};
use baml_types::{
    tracing::events::{HTTPBody, HTTPRequest, HTTPResponse, SSEEvent},
    BamlMedia, TypeIR,
};
use type_builder::objects::{
    ClassBuilder, ClassPropertyBuilder, EnumBuilder, EnumValueBuilder, TypeBuilder,
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

define_raw_ptr_types! {
    Collector,
    Usage,
    FunctionLog,
    Timing,
    StreamTiming,
    // Special cases that need custom handling
    LLMCall => LLMCall as LLMCallWrapper: "LLMCall" (Object::LlmCall),
    LLMStreamCall => LLMStreamCall as LLMStreamCallWrapper: "LLMStreamCall" (Object::LlmStreamCall),
    HTTPRequest => HTTPRequest as HTTPRequestWrapper: "HTTPRequest" (Object::HttpRequest),
    HTTPResponse => HTTPResponse as HTTPResponseWrapper: "HTTPResponse" (Object::HttpResponse),
    HTTPBody => HTTPBody as HTTPBodyWrapper: "HTTPBody" (Object::HttpBody),
    SSEEvent => SSEEvent as SSEEventWrapper: "SSEEvent" (Object::SseResponse),
    BamlMedia => Media as MediaWrapper: "Media" (Object::MediaImage, Object::MediaAudio, Object::MediaPdf, Object::MediaVideo),
    TypeBuilder,
    EnumBuilder,
    EnumValueBuilder,
    ClassBuilder,
    ClassPropertyBuilder,
    TypeIR => TypeDef as TypeWrapper: "Type" (Object::Type),
}

fn create_media_object(
    media_type: baml_types::BamlMediaType,
    mime_type: Option<&str>,
    url: Option<&str>,
    base64: Option<&str>,
) -> Result<MediaWrapper, String> {
    let mime_type = mime_type.map(|s| s.to_string());
    let media = match (url, base64) {
        (Some(url), None) => BamlMedia::url(media_type, url.to_string(), mime_type),
        (None, Some(base64)) => BamlMedia::base64(media_type, base64.to_string(), mime_type),
        (Some(_), Some(_)) => {
            return Err("Only one of url or base64 can be provided".to_string());
        }
        (None, None) => {
            return Err("Must provide either url or base64".to_string());
        }
    };

    Ok(MediaWrapper::from_object(media))
}

#[export_baml_new_fn]
impl RawPtrType {
    #[export_baml_new_fn(ObjectCollector)]
    fn new_collector(name: Option<&str>) -> Result<CollectorWrapper, String> {
        let collector = Collector::new(name.map(|s| s.to_string()));
        Ok(CollectorWrapper::from_object(collector))
    }

    #[export_baml_new_fn(ObjectMediaImage)]
    fn new_media_image(
        mime_type: Option<&str>,
        url: Option<&str>,
        base64: Option<&str>,
    ) -> Result<MediaWrapper, String> {
        create_media_object(baml_types::BamlMediaType::Image, mime_type, url, base64)
    }

    #[export_baml_new_fn(ObjectMediaAudio)]
    fn new_media_audio(
        mime_type: Option<&str>,
        url: Option<&str>,
        base64: Option<&str>,
    ) -> Result<MediaWrapper, String> {
        create_media_object(baml_types::BamlMediaType::Audio, mime_type, url, base64)
    }

    #[export_baml_new_fn(ObjectMediaPdf)]
    fn new_media_pdf(
        mime_type: Option<&str>,
        url: Option<&str>,
        base64: Option<&str>,
    ) -> Result<MediaWrapper, String> {
        create_media_object(baml_types::BamlMediaType::Pdf, mime_type, url, base64)
    }

    #[export_baml_new_fn(ObjectMediaVideo)]
    fn new_media_video(
        mime_type: Option<&str>,
        url: Option<&str>,
        base64: Option<&str>,
    ) -> Result<MediaWrapper, String> {
        create_media_object(baml_types::BamlMediaType::Video, mime_type, url, base64)
    }

    #[export_baml_new_fn(ObjectTypeBuilder)]
    fn new_type_builder() -> Result<TypeBuilderWrapper, String> {
        let type_builder = TypeBuilder::default();
        Ok(TypeBuilderWrapper::from_object(type_builder))
    }
}

// Internal trait used by macros - not part of the public API
pub trait CallMethod {
    fn call_method(
        &self,
        runtime: &BamlRuntime,
        method_name: &str,
        kwargs: &baml_types::BamlMap<String, crate::ffi::Value>,
    ) -> BamlObjectResponse;
}
