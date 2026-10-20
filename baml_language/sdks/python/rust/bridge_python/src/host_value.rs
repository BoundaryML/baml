//! Per-process Python host-value registry.
//!
//! When Python passes a callable as an argument to a BAML function, the
//! inbound encoder (`crate::encode`) calls [`register_callable`] with the
//! function type of the slot, obtains a `u64` key and emits
//! `InboundValue::Handle{key, HOST_VALUE_CALLABLE}` with that function type
//! stated in `value_type`. Rust's `inbound_to_external` decoder constructs a
//! `BexExternalValue::HostCallable`; the engine binds it to an
//! `Object::HostClosure` of that type; later when BAML invokes it, the
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
//! 4. Encodes the result against the return type of the slot the callable
//!    was bound to and completes the call with it
//!    (`bridge_cffi::complete_host_call`, a `HostCallCompletion`).
//! 5. On any Python exception, branches on the exception type:
//!    - A `baml_bridge.errors.BamlError` or `BamlPanic` carrying a BAML
//!      value is unwrapped (`.value`) and encoded against the slot's throws
//!      type, so a BAML `catch (e: MyError)` matches it; a panic value is a
//!      panic whatever the slot declares.
//!    - Anything else (native `ValueError`, `KeyError`, ...) is registered
//!      in the process-global host-value table and completes the call as
//!      the host's own exception: the engine builds the
//!      `baml.errors.HostCallable` whose `_handle` references the original
//!      Python exception object, so the BAML→host decoder on the same
//!      runtime can rehydrate it on round-trip.
//!
//!    The engine holds both to the callable's declared throws type and
//!    builds every panic itself.
//!
//! When the engine drops the last Rust clone of the `HostValueArc`,
//! [`host_release_callback`] fires and removes the registry entry — the
//! Python callable's refcount drops to zero and the GC reclaims it.

use std::{
    collections::HashMap,
    sync::{LazyLock, Mutex, PoisonError},
};

use bridge_cffi::complete_host_call;
use bridge_ctypes::{
    HostKey, WireHandle,
    baml_bridge::cffi::{
        BamlHandleType, BamlOutboundValue, BamlToHostCall, HostCallCompletion,
        baml_outbound_value::Value as OutboundValue,
    },
};
use bridge_encode::{BridgeFault, Completion, Encoded};
use prost::Message;
use pyo3::{
    Bound, Py, PyAny, PyResult, Python,
    prelude::*,
    types::{PyAnyMethods, PyDict, PyModule, PyTuple},
};
use pyo3_stub_gen::derive::gen_stub_pyfunction;

/// A Python object handed to BAML: a callable bound to a function slot, or an
/// opaque object (a thrown exception) referenced by handle.
struct Registered {
    object: Py<PyAny>,
    /// The function type of the slot a callable was bound to. Its result and
    /// thrown value are encoded against this. `None` for opaque objects.
    signature: Option<bex_project::RuntimeTy>,
}

/// Process-wide table of Python objects that have been handed to BAML.
///
/// The key is the engine's reference to the object (never 0). Removal happens in
/// [`host_release_callback`] when Rust drops its last clone of the
/// corresponding `HostValueArc`.
static REGISTRY: LazyLock<bridge_ctypes::HostValueRegistry<Registered>> =
    LazyLock::new(bridge_ctypes::HostValueRegistry::default);

/// The signature of each host call in flight, by callback id, from dispatch
/// until its result or error is encoded.
static CALLBACK_SIGNATURES: LazyLock<Mutex<HashMap<u32, Option<bex_project::RuntimeTy>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

// BUG: a completion whose call has no stored signature (or one that is not a
// function type) is a bridge invariant violation; both completion paths
// currently fall back to encoding against `unknown` instead of completing
// the call as a bridge failure.
fn take_callback_signature(call_id: u32) -> Option<bex_project::RuntimeTy> {
    CALLBACK_SIGNATURES
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .remove(&call_id)
        .flatten()
}

/// Insert a Python callable into the registry, bound to `signature`, and
/// return its key. `marker_key` is the host marker's handle when the callable
/// was defined through the instrumentation surface.
pub(crate) fn register_callable(
    callable: Py<PyAny>,
    marker_key: Option<u64>,
    signature: bex_project::RuntimeTy,
) -> PyResult<HostKey> {
    let descriptor =
        bridge_cffi::host_instrumentation::registration_marker(marker_key.unwrap_or(0), "python")
            .map_err(crate::errors::bridge_error_to_sdk_panic)?;
    let key = REGISTRY.insert(Registered {
        object: callable,
        signature: Some(signature),
    });
    bex_project::HostValueArc::register_annotation(key, descriptor);
    Ok(key)
}

