#![allow(unsafe_code)]
use std::{
    collections::HashMap,
    ffi::{CStr, CString, c_void},
};

use prost::Message;

use crate::{
    args::FunctionArgs,
    async_stream::AsyncStreamingCall,
    codec::BamlDecode,
    error::BamlError,
    ffi::{self, callbacks},
    proto::baml_cffi_v1::CffiValueHolder,
    raw_objects::{Audio, Collector, Image, Pdf, TypeBuilder, Video},
    stream::StreamingCall,
};

/// Handle to the BAML runtime
pub struct BamlRuntime {
    ptr: *const c_void,
}

// Safety: The runtime is thread-safe internally (protected by Rust's runtime)
#[allow(unsafe_code)]
unsafe impl Send for BamlRuntime {}
#[allow(unsafe_code)]
unsafe impl Sync for BamlRuntime {}

pub type StaticRuntimeType = once_cell::sync::Lazy<BamlRuntime>;

impl BamlRuntime {
    /// Create a new runtime from embedded BAML source files
    ///
    /// # Arguments
    /// * `baml_src_dir` - Base directory path for BAML sources
    /// * `files` - Map of relative file paths to file contents
    /// * `env` - Environment variables
    pub fn new(
        baml_src_dir: &str,
        files: &HashMap<String, String>,
        env: &HashMap<String, String>,
    ) -> Result<Self, BamlError> {
        // Initialize callbacks first - now returns Result
        callbacks::initialize_callbacks()
            .map_err(|e| BamlError::internal(format!("Failed to load BAML library: {e}")))?;

        // Encode files and env as JSON (matching CFFI format)
        let files_json = json_encode_map(files)?;
        let env_json = json_encode_map(env)?;

        let dir_cstr = CString::new(baml_src_dir)
            .map_err(|_| BamlError::internal("invalid baml_src_dir path (contains null byte)"))?;
        let files_cstr = CString::new(files_json)
            .map_err(|_| BamlError::internal("invalid files json (contains null byte)"))?;
        let env_cstr = CString::new(env_json)
            .map_err(|_| BamlError::internal("invalid env json (contains null byte)"))?;

        #[allow(unsafe_code)]
        let ptr = unsafe {
            ffi::create_baml_runtime(dir_cstr.as_ptr(), files_cstr.as_ptr(), env_cstr.as_ptr())
                .map_err(|e| BamlError::internal(format!("Failed to load BAML library: {e}")))?
        };

        if ptr.is_null() {
            return Err(BamlError::internal("failed to create runtime"));
        }

        Ok(BamlRuntime { ptr })
    }

    /// Call a function synchronously (blocks until complete)
    pub fn call_function<T: BamlDecode>(
        &self,
        name: &str,
        args: &FunctionArgs,
    ) -> Result<T, BamlError> {
        let encoded = args.encode()?;
        let name_cstr =
            CString::new(name).map_err(|_| BamlError::internal("invalid function name"))?;

        let (id, receiver) = callbacks::create_callback();

        #[allow(unsafe_code)]
        let error_ptr = unsafe {
            ffi::call_function_from_c(
                self.ptr,
                name_cstr.as_ptr(),
                encoded.as_ptr().cast::<i8>(),
                encoded.len(),
                id,
            )
            .map_err(|e| {
                callbacks::remove_callback(id);
                BamlError::internal(format!("Failed to load BAML library: {e}"))
            })?
        };

        // Check for immediate error
        if !error_ptr.is_null() {
            callbacks::remove_callback(id);
            #[allow(unsafe_code)]
            let error_msg = unsafe {
                let cstr = CStr::from_ptr(error_ptr.cast::<i8>());
                cstr.to_string_lossy().into_owned()
            };
            return Err(BamlError::internal(error_msg));
        }

        // Set up cancellation callback if token provided
        // Guard is dropped when function returns, stopping the watcher
        let _cancel_guard = args.cancellation_token.as_ref().map(|token| {
            token.on_cancel(move || {
                #[allow(unsafe_code)]
                unsafe {
                    let _ = ffi::cancel_function_call(id);
                }
            })
        });

        // Wait for result
        let result = receiver.recv();
        match result {
            Ok(callbacks::CallbackResult::Final(data)) => {
                let holder = CffiValueHolder::decode(&data[..])
                    .map_err(|e| BamlError::internal(format!("decode error: {e}")))?;
                T::baml_decode(&holder)
            }
            Ok(callbacks::CallbackResult::Partial(_)) => Err(BamlError::internal(
                "unexpected partial result in sync call",
            )),
            Ok(callbacks::CallbackResult::Error(e)) => Err(e),
            Err(_) => Err(BamlError::internal("callback channel closed")),
        }
    }

