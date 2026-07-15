//! WASM host-value registry and `IoNamespaceHost` impl.
//!
//! When JavaScript passes a callable as an argument to a BAML function, the
//! JS encoder calls [`register_host_callable`] (exposed via wasm-bindgen
//! as `registerHostCallable`) with the user's [`js_sys::Function`]. The
//! registry stores it under a freshly-allocated `u64` key and the encoder
//! emits an `InboundValue::Handle { key, handleType: HOST_VALUE_CALLABLE }`.
//!
//! When BAML invokes the host value, the [`WasmHost`] impl of
//! [`IoNamespaceHost::call_host_value`] encodes the call arguments as a
//! protobuf-encoded `BamlOutboundValue` list, allocates a call id, installs
//! a `CompletionHandle` in the in-flight call table, and fires *this
//! runtime's* JS-supplied `host_dispatch` callback with `(key, callId,
//! argsBytes)`.
//!
//! The JS dispatch wrapper decodes args, invokes the user function (await-ing
//! a returned Promise if any), encodes the result, and calls the wasm-exported
//! [`complete_host_call`] which resolves the `CompletionHandle`.
//!
//! When the engine drops the last Rust clone of the corresponding
//! `HostValueArc`, the global `host_release_dispatch` fires
//! [`host_release_callback`] which removes the registry entry — the user's
//! JS callable becomes GC-eligible.
//!
//! ## Per-runtime dispatch, global registry
//!
//! Dispatch is per-runtime: the JS `host_dispatch` callback — supplied *per*
//! [`BamlWasmRuntime`] at `create` time — lives on the per-runtime [`WasmHost`]
//! (one instance per `BamlWasmRuntime`, wired into its `SysOps`).
//! `IoNamespaceHost::call_host_value` runs on the `WasmHost` that owns the
//! originating runtime, so the dispatch always fires through *that* runtime's
//! wrapper. This is the engine→host path and it is unambiguous because the impl
//! already knows its own runtime; no global routing is needed in this
//! direction. Storing the callback globally instead would let a second runtime
//! clobber the first's wrapper, so a host call from runtime A would dispatch
//! through runtime B's JS wrapper (wrong closure / wrong `completeHostCall`
//! wiring) — which is why it lives on `WasmHost` rather than a `thread_local`.
//!
//! The `callables` map and the `in_flight` call table stay **process-global**
//! and are routed purely by id (mirroring the Node bridge's process-global
//! registry):
//!
//! * The `callables` map only keeps the JS `Function` alive and is consulted
//!   solely to *drop* it on release. Keys are minted by a process-global
//!   atomic and are therefore **globally unique**, so releasing by key drops
//!   exactly the right entry regardless of which runtime registered it. There
//!   is no per-runtime ambiguity to resolve. (In WASM, `registerHostCallable`
//!   is a free function the JS encoder calls *while building the proto args*,
//!   before a `RunStore` start command is invoked — there is no runtime receiver to
//!   attribute the registration to. A global registry sidesteps that entirely.)
//! * The `in_flight` table holds only a `CompletionHandle` (a oneshot sender).
//!   Resolving it is fully self-contained — it needs no runtime context — and
//!   call ids are globally unique, so `complete_host_call(callId)` from runtime
//!   A can never resolve runtime B's pending call.
//!
//! `Function` / the table state are `!Send`, so they live in `thread_local!`;
//! `wasm32-unknown-unknown` is single-threaded.
//!
//! ## In-flight lifetime: RAII eviction on cancel, no timeout
//!
//! An `IN_FLIGHT` entry is removed by exactly one of: the JS wrapper calling
//! `completeHostCall(callId, ...)` (normal completion), or cancellation of the
//! BAML call. On cancel the engine drops the async future returned by
//! [`drain_pending`], which drops the [`WasmInflightGuard`] moved into it; the
//! guard removes the dangling entry so it does not leak. There is **no
//! wall-clock timeout** — a host that never completes a call and is never
//! cancelled leaves the entry pending forever (matching the native bridge; see
//! `sys_native::host_dispatch`).

use std::{cell::RefCell, collections::HashMap, sync::Arc};

