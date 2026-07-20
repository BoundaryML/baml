//! Per-process Python host-value registry.
//!
//! When Python passes a callable as an argument to a BAML function, the
//! inbound encoder (in `proto.py`) calls [`register_host_callable`] to
//! obtain a `u64` key and emits `InboundValue::Handle{key, HOST_VALUE_CALLABLE}`.
//! Rust's `inbound_to_external` decoder constructs a
//! `BexExternalValue::HostValue(Arc<HostValueArc>)`; the engine binds it to
//! an `Object::HostClosure`; later when BAML invokes it, the
//! `call_host_value` sysop fires a `HostDispatchFn` via the bridge_cffi
//! global, which lands here in [`host_dispatch_callback`].
//!
//! The dispatch callback:
//! 1. Looks up the Python callable by `host_value_key`.
//! 2. Decodes the `BamlOutboundValue` args into Python values via
//!    `baml_bridge.proto._decode_value_holder` (already a list shape).
//! 3. Invokes the callable. If the return is a coroutine, runs it to
//!    completion on a fresh `asyncio` event loop. The dispatch runs on a
//!    spawned tokio task (see [`host_dispatch_callback`]), so blocking that
//!    task to drive the coroutine to completion does not stall the engine,
//!    which concurrently awaits the call's completion.
//! 4. Encodes the result into `InboundValue` bytes via
//!    `baml_bridge.proto.encode_call_args`-style serialization, then calls
//!    `bridge_cffi::complete_host_call(call_id, 0, ptr, len)`.
//! 5. On any Python exception, branches on the exception type:
//!    - A `baml_bridge.errors.BamlError` carrying a codegenned BAML value
//!      is unwrapped (`.value`) and encoded as that real BAML class
//!      (preserves catch matching against user-declared throws).
//!    - Anything else (native `ValueError`, `KeyError`, ...) is registered
//!      in the process-global host-value table and encoded as a
//!      `baml.errors.HostCallable` Instance whose `_handle` field
//!      references the original Python exception object so the BAML→host
//!      decoder on the same runtime can rehydrate it on round-trip.
//!
//!    The encoded `InboundValue` rides `complete_host_call(call_id, 1,
//!    ptr, len)`; the engine's `materialize_host_throw` runs the
//!    declared-throws contract check against the surrounding callable's
//!    `E` and either re-injects the value as a catchable BAML throw or
//!    escalates to a `HostContractViolation` panic.
//!
//! When the engine drops the last Rust clone of the `HostValueArc`,
//! [`host_release_callback`] fires and removes the registry entry — the
//! Python callable's refcount drops to zero and the GC reclaims it.

use std::{
    collections::HashMap,
    sync::{
        LazyLock, Mutex,
        atomic::{AtomicU64, Ordering},
    },
};

use bridge_cffi::complete_host_call;
use bridge_ctypes::baml_bridge::cffi::{
    BamlHandle, BamlHandleType, BamlTy, BamlTyClass, InboundClassValue, InboundMapEntry,
    InboundValue, baml_ty::Ty as BamlTyVariant, inbound_map_entry::Key as InboundMapKey,
    inbound_value::Value as InboundValueVariant,
};
use prost::Message;
use pyo3::{
    Py, PyAny, PyResult, Python,
    prelude::*,
    types::{PyAnyMethods, PyDict, PyModule, PyTuple},
};
use pyo3_stub_gen::derive::gen_stub_pyfunction;

/// Process-wide table of Python callables that have been handed to BAML.
///
/// The key is a freshly-allocated `u64` (never 0). Removal happens in
/// [`host_release_callback`] when Rust drops its last clone of the
/// corresponding `HostValueArc`.
struct Registry {
    next_key: AtomicU64,
    table: Mutex<HashMap<u64, Py<PyAny>>>,
}

static REGISTRY: LazyLock<Registry> = LazyLock::new(|| Registry {
    next_key: AtomicU64::new(1),
    table: Mutex::new(HashMap::new()),
});

fn next_key() -> u64 {
    loop {
        let k = REGISTRY.next_key.fetch_add(1, Ordering::Relaxed);
        if k != 0 {
            return k;
        }
    }
}

