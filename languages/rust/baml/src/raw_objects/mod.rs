//! FFI-backed BAML objects (`RawObject` infrastructure)
//!
//! This module contains all types that wrap FFI pointers managed by the Rust
//! runtime. Each type holds a `RawObject` which handles method calls, encoding,
//! and cleanup.

#![allow(unsafe_code)]

/// Macro to define a wrapper type around `RawObject`.
///
/// This reduces boilerplate for all FFI-backed object types.
/// Generates: struct, `from_raw`, `RawObjectTrait` impl, and `BamlEncode` impl.
macro_rules! define_raw_object_wrapper {
    (
        $(#[$meta:meta])*
        $name:ident => $object_type:ident
    ) => {
        $(#[$meta])*
        #[derive(Clone)]
        pub struct $name {
            raw: RawObject,
        }

        impl crate::codec::traits::DecodeHandle for $name {
            fn decode_handle(handle: crate::proto::baml_cffi_v1::BamlObjectHandle, runtime: *const c_void) -> Result<Self, crate::BamlError> {
                let object_type = crate::raw_objects::object_type_from_handle(&handle)?;
                let ptr = crate::raw_objects::extract_ptr_from_handle(&handle)?;
                debug_assert_eq!(object_type, BamlObjectType::$object_type);
                let raw = RawObject::from_pointer(ptr, runtime, object_type);
                Ok(Self { raw })
            }
        }

        impl RawObjectTrait for $name {
            fn raw(&self) -> &RawObject {
                &self.raw
            }
        }

        impl $crate::codec::BamlEncode for $name {
            fn baml_encode(&self) -> $crate::proto::baml_cffi_v1::HostValue {
                $crate::proto::baml_cffi_v1::HostValue {
                    value: Some($crate::proto::baml_cffi_v1::host_value::Value::Handle(
                        self.raw.encode(),
                    )),
                }
            }
        }
    };
}

// Make macro available to submodules

// Submodules for specific object types (Phase 11-13)
mod collector;
mod http;
mod llm_call;
mod media;
mod type_builder;

// Re-export all public types from submodules
use std::{ffi::c_void, sync::Arc};

pub use collector::{Collector, FunctionLog, LogType, StreamTiming, Timing, Usage};
pub use http::{HTTPBody, HTTPRequest, HTTPResponse, SSEResponse};
pub use llm_call::{LLMCall, LLMCallKind, LLMStreamCall};
pub use media::{Audio, Image, Pdf, Video};
use prost::Message;
pub use type_builder::{
    ClassBuilder, ClassPropertyBuilder, EnumBuilder, EnumValueBuilder, TypeBuilder, TypeDef,
};

use crate::{
    baml_unreachable,
    codec::{
        BamlDecode,
        traits::{DecodeHandle, IntoKwargs},
    },
    error::BamlError,
    ffi,
    proto::baml_cffi_v1::{
        BamlObjectConstructorInvocation, BamlObjectHandle, BamlObjectMethodInvocation,
        BamlObjectType, BamlPointerType, CffiValueHolder, HostMapEntry, InvocationResponse,
        baml_object_handle, invocation_response, invocation_response_success,
    },
};

/// Inner data for a FFI-backed BAML object.
///
/// Wrapped in Arc to enable cheap cloning while preserving single-drop
/// semantics.
struct RawObjectInner {
    ptr: i64,
    runtime: *const c_void,
    object_type: BamlObjectType,
}

// Safety: The underlying Rust runtime is thread-safe
#[allow(unsafe_code)]
unsafe impl Send for RawObjectInner {}
#[allow(unsafe_code)]
unsafe impl Sync for RawObjectInner {}

/// A handle to a FFI-backed BAML object.
///
/// This is the base type for Media, Collector, `TypeBuilder`, etc.
/// It wraps a raw pointer managed by the Rust runtime.
/// Uses Arc internally to enable cloning while ensuring the destructor
/// is only called once when the last reference is dropped.
pub(crate) struct RawObject {
    inner: Arc<RawObjectInner>,
}

impl Clone for RawObject {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl RawObject {
    /// Create from an existing FFI pointer
    pub(crate) fn from_pointer(
        ptr: i64,
        runtime: *const c_void,
        object_type: BamlObjectType,
    ) -> Self {
        Self {
            inner: Arc::new(RawObjectInner {
                ptr,
                runtime,
                object_type,
            }),
        }
    }

    /// Create a new object by calling the constructor
    pub(crate) fn new<K: IntoKwargs>(
        runtime: *const c_void,
        object_type: BamlObjectType,
        kwargs: K,
    ) -> Result<Self, BamlError> {
        // Encode constructor invocation
        let invocation = BamlObjectConstructorInvocation {
            r#type: object_type.into(),
            kwargs: kwargs.into_kwargs(),
        };

        let mut buf = Vec::new();
        invocation
            .encode(&mut buf)
            .map_err(|e| BamlError::internal(format!("failed to encode constructor: {e}")))?;

        // Call FFI
        let response_buf = unsafe {
            ffi::call_object_constructor(buf.as_ptr().cast::<i8>(), buf.len())
                .map_err(|e| BamlError::internal(format!("Failed to load BAML library: {e}")))?
        };

        // Decode response
        let response_bytes = unsafe {
            if response_buf.ptr.is_null() {
                return Err(BamlError::internal("null response from constructor"));
            }
            std::slice::from_raw_parts(response_buf.ptr.cast::<u8>(), response_buf.len)
        };

        let response = InvocationResponse::decode(response_bytes)
            .map_err(|e| BamlError::internal(format!("failed to decode response: {e}")))?;

        // Free the buffer - ignore errors during cleanup
        let _ = unsafe { ffi::free_buffer(response_buf) };

        // Extract pointer from response
        match response.response {
            Some(invocation_response::Response::Success(success)) => {
                let Some(invocation_response_success::Result::Object(handle)) = success.result
                else {
                    return Err(BamlError::internal("expected object handle in response"));
                };
                let ptr = extract_ptr_from_handle(&handle)?;
                Ok(Self {
                    inner: Arc::new(RawObjectInner {
                        ptr,
                        runtime,
                        object_type,
                    }),
                })
            }
            Some(invocation_response::Response::Error(e)) => Err(BamlError::internal(e)),
            None => Err(BamlError::internal("empty response")),
        }
    }

    /// Call a method on this object and decode the result using `BamlDecode`.
    ///
    /// This is the primary method for calling object methods that return
    /// values. Use `T = ()` for void methods (side-effect only).
    ///
    /// # Panics
    /// Panics if the FFI call fails. This should never happen in practice since
    /// we control both sides of the FFI boundary.
    ///
    /// # Examples
    /// ```ignore
    /// // No arguments
    /// let is_url: bool = obj.call_method("is_url", ());
    ///
    /// // Single argument
    /// let result: String = obj.call_method("process", ("input", "hello"));
    ///
    /// // Two arguments
    /// let result: i64 = obj.call_method("add", (("a", 1), ("b", 2)));
    ///
    /// // Void method (side-effect only)
    /// obj.call_method::<()>("reset", ());
    /// ```
    pub(crate) fn call_method<T: BamlDecode, K: IntoKwargs>(
        &self,
        method_name: &str,
        kwargs: K,
    ) -> T {
        self.try_call_method(method_name, kwargs)
            .unwrap_or_else(|e| {
                baml_unreachable!(
                    "FFI method call '{}' on {:?} failed: {}",
                    method_name,
                    self.object_type(),
                    e
                )
            })
    }

    /// Call a method on this object and decode the result using `BamlDecode`.
    ///
    /// Returns a `Result` for callers that need to handle FFI errors
    /// explicitly. Most callers should use `call_method` instead.
    ///
    /// Use `T = ()` for void methods (side-effect only).
    pub(crate) fn try_call_method<T: BamlDecode, K: IntoKwargs>(
        &self,
        method_name: &str,
        kwargs: K,
    ) -> Result<T, BamlError> {
        let response = self.call_method_raw_internal(method_name, kwargs.into_kwargs())?;
        match response.response {
            Some(invocation_response::Response::Success(success)) => {
                match success.result {
                    Some(invocation_response_success::Result::Value(value)) => {
                        T::baml_decode(&value)
                    }
                    Some(invocation_response_success::Result::Object(_)) => {
                        Err(BamlError::internal(
                            "method returned object handle, use call_method_for_object instead",
                        ))
                    }
                    Some(invocation_response_success::Result::Objects(_)) => {
                        Err(BamlError::internal(
                            "method returned object handles, use call_method_for_objects instead",
                        ))
                    }
                    None => {
                        // No result - decode as empty value (works for () and Option<T>)
                        T::baml_decode(&CffiValueHolder { value: None })
                    }
                }
            }
            Some(invocation_response::Response::Error(e)) => Err(BamlError::internal(e)),
            None => Err(BamlError::internal("empty response")),
        }
    }

    /// Call a method on this object that returns another object handle.
    ///
    /// Use this for methods that return FFI object references (not values).
    pub(crate) fn call_method_for_object_optional<K: IntoKwargs, Object: DecodeHandle>(
        &self,
        method_name: &str,
        kwargs: K,
    ) -> Result<Option<Object>, BamlError> {
        let response = self.call_method_raw_internal(method_name, kwargs.into_kwargs())?;

        match response.response {
            Some(invocation_response::Response::Success(success)) => match success.result {
                Some(invocation_response_success::Result::Object(handle)) => {
                    Object::decode_handle(handle, self.runtime()).map(Some)
                }
                _ => Err(BamlError::internal("expected object handle in response")),
            },
            Some(invocation_response::Response::Error(e)) => Err(BamlError::internal(e)),
            None => Ok(None),
        }
    }

    pub(crate) fn call_method_for_object<K: IntoKwargs, Object: DecodeHandle>(
        &self,
        method_name: &str,
        kwargs: K,
    ) -> Result<Object, BamlError> {
        match self.call_method_for_object_optional(method_name, kwargs) {
            Ok(Some(object)) => Ok(object),
            Ok(None) => Err(BamlError::internal(format!(
                "expected object handle in response for method {object_type:?}.{method_name}",
                method_name = method_name,
                object_type = self.object_type()
            ))),
            Err(e) => Err(e),
        }
    }

    /// Call a method on this object that returns multiple object handles.
    ///
    /// Use this for methods that return lists of FFI object references.
    fn call_method_for_objects_optional<K: IntoKwargs, Object: DecodeHandle>(
        &self,
        method_name: &str,
        kwargs: K,
    ) -> Result<Option<Vec<Object>>, BamlError> {
        let response = self.call_method_raw_internal(method_name, kwargs.into_kwargs())?;

        match response.response {
            Some(invocation_response::Response::Success(success)) => match success.result {
                Some(invocation_response_success::Result::Objects(handles)) => handles
                    .objects
                    .into_iter()
                    .map(|h| Object::decode_handle(h, self.runtime()))
                    .collect::<Result<Vec<_>, _>>()
                    .map(Some),
                _ => Err(BamlError::internal("expected object handles in response")),
            },
            Some(invocation_response::Response::Error(e)) => Err(BamlError::internal(e)),
            None => Err(BamlError::internal("empty response")),
        }
    }

    pub(crate) fn call_method_for_objects<K: IntoKwargs, Object: DecodeHandle>(
        &self,
        method_name: &str,
        kwargs: K,
    ) -> Result<Vec<Object>, BamlError> {
        match self.call_method_for_objects_optional(method_name, kwargs) {
            Ok(Some(objects)) => Ok(objects),
            Ok(None) => Err(BamlError::internal(format!(
                "Expected a list of objects in response for method {object_type:?}.{method_name}",
                method_name = method_name,
                object_type = self.object_type()
            ))),
            Err(e) => Err(e),
        }
    }

    /// Low-level method call that returns the raw `InvocationResponse`
    fn call_method_raw_internal(
        &self,
        method_name: &str,
        kwargs: Vec<HostMapEntry>,
    ) -> Result<InvocationResponse, BamlError> {
        let invocation = BamlObjectMethodInvocation {
            object: Some(self.encode()),
            method_name: method_name.to_string(),
            kwargs,
        };

        let mut buf = Vec::new();
        invocation
            .encode(&mut buf)
            .map_err(|e| BamlError::internal(format!("failed to encode method call: {e}")))?;

        let response_buf = unsafe {
            ffi::call_object_method(self.inner.runtime, buf.as_ptr().cast::<i8>(), buf.len())
                .map_err(|e| BamlError::internal(format!("Failed to load BAML library: {e}")))?
        };

        // Decode response
        let response_bytes = unsafe {
            if response_buf.ptr.is_null() {
                return Err(BamlError::internal("null response from method call"));
            }
            std::slice::from_raw_parts(response_buf.ptr.cast::<u8>(), response_buf.len)
        };

        let response = InvocationResponse::decode(response_bytes)
            .map_err(|e| BamlError::internal(format!("failed to decode response: {e}")))?;

        // Free the buffer - ignore errors during cleanup
        let _ = unsafe { ffi::free_buffer(response_buf) };

        Ok(response)
    }

    /// Encode to `BamlObjectHandle` for passing to function calls
    pub(crate) fn encode(&self) -> BamlObjectHandle {
        encode_raw_object_handle(self.inner.ptr, self.inner.object_type)
    }

    /// Get the object type
    pub(crate) fn object_type(&self) -> BamlObjectType {
        self.inner.object_type
    }

    /// Get the runtime pointer
    pub(crate) fn runtime(&self) -> *const c_void {
        self.inner.runtime
    }
}

impl Drop for RawObject {
    fn drop(&mut self) {
        // Only call destructor if this is the last reference
        // This ensures the FFI destructor is called exactly once
        if Arc::strong_count(&self.inner) == 1 {
            // Call destructor via FFI
            // Ignore errors during drop - we can't do much about them
            let _ = self.try_call_method::<(), _>("~destructor", ());
        }
    }
}

/// Extract a pointer from a `BamlObjectHandle`
pub(crate) fn extract_ptr_from_handle(handle: &BamlObjectHandle) -> Result<i64, BamlError> {
    match &handle.object {
        Some(obj) => {
            // All variants contain a BamlPointerType
            let ptr = match obj {
                baml_object_handle::Object::Collector(p) => p.pointer,
                baml_object_handle::Object::FunctionLog(p) => p.pointer,
                baml_object_handle::Object::Usage(p) => p.pointer,
                baml_object_handle::Object::Timing(p) => p.pointer,
                baml_object_handle::Object::StreamTiming(p) => p.pointer,
                baml_object_handle::Object::LlmCall(p) => p.pointer,
                baml_object_handle::Object::LlmStreamCall(p) => p.pointer,
                baml_object_handle::Object::HttpRequest(p) => p.pointer,
                baml_object_handle::Object::HttpResponse(p) => p.pointer,
                baml_object_handle::Object::HttpBody(p) => p.pointer,
                baml_object_handle::Object::SseResponse(p) => p.pointer,
                baml_object_handle::Object::MediaImage(p) => p.pointer,
                baml_object_handle::Object::MediaAudio(p) => p.pointer,
                baml_object_handle::Object::MediaPdf(p) => p.pointer,
                baml_object_handle::Object::MediaVideo(p) => p.pointer,
                baml_object_handle::Object::TypeBuilder(p) => p.pointer,
                baml_object_handle::Object::Type(p) => p.pointer,
                baml_object_handle::Object::EnumBuilder(p) => p.pointer,
                baml_object_handle::Object::EnumValueBuilder(p) => p.pointer,
                baml_object_handle::Object::ClassBuilder(p) => p.pointer,
                baml_object_handle::Object::ClassPropertyBuilder(p) => p.pointer,
            };
            Ok(ptr)
        }
        None => Err(BamlError::internal("empty object handle")),
    }
}

/// Get the object type from a `BamlObjectHandle`
pub(crate) fn object_type_from_handle(
    handle: &BamlObjectHandle,
) -> Result<BamlObjectType, BamlError> {
    match &handle.object {
        Some(obj) => {
            let object_type = match obj {
                baml_object_handle::Object::Collector(_) => BamlObjectType::ObjectCollector,
                baml_object_handle::Object::FunctionLog(_) => BamlObjectType::ObjectFunctionLog,
                baml_object_handle::Object::Usage(_) => BamlObjectType::ObjectUsage,
                baml_object_handle::Object::Timing(_) => BamlObjectType::ObjectTiming,
                baml_object_handle::Object::StreamTiming(_) => BamlObjectType::ObjectStreamTiming,
                baml_object_handle::Object::LlmCall(_) => BamlObjectType::ObjectLlmCall,
                baml_object_handle::Object::LlmStreamCall(_) => BamlObjectType::ObjectLlmStreamCall,
                baml_object_handle::Object::HttpRequest(_) => BamlObjectType::ObjectHttpRequest,
                baml_object_handle::Object::HttpResponse(_) => BamlObjectType::ObjectHttpResponse,
                baml_object_handle::Object::HttpBody(_) => BamlObjectType::ObjectHttpBody,
                baml_object_handle::Object::SseResponse(_) => BamlObjectType::ObjectSseResponse,
                baml_object_handle::Object::MediaImage(_) => BamlObjectType::ObjectMediaImage,
                baml_object_handle::Object::MediaAudio(_) => BamlObjectType::ObjectMediaAudio,
                baml_object_handle::Object::MediaPdf(_) => BamlObjectType::ObjectMediaPdf,
                baml_object_handle::Object::MediaVideo(_) => BamlObjectType::ObjectMediaVideo,
                baml_object_handle::Object::TypeBuilder(_) => BamlObjectType::ObjectTypeBuilder,
                baml_object_handle::Object::Type(_) => BamlObjectType::ObjectType,
                baml_object_handle::Object::EnumBuilder(_) => BamlObjectType::ObjectEnumBuilder,
                baml_object_handle::Object::EnumValueBuilder(_) => {
                    BamlObjectType::ObjectEnumValueBuilder
                }
                baml_object_handle::Object::ClassBuilder(_) => BamlObjectType::ObjectClassBuilder,
                baml_object_handle::Object::ClassPropertyBuilder(_) => {
                    BamlObjectType::ObjectClassPropertyBuilder
                }
            };
            Ok(object_type)
        }
        None => Err(BamlError::internal("empty object handle")),
    }
}

/// Encode a raw object pointer and type into a `BamlObjectHandle`
fn encode_raw_object_handle(ptr: i64, object_type: BamlObjectType) -> BamlObjectHandle {
    let pointer = BamlPointerType { pointer: ptr };

    let object = match object_type {
        BamlObjectType::ObjectCollector => baml_object_handle::Object::Collector(pointer),
        BamlObjectType::ObjectFunctionLog => baml_object_handle::Object::FunctionLog(pointer),
        BamlObjectType::ObjectUsage => baml_object_handle::Object::Usage(pointer),
        BamlObjectType::ObjectTiming => baml_object_handle::Object::Timing(pointer),
        BamlObjectType::ObjectStreamTiming => baml_object_handle::Object::StreamTiming(pointer),
        BamlObjectType::ObjectLlmCall => baml_object_handle::Object::LlmCall(pointer),
        BamlObjectType::ObjectLlmStreamCall => baml_object_handle::Object::LlmStreamCall(pointer),
        BamlObjectType::ObjectHttpRequest => baml_object_handle::Object::HttpRequest(pointer),
        BamlObjectType::ObjectHttpResponse => baml_object_handle::Object::HttpResponse(pointer),
        BamlObjectType::ObjectHttpBody => baml_object_handle::Object::HttpBody(pointer),
        BamlObjectType::ObjectSseResponse => baml_object_handle::Object::SseResponse(pointer),
        BamlObjectType::ObjectMediaImage => baml_object_handle::Object::MediaImage(pointer),
        BamlObjectType::ObjectMediaAudio => baml_object_handle::Object::MediaAudio(pointer),
        BamlObjectType::ObjectMediaPdf => baml_object_handle::Object::MediaPdf(pointer),
        BamlObjectType::ObjectMediaVideo => baml_object_handle::Object::MediaVideo(pointer),
        BamlObjectType::ObjectTypeBuilder => baml_object_handle::Object::TypeBuilder(pointer),
        BamlObjectType::ObjectType => baml_object_handle::Object::Type(pointer),
        BamlObjectType::ObjectEnumBuilder => baml_object_handle::Object::EnumBuilder(pointer),
        BamlObjectType::ObjectEnumValueBuilder => {
            baml_object_handle::Object::EnumValueBuilder(pointer)
        }
        BamlObjectType::ObjectClassBuilder => baml_object_handle::Object::ClassBuilder(pointer),
        BamlObjectType::ObjectClassPropertyBuilder => {
            baml_object_handle::Object::ClassPropertyBuilder(pointer)
        }
        BamlObjectType::ObjectUnspecified => {
            // This shouldn't happen, but we need to handle it
            // Use Collector as a fallback (will likely fail at runtime)
            baml_object_handle::Object::Collector(pointer)
        }
    };

    BamlObjectHandle {
        object: Some(object),
    }
}

/// Trait for types backed by `RawObject`
pub(crate) trait RawObjectTrait: Send + Sync {
    /// Get a reference to the underlying `RawObject`
    fn raw(&self) -> &RawObject;

    /// Encode to `BamlObjectHandle` for passing to function calls
    fn encode_handle(&self) -> BamlObjectHandle {
        self.raw().encode()
    }
}