use bex_project::{host_release_dispatch, validate_host_return};
use bridge_ctypes::{
    CffiHandleTableOptions, HANDLE_TABLE, baml_bridge::cffi::InboundValue, inbound_to_external,
};
use js_sys::Function;
use prost::Message;
use sys_ops::io::{
    self, BexExternalValue, CallId, OpError, SysOpContext, SysOpOutput, SysOpResult, VmBamlError,
    VmRustFnError,
};
use sys_types::{BexHeap, CompletionHandle, SysOp};
use wasm_bindgen::prelude::*;

use crate::send_wrapper::SendWrapper;

// ============================================================================
// Process-global, id-routed WASM host-value state
// ============================================================================
//
// Keyed by globally-unique ids (see `next_key` / `next_call_id`), so neither
// table needs a per-runtime split to route correctly: a key/call-id maps to at
// most one entry across the whole process. Only the *dispatch callback* is
// per-runtime (see `WasmHost`).

thread_local! {
    /// `u64 → user JS callable`. Populated by [`register_host_callable`] from
    /// the JS encoder; removed by [`host_release_callback`] when the Rust
    /// runtime drops the last clone of the corresponding `HostValueArc`. Keys
    /// are globally unique, so release-by-key is unambiguous across runtimes.
    static CALLABLES: RefCell<HashMap<u64, SendWrapper<Function>>> = RefCell::new(HashMap::new());

    /// `call_id → CompletionHandle`. Populated by [`WasmHost::call_host_value`]
    /// before firing `host_dispatch`; removed by [`complete_host_call`] when
    /// the JS dispatch wrapper completes the invocation. Call ids are globally
    /// unique, so a completion can never resolve a different runtime's call.
    static IN_FLIGHT: RefCell<HashMap<u32, CompletionHandle>> = RefCell::new(HashMap::new());

    /// Process-global key minter. Globally unique so release-by-key routing is
    /// unambiguous. Zero is reserved as "invalid"; wrap-around skips 0.
    static NEXT_KEY: RefCell<u64> = const { RefCell::new(1) };

    /// Process-global call-id minter. Globally unique so completion routing is
    /// unambiguous. Zero is reserved as "invalid"; wrap-around skips 0.
    static NEXT_CALL_ID: RefCell<u32> = const { RefCell::new(1) };
}

fn next_key() -> u64 {
    NEXT_KEY.with(|cell| {
        let mut next = cell.borrow_mut();
        loop {
            let k = *next;
            *next = next.wrapping_add(1);
            if k != 0 {
                return k;
            }
        }
    })
}

fn next_call_id() -> u32 {
    NEXT_CALL_ID.with(|cell| {
        let mut next = cell.borrow_mut();
        loop {
            let id = *next;
            *next = next.wrapping_add(1);
            if id != 0 {
                return id;
            }
        }
    })
}

/// Register a `CompletionHandle` for an in-flight host call.
///
/// `call_id` must be freshly minted by [`next_call_id`] and not already
/// present. The table is keyed by a *wrapping* `u32`; with RAII eviction of
/// cancelled calls (see [`WasmInflightGuard`]) entries no longer leak, so the
/// live set stays bounded and a wrap-around collision is effectively
/// impossible.
///
/// We never silently overwrite a live entry (that would strand the previous
/// call's `CompletionHandle` and let a late completion resolve the wrong call).
/// A collision trips a `debug_assert!` in debug builds; in release builds
/// (where the assert is stripped) it is caught at runtime by refusing the
/// insert and completing the new call with an error.
///
/// Returns `true` when `completion` was inserted, `false` on collision. On
/// `false` the caller **must not** fire the JS dispatch (the call has already
/// failed) and **must not** build a [`WasmInflightGuard`] for `call_id` — the
/// live entry under that id belongs to the *other* call, so a guard drop would
/// evict it.
#[must_use]
fn insert_in_flight(call_id: u32, completion: CompletionHandle) -> bool {
    let collision = IN_FLIGHT.with(|cell| cell.borrow().contains_key(&call_id));
    debug_assert!(
        !collision,
        "host-call id {call_id} collided with a live in-flight entry; the u32 \
         call-id space wrapped while an entry was still pending (impossible \
         now that cancelled calls are evicted via WasmInflightGuard)"
    );
    if collision {
        // Release builds strip the assert above, so guard at runtime too.
        log::error!(
            "host-call id {call_id} collided with a live in-flight entry; \
             refusing to overwrite and failing the new call"
        );
        completion.complete(Err(OpError::new(
            SysOp::BamlHostCallHostValue,
            VmBamlError::DevOther {
                message: format!("host-call id {call_id} collided with a live in-flight call"),
            },
        )));
        return false;
    }
    IN_FLIGHT.with(|cell| {
        cell.borrow_mut().insert(call_id, completion);
    });
    true
}

