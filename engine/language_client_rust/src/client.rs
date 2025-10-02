use crate::{
    ffi,
    runtime::{RuntimeHandle, RuntimeHandleArc},
    types::{BamlValue, FromBamlValue},
    BamlContext, BamlError, BamlResult, FunctionResult, StreamState,
};
use baml_cffi::baml::cffi::{
    cffi_value_holder, cffi_value_raw_object::Object as RawObjectVariant, CffiEnvVar,
    CffiFunctionArguments, CffiMapEntry, CffiTypeName, CffiTypeNamespace, CffiValueClass,
    CffiValueEnum, CffiValueHolder, CffiValueList, CffiValueMap, CffiValueNull, CffiValueRawObject,
};
use baml_cffi::{rust::media_to_raw, DecodeFromBuffer};
use futures::{Stream, StreamExt};
use once_cell::sync::{Lazy, OnceCell};
use prost::Message;
use std::collections::HashMap;
use std::ffi::CString;
use std::os::raw::{c_char, c_void};
use std::path::Path;
use std::pin::Pin;
use std::slice;
use std::sync::{
    atomic::{AtomicU32, Ordering},
    Arc, Mutex,
};
use std::task::{Context as TaskContext, Poll};
use tokio::sync::{mpsc as async_mpsc, oneshot};

/// High-level BAML client for executing functions
#[derive(Clone, Debug)]
pub struct BamlClient {
    runtime: RuntimeHandleArc,
    callback_manager: CallbackManager,
}

// Ensure BamlClient is Send + Sync
unsafe impl Send for BamlClient {}
unsafe impl Sync for BamlClient {}

/// Manages async callbacks from the BAML runtime
#[derive(Clone, Debug)]
struct CallbackManager {
    registry: Arc<CallbackRegistry>,
}

#[derive(Debug)]
struct CallbackRegistry {
    next_id: AtomicU32,
    pending_calls: Mutex<HashMap<u32, oneshot::Sender<CallbackResult>>>,
    pending_streams: Mutex<HashMap<u32, async_mpsc::UnboundedSender<StreamEvent>>>,
}

#[derive(Debug)]
enum CallbackResult {
    Success { value: BamlValue },
    Error { error: BamlError },
}

#[derive(Debug)]
enum StreamEvent {
    Success { value: BamlValue, is_final: bool },
    Error { error: BamlError },
    Tick,
}

static CALLBACK_REGISTRY: Lazy<Arc<CallbackRegistry>> =
    Lazy::new(|| Arc::new(CallbackRegistry::new()));
static CALLBACKS_REGISTERED: OnceCell<()> = OnceCell::new();

impl CallbackManager {
    fn new() -> Self {
        CallbackManager::ensure_callbacks_registered();
        Self {
            registry: CALLBACK_REGISTRY.clone(),
        }
    }

    fn ensure_callbacks_registered() {
        CALLBACKS_REGISTERED.get_or_init(|| {
            ffi::register_callbacks(
                ffi_result_callback,
                ffi_error_callback,
                ffi_on_tick_callback,
            );
        });
    }

    fn get_next_id(&self) -> u32 {
        self.registry.next_id.fetch_add(1, Ordering::SeqCst)
    }

    fn register_call(&self, id: u32) -> oneshot::Receiver<CallbackResult> {
        self.registry.register_call(id)
    }

    fn register_stream(&self, id: u32) -> async_mpsc::UnboundedReceiver<StreamEvent> {
        self.registry.register_stream(id)
    }

    fn cancel_call(&self, id: u32) {
        self.registry.cancel_call(id);
    }

    fn cancel_stream(&self, id: u32) {
        self.registry.cancel_stream(id);
    }
}

impl CallbackRegistry {
    fn new() -> Self {
        Self {
            next_id: AtomicU32::new(1),
            pending_calls: Mutex::new(HashMap::new()),
            pending_streams: Mutex::new(HashMap::new()),
        }
    }

    fn register_call(&self, id: u32) -> oneshot::Receiver<CallbackResult> {
        let (tx, rx) = oneshot::channel();
        self.pending_calls.lock().unwrap().insert(id, tx);
        rx
    }