    /// Call a function with streaming results
    pub fn call_function_stream<TPartial, TFinal>(
        &self,
        name: &str,
        args: &FunctionArgs,
    ) -> Result<StreamingCall<TPartial, TFinal>, BamlError>
    where
        TPartial: BamlDecode + Send + 'static,
        TFinal: Clone + BamlDecode + Send + 'static,
    {
        let encoded = args.encode()?;
        let name_cstr =
            CString::new(name).map_err(|_| BamlError::internal("invalid function name"))?;

        let (id, receiver) = callbacks::create_callback();

        #[allow(unsafe_code)]
        let error_ptr = unsafe {
            ffi::call_function_stream_from_c(
                self.ptr,
                name_cstr.as_ptr(),
                encoded.as_ptr().cast::<i8>(),
                encoded.len(),
                id,
            )
            .map_err(|e| {
                callbacks::remove_callback(id);
                BamlError::internal(format!("Failed to load BAML library: {e}"))
            })?
        };

        if !error_ptr.is_null() {
            callbacks::remove_callback(id);
            #[allow(unsafe_code)]
            let error_msg = unsafe {
                let cstr = CStr::from_ptr(error_ptr.cast::<i8>());
                cstr.to_string_lossy().into_owned()
            };
            return Err(BamlError::internal(error_msg));
        }

        // Set up cancellation callback if token provided
        let cancel_guard = args.cancellation_token.as_ref().map(|token| {
            token.on_cancel(move || {
                #[allow(unsafe_code)]
                unsafe {
                    let _ = ffi::cancel_function_call(id);
                }
            })
        });

        Ok(StreamingCall::new(id, receiver, cancel_guard))
    }

    /// Call a function asynchronously (non-blocking)
    pub async fn call_function_async<T: BamlDecode>(
        &self,
        name: &str,
        args: &FunctionArgs,
    ) -> Result<T, BamlError> {
        let encoded = args.encode()?;
        let name_cstr =
            CString::new(name).map_err(|_| BamlError::internal("invalid function name"))?;

        let (id, receiver) = callbacks::create_async_callback();

        #[allow(unsafe_code)]
        let error_ptr = unsafe {
            ffi::call_function_from_c(
                self.ptr,
                name_cstr.as_ptr(),
                encoded.as_ptr().cast::<i8>(),
                encoded.len(),
                id,
            )
            .map_err(|e| {
                callbacks::remove_callback(id);
                BamlError::internal(format!("Failed to load BAML library: {e}"))
            })?
        };

        // Check for immediate error
        if !error_ptr.is_null() {
            callbacks::remove_callback(id);
            #[allow(unsafe_code)]
            let error_msg = unsafe {
                let cstr = CStr::from_ptr(error_ptr.cast::<i8>());
                cstr.to_string_lossy().into_owned()
            };
            return Err(BamlError::internal(error_msg));
        }

        // Set up cancellation callback if token provided
        // Guard is dropped when function returns, stopping the watcher
        let _cancel_guard = args.cancellation_token.as_ref().map(|token| {
            token.on_cancel(move || {
                #[allow(unsafe_code)]
                unsafe {
                    let _ = ffi::cancel_function_call(id);
                }
            })
        });

        // Await result (non-blocking)
        match receiver.recv().await {
            Ok(callbacks::CallbackResult::Final(data)) => {
                let holder = CffiValueHolder::decode(&data[..])
                    .map_err(|e| BamlError::internal(format!("decode error: {e}")))?;
                T::baml_decode(&holder)
            }
            Ok(callbacks::CallbackResult::Partial(_)) => Err(BamlError::internal(
                "unexpected partial result in async call",
            )),
            Ok(callbacks::CallbackResult::Error(e)) => Err(e),
            Err(_) => Err(BamlError::internal("callback channel closed")),
        }
    }