/// RAII guard that evicts an in-flight call's `IN_FLIGHT` entry when dropped.
///
/// Owned by the async future returned by [`drain_pending`]. Carries only the
/// `Copy` `call_id`, never the `CompletionHandle` (which lives in the table).
/// On drop it removes the entry if still present:
///
/// * **After normal completion** this is a no-op — `complete_host_call` already
///   removed and fired the handle.
/// * **On cancellation** the future is dropped before completion, so the guard
///   removes the dangling entry. Dropping the removed `CompletionHandle` closes
///   the oneshot, so a later `completeHostCall` for that id hits the benign
///   unknown-id path. No leak.
struct WasmInflightGuard {
    call_id: u32,
}

impl Drop for WasmInflightGuard {
    fn drop(&mut self) {
        let _ = IN_FLIGHT.with(|cell| cell.borrow_mut().remove(&self.call_id));
    }
}

// ============================================================================
// JS-facing wasm-bindgen exports
// ============================================================================

/// Register a JS callable in the WASM host-value table and return its key.
///
/// Exposed to JS as `registerHostCallable(fn) -> bigint`. The key is then
/// embedded in `InboundValue::Handle { key, handleType: HOST_VALUE_CALLABLE }`
/// by the JS encoder. The returned value is a `BigInt` because `u64` does not
/// fit into JS's safe-integer range.
#[wasm_bindgen(js_name = registerHostCallable)]
pub fn register_host_callable(callable: Function) -> u64 {
    let key = next_key();
    CALLABLES.with(|cell| {
        cell.borrow_mut().insert(key, SendWrapper::new(callable));
    });
    key
}

