use std::{
    collections::HashMap,
    ffi::{c_char, c_int},
    panic::{self, AssertUnwindSafe},
    sync::{mpsc, Arc, Mutex, OnceLock},
};

use crate::{
    error::BamlError,
    ffi::bindings,
    raw_objects::{Collector, FunctionLog},
};

/// Result sent via callback channel
pub enum CallbackResult {
    /// Partial streaming result (`is_done` = 0)
    Partial(Vec<u8>),
    /// Final result (`is_done` = 1)
    Final(Vec<u8>),
    /// Error occurred
    Error(BamlError),
}

/// On-tick callback type invoked for each SSE streaming chunk with the current FunctionLog.
pub type OnTickCallback = Arc<dyn Fn(&FunctionLog) + Send + Sync>;

/// Internal bundle of an on-tick callback with its auto-created collector.
///
/// Created by `BamlRuntime::call_function_stream` when an `OnTickCallback` is
/// present on `FunctionArgs`. On each SSE chunk the collector is queried for the
/// latest `FunctionLog` and passed to the callback.
pub(crate) struct OnTickData {
    pub callback: OnTickCallback,
    pub collector: Collector,
}

impl Clone for OnTickData {
    fn clone(&self) -> Self {
        Self {
            callback: self.callback.clone(),
            collector: self.collector.clone(),
        }
    }
}

/// Sync callback data
struct SyncCallbackData {
    sender: mpsc::Sender<CallbackResult>,
    on_tick: Option<OnTickData>,
}

/// Async callback data
struct AsyncCallbackData {
    sender: async_channel::Sender<CallbackResult>,
    on_tick: Option<OnTickData>,
}

/// Callback data - either sync or async
enum CallbackData {
    Sync(SyncCallbackData),
    Async(AsyncCallbackData),
}

/// Global callback storage
static CALLBACKS: OnceLock<Mutex<HashMap<u32, CallbackData>>> = OnceLock::new();

/// Next callback ID counter for sequential generation.
static NEXT_ID: OnceLock<Mutex<u32>> = OnceLock::new();

fn get_callbacks() -> &'static Mutex<HashMap<u32, CallbackData>> {
    CALLBACKS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn get_next_id() -> &'static Mutex<u32> {
    NEXT_ID.get_or_init(|| Mutex::new(1))
}

/// Register callbacks with FFI layer. Must be called once at startup.
///
/// Returns an error if the library cannot be loaded.
pub fn initialize_callbacks() -> Result<(), baml_sys::BamlSysError> {
    static INIT_ERROR: OnceLock<Option<String>> = OnceLock::new();

    let error_msg = INIT_ERROR.get_or_init(|| {
        #[allow(unsafe_code)]
        match unsafe {
            bindings::register_callbacks(result_callback, error_callback, on_tick_callback)
        } {
            Ok(()) => None,
            Err(e) => Some(e.to_string()),
        }
    });

    match error_msg {
        None => Ok(()),
        Some(msg) => Err(baml_sys::BamlSysError::LibraryNotFound {
            searched_paths: vec![std::path::PathBuf::from(msg.clone())],
        }),
    }
}

/// Allocate a unique callback ID, skipping 0 and any IDs still in use.
fn allocate_callback_id(callbacks: &mut HashMap<u32, CallbackData>) -> u32 {
    let mut next_id = get_next_id().lock().unwrap();

    let mut id = *next_id;
    loop {
        if id != 0 && !callbacks.contains_key(&id) {
            break;
        }
        id = id.wrapping_add(1);
        assert!(id != *next_id, "callback ID space exhausted");
    }
    *next_id = id.wrapping_add(1);
    id
}

/// Create a new sync callback ID and channel.
pub fn create_callback() -> (u32, mpsc::Receiver<CallbackResult>) {
    create_callback_with_on_tick(None)
}

/// Create a new sync callback ID and channel with optional on_tick data.
pub(crate) fn create_callback_with_on_tick(
    on_tick: Option<OnTickData>,
) -> (u32, mpsc::Receiver<CallbackResult>) {
    let (sender, receiver) = mpsc::channel();

    let mut callbacks = get_callbacks().lock().unwrap();
    let id = allocate_callback_id(&mut callbacks);

    callbacks.insert(id, CallbackData::Sync(SyncCallbackData { sender, on_tick }));
    drop(callbacks);

    (id, receiver)
}

/// Create a new async callback ID and channel.
pub fn create_async_callback() -> (u32, async_channel::Receiver<CallbackResult>) {
    create_async_callback_with_on_tick(None)
}

/// Create a new async callback ID and channel with optional on_tick data.
pub(crate) fn create_async_callback_with_on_tick(
    on_tick: Option<OnTickData>,
) -> (u32, async_channel::Receiver<CallbackResult>) {
    let (sender, receiver) = async_channel::unbounded();

    let mut callbacks = get_callbacks().lock().unwrap();
    let id = allocate_callback_id(&mut callbacks);

    callbacks.insert(
        id,
        CallbackData::Async(AsyncCallbackData { sender, on_tick }),
    );
    drop(callbacks);

    (id, receiver)
}

