//! Node.js `BamlCallContext` class for cancelling in-flight BAML function calls.

use std::sync::Mutex;

use napi_derive::napi;

/// A call context for cancelling BAML function calls.
#[napi]
pub struct BamlCallContext {
    aborted: Mutex<bool>,
    active_call_ids: Mutex<Vec<u64>>,
}

#[napi]
impl BamlCallContext {
    #[napi(constructor)]
    pub fn new() -> Self {
        Self {
            aborted: Mutex::new(false),
            active_call_ids: Mutex::new(Vec::new()),
        }
    }

    #[napi]
    pub fn abort(&self) {
        if let Ok(mut aborted) = self.aborted.lock() {
            if *aborted {
                return;
            }
            *aborted = true;
        }
        let call_ids = self
            .active_call_ids
            .lock()
            .map(|ids| ids.clone())
            .unwrap_or_default();
        for call_id in call_ids {
            bridge_cffi::cancel_function_call_by_id(call_id);
        }
    }

    #[napi(getter)]
    pub fn aborted(&self) -> bool {
        self.aborted.lock().map(|aborted| *aborted).unwrap_or(true)
    }

    #[napi(js_name = "_attachCallId")]
    pub fn attach_call_id(&self, call_id: String) -> napi::Result<()> {
        let call_id = call_id.parse::<u64>().map_err(|_| {
            napi::Error::new(
                napi::Status::InvalidArg,
                "callId must be a decimal uint64 string",
            )
        })?;
        let newly_attached = if let Ok(mut ids) = self.active_call_ids.lock() {
            if ids.contains(&call_id) {
                false
            } else {
                ids.push(call_id);
                true
            }
        } else {
            false
        };
        if self.aborted() && newly_attached {
            bridge_cffi::cancel_function_call_by_id(call_id);
        }
        Ok(())
    }

    #[napi(js_name = "_detachCallId")]
    pub fn detach_call_id(&self, call_id: String) -> napi::Result<()> {
        let call_id = call_id.parse::<u64>().map_err(|_| {
            napi::Error::new(
                napi::Status::InvalidArg,
                "callId must be a decimal uint64 string",
            )
        })?;
        if let Ok(mut ids) = self.active_call_ids.lock() {
            ids.retain(|id| *id != call_id);
        }
        Ok(())
    }
}
