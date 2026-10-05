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
//!    completion on the originating SDK entry's application loop (async
//!    entry) or a bridge-owned loop with copied application context (sync
//!    entry). Synchronous callbacks run on the originating Python thread.
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

use std::sync::LazyLock;

use bridge_cffi::complete_host_call;
use bridge_ctypes::baml_bridge::cffi::{
    BamlHandle, BamlHandleType, BamlOutboundValue, BamlToHostCall, BamlTy, BamlTyClass,
    InboundClassValue, InboundMapEntry, InboundValue, baml_outbound_value::Value as OutboundValue,
    baml_ty::Ty as BamlTyVariant, inbound_map_entry::Key as InboundMapKey,
    inbound_value::Value as InboundValueVariant,
};
use prost::Message;
use pyo3::{
    Bound, Py, PyAny, PyResult, Python,
    prelude::*,
    types::{PyAnyMethods, PyDict, PyList, PyModule, PyTuple},
};
use pyo3_stub_gen::derive::gen_stub_pyfunction;

/// Process-wide table of Python callables that have been handed to BAML.
///
/// The key is a freshly-allocated `u64` (never 0). Removal happens in
/// [`host_release_callback`] when Rust drops its last clone of the
/// corresponding `HostValueArc`.
static REGISTRY: LazyLock<bridge_ctypes::HostValueRegistry<Py<PyAny>>> =
    LazyLock::new(bridge_ctypes::HostValueRegistry::default);

/// Insert a Python callable into the registry and return its key.
///
/// Exposed to Python as `baml_py.register_host_callable(callable) -> int`.
/// Called from the inbound encoder in `baml_bridge.proto` whenever a Python
/// callable appears as a kwarg.
#[gen_stub_pyfunction]
#[pyfunction]
#[pyo3(signature = (callable, marker=None))]
pub fn register_host_callable(
    callable: Py<PyAny>,
    marker: Option<&crate::py_handle::BamlPyHandle>,
) -> PyResult<u64> {
    let descriptor = bridge_cffi::host_instrumentation::registration_marker(
        marker.map_or(0, |handle| handle.handle_key),
        "python",
    )
    .map_err(crate::errors::bridge_error_to_sdk_panic)?;
    let key = REGISTRY.insert(callable);
    bex_project::HostValueArc::register_annotation(key, descriptor);
    Ok(key)
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
    REGISTRY.insert(value)
}