    /// Call a function with async streaming results
    pub fn call_function_stream_async<TPartial, TFinal>(
        &self,
        name: &str,
        args: &FunctionArgs,
    ) -> Result<AsyncStreamingCall<TPartial, TFinal>, BamlError>
    where
        TPartial: BamlDecode + Send + 'static,
        TFinal: Clone + BamlDecode + Send + 'static,
    {
        let encoded = args.encode()?;
        let name_cstr =
            CString::new(name).map_err(|_| BamlError::internal("invalid function name"))?;

        let (id, receiver) = callbacks::create_async_callback();

        #[allow(unsafe_code)]
        let error_ptr = unsafe {
            ffi::call_function_stream_from_c(
                self.ptr,
                name_cstr.as_ptr(),
                encoded.as_ptr().cast::<i8>(),
                encoded.len(),
                id,
            )
            .map_err(|e| {
                callbacks::remove_callback(id);
                BamlError::internal(format!("Failed to load BAML library: {e}"))
            })?
        };

        if !error_ptr.is_null() {
            callbacks::remove_callback(id);
            #[allow(unsafe_code)]
            let error_msg = unsafe {
                let cstr = CStr::from_ptr(error_ptr.cast::<i8>());
                cstr.to_string_lossy().into_owned()
            };
            return Err(BamlError::internal(error_msg));
        }

        // Set up cancellation callback if token provided
        let cancel_guard = args.cancellation_token.as_ref().map(|token| {
            token.on_cancel(move || {
                #[allow(unsafe_code)]
                unsafe {
                    let _ = ffi::cancel_function_call(id);
                }
            })
        });

        Ok(AsyncStreamingCall::new(id, receiver, cancel_guard))
    }

    /// Parse raw LLM output into typed result
    ///
    /// Given the name of a BAML function and the raw text response from an LLM,
    /// this method parses the response according to the function's output type.
    ///
    /// # Arguments
    /// * `function_name` - Name of the BAML function that defines the output
    ///   type
    /// * `llm_response` - Raw text response from the LLM
    ///
    /// # Example
    /// ```ignore
    /// let raw_response = "Hello, World!";
    /// let result: String = runtime.parse("SayHello", raw_response)?;
    /// ```
    pub fn parse<T: BamlDecode>(
        &self,
        function_name: &str,
        llm_response: &str,
        stream: bool,
    ) -> Result<T, BamlError> {
        // Build args using FunctionArgs with parse-specific fields
        let args = FunctionArgs::new().arg("text", llm_response);
        let args = if stream {
            args.arg("stream", true)
        } else {
            args
        };
        let encoded = args.encode()?;
        let name_cstr = CString::new(function_name)
            .map_err(|_| BamlError::internal("invalid function name"))?;

        let (id, receiver) = callbacks::create_callback();

        #[allow(unsafe_code)]
        let error_ptr = unsafe {
            ffi::call_function_parse_from_c(
                self.ptr,
                name_cstr.as_ptr(),
                encoded.as_ptr().cast::<i8>(),
                encoded.len(),
                id,
            )
            .map_err(|e| {
                callbacks::remove_callback(id);
                BamlError::internal(format!("Failed to load BAML library: {e}"))
            })?
        };

        // Check for immediate error
        if !error_ptr.is_null() {
            callbacks::remove_callback(id);
            #[allow(unsafe_code)]
            let error_msg = unsafe {
                let cstr = CStr::from_ptr(error_ptr.cast::<i8>());
                cstr.to_string_lossy().into_owned()
            };
            return Err(BamlError::internal(format!(
                "function parse error: {error_msg}"
            )));
        }

        // Wait for result
        match receiver.recv() {
            Ok(callbacks::CallbackResult::Final(data)) => {
                if stream {
                    Err(BamlError::internal("unexpected final result in parse call"))
                } else {
                    let holder = CffiValueHolder::decode(&data[..])
                        .map_err(|e| BamlError::internal(format!("decode error: {e}")))?;
                    T::baml_decode(&holder)
                }
            }
            Ok(callbacks::CallbackResult::Partial(data)) => {
                if stream {
                    let holder = CffiValueHolder::decode(&data[..])
                        .map_err(|e| BamlError::internal(format!("decode error: {e}")))?;
                    T::baml_decode(&holder)
                } else {
                    Err(BamlError::internal(
                        "unexpected partial result in parse call",
                    ))
                }
            }
            Ok(callbacks::CallbackResult::Error(e)) => Err(e),
            Err(_) => Err(BamlError::internal("callback channel closed")),
        }
    }

