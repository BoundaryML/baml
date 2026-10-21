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
//!   dispatch (either with a success `InboundValue` or a thrown
//!   `InboundValue` carrying the host's exception as an `Instance`).
//!
//! The dispatch fn pointer and in-flight call table live in
//! `sys_native::host_dispatch` (not here) to avoid a circular dependency:
//! the `call_host_value` sysop impl (also in `sys_native`) needs them,
//! and bridge_cffi → sys_native is the one-way dependency direction.

use bex_project::{HostCallCompletion, host_release_dispatch};
use bridge_ctypes::{HANDLE_TABLE, decode_host_call_completion};
use sys_native::host_dispatch;

pub use super::super::api::BamlHostDispatchCallback as HostDispatchFn;
use super::super::api::BamlHostReleaseCallback;

/// Flush host-value drops caused while decoding or delivering one host-call
/// completion. In particular, a completion racing cancellation decodes its
/// opaque error handle and then drops it on the unknown-id path; without this
/// boundary safepoint that release can remain queued after the engine has
/// already performed its final drain.
///
/// Accepted values remain owned by the completion payload. The release
/// dispatcher's interner guard observes that live `HostValueArc` and will not
/// release its host key prematurely.
struct DrainHostReleasesOnReturn;

impl Drop for DrainHostReleasesOnReturn {
    fn drop(&mut self) {
        host_release_dispatch::drain();
    }
}

/// Register the host dispatch callback. First call wins; subsequent calls
/// are silently ignored (consistent with `register_callback` semantics).
///
/// Delegates to `sys_native::host_dispatch::set_dispatch_fn` so the
/// `call_host_value` sysop (implemented in `sys_native`) can read it.
///
/// # Safety
///
/// `cb` must remain valid for the lifetime of the process.
#[unsafe(no_mangle)]
pub extern "C" fn register_host_dispatch_callback(cb: HostDispatchFn) {
    host_dispatch::set_dispatch_fn(cb);
}

#[unsafe(no_mangle)]
pub extern "C" fn register_host_dispatch_v2(cb: crate::api::BamlHostDispatchV2) {
    host_dispatch::set_dispatch_v2(cb);
}