/// Remove and drop the registry entry for `host_value_key` (if present).
///
/// Shared by the engine-driven release path ([`host_release_callback`]) and
/// the encoder's rollback path ([`release_host_callable`]). Dropping the
/// `Py<PyAny>` requires the GIL.
fn drop_registry_entry(host_value_key: u64) {
    bex_project::HostValueArc::discard_annotation(host_value_key);
    let popped: Option<Py<PyAny>> = match REGISTRY.lock() {
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
    let table = match REGISTRY.lock() {
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

/// Travels with queued/running work, independently of its BAML waiter. Python
/// explicitly finishes it after task exit; dropping an abandoned queue item
/// also releases its undecoded wire arguments and native execution lease.
#[pyo3::pyclass]
pub(crate) struct HostExecutionLease {
    call_id: u32,
    args: Option<Vec<u8>>,
    frame: Option<Box<bridge_ctypes::OwnedHostInvocation>>,
    execution: Option<sys_native::host_dispatch::HostExecutionGuard>,
}

impl HostExecutionLease {
    fn new(
        call_id: u32,
        args: Vec<u8>,
        frame: bridge_ctypes::OwnedHostInvocation,
        execution: sys_native::host_dispatch::HostExecutionGuard,
    ) -> Self {
        Self {
            call_id,
            args: Some(args),
            frame: Some(Box::new(frame)),
            execution: Some(execution),
        }
    }
}

#[pyo3::pymethods]
impl HostExecutionLease {
    fn start(&mut self) -> bool {
        if sys_native::host_dispatch::start_execution(self.call_id).is_none() {
            return false;
        }
        self.args = None; // The decoder owns untransferred wire references.
        true
    }

    fn frame(&self) -> PyResult<(crate::py_handle::BamlPyHandle, Vec<u8>)> {
        let frame = self
            .frame
            .as_ref()
            .ok_or_else(|| pyo3::exceptions::PyRuntimeError::new_err("callback already exited"))?;
        let key =
            crate::py_handle::handle_clone(frame.0.effective_state, "callback invocation capture")?;
        let state =
            crate::py_handle::BamlPyHandle::new(key, BamlHandleType::InvocationState as u64);
        let cancel = frame.0.cancel.as_ref().ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err(
                "callback has no effective cancellation token",
            )
        })?;
        Ok((
            state,
            bridge_cffi::control_projection::clone_outbound(cancel)
                .map_err(crate::errors::bridge_error_to_sdk_panic)?
                .encode_to_vec(),
        ))
    }

    fn finish(&mut self) {
        if let Some(args) = self.args.take() {
            discard_host_call_args(&args);
        }
        drop(self.frame.take());
        drop(self.execution.take());
    }
}

impl Drop for HostExecutionLease {
    fn drop(&mut self) {
        self.finish();
    }
}

/// Dispatch a BAML→host call into Python.
///
/// The borrowed V2 `HostInvocation` carries application arguments, effective
/// invocation state, cancellation, the absolute deadline, and host context.
pub(crate) extern "C" fn host_dispatch_callback(request: *const u8, length: usize) {
    if request.is_null() || length > isize::MAX as usize {
        return;
    }
    // SAFETY: V2 dispatch borrows runtime-owned bytes for this call.
    let bytes = unsafe { std::slice::from_raw_parts(request, length) };
    let frame = match bridge_cffi::invocation_protocol::decode_host_invocation(bytes) {
        Ok(frame) => bridge_ctypes::OwnedHostInvocation(frame),
        Err(_) => return,
    };
    let call_id = frame.0.callback_id;
    let key = frame.0.host_value_key;
    let args = frame.0.application_args.clone();
    catch_dispatch_panic(call_id, || {
        dispatch_host_callable(key, call_id, args.as_ptr(), args.len(), frame)
    });
}

fn dispatch_host_callable(
    host_value_key: u64,
    call_id: u32,
    args: *const u8,
    length: usize,
    frame: bridge_ctypes::OwnedHostInvocation,
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

    let Some(execution) = sys_native::host_dispatch::retain_execution(call_id) else {
        discard_host_call_args(&bytes);
        sys_native::host_dispatch::finish_abi_execution(call_id);
        return;
    };

    // Resolve registration before handing execution to the SDK entry's queue
    // or application loop. Registration holds only the callable, never its
    // parent, thread, Context or loop. Missing registry entries are bridge
    // failures rather than user-code exceptions.
    let callable: Py<PyAny> = match Python::attach(|py| -> Result<Py<PyAny>, String> {
        let table = REGISTRY
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
            discard_host_call_args(&bytes);
            send_dispatch_bridge_failure(call_id, message);
            return;
        }
    };

    crate::callback_dispatch::route(crate::callback_dispatch::Dispatch {
        callable,
        call_id,
        execution: HostExecutionLease::new(call_id, bytes.clone(), frame, execution),
        args: bytes,
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

/// Service a sync entry's dispatch on its calling Python thread. An async
/// result runs on a bridge-owned loop with the caller's current Context copied
/// explicitly; the caller may already be blocking an application loop.
pub(crate) fn dispatch_sync(dispatch: crate::callback_dispatch::Dispatch) {
    let call_id = dispatch.call_id;
    let mut execution = Some(dispatch.execution);
    if !execution.as_mut().unwrap().start() {
        return;
    }
    catch_dispatch_panic(call_id, || {
        let result = Python::attach(|py| -> PyResult<Option<Vec<u8>>> {
            let frame = execution.as_ref().unwrap().frame()?;
            let value = PyModule::import(py, "baml_bridge._dispatch")?
                .getattr("_invoke_with_frame")?
                .call1((dispatch.callable, dispatch.args, frame))?
                .unbind();
            let is_coroutine: bool = PyModule::import(py, "asyncio")?
                .getattr("iscoroutine")?
                .call1((value.bind(py),))?
                .extract()?;
            if !is_coroutine {
                return encode_result_inbound(py, call_id, value).map(Some);
            }
            let context = PyModule::import(py, "contextvars")?
                .getattr("copy_context")?
                .call0()?
                .unbind();
            let rt = bridge_cffi::get_tokio_runtime()
                .map_err(crate::errors::bridge_error_to_sdk_panic)?;
            let execution = execution.take();
            rt.spawn_blocking(move || {
                let _execution = execution;
                catch_dispatch_panic(call_id, || {
                    let result = Python::attach(|py| -> PyResult<Vec<u8>> {
                        let runner = PyModule::import(py, "baml_bridge._dispatch")?
                            .getattr("_run_sync_coroutine")?;
                        let value = context
                            .bind(py)
                            .call_method1("run", (runner, call_id, value.bind(py)))?
                            .unbind();
                        encode_result_inbound(py, call_id, value)
                    });
                    complete_dispatch_result(call_id, result);
                })
            });
            Ok(None)
        });
        match result {
            Ok(None) => {}
            Ok(Some(bytes)) => send_dispatch_success(call_id, &bytes),
            Err(error) => Python::attach(|py| send_dispatch_error_from_pyerr(call_id, py, &error)),
        }
    });
}

fn catch_dispatch_panic(call_id: u32, action: impl FnOnce()) {
    if let Err(panic) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(action)) {
        send_dispatch_bridge_failure(
            call_id,
            format!("host callable dispatch panicked: {}", panic_message(&panic)),
        );
    }
}

fn complete_dispatch_result(call_id: u32, result: PyResult<Vec<u8>>) {
    match result {
        Ok(bytes) => send_dispatch_success(call_id, &bytes),
        Err(error) => Python::attach(|py| send_dispatch_error_from_pyerr(call_id, py, &error)),
    }
}

/// Private Python scheduler entrypoints. Decoding, callable invocation and
/// result encoding run on the selected Python thread, with the original error
/// object preserved by the existing host-value transport.
#[gen_stub_pyfunction]
#[pyfunction]
pub fn _invoke_host_callable(
    py: Python<'_>,
    callable: Py<PyAny>,
    args: Vec<u8>,
) -> PyResult<Py<PyAny>> {
    let (positional, kwargs) = decode_args(py, &args)?;
    callable.call(py, &positional, Some(&kwargs))
}

/// Connect cancellation to the task's actual owning loop. If cancellation
/// already removed this dispatch, Python must not start its body. The hook
/// keeps the loop alive independently of the SDK entry's environment guard.
#[gen_stub_pyfunction]
#[pyfunction]
pub fn _register_host_call_execution(
    py: Python<'_>,
    call_id: u32,
    event_loop: Py<PyAny>,
) -> PyResult<bool> {
    let cancel = PyModule::import(py, "baml_bridge._dispatch")?
        .getattr("_cancel_dispatch")?
        .unbind();
    Ok(sys_native::host_dispatch::set_cancellation_hook(
        call_id,
        move || {
            catch_dispatch_panic(call_id, || {
                Python::attach(|py| {
                    if let Err(error) = event_loop
                        .bind(py)
                        .call_method1("call_soon_threadsafe", (cancel.bind(py), call_id))
                    {
                        // A closed application loop cannot service cancellation;
                        // eviction has already ended the engine's waiter.
                        log::warn!("could not cancel Python host call {call_id}: {error}");
                    }
                });
            });
        },
    ))
}

/// Release wire owners of callbacks that never started, without materializing
/// application classes or invoking Python validators. Host-owned keys are
/// borrowed metadata and belong to the engine's HostValueArc, not HANDLE_TABLE.
pub(crate) fn discard_host_call_args(args: &[u8]) {
    fn discard(value: BamlOutboundValue) {
        match value.value {
            Some(OutboundValue::HandleValue(handle)) => {
                if handle.handle_type != BamlHandleType::HostValueCallable as i32
                    && handle.handle_type != BamlHandleType::HostValueOpaque as i32
                {
                    let _ = bridge_cffi::handle_cffi::release_handle(handle.key);
                }
            }
            Some(OutboundValue::ListValue(list)) => {
                for value in list.items {
                    discard(value);
                }
            }
            Some(OutboundValue::MapValue(map)) => {
                for entry in map.entries {
                    if let Some(value) = entry.value {
                        discard(value);
                    }
                }
            }
            Some(OutboundValue::ClassValue(class)) => {
                for field in class.fields {
                    if let Some(value) = field.value {
                        discard(value);
                    }
                }
            }
            Some(OutboundValue::UnionVariantValue(union)) => {
                if let Some(value) = union.value {
                    discard(*value);
                }
            }
            _ => {}
        }
    }
    if let Ok(call) = BamlToHostCall::decode(args) {
        for arg in call.args {
            if let Some(value) = arg.value {
                discard(value);
            }
        }
    }
    bex_project::host_release_dispatch::drain();
}

#[gen_stub_pyfunction]
#[pyfunction]
pub fn _discard_host_call_args(args: Vec<u8>) {
    discard_host_call_args(&args);
}

#[gen_stub_pyfunction]
#[pyfunction]
pub fn _complete_host_call_success(py: Python<'_>, call_id: u32, value: Py<PyAny>) {
    catch_dispatch_panic(call_id, || {
        complete_dispatch_result(call_id, encode_result_inbound(py, call_id, value));
    });
}

#[gen_stub_pyfunction]
#[pyfunction]
pub fn _complete_host_call_error(py: Python<'_>, call_id: u32, error: Py<PyAny>) {
    catch_dispatch_panic(call_id, || {
        send_dispatch_error_from_pyerr(call_id, py, &pyo3::PyErr::from_value(error.into_bound(py)));
    });
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
    let decoder = PyModule::import(py, "baml_bridge.proto")
        .and_then(|proto| proto.getattr("_decode_host_call_args"))
        .inspect_err(|_| discard_host_call_args(bytes))?;
    // The decoder releases any individual references that did not reach a
    // host wrapper, including failures partway through a nested argument.
    decoder.call1((bytes,))?.extract()
}

/// Encode `value` as an `InboundValue` protobuf using
/// `baml_bridge.proto._set_inbound_value`, then serialize to bytes.
fn encode_result_inbound(py: Python<'_>, call_id: u32, value: Py<PyAny>) -> PyResult<Vec<u8>> {
    let observation = bridge_cffi::host_instrumentation::callback(call_id).map(|owner| {
        let outcome = btel_types::InvocationOutcome::Ok;
        let capture = owner
            .wants_value(outcome)
            .then(|| crate::host_capture::capture(value.bind(py)));
        (owner, capture)
    });

    let inbound_pb2 = PyModule::import(py, "baml_bridge.cffi.v1.baml_inbound_pb2")?;
    let proto = PyModule::import(py, "baml_bridge.proto")?;
    let holder = inbound_pb2.getattr("InboundValue")?.call0()?;
    // Track both ownership kinds created while encoding: host callables in the
    // Python registry and ordinary HANDLE_TABLE keys cloned for wire transfer.
    // If a later nested value fails, the engine never receives the bytes and
    // cannot release/drain either kind. Mirrors `encode_call_args`.
    let registered = pyo3::types::PyList::empty(py);
    let cloned_handles = pyo3::types::PyList::empty(py);
    let kwargs_dict = pyo3::types::PyDict::new(py);
    kwargs_dict.set_item("kwarg_name", "<host-callable result>")?;
    kwargs_dict.set_item("registered", &registered)?;
    kwargs_dict.set_item("cloned_handles", &cloned_handles)?;

    let encoded = (|| -> PyResult<Vec<u8>> {
        proto
            .getattr("_set_inbound_value")?
            .call((&holder, value), Some(&kwargs_dict))?;
        holder.call_method0("SerializeToString")?.extract()
    })();

    if encoded.is_err() {
        rollback_failed_encode(&registered, &cloned_handles);
    } else if let Some((owner, capture)) = observation {
        // Encoding failures are observed by the existing dispatch error path.
        owner.observe(btel_types::InvocationOutcome::Ok, capture);
    }
    encoded
}

fn rollback_failed_encode(registered: &Bound<'_, PyList>, cloned_handles: &Bound<'_, PyList>) {
    for item in registered.iter() {
        if let Ok(key) = item.extract::<u64>() {
            release_host_callable(key);
        }
    }
    for item in cloned_handles.iter() {
        if let Ok(key) = item.extract::<u64>() {
            let _ = bridge_cffi::handle_cffi::release_handle(key);
        }
    }
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
    } else {
        // Nullable is still a required field. Synthetic cancellation errors
        // (e.g. a task cancelled before first execution) have no traceback.
        fields.push(InboundMapEntry {
            key: Some(InboundMapKey::StringKey("traceback".to_owned())),
            value: Some(InboundValue::default()),
        });
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
/// Use for *bridge-layer* faults: unavailable callback execution environment,
/// a caught Rust panic inside the dispatch task, or a missing registry
/// entry (the engine knows about a handle the bridge no longer has). These
/// are infrastructure bugs, not user-code exceptions, so they must not
/// surface as catchable `BamlError(HostCallable(...))`. Mirrors the
/// `send_dispatch_error_*` family in `bridge_typescript`.
pub(crate) fn send_dispatch_bridge_failure(call_id: u32, message: String) {
    sys_native::host_dispatch::complete_with_error(
        call_id,
        sys_native::OpError::new(
            sys_native::SysOp::BamlHostCallHostValue,
            sys_native::VmInternalError::BridgeFailure { message },
        ),
    );
}

/// If `py_err` is a `baml_bridge.errors.BamlError` carrying a codegenned
/// BAML value, encode the unwrapped value (`e.value`) as an `InboundValue` —
/// preserving its real BAML class identity so the BAML caller can
/// `catch (e: MyError)` and read fields just like a BAML-thrown error. A
/// `BamlPanic` is a `BamlError` whose `.value` is normally a `baml.panics.*`
/// class; the engine's namespace-based routing turns that back into a panic
/// on the BAML side.
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
    let exc_value = py_err.value(py);
    if !exc_value.is_instance(&baml_error_cls)? {
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
    let cloned_handles = pyo3::types::PyList::empty(py);
    let kwargs_dict = pyo3::types::PyDict::new(py);
    kwargs_dict.set_item("kwarg_name", "<host-callable throw>")?;
    kwargs_dict.set_item("registered", &registered)?;
    kwargs_dict.set_item("cloned_handles", &cloned_handles)?;

    let encoded = (|| -> PyResult<Vec<u8>> {
        proto
            .getattr("_set_inbound_value")?
            .call((&holder, &inner), Some(&kwargs_dict))?;
        holder.call_method0("SerializeToString")?.extract()
    })();

    if encoded.is_err() {
        rollback_failed_encode(&registered, &cloned_handles);
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
    if let Some(owner) = bridge_cffi::host_instrumentation::callback(call_id) {
        let cancelled = PyModule::import(py, "asyncio")
            .and_then(|module| module.getattr("CancelledError"))
            .is_ok_and(|kind| py_err.matches(py, kind).unwrap_or(false));
        let outcome = if cancelled {
            btel_types::InvocationOutcome::Cancelled
        } else {
            btel_types::InvocationOutcome::Errored
        };
        let capture = if owner.wants_value(outcome) {
            PyModule::import(py, "baml_bridge._instrumentation")
                .and_then(|module| module.getattr("_exception_capture"))
                .and_then(|capture| capture.call1((py_err.value(py),)))
                .ok()
                .map(|value| crate::host_capture::capture(&value))
        } else {
            None
        };
        owner.observe(outcome, capture);
    }

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