/// Insert a Python callable into the registry and return its key.
///
/// Exposed to Python as `baml_py.register_host_callable(callable) -> int`.
/// Called from the inbound encoder in `baml_bridge.proto` whenever a Python
/// callable appears as a kwarg.
#[gen_stub_pyfunction]
#[pyfunction]
pub fn register_host_callable(callable: Py<PyAny>) -> u64 {
    let key = next_key();
    REGISTRY.table.lock().unwrap().insert(key, callable);
    key
}

/// Insert an arbitrary host Python object into the registry and return its key.
///
/// The host-throw path uses this to register the originating native
/// exception so the BAML→host decoder on the same runtime can resolve
/// the `_handle` slot of a `baml.errors.HostCallable` back to the
/// original Python object on round-trip. The table is shared with
/// callable entries (keys are globally unique), and the same
/// `host_release_callback` releases either kind on last-Arc-drop.
fn register_host_opaque(value: Py<PyAny>) -> u64 {
    let key = next_key();
    REGISTRY.table.lock().unwrap().insert(key, value);
    key
}

/// Remove and drop the registry entry for `host_value_key` (if present).
///
/// Shared by the engine-driven release path ([`host_release_callback`]) and
/// the encoder's rollback path ([`release_host_callable`]). Dropping the
/// `Py<PyAny>` requires the GIL.
fn drop_registry_entry(host_value_key: u64) {
    let popped: Option<Py<PyAny>> = match REGISTRY.table.lock() {
        Ok(mut t) => t.remove(&host_value_key),
        Err(e) => {
            // Poisoning means an earlier panic occurred while holding the
            // lock; the table is in an unknown state. Don't try to mutate
            // it (could double-drop the `Py<PyAny>` without the GIL), but
            // log so the originating panic is attributable instead of
            // being swallowed silently. We accept the entry leak: the
            // engine has already dropped its `Arc<HostValueArc>` (we're
            // on the release path), and a poisoned global registry
            // implies the process is in a failing state anyway.
            log::warn!(
                "host-callable registry mutex poisoned during release of key \
                 {host_value_key}: {e}; entry leaked"
            );
            return;
        }
    };
    if let Some(py_obj) = popped {
        // Attaching the GIL is required to drop a `Py<PyAny>`.
        Python::attach(|_py| drop(py_obj));
    }
}

/// Drop the Python callable associated with `host_value_key`.
///
/// Fires when the last Rust clone of the corresponding `HostValueArc` is
/// dropped — see `bex_external_types::host_value::host_release_dispatch`.
pub extern "C" fn host_release_callback(host_value_key: u64) {
    drop_registry_entry(host_value_key);
}

/// Look up the host-registered Python object referenced by a
/// `BamlPyHandle` whose `handle_type` is `HOST_VALUE_CALLABLE` /
/// `HOST_VALUE_OPAQUE`, returning a fresh strong reference if the entry
/// is still live. Used by the outbound error decoder in
/// `baml_bridge.proto` to rehydrate a `baml.errors.HostCallable` thrown
/// by BAML back to the original Python exception object on same-host
/// round-trip.
///
/// Returns `None` if the handle is the wrong kind, the entry has been
/// released (last `HostValueArc` clone already dropped), or the key
/// never existed in this runtime's registry (cross-runtime handle):
/// callers should fall back to a metadata-built exception in that case.
#[gen_stub_pyfunction]
#[pyfunction]
pub fn lookup_host_value(
    py: Python<'_>,
    handle: &crate::py_handle::BamlPyHandle,
) -> Option<Py<PyAny>> {
    use bridge_ctypes::baml_bridge::cffi::BamlHandleType;
    let ht_i32 = i32::try_from(handle.handle_type).ok()?;
    if ht_i32 != BamlHandleType::HostValueCallable as i32
        && ht_i32 != BamlHandleType::HostValueOpaque as i32
    {
        return None;
    }
    let table = match REGISTRY.table.lock() {
        Ok(t) => t,
        Err(e) => {
            // Poisoned: an earlier panic happened while holding the lock.
            // Return None (caller falls back to a metadata-built
            // exception) but log so the originating panic is attributable
            // instead of vanishing into an identity-loss bug report.
            log::warn!(
                "host-callable registry mutex poisoned during lookup of key \
                 {}: {e}; rehydration will fall back to metadata",
                handle.handle_key
            );
            return None;
        }
    };
    table.get(&handle.handle_key).map(|obj| obj.clone_ref(py))
}

