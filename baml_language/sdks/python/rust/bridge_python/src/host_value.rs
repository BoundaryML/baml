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
//!    `baml_core.proto._decode_value_holder` (already a list shape).
//! 3. Invokes the callable. If the return is a coroutine, runs it to
//!    completion on a fresh `asyncio` event loop in the same dispatch
//!    thread — the engine has released its heap permit via
//!    `block_in_place`, so blocking here is safe.
//! 4. Encodes the result into `InboundValue` bytes via
//!    `baml_core.proto.encode_call_args`-style serialization, then calls
//!    `bridge_cffi::complete_host_call(call_id, 0, ptr, len)`.
//! 5. On any Python exception, encodes a `HostCallableError` proto and
//!    calls `complete_host_call(call_id, 1, ptr, len)`.
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
use bridge_ctypes::baml_core::cffi::{HostCallableError, HostCallableErrorCategory};
use prost::Message;
use pyo3::{
    Py, PyAny, PyResult, Python,
    prelude::*,
    types::{PyAnyMethods, PyModule, PyTuple},
};

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
/// Called from the inbound encoder in `baml_core.proto` whenever a Python
/// callable appears as a kwarg.
#[pyfunction]
pub fn register_host_callable(callable: Py<PyAny>) -> u64 {
    let key = next_key();
    REGISTRY.table.lock().unwrap().insert(key, callable);
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
        Err(_) => return, // poisoned; nothing we can do safely
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
#[unsafe(no_mangle)]
pub extern "C" fn host_release_callback(host_value_key: u64) {
    drop_registry_entry(host_value_key);
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
#[pyfunction]
pub fn release_host_callable(host_value_key: u64) {
    drop_registry_entry(host_value_key);
}

/// Dispatch a BAML→host call into Python.
///
/// `args` is a protobuf-encoded `BamlOutboundValue` whose variant is a
/// `BamlValueList` (see `sys_native::host_impls::call_host_value` —
/// args are wrapped as `BexExternalValue::Array` before encoding).
#[unsafe(no_mangle)]
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
    let Ok(handle) = tokio::runtime::Handle::try_current() else {
        // No tokio runtime is active — complete synchronously with an
        // error so the engine doesn't wedge on the in-flight call table.
        send_dispatch_error(
            call_id,
            "RuntimeError",
            "host_dispatch_callback called outside a tokio runtime context",
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
            dispatch_in_python(host_value_key, call_id, bytes);
        }));
        if let Err(panic) = outcome {
            let detail = panic_message(&panic);
            send_dispatch_error(
                call_id,
                "panic",
                &format!("host callable dispatch panicked: {detail}"),
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

/// Synchronous (under-GIL) helper called from the dispatch task.
fn dispatch_in_python(host_value_key: u64, call_id: u32, args_bytes: Vec<u8>) {
    let result = Python::attach(|py| -> PyResult<Vec<u8>> {
        // Resolve the user callable. `clone_ref` keeps the Py<PyAny> in
        // the table — the entry only goes away when Rust releases the
        // last `HostValueArc` clone.
        let callable: Py<PyAny> = {
            let table = REGISTRY.table.lock().map_err(|_| {
                pyo3::exceptions::PyRuntimeError::new_err("host-callable registry poisoned")
            })?;
            match table.get(&host_value_key) {
                Some(c) => c.clone_ref(py),
                None => {
                    return Err(pyo3::exceptions::PyKeyError::new_err(format!(
                        "no host callable registered for key {host_value_key}"
                    )));
                }
            }
        };

        // Decode the engine-side `BamlOutboundValue` (a list shape) into
        // Python positional args via `baml_core.proto._decode_value_holder`.
        let positional = decode_args(py, &args_bytes)?;

        // Invoke the user callable with the decoded positional args.
        let result_obj = callable.call1(py, &positional)?;

        // If the callable returned a coroutine (async function), run it to
        // completion on a fresh asyncio loop. Sync callables fall through.
        let final_result = run_if_coroutine(py, result_obj)?;

        // Encode the result as an `InboundValue` via `baml_core.proto`.
        encode_result_inbound(py, final_result)
    });

    match result {
        Ok(bytes) => send_dispatch_success(call_id, &bytes),
        Err(py_err) => Python::attach(|py| {
            send_dispatch_error_from_pyerr(call_id, py, &py_err);
        }),
    }
}

/// Decode the protobuf `BamlOutboundValue` args (a list shape) into a
/// Python `tuple` of positional arguments by calling
/// `baml_core.proto.decode_value`.
fn decode_args<'py>(py: Python<'py>, bytes: &[u8]) -> PyResult<Bound<'py, PyTuple>> {
    let outbound_pb2 = PyModule::import(py, "baml_core.cffi.v1.baml_outbound_pb2")?;
    let proto = PyModule::import(py, "baml_core.proto")?;

    // Parse the outer BamlOutboundValue.
    let holder = outbound_pb2.getattr("BamlOutboundValue")?.call0()?;
    holder.call_method1("ParseFromString", (bytes,))?;

    // The args list lives in `holder.list_value.items`. `proto.decode_value`
    // decodes a `BamlOutboundValue` into a Python value given the active type
    // map; for the args holder (a `list_value`) it yields a Python list.
    let type_map = proto.getattr("get_type_map")?.call0()?;
    let decoded = proto.getattr("decode_value")?.call1((holder, type_map))?;

    // `decode_value` returns `list` for a list-value. Convert to
    // tuple for `callable.call1(args)`.
    let positional: Bound<'py, PyTuple> = match decoded.extract::<Vec<Py<PyAny>>>() {
        Ok(items) => PyTuple::new(py, items)?,
        Err(_) => {
            return Err(pyo3::exceptions::PyTypeError::new_err(
                "host-callable args decoded to a non-list value",
            ));
        }
    };
    Ok(positional)
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
    let result = match new_loop.call_method1("run_until_complete", (bound,)) {
        Ok(v) => Ok(v.unbind()),
        Err(e) => Err(e),
    };
    // Close the loop regardless of success/error to release its resources.
    let _ = new_loop.call_method0("close");
    result
}