    fn register_stream(&self, id: u32) -> async_mpsc::UnboundedReceiver<StreamEvent> {
        let (tx, rx) = async_mpsc::unbounded_channel();
        self.pending_streams.lock().unwrap().insert(id, tx);
        rx
    }

    fn cancel_call(&self, id: u32) {
        self.pending_calls.lock().unwrap().remove(&id);
    }

    fn cancel_stream(&self, id: u32) {
        self.pending_streams.lock().unwrap().remove(&id);
    }

    fn handle_success(&self, id: u32, is_final: bool, value: BamlValue) {
        if let Some(sender) = self.take_stream_sender(id, is_final) {
            let _ = sender.send(StreamEvent::Success { value, is_final });
            return;
        }

        if is_final {
            if let Some(sender) = self.pending_calls.lock().unwrap().remove(&id) {
                let _ = sender.send(CallbackResult::Success { value });
            }
        }
    }

    fn handle_error(&self, id: u32, is_final: bool, error: BamlError) {
        if let Some(sender) = self.take_stream_sender(id, is_final) {
            let _ = sender.send(StreamEvent::Error { error });
            return;
        }

        if let Some(sender) = self.pending_calls.lock().unwrap().remove(&id) {
            let _ = sender.send(CallbackResult::Error { error });
        }
    }

    fn handle_tick(&self, id: u32) {
        if let Some(sender) = self.pending_streams.lock().unwrap().get(&id).cloned() {
            let _ = sender.send(StreamEvent::Tick);
        }
    }

    fn take_stream_sender(
        &self,
        id: u32,
        is_final: bool,
    ) -> Option<async_mpsc::UnboundedSender<StreamEvent>> {
        let mut streams = self.pending_streams.lock().unwrap();
        let sender = streams.get(&id).cloned();
        if is_final {
            streams.remove(&id);
        }
        sender
    }
}

fn decode_baml_value(ptr: *const i8, length: usize) -> BamlResult<BamlValue> {
    if ptr.is_null() || length == 0 {
        return Err(BamlError::Deserialization(
            "Received empty buffer from BAML runtime".to_string(),
        ));
    }

    BamlValue::from_c_buffer(ptr as *const c_char, length).map_err(|err| {
        BamlError::Deserialization(format!("Failed to decode value from runtime: {err}"))
    })
}

fn make_runtime_error(message: String) -> BamlError {
    BamlError::Runtime(anyhow::anyhow!(message))
}

fn take_error_message(ptr: *const c_void) -> String {
    if ptr.is_null() {
        return "Unknown error from BAML runtime".to_string();
    }

    unsafe {
        let boxed = Box::from_raw(ptr as *mut CString);
        boxed.to_string_lossy().to_string()
    }
}

fn read_utf8(ptr: *const i8, length: usize) -> String {
    if ptr.is_null() || length == 0 {
        return String::new();
    }

    unsafe {
        let bytes = slice::from_raw_parts(ptr as *const u8, length);
        String::from_utf8_lossy(bytes).to_string()
    }
}

extern "C" fn ffi_result_callback(call_id: u32, is_done: i32, content: *const i8, length: usize) {
    let is_final = is_done != 0;
    let registry = CALLBACK_REGISTRY.as_ref();

    match decode_baml_value(content, length) {
        Ok(value) => registry.handle_success(call_id, is_final, value),
        Err(error) => registry.handle_error(call_id, is_final, error),
    }
}

extern "C" fn ffi_error_callback(call_id: u32, is_done: i32, content: *const i8, length: usize) {
    let message = read_utf8(content, length);
    let is_final = is_done != 0;
    let registry = CALLBACK_REGISTRY.as_ref();
    registry.handle_error(call_id, is_final, make_runtime_error(message));
}

extern "C" fn ffi_on_tick_callback(call_id: u32) {
    let registry = CALLBACK_REGISTRY.as_ref();
    registry.handle_tick(call_id);
}