/// Release a host callable the inbound encoder registered but never handed to
/// the engine — the encode-error rollback path.
///
/// Exposed to Python as `baml_py.release_host_callable(key)`. When
/// `encode_call_args` registers a callable for an early kwarg and then a
/// later kwarg fails to encode, the `CallFunctionArgs` is never sent, so the
/// engine never decodes — and so never releases — that key. Without this the
/// registry entry (holding a strong ref to the user callable) would leak for
/// the life of the process. The encoder calls this for every key it
/// registered during a failed encode.
#[gen_stub_pyfunction]
#[pyfunction]
pub fn release_host_callable(host_value_key: u64) {
    drop_registry_entry(host_value_key);
}

/// Dispatch a BAML→host call into Python.
///
/// `args` is a protobuf-encoded `BamlOutboundValue` whose variant is a
/// `BamlValueList` (see `sys_native::host_impls::call_host_value` —
/// args are wrapped as `BexExternalValue::Array` before encoding).
#[expect(
    clippy::not_unsafe_ptr_arg_deref,
    reason = "C ABI entry point: pointer validity is the caller's contract, documented \
              alongside the registered HostDispatchFn signature in bridge_cffi"
)]
pub extern "C" fn host_dispatch_callback(
    host_value_key: u64,
    call_id: u32,
    args: *const u8,
    length: usize,
) {
    // Copy the wire bytes into a Vec — the dispatch task may outlive the
    // caller's stack frame, and we need a `'static` slice anyway.
    let bytes: Vec<u8> = if length == 0 || args.is_null() {
        Vec::new()
    } else {
        // SAFETY: the engine guarantees `args` is valid for `length` bytes
        // for the duration of this call (see `host_dispatch::fire_dispatch`).
        unsafe { std::slice::from_raw_parts(args, length) }.to_vec()
    };

    // We're called on a tokio worker from the engine's sysop dispatch (see
    // `sys_native::host_dispatch::fire_dispatch`). Spawn a task so the
    // dispatch callback returns promptly and the engine can continue
    // making progress on other work while Python runs.
    //
    // The Python encode/decode and the user callable invocation happen
    // inside this task with the GIL held. Async user callables are run
    // to completion on a freshly-created asyncio loop inside the task —
    // we do not currently integrate with a Python event loop running
    // concurrently in another thread.
    // Resolve the callable *before* spawning so a missing entry (or a
    // poisoned registry mutex) surfaces as `BridgeFailure` — an
    // infrastructure fault — instead of an opaque `HostCallable` wrapping
    // a `PyKeyError`. The bridge knowing about a handle the dispatcher
    // can no longer find is a bug in the bridge, not a user error.
    // Mirrors `bridge_typescript::host_dispatch_callback`'s pre-spawn lookup.
    //
    // The `Py<PyAny>` is `Send + Sync` and survives moving into the
    // spawned task without holding the GIL; it's only re-attached inside
    // `dispatch_in_python` to invoke the callable.
    let callable: Py<PyAny> = match Python::attach(|py| -> Result<Py<PyAny>, String> {
        let table = REGISTRY
            .table
            .lock()
            .map_err(|e| format!("host-callable registry mutex poisoned: {e}"))?;
        match table.get(&host_value_key) {
            Some(c) => Ok(c.clone_ref(py)),
            None => Err(format!(
                "no host callable registered for key {host_value_key}"
            )),
        }
    }) {
        Ok(c) => c,
        Err(message) => {
            send_dispatch_bridge_failure(call_id, message);
            return;
        }
    };

    let Ok(handle) = tokio::runtime::Handle::try_current() else {
        // No tokio runtime is active — complete synchronously with an
        // error so the engine doesn't wedge on the in-flight call table.
        // This is an infrastructure fault (the bridge can't even schedule
        // the dispatch), not a user error → BridgeFailure → SdkPanic.
        send_dispatch_bridge_failure(
            call_id,
            "host_dispatch_callback called outside a tokio runtime context".to_string(),
        );
        return;
    };
    handle.spawn(async move {
        // A Rust-level *panic* (not a `PyErr`) inside `dispatch_in_python`
        // would unwind out of this task and silently drop the in-flight
        // `call_id`, leaving the engine awaiting it forever (there is no
        // timeout). Catch the unwind and complete the call with an error so
        // the engine always makes progress. `AssertUnwindSafe` is sound here:
        // on a caught panic we touch no state that the panic could have left
        // logically inconsistent — we only read `call_id` (a `Copy` `u32`)
        // and emit a fresh error payload. The normal `PyErr` path inside
        // `dispatch_in_python` is unaffected: it returns `Err(_)` and
        // completes the call itself, so `catch_unwind` sees `Ok(())`.
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            dispatch_in_python(callable, call_id, bytes);
        }));
        if let Err(panic) = outcome {
            let detail = panic_message(&panic);
            // A caught Rust-level panic in the dispatch task is a bridge
            // bug, not a user-callable exception → BridgeFailure → SdkPanic.
            send_dispatch_bridge_failure(
                call_id,
                format!("host callable dispatch panicked: {detail}"),
            );
        }
    });
}

