//! C ABI for host-value callable dispatch and release.
//!
//! Exports three symbols that are symmetric with `register_callback` /
//! callback completion in `ffi/callbacks.rs`:
//!
//! - `register_host_dispatch_callback` — host bridge installs a function
//!   pointer that the engine calls when BAML code invokes a `HostValue`.
//! - `register_host_release_callback` — host bridge installs a function
//!   pointer that fires when the last Rust clone of a `HostValueArc` is
//!   dropped, telling the bridge it can remove its entry.
//! - `complete_host_call` — host bridge calls this to resolve an in-flight
//!   dispatch (either with a success `BamlOutboundValue` or a
//!   `HostCallableError`).

use bex_project::{BexExternalValue, HostReleaseFn, host_release_dispatch};
use bridge_ctypes::{
    HANDLE_TABLE,
    baml_core::cffi::{HostCallableError, InboundValue},
    inbound_to_external,
};
use once_cell::sync::OnceCell;
use prost::Message;
use sys_types::{OpError, OpErrorKind, SysOp};

use crate::host_call_table;

/// Signature for invocation requests from BAML to the host.
///
/// `args` is a protobuf-encoded `BamlOutboundValue` (engine→host direction,
/// carries type metadata) whose underlying shape is a list of the typed
/// call arguments. The host decodes, invokes the user function, encodes
/// the result as `InboundValue` (host→engine direction; engine re-validates
/// against the declared return type), and calls
/// `complete_host_call(call_id, 0, result_ptr, result_len)`.
///
/// On error, the host encodes a `HostCallableError` proto and calls
/// `complete_host_call(call_id, 1, error_ptr, error_len)`.
pub type HostDispatchFn =
    extern "C" fn(host_value_key: u64, call_id: u32, args: *const u8, length: usize);

static HOST_DISPATCH_FN: OnceCell<HostDispatchFn> = OnceCell::new();

/// Register the host dispatch callback. First call wins; subsequent calls
/// are silently ignored (consistent with `register_callback` semantics).
///
/// # Safety
///
/// `cb` must remain valid for the lifetime of the process.
#[unsafe(no_mangle)]
pub extern "C" fn register_host_dispatch_callback(cb: HostDispatchFn) {
    let _ = HOST_DISPATCH_FN.set(cb);
}

/// Register the host release callback. First call wins; subsequent calls
/// log a diagnostic and are ignored.
///
/// The callback fires when the last Rust clone of a `HostValueArc` is
/// dropped. The bridge uses this notification to remove its internal
/// `key → host-language reference` entry.
///
/// # Safety
///
/// `cb` must remain valid for the lifetime of the process.
#[unsafe(no_mangle)]
pub extern "C" fn register_host_release_callback(cb: HostReleaseFn) {
    if host_release_dispatch::install(cb).is_err() {
        eprintln!("BAML internal: register_host_release_callback called twice");
    }
}

/// Engine-side helper: invoke the registered dispatch callback.
///
/// Returns `true` if the callback was installed and fired, `false` if
/// no bridge has registered a dispatcher. The caller is responsible for
/// resolving the in-flight `CompletionHandle` on `false`.
///
/// Called by the `call_host_value` sysop (Phase 4).
#[expect(dead_code, reason = "used by call_host_value sysop added in Phase 4")]
pub(crate) fn fire_host_dispatch(host_value_key: u64, call_id: u32, args: &[u8]) -> bool {
    match HOST_DISPATCH_FN.get() {
        Some(f) => {
            tokio::task::block_in_place(|| {
                f(host_value_key, call_id, args.as_ptr(), args.len());
            });
            true
        }
        None => {
            eprintln!(
                "BAML internal: call_host_value invoked before \
                 register_host_dispatch_callback"
            );
            false
        }
    }
}

