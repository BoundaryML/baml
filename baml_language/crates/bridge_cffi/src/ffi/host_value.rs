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

use bex_project::{BexExternalValue, host_release_dispatch};
use bridge_ctypes::{HANDLE_TABLE, baml_bridge::cffi::InboundValue, inbound_to_external};
use prost::Message;
use sys_native::host_dispatch;
use sys_types::{OpError, SysOp, VmBamlError, VmInternalError};

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
/// `InboundValue` carrying the thrown value (typically an `Instance` of
/// `baml.errors.HostCallable` with the host exception's metadata, or a
/// codegenned BAML error class). The engine's `materialize_host_throw`
/// runs the declared-throws contract check against the decoded value.
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
    // This FFI boundary holds no heap permit or engine/interner/table lock, so
    // it is a safe place to invoke the host release callback on return.
    let _drain_host_releases = DrainHostReleasesOnReturn;

    // A non-zero length with a null pointer is an ABI violation: the caller
    // promised `length` bytes but gave us nothing to read. Don't silently treat
    // it as an empty payload (which would mask the bug as a `Null` return or a
    // "no payload" error) — fail the call explicitly so the host sees it.
    if length > 0 && content.is_null() {
        host_dispatch::complete_with_error(
            call_id,
            OpError::new(
                SysOp::BamlHostCallHostValue,
                VmBamlError::Io {
                    message: format!(
                        "complete_host_call: null content pointer with length {length}"
                    ),
                },
            ),
        );
        return;
    }

    // `from_raw_parts`'s soundness contract requires the slice's *total
    // byte length* to fit in `isize` — a buggy host SDK passing garbage
    // on a 64-bit platform could violate this and trigger undefined
    // behaviour inside the slice access or downstream protobuf decoding.
    // Reject as a `BridgeFailure` so the bug surfaces loudly.
    if length > isize::MAX as usize {
        host_dispatch::complete_with_error(
            call_id,
            OpError::new(
                SysOp::BamlHostCallHostValue,
                VmInternalError::BridgeFailure {
                    message: format!("complete_host_call: length {length} exceeds isize::MAX"),
                },
            ),
        );
        return;
    }

    // SAFETY: caller promises ptr is valid for `length` bytes; the guard above
    // ruled out a null pointer whenever `length > 0`.
    let bytes: &[u8] = if length == 0 {
        &[]
    } else {
        unsafe { std::slice::from_raw_parts(content as *const u8, length) }
    };

    // Strict 0/1 contract: any other value is a bridge wire-protocol bug
    // (an `i32` could carry uninitialised memory, a forgotten cast, or
    // someone repurposing the flag) — surface it as `BridgeFailure` so the
    // bug is loud, instead of silently aliasing into the throw branch.
    if is_error != 0 && is_error != 1 {
        host_dispatch::complete_with_error(
            call_id,
            OpError::new(
                SysOp::BamlHostCallHostValue,
                VmInternalError::BridgeFailure {
                    message: format!(
                        "complete_host_call: invalid is_error value {is_error}; \
                         expected 0 (success) or 1 (error)"
                    ),
                },
            ),
        );
        return;
    }

    if is_error == 0 {
        // Success: decode InboundValue → BexExternalValue.
        if bytes.is_empty() {
            // No payload → Null return.
            host_dispatch::complete_with_value(call_id, BexExternalValue::Null);
            return;
        }
        let inbound = match InboundValue::decode(bytes) {
            Ok(v) => v,
            Err(e) => {
                host_dispatch::complete_with_error(
                    call_id,
                    OpError::new(
                        SysOp::BamlHostCallHostValue,
                        VmBamlError::ParseError {
                            message: format!("complete_host_call decode failure: {e}"),
                        },
                    ),
                );
                return;
            }
        };
        match inbound_to_external(inbound, &HANDLE_TABLE) {
            Ok(v) => host_dispatch::complete_with_value(call_id, v),
            Err(e) => host_dispatch::complete_with_error(
                call_id,
                OpError::new(
                    SysOp::BamlHostCallHostValue,
                    VmBamlError::ParseError {
                        message: format!("complete_host_call decode failure: {e}"),
                    },
                ),
            ),
        }
    } else {
        // Throw: decode `InboundValue` → `BexExternalValue` → engine.
        //
        // The host bridge SDK wraps native exceptions in a synthetic
        // `Instance` of class `baml.errors.HostCallable` carrying
        // `message` / `class_name` / `language` / `traceback` fields
        // (and a hidden `HostValue(arc, kind=Error)` reference field, on
        // bridges that support same-host round-trip). Codegenned BAML
        // values flow through as their own `Instance` / primitive
        // shape. Either way, the engine's `materialize_host_throw`
        // runs the declared-throws contract check on this value and
        // either materialises it as a catchable throw or escalates to
        // a `HostContractViolation` panic.
        if bytes.is_empty() {
            // An empty throw payload is a host bridge bug, not a user
            // contract violation: `is_error == 1` requires a protobuf-
            // encoded `InboundValue`, and only the bridge itself decides
            // what to send on the wire. A misbehaving bridge is an
            // infrastructure fault — surface it as `BridgeFailure` (which
            // codegens to `baml.panics.SdkPanic` on the host side), not as
            // `HostContractViolation` (which would falsely accuse the
            // user's callable of returning the wrong shape).
            host_dispatch::complete_with_error(
                call_id,
                OpError::new(
                    SysOp::BamlHostCallHostValue,
                    VmInternalError::BridgeFailure {
                        message: "host bridge called complete_host_call(is_error=1) \
                                  with no payload; expected a protobuf-encoded \
                                  InboundValue describing the thrown value"
                            .to_string(),
                    },
                ),
            );
            return;
        }
        let inbound = match InboundValue::decode(bytes) {
            Ok(v) => v,
            Err(e) => {
                host_dispatch::complete_with_error(
                    call_id,
                    OpError::new(
                        SysOp::BamlHostCallHostValue,
                        VmBamlError::ParseError {
                            message: format!(
                                "complete_host_call throw-payload decode failure: {e}"
                            ),
                        },
                    ),
                );
                return;
            }
        };
        match inbound_to_external(inbound, &HANDLE_TABLE) {
            Ok(v) => host_dispatch::complete_with_throw(call_id, v),
            Err(e) => host_dispatch::complete_with_error(
                call_id,
                OpError::new(
                    SysOp::BamlHostCallHostValue,
                    VmBamlError::ParseError {
                        message: format!("complete_host_call throw-payload decode failure: {e}"),
                    },
                ),
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{
        LazyLock, Mutex, Once,
        atomic::{AtomicU64, Ordering},
    };

    use super::*;

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

    fn host_callable_throw_payload(key: u64) -> Vec<u8> {
        use bridge_ctypes::baml_bridge::cffi::{
            BamlHandle, BamlHandleType, BamlTy, BamlTyClass, InboundClassValue, InboundMapEntry,
            InboundValue, baml_ty::Ty as BamlTyVariant, inbound_map_entry::Key as InboundMapKey,
            inbound_value::Value as InboundValueVariant,
        };

        InboundValue {
            value_type: Some(BamlTy {
                ty: Some(BamlTyVariant::ClassTy(BamlTyClass {
                    name: "baml.errors.HostCallable".to_string(),
                    type_args: vec![],
                })),
            }),
            value: Some(InboundValueVariant::ClassValue(InboundClassValue {
                fields: vec![InboundMapEntry {
                    key: Some(InboundMapKey::StringKey("_handle".to_string())),
                    value: Some(InboundValue {
                        value_type: None,
                        value: Some(InboundValueVariant::Handle(BamlHandle {
                            key,
                            handle_type: BamlHandleType::HostValueOpaque.into(),
                        })),
                    }),
                }],
            })),
        }
        .encode_to_vec()
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

    /// Verify `complete_host_call` with an empty success payload completes with Null.
    #[tokio::test]
    async fn complete_host_call_empty_success_completes_null() {
        use sys_types::{SysOp, SysOpResult};

        let (result, completion) = SysOpResult::pending(SysOp::BamlHostCallHostValue);
        let id = host_dispatch::next_call_id();
        assert!(
            host_dispatch::insert(id, completion),
            "a fresh call id must insert without colliding"
        );

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

    /// Verify `complete_host_call(is_error=1)` decodes the `InboundValue`
    /// payload into a `BexExternalValue` and delivers it through the
    /// host-throw path (`OpError.host_thrown = Some(...)`). The engine
    /// runs the contract check on this value downstream — here we just
    /// pin the wire-side decode + delivery.
    #[tokio::test]
    async fn complete_host_call_throw_payload_delivers_external_value() {
        use bridge_ctypes::baml_bridge::cffi::{
            BamlTy, BamlTyClass, InboundClassValue, InboundMapEntry, InboundValue,
            baml_ty::Ty as BamlTyVariant, inbound_map_entry::Key as InboundMapKey,
            inbound_value::Value as InboundValueVariant,
        };
        use prost::Message;
        use sys_types::{SysOp, SysOpResult};

        // Build an InboundValue.Class for `baml.errors.HostCallable` with a
        // `message` field — mirrors what an SDK encodes when raising a
        // native host exception through the bridge.
        let message_entry = InboundMapEntry {
            key: Some(InboundMapKey::StringKey("message".to_string())),
            value: Some(InboundValue {
                value_type: None,
                value: Some(InboundValueVariant::StringValue("bad input".to_string())),
            }),
        };
        let inbound = InboundValue {
            value_type: Some(BamlTy {
                ty: Some(BamlTyVariant::ClassTy(BamlTyClass {
                    name: "baml.errors.HostCallable".to_string(),
                    type_args: vec![],
                })),
            }),
            value: Some(InboundValueVariant::ClassValue(InboundClassValue {
                fields: vec![message_entry],
            })),
        };
        let encoded = inbound.encode_to_vec();

        let (result, completion) = SysOpResult::pending(SysOp::BamlHostCallHostValue);
        let id = host_dispatch::next_call_id();
        assert!(
            host_dispatch::insert(id, completion),
            "a fresh call id must insert without colliding"
        );

        complete_host_call(id, 1, encoded.as_ptr() as *const i8, encoded.len());

        match result {
            sys_types::SysOpResult::Async(fut) => {
                let err = fut.await.expect_err("should be error");
                let thrown: &BexExternalValue = match &err.payload {
                    sys_types::OpErrorPayload::HostThrown(v) => v,
                    other => panic!("expected HostThrown payload, got {other:?}"),
                };
                match thrown {
                    BexExternalValue::Union { value, metadata }
                        if metadata.is_inbound_type_annotation =>
                    {
                        assert_eq!(
                            metadata.selected_option.to_string(),
                            "baml.errors.HostCallable"
                        );
                        match &**value {
                            BexExternalValue::Instance { fields, .. } => {
                                match fields.get("message") {
                                    Some(BexExternalValue::String(s)) => {
                                        assert_eq!(s.as_str(), "bad input");
                                    }
                                    other => panic!("expected `message: String`, got {other:?}"),
                                }
                            }
                            other => panic!("expected annotated Instance, got {other:?}"),
                        }
                    }
                    other => panic!("expected inbound type annotation, got {other:?}"),
                }
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
            let payload = host_callable_throw_payload(key);
            complete_host_call(call_id, 1, payload.as_ptr().cast(), payload.len());
            assert!(
                take_recorded_release(key),
                "late completion did not synchronously release host key {key}"
            );
        }

        let key = NEXT_TEST_HOST_KEY.fetch_add(1, Ordering::Relaxed);
        let payload = host_callable_throw_payload(key);
        let (result, completion) = SysOpResult::pending(SysOp::BamlHostCallHostValue);
        let call_id = host_dispatch::next_call_id();
        assert!(host_dispatch::insert(call_id, completion));
        complete_host_call(call_id, 1, payload.as_ptr().cast(), payload.len());
        assert!(
            !take_recorded_release(key),
            "accepted throw released native error identity before delivery"
        );

        let SysOpResult::Async(result) = result else {
            panic!("expected asynchronous host-call result")
        };
        let error = result.await.expect_err("throw should complete as an error");
        let OpErrorPayload::HostThrown(value) = &error.payload else {
            panic!("expected host-thrown completion, got {:?}", error.payload)
        };
        assert!(matches!(
            &**value,
            BexExternalValue::Union {
                value,
                metadata,
            } if metadata.is_inbound_type_annotation
                && matches!(&**value, BexExternalValue::Instance { .. })
        ));
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
        complete_host_call(id, 1, probe.as_ptr() as *const i8, usize::MAX);

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