/// Extract a human-readable message from a caught panic payload. Panic
/// payloads are most often `&str` or `String`; anything else is reported
/// generically.
fn panic_message(panic: &(dyn std::any::Any + Send)) -> String {
    if let Some(s) = panic.downcast_ref::<&str>() {
        (*s).to_string()
    } else if let Some(s) = panic.downcast_ref::<String>() {
        s.clone()
    } else {
        "<non-string panic payload>".to_string()
    }
}

/// Synchronous (under-GIL) helper called from the dispatch task. The
/// `callable` was resolved from the registry before the spawn so its
/// presence is guaranteed (missing-key faults surface as `BridgeFailure`
/// earlier in `host_dispatch_callback`).
fn dispatch_in_python(callable: Py<PyAny>, call_id: u32, args_bytes: Vec<u8>) {
    let result = Python::attach(|py| -> PyResult<Vec<u8>> {
        // Decode the engine-side `BamlToHostCall` into the callable's
        // positional args + supplied-optional kwargs.
        let (positional, kwargs) = decode_args(py, &args_bytes)?;

        // Invoke the user callable: required args positionally, supplied
        // optionals by keyword. Omitted optionals are absent, so the callable's
        // own defaults apply.
        let result_obj = callable.call(py, &positional, Some(&kwargs))?;

        // If the callable returned a coroutine (async function), run it to
        // completion on a fresh asyncio loop. Sync callables fall through.
        let final_result = run_if_coroutine(py, result_obj)?;

        // Encode the result as an `InboundValue` via `baml_bridge.proto`.
        encode_result_inbound(py, final_result)
    });

    match result {
        Ok(bytes) => send_dispatch_success(call_id, &bytes),
        Err(py_err) => Python::attach(|py| {
            send_dispatch_error_from_pyerr(call_id, py, &py_err);
        }),
    }
}

/// Decode the protobuf `BamlToHostCall` into the callable's positional args
/// (a `tuple`) and supplied-optional kwargs (a `dict`), each value decoded via
/// `baml_bridge.proto.decode_value`. The engine already resolved the call against
/// the callable's declared params and dropped omitted optionals, so `args` is a
/// flat declared-order list; partition it by each arg's `is_optional_arg` flag —
/// required args go positional, supplied optionals become kwargs keyed by
/// `arg_name`. Omitted optionals are absent, so the callable's own defaults
/// apply.
fn decode_args<'py>(
    py: Python<'py>,
    bytes: &[u8],
) -> PyResult<(Bound<'py, PyTuple>, Bound<'py, PyDict>)> {
    let outbound_pb2 = PyModule::import(py, "baml_bridge.cffi.v1.baml_outbound_pb2")?;
    let proto = PyModule::import(py, "baml_bridge.proto")?;
    let type_map = proto.getattr("get_type_map")?.call0()?;
    let decode_value = proto.getattr("decode_value")?;

    let to_host_call = outbound_pb2.getattr("BamlToHostCall")?.call0()?;
    to_host_call.call_method1("ParseFromString", (bytes,))?;

    let mut positional_items: Vec<Bound<'py, PyAny>> = Vec::new();
    let kwargs = PyDict::new(py);
    for arg in to_host_call.getattr("args")?.try_iter()? {
        let arg = arg?;
        let value = decode_value.call1((arg.getattr("value")?, &type_map))?;
        if arg.getattr("is_optional_arg")?.extract::<bool>()? {
            let name: String = arg.getattr("arg_name")?.extract()?;
            kwargs.set_item(name, value)?;
        } else {
            positional_items.push(value);
        }
    }
    let positional = PyTuple::new(py, positional_items)?;

    Ok((positional, kwargs))
}