/// Complete an in-flight host call from JS.
///
/// Exposed to JS as `completeHostCall(callId, isError, content)`.
///
/// On success (`is_error == 0`), `content` is a protobuf-encoded `InboundValue`
/// (host→engine direction, no type metadata — engine re-validates against the
/// declared return type).
///
/// On error (`is_error != 0`), `content` is a protobuf-encoded `InboundValue`
/// representing the thrown value. The host bridge SDK wraps native exceptions
/// in a synthetic `Instance` of class `baml.errors.HostCallable` carrying
/// `message` / `class_name` / `language` / `traceback` fields; codegenned
/// BAML errors flow through as their own `Instance` shape. The engine's
/// `materialize_host_throw` runs the declared-throws contract check on the
/// decoded value and either re-injects it as a catchable throw or escalates
/// to a `HostContractViolation` panic.
///
/// `call_id` is globally unique, so it resolves the originating runtime's
/// pending call unambiguously. If `call_id` is unknown the call is silently
/// dropped (likely a stale completion racing with a cancellation).
#[wasm_bindgen(js_name = completeHostCall)]
pub fn complete_host_call(call_id: u32, is_error: i32, content: &[u8]) {
    let completion = IN_FLIGHT.with(|cell| cell.borrow_mut().remove(&call_id));
    let Some(completion) = completion else {
        log::warn!("completeHostCall for unknown call id {call_id}");
        return;
    };

    // Strict 0/1 contract: any other value is a bridge wire-protocol bug
    // (an `i32` could carry uninitialised memory, a forgotten cast, or
    // someone repurposing the flag) — surface it as `BridgeFailure` so the
    // bug is loud, instead of silently aliasing into the throw branch.
    if is_error != 0 && is_error != 1 {
        completion.complete(Err(OpError::new(
            SysOp::BamlHostCallHostValue,
            sys_types::VmInternalError::BridgeFailure {
                message: format!(
                    "completeHostCall: invalid isError value {is_error}; \
                     expected 0 (success) or 1 (error)"
                ),
            },
        )));
        return;
    }

    if is_error == 0 {
        // Success: decode InboundValue → BexExternalValue.
        if content.is_empty() {
            // No payload → Null return.
            completion.complete(Ok(BexExternalValue::Null));
            return;
        }
        let inbound = match InboundValue::decode(content) {
            Ok(v) => v,
            Err(e) => {
                completion.complete(Err(OpError::new(
                    SysOp::BamlHostCallHostValue,
                    VmBamlError::ParseError {
                        message: format!("completeHostCall decode failure: {e}"),
                    },
                )));
                return;
            }
        };
        match inbound_to_external(inbound, &HANDLE_TABLE) {
            Ok(v) => completion.complete(Ok(v)),
            Err(e) => completion.complete(Err(OpError::new(
                SysOp::BamlHostCallHostValue,
                VmBamlError::ParseError {
                    message: format!("completeHostCall decode failure: {e}"),
                },
            ))),
        }
    } else {
        // Throw: decode `InboundValue` → `BexExternalValue` → engine. The
        // engine's `materialize_host_throw` runs the declared-throws
        // contract check on the decoded value.
        let mapped = if content.is_empty() {
            // An empty throw payload is a host bridge bug, not a user
            // contract violation: `is_error == 1` requires a protobuf-
            // encoded `InboundValue`, and only the bridge itself decides
            // what to send on the wire. A misbehaving bridge is an
            // infrastructure fault — surface it as `BridgeFailure` (which
            // codegens to `baml.panics.SdkPanic` on the host side), not as
            // `HostContractViolation` (which would falsely accuse the
            // user's callable of returning the wrong shape).
            OpError::new(
                SysOp::BamlHostCallHostValue,
                sys_types::VmInternalError::BridgeFailure {
                    message: "host bridge called completeHostCall(isError=1) \
                              with no payload; expected a protobuf-encoded \
                              InboundValue describing the thrown value"
                        .to_string(),
                },
            )
        } else {
            match InboundValue::decode(content) {
                Ok(inbound) => match inbound_to_external(inbound, &HANDLE_TABLE) {
                    Ok(v) => OpError::host_thrown_value(SysOp::BamlHostCallHostValue, v),
                    Err(e) => OpError::new(
                        SysOp::BamlHostCallHostValue,
                        VmBamlError::ParseError {
                            message: format!("completeHostCall throw-payload decode failure: {e}"),
                        },
                    ),
                },
                Err(e) => OpError::new(
                    SysOp::BamlHostCallHostValue,
                    VmBamlError::ParseError {
                        message: format!("completeHostCall throw-payload decode failure: {e}"),
                    },
                ),
            }
        };
        completion.complete(Err(mapped));
    }
}

// ============================================================================
// host_release_dispatch wiring (drop of last HostValueArc clone)
// ============================================================================

/// `HostReleaseFn` C-ABI handler installed via [`host_release_dispatch::install`].
///
/// Fires (via the engine's `host_release_dispatch::drain()` at a safepoint)
/// when the last Rust clone of the corresponding `HostValueArc` is dropped —
/// see `bex_project::host_release_dispatch`. Dropping the
/// [`SendWrapper<Function>`] releases the underlying `js_sys::Function`,
/// allowing the user's JS callable to become GC-eligible.
///
/// Routes by the bare `key` against the process-global [`CALLABLES`] table.
/// Keys are globally unique, so this removes exactly the right entry regardless
/// of which runtime registered the callable.
extern "C" fn host_release_callback(key: u64) {
    CALLABLES.with(|cell| {
        cell.borrow_mut().remove(&key);
    });
}

/// Install the global release-dispatch callback. Idempotent (first call wins).
///
/// Called from [`WasmHost::new`] so the wiring is present whenever a runtime
/// that supports host callables is active.
fn ensure_release_dispatch_installed() {
    // First-call-wins: subsequent installs return `Err(AlreadyInstalled)`
    // which we ignore (the same fn pointer is being installed every time).
    let _ = host_release_dispatch::install(host_release_callback);
}