fn encode_baml_value(value: &BamlValue) -> BamlResult<CffiValueHolder> {
    use cffi_value_holder::Value as HolderValue;

    let encoded_value = match value {
        BamlValue::Null => HolderValue::NullValue(CffiValueNull {}),
        BamlValue::Bool(b) => HolderValue::BoolValue(*b),
        BamlValue::Int(i) => HolderValue::IntValue(*i),
        BamlValue::Float(f) => HolderValue::FloatValue(*f),
        BamlValue::String(s) => HolderValue::StringValue(s.clone()),
        BamlValue::List(items) => {
            let mut values = Vec::with_capacity(items.len());
            for item in items {
                values.push(encode_baml_value(item)?);
            }
            HolderValue::ListValue(CffiValueList {
                value_type: None,
                values,
            })
        }
        BamlValue::Map(entries) => {
            let mut encoded_entries = Vec::with_capacity(entries.len());
            for (key, item) in entries.iter() {
                encoded_entries.push(CffiMapEntry {
                    key: key.clone(),
                    value: Some(encode_baml_value(item)?),
                });
            }
            HolderValue::MapValue(CffiValueMap {
                key_type: None,
                value_type: None,
                entries: encoded_entries,
            })
        }
        BamlValue::Enum(name, variant) => HolderValue::EnumValue(CffiValueEnum {
            name: Some(CffiTypeName {
                namespace: CffiTypeNamespace::Internal.into(),
                name: name.clone(),
            }),
            value: variant.clone(),
            is_dynamic: false,
        }),
        BamlValue::Class(name, fields) => {
            let mut encoded_fields = Vec::with_capacity(fields.len());
            for (field_name, field_value) in fields.iter() {
                encoded_fields.push(CffiMapEntry {
                    key: field_name.clone(),
                    value: Some(encode_baml_value(field_value)?),
                });
            }
            HolderValue::ClassValue(CffiValueClass {
                name: Some(CffiTypeName {
                    namespace: CffiTypeNamespace::Internal.into(),
                    name: name.clone(),
                }),
                fields: encoded_fields,
            })
        }
        BamlValue::Media(media) => {
            let raw = media_to_raw(media);
            HolderValue::ObjectValue(CffiValueRawObject {
                object: Some(RawObjectVariant::Media(raw)),
            })
        }
    };

    Ok(CffiValueHolder {
        value: Some(encoded_value),
        r#type: None,
    })
}

impl BamlClient {
    /// Create a new BAML client from environment variables
    ///
    /// This will look for BAML configuration in environment variables
    /// and initialize the runtime accordingly.
    pub fn from_env() -> BamlResult<Self> {
        let env_vars: HashMap<String, String> = std::env::vars().collect();
        Self::from_file_content(".", &HashMap::new(), env_vars)
    }

    /// Create a new BAML client from a directory containing BAML source files
    #[cfg(not(target_arch = "wasm32"))]
    pub fn from_directory<P: AsRef<Path>>(
        path: P,
        env_vars: HashMap<String, String>,
    ) -> BamlResult<Self> {
        // Read all .baml files from the directory
        use std::fs;

        let mut files = HashMap::new();
        let dir_path = path.as_ref();

        fn read_baml_files(
            dir: &Path,
            files: &mut HashMap<String, String>,
            base_path: &Path,
        ) -> BamlResult<()> {
            let entries = fs::read_dir(dir).map_err(|e| {
                BamlError::Configuration(format!("Failed to read directory {:?}: {}", dir, e))
            })?;

            for entry in entries {
                let entry = entry.map_err(|e| {
                    BamlError::Configuration(format!("Failed to read directory entry: {}", e))
                })?;
                let path = entry.path();

                if path.is_dir() {
                    read_baml_files(&path, files, base_path)?;
                } else if path.extension().and_then(|s| s.to_str()) == Some("baml") {
                    let content = fs::read_to_string(&path).map_err(|e| {
                        BamlError::Configuration(format!("Failed to read file {:?}: {}", path, e))
                    })?;
                    let relative_path = path.strip_prefix(base_path).map_err(|e| {
                        BamlError::Configuration(format!(
                            "Failed to get relative path for {:?}: {}",
                            path, e
                        ))
                    })?;
                    files.insert(relative_path.to_string_lossy().to_string(), content);
                }
            }
            Ok(())
        }

        read_baml_files(dir_path, &mut files, dir_path)?;

        Self::from_file_content(dir_path.to_string_lossy().as_ref(), &files, env_vars)
    }