/// Remove callback data for a given ID
pub fn remove_callback(id: u32) {
    let mut callbacks = get_callbacks().lock().unwrap();
    callbacks.remove(&id);
}

/// Result callback invoked by FFI
extern "C" fn result_callback(call_id: u32, is_done: c_int, content: *const c_char, length: usize) {
    let data = if !content.is_null() && length > 0 {
        #[allow(unsafe_code)]
        let slice = unsafe { std::slice::from_raw_parts(content.cast::<u8>(), length) };
        slice.to_vec()
    } else {
        Vec::new()
    };

    let result = if is_done != 0 {
        CallbackResult::Final(data)
    } else {
        CallbackResult::Partial(data)
    };

    let callbacks = get_callbacks().lock().unwrap();
    if let Some(cb_data) = callbacks.get(&call_id) {
        match cb_data {
            CallbackData::Sync(sync_data) => {
                let _ = sync_data.sender.send(result);
            }
            CallbackData::Async(async_data) => {
                let _ = async_data.sender.send_blocking(result);
            }
        }
    }

    drop(callbacks);
    if is_done != 0 {
        remove_callback(call_id);
    }
}

/// Error callback invoked by FFI
extern "C" fn error_callback(call_id: u32, _is_done: c_int, content: *const c_char, length: usize) {
    let error_msg = if !content.is_null() && length > 0 {
        #[allow(unsafe_code)]
        let slice = unsafe { std::slice::from_raw_parts(content.cast::<u8>(), length) };
        String::from_utf8_lossy(slice).into_owned()
    } else {
        "Unknown error".to_string()
    };

    let callbacks = get_callbacks().lock().unwrap();
    if let Some(cb_data) = callbacks.get(&call_id) {
        let error = CallbackResult::Error(BamlError::internal(error_msg));
        match cb_data {
            CallbackData::Sync(sync_data) => {
                let _ = sync_data.sender.send(error);
            }
            CallbackData::Async(async_data) => {
                let _ = async_data.sender.send_blocking(error);
            }
        }
    }

    drop(callbacks);
    remove_callback(call_id);
}

/// On-tick callback for streaming updates.
///
/// Invoked by the CFFI layer on each SSE streaming chunk. Queries the bundled
/// collector for the latest FunctionLog and passes it to the user's callback.
/// Panics in user code are caught to prevent unwinding across the FFI boundary.
extern "C" fn on_tick_callback(call_id: u32) {
    let callbacks = get_callbacks().lock().unwrap();
    if let Some(cb_data) = callbacks.get(&call_id) {
        let on_tick = match cb_data {
            CallbackData::Sync(d) => d.on_tick.clone(),
            CallbackData::Async(d) => d.on_tick.clone(),
        };
        drop(callbacks);

        if let Some(data) = on_tick {
            if let Some(log) = data.collector.last() {
                let _ = panic::catch_unwind(AssertUnwindSafe(|| {
                    (data.callback)(&log);
                }));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_callback_id_generation() {
        let (id1, _rx1) = create_callback();
        let (id2, _rx2) = create_callback();
        let (id3, _rx3) = create_callback();

        assert_ne!(id1, id2);
        assert_ne!(id2, id3);
        assert_ne!(id1, id3);

        remove_callback(id1);
        remove_callback(id2);
        remove_callback(id3);
    }

    #[test]
    fn test_callback_removal() {
        let (id, _rx) = create_callback();

        {
            let callbacks = get_callbacks().lock().unwrap();
            assert!(callbacks.contains_key(&id));
        }

        remove_callback(id);

        {
            let callbacks = get_callbacks().lock().unwrap();
            assert!(!callbacks.contains_key(&id));
        }
    }

    #[test]
    fn test_async_callback_id_generation() {
        let (id1, _rx1) = create_async_callback();
        let (id2, _rx2) = create_async_callback();
        let (id3, _rx3) = create_async_callback();

        assert_ne!(id1, id2);
        assert_ne!(id2, id3);
        assert_ne!(id1, id3);

        remove_callback(id1);
        remove_callback(id2);
        remove_callback(id3);
    }

    #[test]
    fn test_mixed_sync_async_callbacks() {
        let (sync_id, _rx_sync) = create_callback();
        let (async_id, _rx_async) = create_async_callback();
        let (sync_id2, _rx_sync2) = create_callback();

        assert_ne!(sync_id, async_id);
        assert_ne!(async_id, sync_id2);
        assert_ne!(sync_id, sync_id2);

        remove_callback(sync_id);
        remove_callback(async_id);
        remove_callback(sync_id2);
    }

    #[test]
    fn test_on_tick_callback_none() {
        let (id, _rx) = create_callback_with_on_tick(None);
        on_tick_callback(id);
        remove_callback(id);
    }
}
