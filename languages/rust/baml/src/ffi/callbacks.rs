use std::collections::HashMap;
use std::ffi::c_int;
use std::sync::{mpsc, Mutex, OnceLock};

use crate::error::BamlError;
use crate::ffi::bindings;

/// Result sent via callback channel
pub enum CallbackResult {
    /// Partial streaming result (is_done = 0)
    Partial(Vec<u8>),
    /// Final result (is_done = 1)
    Final(Vec<u8>),
    /// Error occurred
    Error(BamlError),
}

/// Callback data stored per call ID
struct CallbackData {
    sender: mpsc::Sender<CallbackResult>,
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
pub fn initialize_callbacks() {
    // Only register once
    static INITIALIZED: OnceLock<()> = OnceLock::new();
    INITIALIZED.get_or_init(|| {
        #[allow(unsafe_code)]
        unsafe {
            bindings::register_callbacks(result_callback, error_callback, on_tick_callback);
        }
    });
}

/// Create a new callback ID and channel.
///
/// Uses sequential IDs with collision checking to ensure uniqueness even if
/// IDs wrap around while old callbacks are still pending.
pub fn create_callback() -> (u32, mpsc::Receiver<CallbackResult>) {
    let (sender, receiver) = mpsc::channel();

    let mut callbacks = get_callbacks().lock().unwrap();
    let mut next_id = get_next_id().lock().unwrap();

    // Find an unused ID, skipping 0 and any IDs still in use
    let mut id = *next_id;
    loop {
        if id != 0 && !callbacks.contains_key(&id) {
            break;
        }
        id = id.wrapping_add(1);
        if id == *next_id {
            // We've wrapped all the way around - this should never happen
            // as it would require 2^32 simultaneous pending callbacks
            panic!("callback ID space exhausted");
        }
    }
    *next_id = id.wrapping_add(1);

    callbacks.insert(id, CallbackData { sender });
    drop(callbacks);
    drop(next_id);

    (id, receiver)
}

/// Remove callback data for a given ID
pub fn remove_callback(id: u32) {
    let mut callbacks = get_callbacks().lock().unwrap();
    callbacks.remove(&id);
}

/// Result callback invoked by FFI
extern "C" fn result_callback(call_id: u32, is_done: c_int, content: *const i8, length: usize) {
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
        // Ignore send errors - receiver may have been dropped
        let _ = cb_data.sender.send(result);
    }

    // Clean up on final result
    drop(callbacks);
    if is_done != 0 {
        remove_callback(call_id);
    }
}

/// Error callback invoked by FFI
extern "C" fn error_callback(call_id: u32, _is_done: c_int, content: *const i8, length: usize) {
    let error_msg = if !content.is_null() && length > 0 {
        #[allow(unsafe_code)]
        let slice = unsafe { std::slice::from_raw_parts(content.cast::<u8>(), length) };
        String::from_utf8_lossy(slice).into_owned()
    } else {
        "Unknown error".to_string()
    };

    let callbacks = get_callbacks().lock().unwrap();
    if let Some(cb_data) = callbacks.get(&call_id) {
        let _ = cb_data
            .sender
            .send(CallbackResult::Error(BamlError::internal(error_msg)));
    }

    drop(callbacks);
    remove_callback(call_id);
}

/// On-tick callback for streaming updates
extern "C" fn on_tick_callback(_call_id: u32) {
    // Currently unused - can be extended for streaming progress
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_callback_id_generation() {
        let (id1, _rx1) = create_callback();
        let (id2, _rx2) = create_callback();
        let (id3, _rx3) = create_callback();

        // IDs should be unique and sequential
        assert_ne!(id1, id2);
        assert_ne!(id2, id3);
        assert_ne!(id1, id3);

        // Clean up
        remove_callback(id1);
        remove_callback(id2);
        remove_callback(id3);
    }

    #[test]
    fn test_callback_removal() {
        let (id, _rx) = create_callback();

        // Should exist
        {
            let callbacks = get_callbacks().lock().unwrap();
            assert!(callbacks.contains_key(&id));
        }

        // Remove it
        remove_callback(id);

        // Should not exist
        {
            let callbacks = get_callbacks().lock().unwrap();
            assert!(!callbacks.contains_key(&id));
        }
    }
}
