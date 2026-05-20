//! Host-value dispatch infrastructure for the `call_host_value` sysop.
//!
//! This module provides two shared services:
//!
//! 1. **Dispatch function pointer** (`HostDispatchFn`) — a process-global
//!    callback installed by the bridge (via `bridge_cffi`) when the host
//!    language registers its dispatch function. The `call_host_value` sysop
//!    reads it to invoke the host callable.
//!
//! 2. **Call table** — a process-global map from `call_id: u32` to
//!    `CompletionHandle`. Inserted by `call_host_value` before dispatch;
//!    removed and resolved by `complete_host_call` (also `bridge_cffi`).
//!
//! These live in `sys_native` (not `bridge_cffi`) so that the sysop
//! implementation in `sys_native` can use them without introducing a circular
//! dependency (`bridge_cffi` already depends on `sys_native`).

use std::{
    collections::HashMap,
    sync::{
        RwLock,
        atomic::{AtomicU32, Ordering},
    },
};

use once_cell::sync::{Lazy, OnceCell};
use sys_types::{BexExternalValue, CompletionHandle, OpError};

/// C-compatible dispatch callback installed by the host bridge.
///
/// Called by `call_host_value` when BAML code invokes a `HostValue`. The
/// bridge decodes `args`, invokes the host callable, and resolves the
/// in-flight call via `complete_host_call`.
pub type HostDispatchFn =
    extern "C" fn(host_value_key: u64, call_id: u32, args: *const u8, length: usize);

static HOST_DISPATCH_FN: OnceCell<HostDispatchFn> = OnceCell::new();

/// Install the dispatch callback. First-call-wins; subsequent calls are
/// silently ignored (consistent with `register_callback` semantics).
pub fn set_dispatch_fn(f: HostDispatchFn) {
    let _ = HOST_DISPATCH_FN.set(f);
}

/// Invoke the registered dispatch callback.
///
/// Returns `true` if the callback was installed and fired, `false` if
/// no bridge has registered a dispatcher yet. The caller is responsible
/// for resolving the in-flight `CompletionHandle` on `false`.
pub fn fire_dispatch(host_value_key: u64, call_id: u32, args: &[u8]) -> bool {
    match HOST_DISPATCH_FN.get() {
        Some(f) => {
            tokio::task::block_in_place(|| {
                f(host_value_key, call_id, args.as_ptr(), args.len());
            });
            true
        }
        None => {
            tracing::warn!(
                "call_host_value invoked before register_host_dispatch_callback: \
                 no host dispatch fn registered"
            );
            false
        }
    }
}

// ============================================================================
// In-flight call table
// ============================================================================

static TABLE: Lazy<RwLock<HashMap<u32, CompletionHandle>>> =
    Lazy::new(|| RwLock::new(HashMap::new()));

static NEXT_CALL_ID: AtomicU32 = AtomicU32::new(1);

/// Allocate a fresh call id that is unique across the process lifetime.
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
pub fn complete_with_value(call_id: u32, value: BexExternalValue) {
    if let Some(c) = take(call_id) {
        c.complete(Ok(value));
    } else {
        tracing::warn!("complete_host_call for unknown call id {call_id}");
    }
}

/// Complete an in-flight call with an error.
pub fn complete_with_error(call_id: u32, err: OpError) {
    if let Some(c) = take(call_id) {
        c.complete(Err(err));
    } else {
        tracing::warn!("complete_host_call(error) for unknown call id {call_id}");
    }
}