/// How many Python objects BAML currently holds through the registry. For
/// tests that check a failed encode released what it registered.
#[gen_stub_pyfunction]
#[pyfunction]
pub fn _host_value_registry_len() -> usize {
    REGISTRY
        .lock()
        .map(|table| table.len())
        .unwrap_or_else(|poisoned| poisoned.into_inner().len())
}

/// Insert an arbitrary host Python object into the registry and return its key.
///
/// The host-throw path uses this to register the originating native
/// exception so the BAML→host decoder on the same runtime can resolve
/// the `_handle` slot of a `baml.errors.HostCallable` back to the
/// original Python object on round-trip. The table is shared with
/// callable entries (keys are globally unique), and the same
/// `host_release_callback` releases either kind on last-Arc-drop.
fn register_host_opaque(value: Py<PyAny>) -> HostKey {
    REGISTRY.insert(Registered {
        object: value,
        signature: None,
    })
}

/// Remove and drop the registry entry for `host_value_key` (if present).
///
/// Shared by the engine-driven release path ([`host_release_callback`]) and
/// the encoder's rollback path ([`release_registered`]). Dropping the
/// `Py<PyAny>` requires the GIL.
fn drop_registry_entry(host_value_key: HostKey) {
    bex_project::HostValueArc::discard_annotation(host_value_key);
    let popped: Option<Registered> = match REGISTRY.lock() {
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
    if let Some(entry) = popped {
        // Attaching the GIL is required to drop a `Py<PyAny>`.
        Python::attach(|_py| drop(entry));
    }
}

/// Drop the Python callable associated with `host_value_key`.
///
/// Fires when the last Rust clone of the corresponding `HostValueArc` is
/// dropped — see `bex_external_types::host_value::host_release_dispatch`.
pub extern "C" fn host_release_callback(host_value_key: u64) {
    if let Some(key) = HostKey::new(host_value_key) {
        drop_registry_entry(key);
    }
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
    table
        .get(&HostKey::new(handle.handle_key)?)
        .map(|entry| entry.object.clone_ref(py))
}

/// Release a host callable the inbound encoder registered but never handed to
/// the engine — the encode-error rollback path.
///
/// When an encode registers a callable for an early argument and a later
/// argument fails, the request is never sent, so the engine never decodes —
/// and so never releases — that key. Without this the registry entry
/// (holding a strong ref to the user callable) would leak for the life of
/// the process.
pub(crate) fn release_registered(host_value_key: HostKey) {
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
        match HostKey::new(host_value_key).and_then(|key| table.get(&key)) {
            Some(entry) => {
                // The result is encoded against the slot's signature once the
                // callable returns; keep it by callback id until then.
                CALLBACK_SIGNATURES
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .insert(call_id, entry.signature.clone());
                Ok(entry.object.clone_ref(py))
            }
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
        let result = Python::attach(|py| -> PyResult<()> {
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
                complete_with_value(py, call_id, value.bind(py));
                return Ok(());
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
                    let result = Python::attach(|py| -> PyResult<()> {
                        let runner = PyModule::import(py, "baml_bridge._dispatch")?
                            .getattr("_run_sync_coroutine")?;
                        let value = context
                            .bind(py)
                            .call_method1("run", (runner, call_id, value.bind(py)))?
                            .unbind();
                        complete_with_value(py, call_id, value.bind(py));
                        Ok(())
                    });
                    complete_dispatch_result(call_id, result);
                })
            });
            Ok(())
        });
        complete_dispatch_result(call_id, result);
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