/// If `value` is a coroutine, run it to completion on a fresh asyncio loop
/// and return the resolved result. Otherwise return `value` unchanged.
fn run_if_coroutine(py: Python<'_>, value: Py<PyAny>) -> PyResult<Py<PyAny>> {
    let bound = value.bind(py);
    let asyncio = PyModule::import(py, "asyncio")?;
    let is_coro: bool = asyncio.getattr("iscoroutine")?.call1((bound,))?.extract()?;
    if !is_coro {
        return Ok(value);
    }
    // Run the coroutine to completion on a new event loop in the current
    // thread. The bridge is on a tokio worker — Python sees a fresh loop
    // dedicated to this dispatch only.
    let new_loop = asyncio.getattr("new_event_loop")?.call0()?;
    let set_event_loop = asyncio.getattr("set_event_loop")?;
    let result = (|| -> PyResult<Py<PyAny>> {
        // Install the fresh loop as this thread's current loop for the run.
        // Coroutines (and libraries they use) commonly reach for the ambient
        // loop via `asyncio.get_event_loop()`/`ensure_future()`; on a non-main
        // thread with no loop set that raises "no current event loop in thread"
        // or binds work to an unrelated loop. Installing it first makes the
        // running loop and the thread's current loop agree.
        set_event_loop.call1((&new_loop,))?;
        Ok(new_loop
            .call_method1("run_until_complete", (bound,))?
            .unbind())
    })();
    // Close the loop and clear it as the thread's current loop regardless of
    // success/error. tokio reuses worker threads, so a leftover (now-closed)
    // current loop would poison the next dispatch that lands on this thread.
    let _ = new_loop.call_method0("close");
    let _ = set_event_loop.call1((py.None(),));
    result
}

/// Encode `value` as an `InboundValue` protobuf using
/// `baml_bridge.proto._set_inbound_value`, then serialize to bytes.
fn encode_result_inbound(py: Python<'_>, value: Py<PyAny>) -> PyResult<Vec<u8>> {
    let inbound_pb2 = PyModule::import(py, "baml_bridge.cffi.v1.baml_inbound_pb2")?;
    let proto = PyModule::import(py, "baml_bridge.proto")?;
    let holder = inbound_pb2.getattr("InboundValue")?.call0()?;
    // Track host callables registered while encoding so we can release them on
    // failure. A callback nested in the result (e.g. `{"cb": lambda x: x}`)
    // gets registered in the process-wide table before encoding finishes; if
    // encoding/serialization then aborts, the engine never receives these
    // bytes and nothing would release it — a leak. Mirrors the rollback
    // `encode_call_args` already does for the argument path.
    let registered = pyo3::types::PyList::empty(py);
    let kwargs_dict = pyo3::types::PyDict::new(py);
    kwargs_dict.set_item("kwarg_name", "<host-callable result>")?;
    kwargs_dict.set_item("registered", &registered)?;

    let encoded = (|| -> PyResult<Vec<u8>> {
        proto
            .getattr("_set_inbound_value")?
            .call((&holder, value), Some(&kwargs_dict))?;
        holder.call_method0("SerializeToString")?.extract()
    })();

    if encoded.is_err() {
        for item in registered.iter() {
            if let Ok(key) = item.extract::<u64>() {
                release_host_callable(key);
            }
        }
    }
    encoded
}

/// Send a `complete_host_call` success with the given `InboundValue`-encoded
/// payload. `complete_host_call` is `extern "C"` but not `unsafe` — the
/// invariants documented on its declaration (valid-for-length bytes) are
/// satisfied by `bytes`'s slice contract.
fn send_dispatch_success(call_id: u32, bytes: &[u8]) {
    complete_host_call(call_id, 0, bytes.as_ptr() as *const i8, bytes.len());
}