    /// Create a new BAML client from file contents
    pub fn from_file_content(
        root_path: &str,
        files: &HashMap<String, String>,
        env_vars: HashMap<String, String>,
    ) -> BamlResult<Self> {
        // Serialize files and env_vars to JSON for the C FFI
        let src_files_json = serde_json::to_string(files).map_err(|e| {
            BamlError::invalid_argument(format!("Failed to serialize files: {}", e))
        })?;
        let env_vars_json = serde_json::to_string(&env_vars).map_err(|e| {
            BamlError::invalid_argument(format!("Failed to serialize env_vars: {}", e))
        })?;

        // Create the BAML runtime via FFI
        // Convert strings to C strings
        let root_path_c = std::ffi::CString::new(root_path)
            .map_err(|e| BamlError::invalid_argument(format!("Invalid root_path: {}", e)))?;
        let src_files_json_c = std::ffi::CString::new(src_files_json)
            .map_err(|e| BamlError::invalid_argument(format!("Invalid src_files_json: {}", e)))?;
        let env_vars_json_c = std::ffi::CString::new(env_vars_json)
            .map_err(|e| BamlError::invalid_argument(format!("Invalid env_vars_json: {}", e)))?;

        let runtime_ptr = ffi::create_baml_runtime(
            root_path_c.as_ptr(),
            src_files_json_c.as_ptr(),
            env_vars_json_c.as_ptr(),
        );

        // Check if runtime creation failed (null pointer indicates error)
        if runtime_ptr.is_null() {
            return Err(BamlError::Runtime(anyhow::anyhow!(
                "Failed to create BAML runtime"
            )));
        }

        let callback_manager = CallbackManager::new();
        let runtime = Arc::new(RuntimeHandle::new(runtime_ptr));

        Ok(Self {
            runtime,
            callback_manager,
        })
    }

    /// Create a new BAML client with a pre-configured runtime pointer
    ///
    /// This is primarily for internal use where you already have a runtime pointer
    /// from the FFI interface.
    pub fn with_runtime_ptr(runtime_ptr: *const c_void) -> BamlResult<Self> {
        if runtime_ptr.is_null() {
            return Err(BamlError::Configuration(
                "Cannot create client from null runtime pointer".to_string(),
            ));
        }

        let callback_manager = CallbackManager::new();
        let runtime = Arc::new(RuntimeHandle::new(runtime_ptr));

        Ok(Self {
            runtime,
            callback_manager,
        })
    }

    /// Call a BAML function asynchronously
    pub async fn call_function<T>(&self, function_name: &str, context: BamlContext) -> BamlResult<T>
    where
        T: FromBamlValue,
    {
        let result = self.call_function_raw(function_name, context).await?;
        T::from_baml_value(result.data)
    }

    /// Call a BAML function and return the raw result
    pub async fn call_function_raw(
        &self,
        function_name: &str,
        context: BamlContext,
    ) -> BamlResult<FunctionResult> {
        self.bind_collectors(&context)?;
        let encoded_args = Self::encode_function_arguments(&context)?;

        // Get a unique ID for this call
        let call_id = self.callback_manager.get_next_id();

        // Register for the callback
        let callback_receiver = self.callback_manager.register_call(call_id);

        // Make the FFI call
        // Convert strings to C strings
        let function_name_c = std::ffi::CString::new(function_name)
            .map_err(|e| BamlError::invalid_argument(format!("Invalid function_name: {}", e)))?;
        let result_ptr = ffi::call_function_from_c(
            self.runtime.ptr(),
            function_name_c.as_ptr(),
            encoded_args.as_ptr() as *const c_char,
            encoded_args.len(),
            call_id,
        );

        // Check if the call failed (non-null pointer indicates error)
        if !result_ptr.is_null() {
            self.callback_manager.cancel_call(call_id);
            let message = take_error_message(result_ptr);
            return Err(make_runtime_error(message));
        }

        // Wait for the callback result
        let callback_result = match callback_receiver.await {
            Ok(result) => result,
            Err(_) => {
                self.callback_manager.cancel_call(call_id);
                return Err(make_runtime_error(
                    "Callback channel closed before response".to_string(),
                ));
            }
        };

        match callback_result {
            CallbackResult::Success { value } => {
                Ok(FunctionResult::new(value, call_id.to_string()))
            }
            CallbackResult::Error { error } => Err(error),
        }
    }

