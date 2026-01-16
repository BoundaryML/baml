pub mod collector;
pub mod media;
pub mod type_builder;

use std::{
    any::type_name,
    fs::OpenOptions,
    io::Write,
    ops::Deref,
    sync::{atomic::AtomicBool, Arc, Mutex, OnceLock},
    time::{SystemTime, UNIX_EPOCH},
};

/// Get the FFI log file path from env, or None if logging is disabled
fn ffi_log_file() -> Option<&'static str> {
    static FILE: OnceLock<Option<String>> = OnceLock::new();
    FILE.get_or_init(|| std::env::var("BAML_FFI_LOG").ok())
        .as_deref()
}

/// Global mutex for Rust FFI log file access
fn ffi_log_mutex() -> &'static Mutex<()> {
    static MUTEX: OnceLock<Mutex<()>> = OnceLock::new();
    MUTEX.get_or_init(|| Mutex::new(()))
}

/// Get current timestamp in microseconds since epoch
fn timestamp_micros() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_micros())
        .unwrap_or(0)
}

/// Write a log message to the Rust FFI log file
fn write_ffi_log(msg: &str) {
    if let Some(path) = ffi_log_file() {
        let _guard = ffi_log_mutex().lock().unwrap();
        if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
            let _ = writeln!(file, "{}", msg);
        }
    }
}

macro_rules! ffi_log {
    ($($arg:tt)*) => {
        if ffi_log_file().is_some() {
            let ts = timestamp_micros();
            let msg = format!($($arg)*);
            // Insert timestamp after the opening bracket
            let msg = if msg.starts_with('[') {
                let bracket_end = msg.find(']').unwrap_or(0);
                format!("{} ts={}{}", &msg[..bracket_end], ts, &msg[bracket_end..])
            } else {
                format!("ts={} {}", ts, msg)
            };
            write_ffi_log(&msg);
        }
    };
}

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
    baml::cffi::{self, BamlPointerType},
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
    /// Creates a wrapper from a raw pointer.
    ///
    /// When `persist=true` (borrowing from Go): We increment the strong count before
    /// taking ownership, so when this wrapper drops, the original Go reference remains valid.
    ///
    /// When `persist=false` (taking ownership): We take ownership of the raw pointer directly,
    /// so when this wrapper drops, the reference count decrements.
    pub fn from_raw(object: *const libc::c_void, persist: bool) -> Self {
        unsafe {
            if persist {
                // Borrowing: increment refcount first, then take the raw pointer.
                // This way when we drop, the original Go reference remains valid.
                Arc::increment_strong_count(object as *const T);
            }
            let arc = Arc::from_raw(object as *const T);
            Self {
                inner: arc,
                persist: AtomicBool::new(persist),
            }
        }
    }

    /// Called when Go is done with this object. Releases Go's ownership of the raw pointer.
    pub fn destroy(self) {
        let ptr = Arc::as_ptr(&self.inner) as i64;
        // Log when Go releases its reference
        ffi_log!("[FFI_GO_RELEASE] type={} ptr={:#x}", type_name::<T>(), ptr);
        // Decrement the strong count to release Go's ownership
        unsafe {
            Arc::decrement_strong_count(Arc::as_ptr(&self.inner));
        }
        // The wrapper will drop normally, decrementing the count again
        // (but we incremented it in from_raw when persist=true, so this balances out)
    }

    pub fn from_object(object: T) -> Self {
        let arc = Arc::new(object);
        let ptr = Arc::as_ptr(&arc) as i64;
        // Log when a new object is created - this is the "birth" of the object
        ffi_log!("[FFI_CREATE] type={} ptr={:#x}", type_name::<T>(), ptr);
        Self {
            inner: arc,
            persist: AtomicBool::new(false),
        }
    }

    pub fn from_arc(object: Arc<T>) -> Self {
        // Note: from_arc receives an existing Arc, not creating a new one
        // The Arc was created elsewhere (e.g., in baml_runtime)
        let ptr = Arc::as_ptr(&object) as i64;
        ffi_log!("[FFI_WRAP_ARC] type={} ptr={:#x}", type_name::<T>(), ptr);
        Self {
            inner: object,
            persist: AtomicBool::new(false),
        }
    }

    /// Returns a raw pointer for Go to hold. This creates a new reference.
    pub fn pointer(&self) -> BamlPointerType {
        let cloned = self.inner.clone();
        let ptr = Arc::into_raw(cloned) as i64;
        // Log when we give a pointer to Go - Go now owns this reference
        ffi_log!("[FFI_GIVE_GO] type={} ptr={:#x}", type_name::<T>(), ptr);
        BamlPointerType { pointer: ptr }
    }
}

impl<T> Drop for RawPtrWrapper<T> {
    fn drop(&mut self) {
        let strong_count = Arc::strong_count(&self.inner);
        let ptr = Arc::as_ptr(&self.inner) as i64;
        // Log when this is the last reference and object will be freed
        if strong_count == 1 {
            ffi_log!("[FFI_FREE] type={} ptr={:#x}", type_name::<T>(), ptr);
        }
        // Arc drops normally after this
    }
}

impl<T> Deref for RawPtrWrapper<T> {
    type Target = Arc<T>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

// Reference counting is properly managed by:
// - from_raw(persist=true): increments refcount before taking raw pointer
// - from_raw(persist=false): takes ownership of raw pointer directly
// - destroy(): decrements refcount to release Go's ownership
// - pointer(): creates a new reference for Go to hold
// - Drop: logs [FFI_FREE] when strong_count==1, then Arc drops normally

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

impl RawPtrType {
    pub(crate) fn create_collector(name: Option<&str>) -> Result<Self, String> {
        Self::new_collector(name).map(RawPtrType::from)
    }

    pub(crate) fn create_type_builder() -> Result<Self, String> {
        Self::new_type_builder().map(RawPtrType::from)
    }
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