/// Build an `InboundValue` carrying a `baml.errors.HostCallable` Instance.
/// `handle_key` references the originating native exception in the
/// process-global registry (set by [`register_host_opaque`]); the BAML
/// class's `_handle` field carries it as a `BamlHandle` of type
/// `HOST_VALUE_OPAQUE` so a same-host decoder can rehydrate the exact
/// Python exception object on round-trip. The remaining
/// `class_name` / `message` / `language` / `traceback` fields are
/// metadata for debugging/printing/user convenience and do not
/// participate in error matching or rehydration.
fn build_host_callable_inbound(
    class_name: &str,
    message: &str,
    traceback: Option<&str>,
    handle_key: u64,
) -> InboundValue {
    fn string_field(key: &str, value: &str) -> InboundMapEntry {
        InboundMapEntry {
            key: Some(InboundMapKey::StringKey(key.to_string())),
            value: Some(InboundValue {
                value_type: None,
                value: Some(InboundValueVariant::StringValue(value.to_string())),
            }),
        }
    }
    let handle_field = InboundMapEntry {
        key: Some(InboundMapKey::StringKey("_handle".to_string())),
        value: Some(InboundValue {
            value_type: None,
            value: Some(InboundValueVariant::Handle(BamlHandle {
                key: handle_key,
                handle_type: BamlHandleType::HostValueOpaque as i32,
            })),
        }),
    };
    let mut fields = vec![
        string_field("message", message),
        string_field("class_name", class_name),
        string_field("language", "python"),
    ];
    if let Some(tb) = traceback {
        fields.push(string_field("traceback", tb));
    }
    fields.push(handle_field);
    InboundValue {
        value_type: Some(BamlTy {
            ty: Some(BamlTyVariant::ClassTy(BamlTyClass {
                name: "baml.errors.HostCallable".to_string(),
                type_args: vec![],
            })),
        }),
        value: Some(InboundValueVariant::ClassValue(InboundClassValue {
            fields,
        })),
    }
}

/// Complete an in-flight host call with `VmInternalError::BridgeFailure` —
/// the engine surfaces this as `baml.panics.SdkPanic` to the host SDK.
///
/// Use for *bridge-layer* faults: no tokio runtime when the dispatch fires,
/// a caught Rust panic inside the dispatch task, or a missing registry
/// entry (the engine knows about a handle the bridge no longer has). These
/// are infrastructure bugs, not user-code exceptions, so they must not
/// surface as catchable `BamlError(HostCallable(...))`. Mirrors the
/// `send_dispatch_error_*` family in `bridge_typescript`.
fn send_dispatch_bridge_failure(call_id: u32, message: String) {
    sys_native::host_dispatch::complete_with_error(
        call_id,
        sys_native::OpError::new(
            sys_native::SysOp::BamlHostCallHostValue,
            sys_native::VmInternalError::BridgeFailure { message },
        ),
    );
}