/// A dispatch that raised before its callable's value could be completed
/// with: the error is the call's result.
fn complete_dispatch_result(call_id: u32, result: PyResult<()>) {
    if let Err(error) = result {
        Python::attach(|py| send_dispatch_error_from_pyerr(call_id, py, &error));
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
/// application classes or invoking Python validators. A host value is the
/// engine's reference, not an owner of this frame's: nothing to release.
pub(crate) fn discard_host_call_args(args: &[u8]) {
    fn discard(value: BamlOutboundValue) {
        match value.value {
            Some(OutboundValue::HandleValue(handle)) => {
                if let Ok(WireHandle::Engine(key)) = WireHandle::try_from(&handle) {
                    let _ = bridge_cffi::handle_cffi::release_handle(key.get());
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
    catch_dispatch_panic(call_id, || complete_with_value(py, call_id, value.bind(py)));
}

/// Complete `call_id` with the value its callable returned, encoded against
/// the return type of the slot the callable was bound to.
fn complete_with_value(py: Python<'_>, call_id: u32, value: &Bound<'_, PyAny>) {
    let observation = bridge_cffi::host_instrumentation::callback(call_id).map(|owner| {
        let outcome = btel_types::InvocationOutcome::Ok;
        let capture = owner
            .wants_value(outcome)
            .then(|| crate::host_capture::capture(value));
        (owner, capture)
    });
    let ret = match take_callback_signature(call_id) {
        Some(bex_project::RuntimeTy::Function { ret, .. }) => *ret,
        Some(_) | None => bex_project::RuntimeTy::Unknown,
    };
    let completion = crate::encode::encode_result(py, value, &ret);
    // The outcome the engine sees, for the caller's instrumentation: a
    // value that completes the call as a panic is an error in the trace,
    // whatever the callable returned.
    match &completion {
        Completion::Wire(_) => {
            if let Some((owner, capture)) = observation {
                owner.observe(btel_types::InvocationOutcome::Ok, capture);
            }
        }
        Completion::HostError(_) => {}
        Completion::ContractViolation(error) => observe_dispatch_error(
            call_id,
            py,
            &pyo3::exceptions::PyTypeError::new_err(error.to_string()),
        ),
        Completion::BridgeFailure(fault) => {
            let message = match fault {
                BridgeFault::Value(error) => error.to_string(),
                BridgeFault::Host(error) => error.to_string(),
            };
            observe_dispatch_error(
                call_id,
                py,
                &pyo3::exceptions::PyTypeError::new_err(message),
            );
        }
    }
    match completion.into_wire(Encoded::Returned) {
        Ok(wire) => complete(call_id, &wire),
        // The callable's own code raised while its result was read: the
        // error it threw, as any exception a callable raises.
        Err(error) => send_dispatch_error_from_pyerr(call_id, py, &error),
    }
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

/// Complete `call_id` with what its callable did. `complete_host_call` is
/// `extern "C"` but not `unsafe` — the invariants documented on its
/// declaration (valid-for-length bytes) are satisfied by the slice contract.
fn complete(call_id: u32, completion: &HostCallCompletion) {
    let bytes = completion.encode_to_vec();
    complete_host_call(call_id, bytes.as_ptr() as *const i8, bytes.len());
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
    drop(take_callback_signature(call_id));
    sys_native::host_dispatch::complete_with_error(
        call_id,
        sys_native::OpError::new(
            sys_native::SysOp::BamlHostCallHostValue,
            sys_native::VmInternalError::BridgeFailure { message },
        ),
    );
}

/// If `py_err` is a `baml_bridge.errors.BamlError` *or* `BamlPanic` carrying
/// a BAML value, the completion that throws the unwrapped value (`e.value`),
/// encoded against the throws type of the slot the callable was bound to, so
/// the BAML caller can `catch (e: MyError)` and read fields just like a
/// BAML-thrown error. A `BamlPanic.value` is a `baml.panics.*` instance,
/// which the engine throws as a panic whatever the slot declares.
/// (`BamlPanic` is a `BaseException`, not a `BamlError` subclass, so it must
/// be checked separately.)
///
/// Returns `Ok(None)` for:
/// - any other exception type — the caller completes the call as the host's
///   own exception;
/// - `BamlError(value=None)` / `BamlPanic(value=None)` — a bare wrapper
///   carries no BAML value to throw.
///
/// An `Err(_)` here (errors module missing, the host's own exception while
/// the value was read) is also collapsed to the host-exception path by the
/// caller — the call always completes.
fn try_encode_baml_error_throw(
    py: Python<'_>,
    call_id: u32,
    py_err: &pyo3::PyErr,
) -> PyResult<Option<HostCallCompletion>> {
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
    // enum / primitive) and encode it against the throws type of the slot
    // the callable was bound to, the same way a result is encoded against
    // the return type.
    let inner = exc_value.getattr("value")?;
    if inner.is_none() {
        // `BamlError(value=None)` is a bare wrapper — emit nothing here;
        // the caller will fall through to the opaque path.
        return Ok(None);
    }
    let throws = match take_callback_signature(call_id) {
        Some(bex_project::RuntimeTy::Function { throws, .. }) => *throws,
        Some(_) | None => bex_project::RuntimeTy::Unknown,
    };
    crate::encode::encode_result(py, &inner, &throws)
        .into_wire(Encoded::Thrown)
        .map(Some)
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
    observe_dispatch_error(call_id, py, py_err);

    if let Ok(Some(completion)) = try_encode_baml_error_throw(py, call_id, py_err) {
        complete(call_id, &completion);
        return;
    }
    send_dispatch_error_opaque(call_id, py, py_err);
}

/// Report a failed dispatch to the instrumentation owner of `call_id`, if any.
fn observe_dispatch_error(call_id: u32, py: Python<'_>, py_err: &pyo3::PyErr) {
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
}

/// Complete `call_id` as the host's own exception: `py_err`'s identity and a
/// handle to the exception object itself, from which the engine builds the
/// `baml.errors.HostCallable`.
fn send_dispatch_error_opaque(call_id: u32, py: Python<'_>, py_err: &pyo3::PyErr) {
    // The host's own exception is encoded against nothing.
    drop(take_callback_signature(call_id));

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
    complete(
        call_id,
        &bridge_encode::raised(message, class_name, "python", traceback, handle_key),
    );
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