// ============================================================================
// WasmHost: IoNamespaceHost impl
// ============================================================================

/// WASM host-value dispatch sysop impl.
///
/// One instance per `BamlWasmRuntime`, wired into its `SysOps`. Holds *this*
/// runtime's JS `host_dispatch` callback (supplied at `BamlWasmRuntime::create`).
/// Implements [`IoNamespaceHost::call_host_value`] by encoding the inbound BAML
/// args to a `BamlOutboundValue` list, allocating a fresh (globally-unique)
/// call id, installing a `CompletionHandle`, and firing *this runtime's* JS
/// dispatch callback. Because the impl runs on the `WasmHost` that owns the
/// originating runtime, the dispatch always reaches the correct wrapper — no
/// global routing is needed for the engine→host direction.
pub(crate) struct WasmHost {
    /// This runtime's JS dispatch callback.
    ///
    /// JS signature: `(key: bigint, callId: number, argsBytes: Uint8Array) => void`.
    /// The wrapper is responsible for calling [`complete_host_call`] back.
    host_dispatch: SendWrapper<Function>,
}

impl WasmHost {
    /// Construct a new `WasmHost` bound to this runtime's `host_dispatch`
    /// callback, and install the global release callback (idempotent — only the
    /// first installation wins; the same fn pointer is installed each time).
    pub(crate) fn new(host_dispatch: Function) -> Self {
        ensure_release_dispatch_installed();
        Self {
            host_dispatch: SendWrapper::new(host_dispatch),
        }
    }
}