    /// Call a BAML function with streaming support
    pub async fn call_function_stream<T>(
        &self,
        function_name: &str,
        context: BamlContext,
    ) -> BamlResult<impl futures::Stream<Item = BamlResult<StreamState<T>>>>
    where
        T: FromBamlValue + Send + Sync + 'static,
    {
        let stream = self
            .call_function_stream_raw(function_name, context)
            .await?;
        let mut last_value: Option<crate::types::BamlValue> = None;
        Ok(stream.filter_map(move |result| {
            let output = match result {
                Ok(stream_state) => match stream_state {
                    StreamState::Partial(value) => {
                        let merged = crate::types::overlay_baml_value(last_value.clone(), value);
                        if crate::types::baml_value_has_data(&merged) {
                            last_value = Some(merged.clone());
                            Some(crate::types::with_partial_deserialization(|| {
                                T::from_baml_value(merged).map(StreamState::Partial)
                            }))
                        } else {
                            last_value = Some(merged);
                            None
                        }
                    }
                    StreamState::Final(value) => {
                        let merged = crate::types::overlay_baml_value(last_value.clone(), value);
                        last_value = None;
                        Some(T::from_baml_value(merged).map(StreamState::Final))
                    }
                },
                Err(e) => Some(Err(e)),
            };
            futures::future::ready(output)
        }))
    }

    /// Call a BAML function with streaming support, returning raw results
    pub async fn call_function_stream_raw(
        &self,
        function_name: &str,
        context: BamlContext,
    ) -> BamlResult<BamlStream> {
        self.bind_collectors(&context)?;
        let encoded_args = Self::encode_function_arguments(&context)?;

        // Get a unique ID for this call
        let call_id = self.callback_manager.get_next_id();

        // Register for stream events
        let stream_receiver = self.callback_manager.register_stream(call_id);

        // Make the FFI call
        // Convert strings to C strings
        let function_name_c = std::ffi::CString::new(function_name)
            .map_err(|e| BamlError::invalid_argument(format!("Invalid function_name: {}", e)))?;
        let result_ptr = ffi::call_function_stream_from_c(
            self.runtime.ptr(),
            function_name_c.as_ptr(),
            encoded_args.as_ptr() as *const c_char,
            encoded_args.len(),
            call_id,
        );

        // Check if the call failed (non-null pointer indicates error)
        if !result_ptr.is_null() {
            self.callback_manager.cancel_stream(call_id);
            let message = take_error_message(result_ptr);
            return Err(make_runtime_error(message));
        }

        Ok(BamlStream::new(stream_receiver))
    }

    /// Get the runtime pointer (for advanced use cases)
    pub fn runtime_ptr(&self) -> *const c_void {
        self.runtime.ptr()
    }

    fn bind_collectors(&self, context: &BamlContext) -> BamlResult<()> {
        for collector in &context.collectors {
            collector.bind_runtime(self.runtime.clone())?;
        }
        Ok(())
    }