/// If `py_err` is a `baml_bridge.errors.BamlError` *or* `BamlPanic` carrying
/// a codegenned BAML value, encode the unwrapped value (`e.value`) as an
/// `InboundValue` — preserving its real BAML class identity so the BAML
/// caller can `catch (e: MyError)` and read fields just like a BAML-thrown
/// error. `BamlPanic.value` is normally a `baml.panics.*` class; the
/// engine's namespace-based routing turns that back into a panic on the
/// BAML side. (`BamlPanic` is a `BaseException`, not a `BamlError`
/// subclass, so it must be checked separately.)
///
/// Returns `Ok(None)` for:
/// - any other exception type — caller falls back to the opaque
///   `baml.errors.HostCallable` path;
/// - `BamlError(value=None)` / `BamlPanic(value=None)` — encoding `None`
///   would emit BAML `null`, which fails contract check for any concrete
///   `E` and produces a nonsensical `null` throw under `E=unknown`; the
///   opaque path always produces a well-formed `HostCallable` instance the
///   engine can route.
///
/// An `Err(_)` here (proto module missing, `_set_inbound_value` rejected
/// the value) is also collapsed to the opaque path by the caller — the
/// call always completes, even if the BAML-class identity is lost.
fn try_encode_baml_error_throw(py: Python<'_>, py_err: &pyo3::PyErr) -> PyResult<Option<Vec<u8>>> {
    let errors_mod = match PyModule::import(py, "baml_bridge.errors") {
        Ok(m) => m,
        // Defensive: missing module would be a packaging bug. Fall through.
        Err(_) => return Ok(None),
    };
    let baml_error_cls = errors_mod.getattr("BamlError")?;
    let baml_panic_cls = errors_mod.getattr("BamlPanic")?;
    let exc_value = py_err.value(py);
    let is_baml_error = exc_value.is_instance(&baml_error_cls)?;
    let is_baml_panic = exc_value.is_instance(&baml_panic_cls)?;
    if !is_baml_error && !is_baml_panic {
        return Ok(None);
    }

    // Unwrap the underlying value (the codegenned BAML pydantic model /
    // enum / primitive) and run it through the same encoder used for
    // host-call success results. `_set_inbound_value` already knows how
    // to map a pydantic instance to `InboundValue.Class(name=<BAML FQN>,
    // fields=…)` via `get_type_map().py_type_to_baml_type(type(value))`.
    let inner = exc_value.getattr("value")?;
    if inner.is_none() {
        // `BamlError(value=None)` is a bare wrapper — emit nothing here;
        // the caller will fall through to the opaque path.
        return Ok(None);
    }
    let inbound_pb2 = PyModule::import(py, "baml_bridge.cffi.v1.baml_inbound_pb2")?;
    let proto = PyModule::import(py, "baml_bridge.proto")?;
    let holder = inbound_pb2.getattr("InboundValue")?.call0()?;
    let registered = pyo3::types::PyList::empty(py);
    let kwargs_dict = pyo3::types::PyDict::new(py);
    kwargs_dict.set_item("kwarg_name", "<host-callable throw>")?;
    kwargs_dict.set_item("registered", &registered)?;

    let encoded = (|| -> PyResult<Vec<u8>> {
        proto
            .getattr("_set_inbound_value")?
            .call((&holder, &inner), Some(&kwargs_dict))?;
        holder.call_method0("SerializeToString")?.extract()
    })();

    if encoded.is_err() {
        for item in registered.iter() {
            if let Ok(key) = item.extract::<u64>() {
                release_host_callable(key);
            }
        }
    }
    encoded.map(Some)
}

/// Encode a Python exception as an `InboundValue` and send via
/// `complete_host_call`. Must be called under the GIL.
///
/// Branches on the exception type:
/// - `baml_bridge.errors.BamlError` → unwrap `.value` and emit it as its
///   real BAML class. The BAML caller's `catch (e: MyError)` matches.
/// - Anything else (native `ValueError`, `KeyError`, ...) → emit an
///   opaque `baml.errors.HostCallable` Instance carrying the four
///   metadata fields.
fn send_dispatch_error_from_pyerr(call_id: u32, py: Python<'_>, py_err: &pyo3::PyErr) {
    if let Ok(Some(bytes)) = try_encode_baml_error_throw(py, py_err) {
        complete_host_call(call_id, 1, bytes.as_ptr() as *const i8, bytes.len());
        return;
    }

    let class_name = py_err
        .get_type(py)
        .name()
        .map(|name| name.to_string())
        .unwrap_or_else(|_| "Exception".to_string());
    let message = py_err.to_string();
    let traceback = format_traceback(py, py_err);

    // Register the native Python exception in the process-global
    // host-value table so the BAML→host decoder on this runtime can
    // resolve `_handle` back to the original `ValueError`/`KeyError`/...
    // object on round-trip.
    let handle_key = register_host_opaque(py_err.value(py).clone().unbind().into_any());

    let bytes =
        build_host_callable_inbound(&class_name, &message, traceback.as_deref(), handle_key)
            .encode_to_vec();
    complete_host_call(call_id, 1, bytes.as_ptr() as *const i8, bytes.len());
}

/// Best-effort: format a Python traceback via the `traceback` stdlib
/// module. Returns `None` if the exception has no `__traceback__` (e.g.
/// constructed but not raised) or if formatting fails.
fn format_traceback(py: Python<'_>, py_err: &pyo3::PyErr) -> Option<String> {
    let tb = py_err.traceback(py)?;
    let traceback_mod = PyModule::import(py, "traceback").ok()?;
    let exc_type = py_err.get_type(py);
    let exc_value = py_err.value(py);
    let formatted = traceback_mod
        .getattr("format_exception")
        .ok()?
        .call1((exc_type, exc_value, tb))
        .ok()?;
    let lines: Vec<String> = formatted.extract().ok()?;
    Some(lines.concat())
}