impl io::IoNamespaceHost for WasmHost {
    fn call_host_value(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        handle: BexExternalValue,
        args: Vec<BexExternalValue>,
        type_arg_0: baml_type::RuntimeTy,
        _type_arg_1: baml_type::RuntimeTy,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<BexExternalValue> {
        // Extract the HostValueArc from the incoming handle.
        let host_arc = match handle {
            BexExternalValue::HostValue(arc) => arc,
            other => {
                return SysOpOutput::err(VmBamlError::InvalidArgument {
                    message: format!("expected HostValue, got {other:?}"),
                });
            }
        };

        // The VM split the call's args by the callable's declared params and
        // packed them as `[positional_array, optional_map]` (see
        // `host_closure_call_sysop`). Unpack and encode them into the
        // `BamlToHostCall`'s flat `args` list (mirrors
        // `sys_native::host_impls::call_host_value`): required args first, then
        // the supplied optionals (tagged + keyed by name). The JS dispatch
        // wrapper applies its calling convention.
        let options = CffiHandleTableOptions::for_wire();
        let mut pack = args.into_iter();
        let positional = match pack.next() {
            Some(BexExternalValue::Array { items, .. }) => items,
            other => {
                return SysOpOutput::err(sys_types::VmInternalError::BridgeFailure {
                    message: format!(
                        "host-call args pack[0] must be the positional array, got {other:?}"
                    ),
                });
            }
        };
        let optional = match pack.next() {
            Some(BexExternalValue::Map { entries, .. }) => entries,
            other => {
                return SysOpOutput::err(sys_types::VmInternalError::BridgeFailure {
                    message: format!(
                        "host-call args pack[1] must be the optional map, got {other:?}"
                    ),
                });
            }
        };
        let encoded: Vec<u8> =
            match bridge_ctypes::build_to_host_call(&positional, &optional, &options) {
                Ok(to_host_call) => to_host_call.encode_to_vec(),
                Err(e) => {
                    // Arg encoding is bridge-side serialization, not a
                    // host-language error. A failure here means the engine
                    // had a `BexExternalValue` it could not put on the wire
                    // — an engine/bridge bug. Surface as a fatal internal
                    // error rather than a catchable `VmBamlError`.
                    return SysOpOutput::err(sys_types::VmInternalError::BridgeFailure {
                        message: format!("failed to encode host-call arguments: {e}"),
                    });
                }
            };

        // Allocate a fresh (globally-unique) call id and create a
        // CompletionHandle, recorded in the process-global in-flight table so
        // the free-function `complete_host_call` can resolve it by id.
        let call_id = next_call_id();
        let (result, completion) = SysOpResult::pending(SysOp::BamlHostCallHostValue);

        // On a (2^32-wrap) id collision `insert_in_flight` returns `false` after
        // failing this call's `completion`, and the live entry under `call_id`
        // belongs to the *other* call. So only fire the dispatch — and only
        // install the eviction guard (in `drain_pending`) — when our entry
        // actually went in; otherwise `result` already carries the collision
        // error and dispatching would run the host callback for a failed call.
        let inserted = insert_in_flight(call_id, completion);
        if inserted {
            // Fire *this runtime's* JS dispatch callback. The signature is
            // `(key: bigint, callId: number, argsBytes: Uint8Array) => void`.
            let host_dispatch = (*self.host_dispatch).clone();

            let key_js = JsValue::from(host_arc.key);
            // u32 → f64 is lossless; the JS side reads `callId` as a plain Number.
            let call_id_js = JsValue::from_f64(f64::from(call_id));
            let args_js = js_sys::Uint8Array::new_with_length(
                encoded
                    .len()
                    .try_into()
                    .expect("host-call args payload exceeds u32::MAX"),
            );
            args_js.copy_from(&encoded);
            if let Err(err) =
                host_dispatch.call3(&JsValue::NULL, &key_js, &call_id_js, &args_js.into())
            {
                // The JS dispatch threw synchronously (before it could schedule
                // work). Complete the in-flight call with an error.
                let popped = IN_FLIGHT.with(|cell| cell.borrow_mut().remove(&call_id));
                if let Some(c) = popped {
                    let msg = err.as_string().unwrap_or_else(|| format!("{err:?}"));
                    // The WASM bridge's JS dispatch wrapper threw before
                    // scheduling work — this is a bridge-layer fault.
                    // The bridge is engine-owned infrastructure, so its
                    // failure is treated as an engine bug (internal,
                    // non-catchable) rather than a `HostCallable`
                    // (which represents a user-level host-language
                    // exception with a rehydration handle).
                    c.complete(Err(OpError::new(
                        SysOp::BamlHostCallHostValue,
                        sys_types::VmInternalError::BridgeFailure {
                            message: format!("host_dispatch JS callback threw: {msg}"),
                        },
                    )));
                }
            }
        }

        drain_pending(result, type_arg_0, call_id, inserted)
    }
}

/// Convert a `SysOpResult` from `SysOpResult::pending` into a `SysOpOutput`
/// suitable for the trait return type. The host's result lands as a
/// `BexExternalValue`; on error we strip the `SysOp` wrapper to expose only
/// the kind (the trait's contract).
///
/// On success the host's returned value is strictly validated against the
/// declared return type `expected` via the shared [`validate_host_return`]
/// guard — the same check the native bridge performs — so a host that returns
/// a value violating the declared `T` surfaces as a catchable
/// `root.errors.HostCallable` rather than corrupting the VM. Class *field
/// types* are validated engine-side where the resolved schema is available;
/// this guard covers scalar discrimination (`int` ≠ `float`), container
/// recursion, enum identity, and class-name identity.
///
/// When `install_guard` is true, `call_id` owns the in-flight entry installed
/// by the caller, and a [`WasmInflightGuard`] for it is moved into the async
/// future so that cancellation (the future being dropped before completion)
/// evicts the dangling entry. `install_guard` is `false` on the id-collision
/// path, where the live entry under `call_id` belongs to another call — guarding
/// it would evict that entry, so no guard is built and `result` simply resolves
/// to the collision error. The `Ready` arms drop the guard inline — also
/// correct, just not reached in practice since `SysOpResult::pending` always
/// yields `Async`.
fn drain_pending(
    result: SysOpResult,
    expected: baml_type::RuntimeTy,
    call_id: u32,
    install_guard: bool,
) -> SysOpOutput<BexExternalValue> {
    let guard = install_guard.then_some(WasmInflightGuard { call_id });
    match result {
        SysOpResult::Ready(Ok(value)) => match validate_host_return_value(&value, &expected) {
            Ok(()) => SysOpOutput::ok(value),
            Err(kind) => SysOpOutput::err(kind),
        },
        SysOpResult::Ready(Err(err)) => match err.payload {
            sys_types::OpErrorPayload::Vm(kind) => SysOpOutput::err(kind),
            // `SysOpResult::pending` always yields `Async`, so a Ready(Err)
            // here can't actually originate from the host-throw path.
            sys_types::OpErrorPayload::HostThrown(_) => {
                unreachable!("Ready(Err) is never produced for the host-callable sysop")
            }
        },
        SysOpResult::Async(fut) => {
            SysOpOutput::Async(Box::pin(crate::send_wrapper::SendFuture(async move {
                // Move the guard into the future so cancellation (future drop)
                // evicts the in-flight entry. `None` on the collision path —
                // there is no entry of ours to evict and `fut` resolves to the
                // collision error.
                let _guard = guard;
                // `?` propagates `OpError → VmRustFnError` via the `From`
                // impl in `sys_types`; `validate_host_return_value` yields
                // `Result<_, VmRustFnError>` directly.
                let value = fut.await?;
                validate_host_return_value(&value, &expected)?;
                Ok(value)
            })))
        }
    }
}

/// Strictly validate a host-returned value against the declared return type.
/// A mismatch is a `baml.panics.HostContractViolation` panic — the host has
/// violated its typed contract, so the call cannot be reasonably continued.
/// Mirrors the native bridge's `validate_return_value` in
/// `sys_native::host_impls`.
fn validate_host_return_value(
    value: &BexExternalValue,
    expected: &baml_type::RuntimeTy,
) -> Result<(), VmRustFnError> {
    validate_host_return(value, expected).map_err(|err| {
        sys_types::VmPanic::HostContractViolation {
            message: format!(
                "host callable returned a value of the wrong type: {err} (expected {expected})"
            ),
            class_name: None,
            language: None,
        }
        .into()
    })
}

#[cfg(test)]
mod tests {
    //! Host-target unit tests for the id-routing layer.
    //!
    //! The full BAML→host round trip is exercised by the `wasm_bindgen_test`
    //! suite in `tests/host_callable.rs` (including the two-runtime
    //! `two_runtimes_dispatch_through_their_own_wrapper` test), which needs a
    //! wasm runtime (`wasm-pack test --node`). These plain `#[test]`s
    //! cover what the routing layer guarantees *without* JS: globally-unique id
    //! minting and `complete_host_call`'s by-id resolution against the shared
    //! in-flight table — the property that lets two runtimes share one global
    //! table yet never resolve each other's pending calls.