    // =========================================================================
    // Media Factory Methods
    // =========================================================================

    /// Create an Image from a URL
    pub fn new_image_from_url(&self, url: &str, mime_type: Option<&str>) -> Image {
        Image::from_url(self.ptr, url, mime_type)
    }

    /// Create an Image from base64-encoded data
    pub fn new_image_from_base64(&self, base64: &str, mime_type: Option<&str>) -> Image {
        Image::from_base64(self.ptr, base64, mime_type)
    }

    /// Create Audio from a URL
    pub fn new_audio_from_url(&self, url: &str, mime_type: Option<&str>) -> Audio {
        Audio::from_url(self.ptr, url, mime_type)
    }

    /// Create Audio from base64-encoded data
    pub fn new_audio_from_base64(&self, base64: &str, mime_type: Option<&str>) -> Audio {
        Audio::from_base64(self.ptr, base64, mime_type)
    }

    /// Create a PDF from a URL
    pub fn new_pdf_from_url(&self, url: &str, mime_type: Option<&str>) -> Pdf {
        Pdf::from_url(self.ptr, url, mime_type)
    }

    /// Create a PDF from base64-encoded data
    pub fn new_pdf_from_base64(&self, base64: &str, mime_type: Option<&str>) -> Pdf {
        Pdf::from_base64(self.ptr, base64, mime_type)
    }

    /// Create a Video from a URL
    pub fn new_video_from_url(&self, url: &str, mime_type: Option<&str>) -> Video {
        Video::from_url(self.ptr, url, mime_type)
    }

    /// Create a Video from base64-encoded data
    pub fn new_video_from_base64(&self, base64: &str, mime_type: Option<&str>) -> Video {
        Video::from_base64(self.ptr, base64, mime_type)
    }

    // =========================================================================
    // Collector Factory Methods
    // =========================================================================

    /// Create a new collector for telemetry
    pub fn new_collector(&self, name: &str) -> Collector {
        Collector::new(self.ptr, name)
    }

    // =========================================================================
    // TypeBuilder Factory Methods
    // =========================================================================

    /// Create a new `TypeBuilder` for dynamic type construction
    pub fn new_type_builder(&self) -> TypeBuilder {
        TypeBuilder::new(self.ptr)
    }
}

impl Drop for BamlRuntime {
    fn drop(&mut self) {
        #[allow(unsafe_code)]
        // Ignore errors during drop - the library should already be loaded at this point
        // and we can't do much about errors during cleanup anyway
        let _ = unsafe { ffi::destroy_baml_runtime(self.ptr) };
    }
}

/// Simple JSON encoding for maps
///
/// This is a minimal implementation to avoid adding `serde_json` as a
/// dependency. For simplicity, we assume keys and values don't contain
/// problematic characters that would require complex escaping beyond basic
/// escapes.
fn json_encode_map(map: &HashMap<String, String>) -> Result<String, BamlError> {
    serde_json::to_string(map)
        .map_err(|e| BamlError::internal(format!("failed to encode map: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Escape a string for JSON encoding
    fn json_escape_string(s: &str) -> String {
        serde_json::json!(s).to_string()
    }

    #[test]
    fn test_json_encode_empty_map() {
        let map: HashMap<String, String> = HashMap::new();
        let result = json_encode_map(&map).unwrap();
        assert_eq!(result, "{}");
    }

    #[test]
    fn test_json_encode_simple_map() {
        let mut map = HashMap::new();
        map.insert("key".to_string(), "value".to_string());
        let result = json_encode_map(&map).unwrap();
        assert_eq!(result, "{\"key\":\"value\"}");
    }

    #[test]
    fn test_json_escape_quotes() {
        let escaped = json_escape_string("hello \"world\"");
        assert_eq!(escaped, "hello \\\"world\\\"");
    }

    #[test]
    fn test_json_escape_backslash() {
        let escaped = json_escape_string("path\\to\\file");
        assert_eq!(escaped, "path\\\\to\\\\file");
    }

    #[test]
    fn test_json_escape_newlines() {
        let escaped = json_escape_string("line1\nline2\rline3");
        assert_eq!(escaped, "line1\\nline2\\rline3");
    }

    #[test]
    fn test_json_escape_tabs() {
        let escaped = json_escape_string("col1\tcol2");
        assert_eq!(escaped, "col1\\tcol2");
    }
}