/// Encode `value` as an `InboundValue` protobuf using
/// `baml_core.proto._set_inbound_value`, then serialize to bytes.
fn encode_result_inbound(py: Python<'_>, value: Py<PyAny>) -> PyResult<Vec<u8>> {
    let inbound_pb2 = PyModule::import(py, "baml_core.cffi.v1.baml_inbound_pb2")?;
    let proto = PyModule::import(py, "baml_core.proto")?;
    let holder = inbound_pb2.getattr("InboundValue")?.call0()?;
    let kwargs_dict = pyo3::types::PyDict::new(py);
    kwargs_dict.set_item("kwarg_name", "<host-callable result>")?;
    proto
        .getattr("_set_inbound_value")?
        .call((&holder, value), Some(&kwargs_dict))?;
    let bytes_obj = holder.call_method0("SerializeToString")?;
    bytes_obj.extract()
}

/// Send a `complete_host_call` success with the given `InboundValue`-encoded
/// payload. `complete_host_call` is `extern "C"` but not `unsafe` — the
/// invariants documented on its declaration (valid-for-length bytes) are
/// satisfied by `bytes`'s slice contract.
fn send_dispatch_success(call_id: u32, bytes: &[u8]) {
    complete_host_call(call_id, 0, bytes.as_ptr() as *const i8, bytes.len());
}

/// Encode a plain `(class_name, message)` host error and send via
/// `complete_host_call`.
fn send_dispatch_error(call_id: u32, class_name: &str, message: &str) {
    let err = HostCallableError {
        class_name: class_name.to_string(),
        message: message.to_string(),
        traceback: None,
        language: Some("python".to_string()),
        category: HostCallableErrorCategory::HostCallableHostError as i32,
    };
    let bytes = err.encode_to_vec();
    complete_host_call(call_id, 1, bytes.as_ptr() as *const i8, bytes.len());
}

/// Encode a Python exception into `HostCallableError` and send via
/// `complete_host_call`. Must be called under the GIL.
fn send_dispatch_error_from_pyerr(call_id: u32, py: Python<'_>, py_err: &pyo3::PyErr) {
    let class_name = py_err
        .get_type(py)
        .name()
        .map(|name| name.to_string())
        .unwrap_or_else(|_| "Exception".to_string());
    let message = py_err.to_string();
    let traceback = format_traceback(py, py_err);

    let err = HostCallableError {
        class_name,
        message,
        traceback,
        language: Some("python".to_string()),
        category: HostCallableErrorCategory::HostCallableHostError as i32,
    };
    let bytes = err.encode_to_vec();
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