    use std::collections::HashSet;

    use super::*;

    /// Minted keys must be unique, monotonic, and never 0 (0 is the reserved
    /// "invalid" sentinel). Globally-unique keys are what make release-by-key
    /// routing unambiguous across runtimes.
    #[test]
    fn next_key_is_unique_and_nonzero() {
        let mut seen = HashSet::new();
        for _ in 0..1000 {
            let k = next_key();
            assert_ne!(k, 0, "minted key must never be the reserved 0");
            assert!(seen.insert(k), "minted key {k} was handed out twice");
        }
    }

    /// Same invariants for call ids.
    #[test]
    fn next_call_id_is_unique_and_nonzero() {
        let mut seen = HashSet::new();
        for _ in 0..1000 {
            let id = next_call_id();
            assert_ne!(id, 0, "minted call id must never be the reserved 0");
            assert!(seen.insert(id), "minted call id {id} was handed out twice");
        }
    }

    /// `complete_host_call` must resolve *only* the in-flight entry whose id
    /// matches, leaving other runtimes' entries untouched. This is the
    /// table-level guarantee behind two-runtime independence: both runtimes
    /// share the one global `IN_FLIGHT` table but mint disjoint globally-unique
    /// ids, so a completion for runtime A's id can never resolve runtime B's
    /// pending call.
    #[test]
    fn complete_host_call_resolves_only_the_matching_id() {
        // Two pending calls standing in for two different runtimes.
        let id_a = next_call_id();
        let id_b = next_call_id();
        let (result_a, completion_a) = SysOpResult::pending(SysOp::BamlHostCallHostValue);
        let (result_b, completion_b) = SysOpResult::pending(SysOp::BamlHostCallHostValue);
        IN_FLIGHT.with(|cell| {
            let mut t = cell.borrow_mut();
            t.insert(id_a, completion_a);
            t.insert(id_b, completion_b);
        });

        let SysOpResult::Async(fut_a) = result_a else {
            panic!("pending() must produce an async result");
        };
        let SysOpResult::Async(mut fut_b) = result_b else {
            panic!("pending() must produce an async result");
        };

        // Complete A with an empty success payload (→ Null). B must stay pending.
        complete_host_call(id_a, 0, &[]);

        // A's future is now resolved with Null; B's table entry is still present.
        let a = futures::executor::block_on(fut_a).expect("A completes successfully");
        assert!(matches!(a, BexExternalValue::Null), "A resolved with Null");
        IN_FLIGHT.with(|cell| {
            assert!(
                cell.borrow().contains_key(&id_b),
                "completing A must not touch B's in-flight entry"
            );
            assert!(
                !cell.borrow().contains_key(&id_a),
                "A's entry must be removed once completed"
            );
        });

        // An unknown id is a silent no-op (stale completion / dropped runtime).
        complete_host_call(u32::MAX, 0, &[]);

        // B's future must still be pending (not yet polled to completion).
        let waker = futures::task::noop_waker();
        let mut cx = std::task::Context::from_waker(&waker);
        assert!(
            std::future::Future::poll(std::pin::Pin::new(&mut fut_b), &mut cx).is_pending(),
            "B must remain pending until its own id is completed"
        );

        // Now complete B and confirm it resolves independently.
        complete_host_call(id_b, 0, &[]);
        let b = futures::executor::block_on(fut_b).expect("B completes successfully");
        assert!(matches!(b, BexExternalValue::Null), "B resolved with Null");
    }

