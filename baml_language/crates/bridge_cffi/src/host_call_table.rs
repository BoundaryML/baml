//! In-flight host-call completion table.
//!
//! Keyed by the `call_id: u32` exposed across the FFI. Inserted by the
//! `call_host_value` sysop when it calls `SysOpResult::pending`; removed
//! and fired by `complete_host_call`.

use bex_project::BexExternalValue;
use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::sync::RwLock;
use std::sync::atomic::{AtomicU32, Ordering};
use sys_types::{CompletionHandle, OpError};

static TABLE: Lazy<RwLock<HashMap<u32, CompletionHandle>>> =
    Lazy::new(|| RwLock::new(HashMap::new()));

static NEXT_CALL_ID: AtomicU32 = AtomicU32::new(1);

/// Allocate a fresh call-id that is unique across the process lifetime.
///
/// Zero is reserved as "invalid"; wrap-around skips 0.
pub fn next_call_id() -> u32 {
    loop {
        let id = NEXT_CALL_ID.fetch_add(1, Ordering::Relaxed);
        if id != 0 {
            return id;
        }
    }
}

/// Register a `CompletionHandle` for an in-flight host call.
pub fn insert(call_id: u32, completion: CompletionHandle) {
    TABLE.write().unwrap().insert(call_id, completion);
}

/// Remove and return the `CompletionHandle` for the given call id, if any.
pub fn take(call_id: u32) -> Option<CompletionHandle> {
    TABLE.write().unwrap().remove(&call_id)
}

/// Complete an in-flight call with a successful value.
///
/// If `call_id` is not found (e.g. already completed or cancelled),
/// logs a diagnostic and does nothing.
pub fn complete_with_value(call_id: u32, value: BexExternalValue) {
    if let Some(c) = take(call_id) {
        c.complete(Ok(value));
    } else {
        eprintln!("BAML internal: complete_host_call for unknown id {call_id}");
    }
}

/// Complete an in-flight call with an error.
///
/// If `call_id` is not found, logs a diagnostic and does nothing.
pub fn complete_with_error(call_id: u32, err: OpError) {
    if let Some(c) = take(call_id) {
        c.complete(Err(err));
    } else {
        eprintln!("BAML internal: complete_host_call(error) for unknown id {call_id}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sys_types::{SysOp, SysOpResult};

    #[tokio::test]
    async fn insert_take_round_trip() {
        // BamlSysShell is used as a convenient placeholder for the SysOp
        // argument. Phase 4 will introduce SysOp::BamlHostCallHostValue.
        let (_result, completion) = SysOpResult::pending(SysOp::BamlSysShell);
        let id = next_call_id();
        insert(id, completion);
        let taken = take(id).expect("take must yield");
        taken.complete(Ok(BexExternalValue::Int(7)));
    }

    #[test]
    fn unknown_id_does_not_panic() {
        complete_with_value(u32::MAX, BexExternalValue::Null);
    }
}