    fn encode_function_arguments(context: &BamlContext) -> BamlResult<Vec<u8>> {
        if context.client_registry.is_some() {
            return Err(BamlError::Configuration(
                "Client registry overrides are not yet supported in the Rust client".to_string(),
            ));
        }

        let mut kwargs = Vec::with_capacity(context.args.len());
        for (key, value) in context.args.iter() {
            kwargs.push(CffiMapEntry {
                key: key.clone(),
                value: Some(encode_baml_value(value)?),
            });
        }

        let mut env = Vec::with_capacity(context.env_vars.len());
        for (key, value) in context.env_vars.iter() {
            env.push(CffiEnvVar {
                key: key.clone(),
                value: value.clone(),
            });
        }

        let collectors = context
            .collectors
            .iter()
            .map(|collector| collector.as_ref().to_cffi())
            .collect();

        let type_builder = context
            .type_builder
            .as_ref()
            .map(|builder| builder.to_cffi());

        let mut tags = Vec::with_capacity(context.tags.len());
        for (key, value) in context.tags.iter() {
            tags.push(CffiMapEntry {
                key: key.clone(),
                value: Some(CffiValueHolder {
                    value: Some(cffi_value_holder::Value::StringValue(value.clone())),
                    r#type: None,
                }),
            });
        }

        let args = CffiFunctionArguments {
            kwargs,
            client_registry: None,
            env,
            collectors,
            type_builder,
            tags,
        };

        let mut buffer = Vec::new();
        args.encode(&mut buffer).map_err(|err| {
            BamlError::Serialization(format!("Failed to encode function arguments: {err}"))
        })?;

        Ok(buffer)
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn encode_context_for_test(context: &BamlContext) -> BamlResult<Vec<u8>> {
        Self::encode_function_arguments(context)
    }
}

/// Stream wrapper for BAML function streaming results
pub struct BamlStream {
    receiver: async_mpsc::UnboundedReceiver<StreamEvent>,
}

impl BamlStream {
    fn new(receiver: async_mpsc::UnboundedReceiver<StreamEvent>) -> Self {
        Self { receiver }
    }
}

impl Stream for BamlStream {
    type Item = BamlResult<StreamState<BamlValue>>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut TaskContext<'_>) -> Poll<Option<Self::Item>> {
        loop {
            match self.receiver.poll_recv(cx) {
                Poll::Ready(Some(StreamEvent::Tick)) => continue,
                Poll::Ready(Some(StreamEvent::Success { value, is_final })) => {
                    let state = if is_final {
                        StreamState::Final(value)
                    } else {
                        StreamState::Partial(value)
                    };
                    return Poll::Ready(Some(Ok(state)));
                }
                Poll::Ready(Some(StreamEvent::Error { error })) => {
                    return Poll::Ready(Some(Err(error)));
                }
                Poll::Ready(None) => return Poll::Ready(None),
                Poll::Pending => return Poll::Pending,
            }
        }
    }
}

// BamlStream is our implementation of function result streaming

/// Builder for creating BAML clients with custom configuration
#[derive(Default)]
pub struct BamlClientBuilder {
    env_vars: HashMap<String, String>,
    root_path: Option<String>,
    files: HashMap<String, String>,
    directory: Option<std::path::PathBuf>,
}

impl BamlClientBuilder {
    /// Create a new client builder
    pub fn new() -> Self {
        Self::default()
    }

    /// Set an environment variable
    pub fn env_var<K: Into<String>, V: Into<String>>(mut self, key: K, value: V) -> Self {
        self.env_vars.insert(key.into(), value.into());
        self
    }

    /// Set multiple environment variables
    pub fn env_vars<I, K, V>(mut self, env_vars: I) -> Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<String>,
        V: Into<String>,
    {
        for (key, value) in env_vars {
            self.env_vars.insert(key.into(), value.into());
        }
        self
    }

    /// Set the root path for file content loading
    pub fn root_path<S: Into<String>>(mut self, path: S) -> Self {
        self.root_path = Some(path.into());
        self
    }

    /// Add a file with content
    pub fn file<K: Into<String>, V: Into<String>>(mut self, path: K, content: V) -> Self {
        self.files.insert(path.into(), content.into());
        self
    }

    /// Set multiple files
    pub fn files<I, K, V>(mut self, files: I) -> Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<String>,
        V: Into<String>,
    {
        for (path, content) in files {
            self.files.insert(path.into(), content.into());
        }
        self
    }

    /// Set directory to load BAML files from
    #[cfg(not(target_arch = "wasm32"))]
    pub fn directory<P: Into<std::path::PathBuf>>(mut self, path: P) -> Self {
        self.directory = Some(path.into());
        self
    }

    /// Build the client
    pub fn build(mut self) -> BamlResult<BamlClient> {
        // Add environment variables if none explicitly set
        if self.env_vars.is_empty() {
            self.env_vars = std::env::vars().collect();
        }

        #[cfg(not(target_arch = "wasm32"))]
        if let Some(directory) = self.directory {
            return BamlClient::from_directory(directory, self.env_vars);
        }

        if !self.files.is_empty() {
            let root_path = self.root_path.unwrap_or_else(|| ".".to_string());
            return BamlClient::from_file_content(&root_path, &self.files, self.env_vars);
        }

        BamlClient::from_env()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builder() {
        let builder = BamlClientBuilder::new()
            .env_var("TEST_VAR", "test_value")
            .root_path("/tmp");

        // We can't actually build without valid BAML files, but we can test the builder construction
        assert_eq!(
            builder.env_vars.get("TEST_VAR"),
            Some(&"test_value".to_string())
        );
    }
}