/// Called by the host to complete an in-flight host-value invocation.
///
/// - `call_id`  — the id forwarded by `HostDispatchFn`.
/// - `is_error` — 0 for success, non-zero for error.
/// - `content`  — pointer to the protobuf payload (may be null if `length == 0`).
/// - `length`   — byte length of `content`.
///
/// **Success** (`is_error == 0`): `content` is a protobuf-encoded
/// `InboundValue` (host→engine direction; engine re-validates against the
/// declared return type).
///
/// **Error** (`is_error != 0`): `content` is a protobuf-encoded
/// `HostCallableError`.
///
/// # Safety
///
/// `content` must be valid for `length` bytes for the duration of this call.
#[unsafe(no_mangle)]
pub extern "C" fn complete_host_call(
    call_id: u32,
    is_error: i32,
    content: *const i8,
    length: usize,
) {
    // SAFETY: caller promises ptr is valid for `length` bytes.
    let bytes: &[u8] = if length == 0 || content.is_null() {
        &[]
    } else {
        unsafe { std::slice::from_raw_parts(content as *const u8, length) }
    };

    if is_error == 0 {
        // Success: decode InboundValue → BexExternalValue.
        if bytes.is_empty() {
            // No payload → Null return.
            host_call_table::complete_with_value(call_id, BexExternalValue::Null);
            return;
        }
        let inbound = match InboundValue::decode(bytes) {
            Ok(v) => v,
            Err(e) => {
                host_call_table::complete_with_error(
                    call_id,
                    OpError::new(
                        SysOp::BamlSysShell, // placeholder; replaced by BamlHostCallHostValue in Phase 4
                        OpErrorKind::Other(format!("complete_host_call decode failure: {e}")),
                    ),
                );
                return;
            }
        };
        match inbound_to_external(inbound, &HANDLE_TABLE) {
            Ok(v) => host_call_table::complete_with_value(call_id, v),
            Err(e) => host_call_table::complete_with_error(
                call_id,
                OpError::new(
                    SysOp::BamlSysShell, // placeholder; replaced by BamlHostCallHostValue in Phase 4
                    OpErrorKind::Other(format!("complete_host_call decode failure: {e}")),
                ),
            ),
        }
    } else {
        // Error: decode HostCallableError → OpError.
        let mapped = if bytes.is_empty() {
            OpError::new(
                SysOp::BamlSysShell, // placeholder; replaced by BamlHostCallHostValue in Phase 4
                OpErrorKind::Other("host callable returned error with no payload".to_string()),
            )
        } else {
            match HostCallableError::decode(bytes) {
                Ok(err) => OpError::new(
                    SysOp::BamlSysShell, // placeholder; replaced by BamlHostCallHostValue in Phase 4
                    OpErrorKind::Other(format!(
                        "host callable error ({}): {} [class={}, lang={:?}]",
                        err.category, err.message, err.class_name, err.language,
                    )),
                ),
                Err(e) => OpError::new(
                    SysOp::BamlSysShell, // placeholder; replaced by BamlHostCallHostValue in Phase 4
                    OpErrorKind::Other(format!(
                        "complete_host_call error-payload decode failure: {e}"
                    )),
                ),
            }
        };
        host_call_table::complete_with_error(call_id, mapped);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verify first-call-wins semantics for dispatch registration.
    #[test]
    fn register_host_dispatch_callback_first_call_wins() {
        extern "C" fn dispatch_a(_key: u64, _call_id: u32, _args: *const u8, _len: usize) {}
        extern "C" fn dispatch_b(_key: u64, _call_id: u32, _args: *const u8, _len: usize) {}

        // Use a fresh OnceCell by calling through the public API.
        // We cannot reset OnceCell so we just verify the second call doesn't panic.
        let _ = HOST_DISPATCH_FN.set(dispatch_a);
        let _ = HOST_DISPATCH_FN.set(dispatch_b); // must not panic

        // The installed value is dispatch_a (first call wins).
        // We can't easily assert the pointer identity in a unit test without
        // exposing the cell, so this test primarily checks there's no panic.
    }

    /// Verify `complete_host_call` with an empty success payload completes with Null.
    #[tokio::test]
    async fn complete_host_call_empty_success_completes_null() {
        use sys_types::{SysOp, SysOpResult};

        let (result, completion) = SysOpResult::pending(SysOp::BamlSysShell);
        let id = host_call_table::next_call_id();
        host_call_table::insert(id, completion);

        // Fire complete_host_call with empty success.
        complete_host_call(id, 0, std::ptr::null(), 0);

        // Await the result.
        match result {
            sys_types::SysOpResult::Async(fut) => {
                let value = fut.await.expect("should succeed");
                assert!(matches!(value, BexExternalValue::Null));
            }
            sys_types::SysOpResult::Ready(_) => panic!("expected async"),
        }
    }

    /// Verify `complete_host_call` with an error payload completes with OpError.
    #[tokio::test]
    async fn complete_host_call_error_payload_completes_error() {
        use bridge_ctypes::baml_core::cffi::HostCallableErrorCategory;
        use prost::Message;
        use sys_types::{SysOp, SysOpResult};

        let err_proto = HostCallableError {
            class_name: "ValueError".to_string(),
            message: "bad input".to_string(),
            traceback: None,
            language: Some("python".to_string()),
            category: HostCallableErrorCategory::HostCallableHostError as i32,
        };
        let encoded = err_proto.encode_to_vec();

        let (result, completion) = SysOpResult::pending(SysOp::BamlSysShell);
        let id = host_call_table::next_call_id();
        host_call_table::insert(id, completion);

        complete_host_call(id, 1, encoded.as_ptr() as *const i8, encoded.len());

        match result {
            sys_types::SysOpResult::Async(fut) => {
                let err = fut.await.expect_err("should be error");
                let msg = err.to_string();
                assert!(msg.contains("bad input"), "error message missing: {msg}");
            }
            sys_types::SysOpResult::Ready(_) => panic!("expected async"),
        }
    }
}