#[unsafe(no_mangle)]
pub extern "C" fn register_host_cancel_callback(cb: crate::api::BamlHostCancel) {
    host_dispatch::set_cancel_fn(cb);
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
pub extern "C" fn register_host_release_callback(cb: BamlHostReleaseCallback) {
    if host_release_dispatch::install(cb).is_err() {
        eprintln!("BAML internal: register_host_release_callback called twice");
    }
}

/// Complete an in-flight BAML→host callback with what the host's callable
/// did.
///
/// `content` is a protobuf-encoded `HostCallCompletion`: the callable's
/// result encoded against its declared return type, a BAML value it threw
/// encoded against its declared throws type, an exception of the host's own,
/// a contract the host already found broken, or a failure of the bridge's.
/// The engine holds the first two to the declared types itself and builds
/// every panic; a payload that is not a completion, or carries no outcome, is
/// the bridge's failure.
///
/// # Safety
///
/// `content` must be valid for `length` bytes for the duration of this call.
#[unsafe(no_mangle)]
pub extern "C" fn complete_host_call(call_id: u32, content: *const i8, length: usize) {
    // This FFI boundary holds no heap permit or engine/index/table lock, so
    // it is a safe place to invoke the host release callback on return.
    let _drain_host_releases = DrainHostReleasesOnReturn;

    let bridge_failure = |message: String| {
        host_dispatch::complete_host_call_completion(
            call_id,
            HostCallCompletion::BridgeFailure(message),
        );
    };
    // A non-zero length with a null pointer is an ABI violation: the caller
    // promised `length` bytes but gave us nothing to read. It is the bridge's
    // failure, never the callable's or the program's.
    if length > 0 && content.is_null() {
        bridge_failure(format!(
            "complete_host_call: null content pointer with length {length}"
        ));
        return;
    }
    // `from_raw_parts`'s soundness contract requires the slice's *total
    // byte length* to fit in `isize` — a buggy host SDK passing garbage
    // on a 64-bit platform could violate this and trigger undefined
    // behaviour inside the slice access or downstream protobuf decoding.
    if length > isize::MAX as usize {
        bridge_failure(format!(
            "complete_host_call: length {length} exceeds isize::MAX"
        ));
        return;
    }
    // SAFETY: caller promises ptr is valid for `length` bytes; the guard above
    // ruled out a null pointer whenever `length > 0`.
    let bytes: &[u8] = if length == 0 {
        &[]
    } else {
        unsafe { std::slice::from_raw_parts(content as *const u8, length) }
    };
    host_dispatch::complete_host_call_completion(
        call_id,
        decode_host_call_completion(bytes, &HANDLE_TABLE),
    );
}

#[cfg(test)]
mod tests {
    use std::sync::{
        LazyLock, Mutex, Once,
        atomic::{AtomicU64, Ordering},
    };

    use bex_project::BexExternalValue;
    use bridge_ctypes::baml_bridge::cffi::{
        HostCallCompletion as WireCompletion, HostException as WireException, InboundValue,
        host_call_completion::Outcome,
    };
    use prost::Message;

    use super::*;

    fn wire_completion(outcome: Outcome) -> Vec<u8> {
        WireCompletion {
            outcome: Some(outcome),
        }
        .encode_to_vec()
    }

    fn complete(call_id: u32, bytes: &[u8]) {
        complete_host_call(call_id, bytes.as_ptr().cast(), bytes.len());
    }

    static RELEASE_CALLBACK_INIT: Once = Once::new();
    static RELEASED_HOST_KEYS: LazyLock<Mutex<Vec<u64>>> = LazyLock::new(|| Mutex::new(Vec::new()));
    static NEXT_TEST_HOST_KEY: AtomicU64 = AtomicU64::new(10_000_000);

    extern "C" fn record_host_release(key: u64) {
        RELEASED_HOST_KEYS.lock().unwrap().push(key);
    }

    fn install_release_recorder() {
        RELEASE_CALLBACK_INIT.call_once(|| register_host_release_callback(record_host_release));
    }

    fn take_recorded_release(key: u64) -> bool {
        let mut released = RELEASED_HOST_KEYS.lock().unwrap();
        released
            .iter()
            .position(|candidate| *candidate == key)
            .map(|index| released.swap_remove(index))
            .is_some()
    }

    /// The completion for an exception of the host's own, registered under `key`.
    fn raised_payload(key: u64) -> Vec<u8> {
        wire_completion(Outcome::Raised(WireException {
            message: "bad input".to_string(),
            class_name: "ValueError".to_string(),
            language: "test".to_string(),
            traceback: None,
            handle: key,
        }))
    }

    /// Verify first-call-wins semantics for dispatch registration.
    #[test]
    fn register_host_dispatch_callback_first_call_wins() {
        extern "C" fn dispatch_a(_key: u64, _call_id: u32, _args: *const u8, _len: usize) {}
        extern "C" fn dispatch_b(_key: u64, _call_id: u32, _args: *const u8, _len: usize) {}

        // Use a fresh OnceCell by calling through the public API.
        // We cannot reset OnceCell so we just verify the second call doesn't panic.
        register_host_dispatch_callback(dispatch_a);
        register_host_dispatch_callback(dispatch_b); // must not panic

        // The installed value is dispatch_a (first call wins).
        // We can't easily assert the pointer identity in a unit test without
        // exposing the cell, so this test primarily checks there's no panic.
    }

    /// A completion whose callable returned `null` completes with Null; an
    /// empty payload carries no outcome and is the bridge's failure.
    #[tokio::test]
    async fn complete_host_call_null_return_completes_null() {
        use sys_types::{SysOp, SysOpResult};

        let (result, completion) = SysOpResult::pending(SysOp::BamlHostCallHostValue);
        let id = host_dispatch::next_call_id();
        assert!(
            host_dispatch::insert(id, completion),
            "a fresh call id must insert without colliding"
        );

        complete(
            id,
            &wire_completion(Outcome::Returned(InboundValue::default())),
        );
        match result {
            sys_types::SysOpResult::Async(fut) => {
                let value = fut.await.expect("should succeed");
                assert!(matches!(value, BexExternalValue::Null));
            }
            sys_types::SysOpResult::Ready(_) => panic!("expected async"),
        }

        let (result, completion) = SysOpResult::pending(SysOp::BamlHostCallHostValue);
        let id = host_dispatch::next_call_id();
        assert!(host_dispatch::insert(id, completion));
        complete_host_call(id, std::ptr::null(), 0);
        match result {
            sys_types::SysOpResult::Async(fut) => {
                let error = fut.await.expect_err("no outcome is a bridge failure");
                assert!(matches!(
                    error.payload,
                    sys_types::OpErrorPayload::Vm(sys_types::VmRustFnError::InternalError(
                        sys_types::VmInternalError::BridgeFailure { .. }
                    ))
                ));
            }
            sys_types::SysOpResult::Ready(_) => panic!("expected async"),
        }
    }

    /// An exception of the host's own is delivered as the host's outcome, for
    /// the engine to build `baml.errors.HostCallable` from and hold to the
    /// callable's declared throws type; here we pin the wire-side decode and
    /// delivery.
    #[tokio::test]
    async fn complete_host_call_raised_exception_delivers_host_outcome() {
        use sys_types::{SysOp, SysOpResult};

        let (result, completion) = SysOpResult::pending(SysOp::BamlHostCallHostValue);
        let id = host_dispatch::next_call_id();
        assert!(
            host_dispatch::insert(id, completion),
            "a fresh call id must insert without colliding"
        );
        let key = NEXT_TEST_HOST_KEY.fetch_add(1, Ordering::Relaxed);
        complete(id, &raised_payload(key));

        match result {
            sys_types::SysOpResult::Async(fut) => {
                let err = fut.await.expect_err("should be error");
                let sys_types::OpErrorPayload::Host(sys_types::HostOutcome::Raised(exception)) =
                    &err.payload
                else {
                    panic!("expected a raised host exception, got {:?}", err.payload)
                };
                assert_eq!(exception.message, "bad input");
                assert_eq!(exception.class_name, "ValueError");
                assert_eq!(exception.handle.key.get(), key);
            }
            sys_types::SysOpResult::Ready(_) => panic!("expected async"),
        }
    }

    /// A late callback error/panic can arrive after cancellation evicted its
    /// call id. Decoding that payload creates a HostValueArc for the native
    /// error identity; the unknown-id path drops it. `complete_host_call` must
    /// synchronously drain the resulting release without relying on a future
    /// engine call. Conversely, an accepted throw must retain the handle until
    /// the receiver consumes and drops it.
    #[tokio::test]
    async fn late_unknown_throw_releases_opaque_handle_but_accepted_throw_retains_it() {
        use sys_types::{OpErrorPayload, SysOp, SysOpResult};

        install_release_recorder();

        for _ in 0..64 {
            let key = NEXT_TEST_HOST_KEY.fetch_add(1, Ordering::Relaxed);
            let call_id = host_dispatch::next_call_id();
            complete(call_id, &raised_payload(key));
            assert!(
                take_recorded_release(key),
                "late completion did not synchronously release host key {key}"
            );
        }

        let key = NEXT_TEST_HOST_KEY.fetch_add(1, Ordering::Relaxed);
        let (result, completion) = SysOpResult::pending(SysOp::BamlHostCallHostValue);
        let call_id = host_dispatch::next_call_id();
        assert!(host_dispatch::insert(call_id, completion));
        complete(call_id, &raised_payload(key));
        assert!(
            !take_recorded_release(key),
            "accepted throw released native error identity before delivery"
        );

        let SysOpResult::Async(result) = result else {
            panic!("expected asynchronous host-call result")
        };
        let error = result.await.expect_err("throw should complete as an error");
        let OpErrorPayload::Host(sys_types::HostOutcome::Raised(exception)) = &error.payload else {
            panic!(
                "expected the host's raised exception, got {:?}",
                error.payload
            )
        };
        assert_eq!(exception.handle.key.get(), key);
        assert!(
            !take_recorded_release(key),
            "delivered throw did not retain native error identity"
        );
        drop(error);
        host_release_dispatch::drain();
        assert!(
            take_recorded_release(key),
            "dropped accepted throw did not release native error identity"
        );
    }

    /// Verify the `length > isize::MAX as usize` guard surfaces as a
    /// `BridgeFailure` (→ `SdkPanic` on the host), not a silent UB from
    /// `from_raw_parts`. A buggy host SDK passing garbage on a 64-bit
    /// platform could violate the slice-soundness contract; the guard
    /// turns that into a loud, host-attributable fault.
    #[tokio::test]
    async fn complete_host_call_length_exceeds_isize_max_surfaces_as_bridge_failure() {
        use sys_types::{SysOp, SysOpResult, VmInternalError, VmRustFnError};

        let (result, completion) = SysOpResult::pending(SysOp::BamlHostCallHostValue);
        let id = host_dispatch::next_call_id();
        assert!(
            host_dispatch::insert(id, completion),
            "a fresh call id must insert without colliding"
        );

        // We never dereference the pointer because the guard fires first;
        // a small valid-but-unread byte slice is enough to pass the null
        // check. `usize::MAX > isize::MAX` on both 32- and 64-bit targets.
        let probe: [u8; 1] = [0];
        complete_host_call(id, probe.as_ptr() as *const i8, usize::MAX);

        match result {
            SysOpResult::Async(fut) => {
                let err = fut.await.expect_err("should surface as a bridge failure");
                match err.payload {
                    sys_types::OpErrorPayload::Vm(VmRustFnError::InternalError(
                        VmInternalError::BridgeFailure { ref message },
                    )) => {
                        assert!(
                            message.contains("isize::MAX"),
                            "expected the length-overflow guard's message, got {message:?}"
                        );
                    }
                    other => panic!(
                        "expected VmInternalError::BridgeFailure for an over-isize length, \
                         got {other:?}"
                    ),
                }
            }
            SysOpResult::Ready(_) => panic!("expected async"),
        }
    }
}