    /// Cancellation: the async future returned by `drain_pending` carries a
    /// [`WasmInflightGuard`]. Dropping the future before completion (the engine's
    /// cancel arm) must evict the in-flight entry so it does not leak.
    #[test]
    fn guard_drop_evicts_in_flight_entry_on_cancel() {
        let call_id = next_call_id();
        let (result, completion) = SysOpResult::pending(SysOp::BamlHostCallHostValue);
        assert!(
            insert_in_flight(call_id, completion),
            "fresh id must insert cleanly"
        );
        assert!(
            IN_FLIGHT.with(|c| c.borrow().contains_key(&call_id)),
            "entry must be present after insert"
        );

        // `drain_pending` wraps the result + guard into the returned future.
        let SysOpOutput::Async(fut) =
            drain_pending(result, baml_type::RuntimeTy::unknown(), call_id, true)
        else {
            panic!("pending() must yield an async output");
        };

        // Model the engine dropping the cancelled future.
        drop(fut);

        assert!(
            !IN_FLIGHT.with(|c| c.borrow().contains_key(&call_id)),
            "guard drop on cancel must evict the in-flight entry (no leak)"
        );
    }

    /// Normal completion removes the entry; a later guard drop is a benign
    /// no-op (no double-remove, no panic).
    #[test]
    fn guard_drop_after_normal_completion_is_noop() {
        let call_id = next_call_id();
        let (result, completion) = SysOpResult::pending(SysOp::BamlHostCallHostValue);
        assert!(
            insert_in_flight(call_id, completion),
            "fresh id must insert cleanly"
        );

        let SysOpOutput::Async(fut) =
            drain_pending(result, baml_type::RuntimeTy::unknown(), call_id, true)
        else {
            panic!("pending() must yield an async output");
        };

        // Host completes normally — this removes-and-fires the handle.
        complete_host_call(call_id, 0, &[]);
        assert!(
            !IN_FLIGHT.with(|c| c.borrow().contains_key(&call_id)),
            "completion must remove the in-flight entry"
        );

        // The future resolves (Null) and its guard drop is a no-op.
        let value = futures::executor::block_on(fut).expect("completes successfully");
        assert!(matches!(value, BexExternalValue::Null));
        assert!(
            !IN_FLIGHT.with(|c| c.borrow().contains_key(&call_id)),
            "entry stays removed after the future (and its guard) is dropped"
        );
    }
}
