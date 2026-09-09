//! `BamlPyHandle` — Python wrapper for a `HANDLE_TABLE` row.
//!
//! Holds a `(handle_key, handle_type)` pair. The actual
//! `CffiHandleTableEntry` lives in `bridge_ctypes::HANDLE_TABLE`; the
//! Python object is just a token. This matches what Go/Rust client
//! bindings will look like — they can only see `u64` handles across the
//! CFFI boundary, so Python uses the same model for uniformity.
//!
//! Lifecycle:
//! - Runtime-result decoding creates provisional handles rooted by its owned
//!   result. They become usable and independently owned only after adoption.
//! - Failed decoding invalidates provisional handles, including any retained
//!   by a model validator. The receipt releases their engine leases.
//! - Internal constructors and remaining raw-value paths use `new(key, ht)`
//!   for an already-transferred lease. Copying acquires another table lease;
//!   identity-deduplicated keys may represent several such ownerships.
//! - Dropping an adopted/nonprovisional handle releases one engine lease.
//!
//! There is no public `handle_type()` method. The wire transmits
//! `BamlHandle.handle_type`, the Python object stores it on construction,
//! and Rust-internal callers (media class validation) read the field directly.

use bridge_cffi::{
    __testonly_seed_function_ref, __testonly_seed_generic_media, BamlCffiStatus, baml_handle_clone,
    baml_handle_release,
};
use pyo3::prelude::*;
use pyo3_stub_gen::{
    derive::{gen_methods_from_python, gen_stub_pyclass, gen_stub_pyfunction, gen_stub_pymethods},
    inventory::submit,
};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU8, Ordering},
};

pub(crate) const TRANSFER_PENDING: u8 = 0;
pub(crate) const TRANSFER_ADOPTED: u8 = 1;
pub(crate) const TRANSFER_DISCARDED: u8 = 2;

pub(crate) struct HandleTransferState {
    pub(crate) status: AtomicU8,
    runtime: Mutex<Option<Arc<dyn bex_project::Bex>>>,
    transfers: bridge_ctypes::TransferSession<'static>,
}

impl HandleTransferState {
    pub(crate) fn new(
        runtime: Option<Arc<dyn bex_project::Bex>>,
        transfers: bridge_ctypes::TransferSession<'static>,
    ) -> Self {
        Self {
            status: AtomicU8::new(TRANSFER_PENDING),
            runtime: Mutex::new(runtime),
            transfers,
        }
    }
    pub(crate) fn discard(&self) {
        self.status.store(TRANSFER_DISCARDED, Ordering::Release);
        let runtime = self
            .runtime
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take();
        drop(runtime);
    }
}

pub(crate) fn status_to_pyerr(context: &str, status: BamlCffiStatus) -> PyErr {
    let detail = match status {
        BamlCffiStatus::Ok => "ok",
        BamlCffiStatus::InvalidHandle => "invalid handle",
        BamlCffiStatus::TypeMismatch => "handle type mismatch",
        BamlCffiStatus::UnsupportedHandleType => "unsupported handle type",
        BamlCffiStatus::InternalError => "internal error",
        BamlCffiStatus::UnexpectedNullptr => "unexpected null pointer",
    };
    pyo3::exceptions::PyRuntimeError::new_err(format!("{context}: {detail}"))
}

pub(crate) fn handle_clone(key: u64, context: &str) -> PyResult<u64> {
    let mut out_key = 0;
    match unsafe { baml_handle_clone(key, &mut out_key) } {
        BamlCffiStatus::Ok => Ok(out_key),
        status => Err(status_to_pyerr(context, status)),
    }
}

pub(crate) fn release_wire_handle(key: u64, context: &str) -> PyResult<()> {
    match unsafe { baml_handle_release(key) } {
        BamlCffiStatus::Ok => Ok(()),
        status => Err(status_to_pyerr(context, status)),
    }
}

#[gen_stub_pyclass]
#[pyclass]
pub struct BamlPyHandle {
    pub(crate) handle_key: u64,
    /// `BamlHandleType` as u64 — same width as `handle_key` for uniformity
    /// across the CFFI surface. Set at construction from the wire field
    /// (decode path) or from the entry's intrinsic kind (encode/seed
    /// paths). Read by inbound encode (`_clone_key_for_wire`) and by media
    /// `_from_pyhandle` validation.
    pub(crate) handle_type: u64,
    transfer_state: Option<Arc<HandleTransferState>>,
}

#[gen_stub_pymethods]
#[pymethods]
impl BamlPyHandle {
    /// Inspect immutable loader evidence during provisional result decoding.
    /// This neither adopts the reference nor allows it to execute a call.
    fn _sdk_concrete_name(&self, bundle_id: Vec<u8>) -> PyResult<Option<String>> {
        let Some(state) = &self.transfer_state else {
            return Ok(None);
        };
        if state.status.load(Ordering::Acquire) == TRANSFER_DISCARDED {
            return Err(pyo3::exceptions::PyRuntimeError::new_err(
                "BAML result was discarded",
            ));
        }
        if bundle_id.len() != 32 {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "invalid SDK bundle identity",
            ));
        }
        let entry = bridge_ctypes::HANDLE_TABLE
            .resolve(self.handle_key)
            .ok_or_else(|| {
                pyo3::exceptions::PyRuntimeError::new_err("BAML reference is no longer available")
            })?;
        let bridge_ctypes::CffiHandleTableEntry::Adt(
            bex_project::BexExternalAdt::TaggedHeapHandle {
                kind: bex_project::TaggedHeapHandleKind::ConcreteObject,
                sdk_declaration,
                ..
            },
        ) = entry.as_ref()
        else {
            return Err(pyo3::exceptions::PyTypeError::new_err(
                "reference is not a concrete object",
            ));
        };
        let Some(declaration) = sdk_declaration else {
            return Ok(None);
        };
        if declaration.bundle_id.as_slice() != bundle_id {
            return Err(pyo3::exceptions::PyRuntimeError::new_err(
                "SDK bundle does not match the receiver's issuing program",
            ));
        }
        Ok(Some(declaration.name.to_string()))
    }

    #[new]
    fn py_new(handle_key: u64, handle_type: u64) -> Self {
        Self::new(handle_key, handle_type)
    }

    fn __copy__(&self) -> PyResult<Self> {
        self.ensure_active()?;
        let new_key = handle_clone(self.handle_key, "BamlPyHandle.__copy__")?;
        let mut copied = Self::new(new_key, self.handle_type);
        copied.transfer_state = self.transfer_state.clone();
        Ok(copied)
    }

    fn __deepcopy__(&self, _memo: &Bound<'_, PyAny>) -> PyResult<Self> {
        self.__copy__()
    }

    /// Clone this handle for inbound wire ownership.
    fn _clone_key_for_wire(&self) -> PyResult<(u64, u64)> {
        self.ensure_active()?;
        let new_key = handle_clone(self.handle_key, "BamlPyHandle._clone_key_for_wire")?;
        Ok((new_key, self.handle_type))
    }

    /// Borrow this handle key as a call target without transferring ownership.
    fn _key_for_call(&self) -> PyResult<u64> {
        self.ensure_active()?;
        use bridge_ctypes::baml_bridge::cffi::BamlHandleType;

        if self.handle_type != BamlHandleType::FunctionRef as u64 {
            return Err(pyo3::exceptions::PyTypeError::new_err(
                "handle does not reference a BAML callable",
            ));
        }
        Ok(self.handle_key)
    }

    /// Borrow a concrete receiver key; native preparation pins its actual row.
    fn _key_for_concrete_call(&self) -> PyResult<u64> {
        self.ensure_active()?;
        use bridge_ctypes::baml_bridge::cffi::BamlHandleType;
        if self.handle_type != BamlHandleType::ConcreteObject as u64 {
            return Err(pyo3::exceptions::PyTypeError::new_err(
                "handle does not reference a concrete BAML object",
            ));
        }
        Ok(self.handle_key)
    }
}

submit! {
    gen_methods_from_python! {
        r#"
        import typing

        class BamlPyHandle:
            def _call_method(self, member: str, args_proto: bytes) -> typing.Awaitable["BamlEncodedResult"]:
                """Invoke an interface member on this reference's issuing runtime."""

            def _call_owned_function(self, args_proto: bytes) -> typing.Awaitable["BamlEncodedResult"]:
                """Invoke a checked call on this reference's issuing runtime."""

            def _call_owned_function_sync(self, args_proto: bytes) -> "BamlEncodedResult":
                """Invoke a checked call synchronously on the issuing runtime."""
        "#
    }
}

#[pymethods]
impl BamlPyHandle {
    fn _call_owned_function(&self, py: Python<'_>, args_proto: Vec<u8>) -> PyResult<Py<PyAny>> {
        crate::runtime::call_owned_function(py, self, args_proto)
    }

    fn _call_owned_function_sync(
        &self,
        py: Python<'_>,
        args_proto: Vec<u8>,
    ) -> PyResult<crate::encoded_result::BamlEncodedResult> {
        crate::runtime::call_owned_function_sync(py, self, args_proto)
    }

    /// Invoke an interface member on this reference's issuing runtime.
    fn _call_method(
        &self,
        py: Python<'_>,
        member: String,
        args_proto: Vec<u8>,
    ) -> PyResult<Py<PyAny>> {
        crate::runtime::call_interface_method(py, self, member, args_proto)
    }
}

impl BamlPyHandle {
    pub fn new(handle_key: u64, handle_type: u64) -> Self {
        Self {
            handle_key,
            handle_type,
            transfer_state: None,
        }
    }

    pub(crate) fn provisional(
        handle_key: u64,
        handle_type: u64,
        state: Arc<HandleTransferState>,
    ) -> Self {
        Self {
            handle_key,
            handle_type,
            transfer_state: Some(state),
        }
    }

    pub(crate) fn ensure_active(&self) -> PyResult<()> {
        if !self.is_active() {
            return Err(pyo3::exceptions::PyRuntimeError::new_err(
                "BAML reference has not been adopted or its result was discarded",
            ));
        }
        Ok(())
    }

    pub(crate) fn invocation_owner(
        &self,
    ) -> PyResult<(
        Arc<dyn bex_project::Bex>,
        bridge_ctypes::TransferSession<'static>,
    )> {
        self.ensure_active()?;
        let state = self.transfer_state.as_ref().ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err("BAML reference has no issuing runtime")
        })?;
        if state.transfers.is_closed() {
            return Err(pyo3::exceptions::PyRuntimeError::new_err(
                "BAML reference's issuing runtime was closed or replaced",
            ));
        }
        let runtime = state
            .runtime
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
            .ok_or_else(|| {
                pyo3::exceptions::PyRuntimeError::new_err("BAML reference has no issuing runtime")
            })?;
        Ok((runtime, state.transfers.clone()))
    }

    fn is_active(&self) -> bool {
        self.transfer_state
            .as_ref()
            .is_none_or(|state| state.status.load(Ordering::Acquire) == TRANSFER_ADOPTED)
    }
}

impl Drop for BamlPyHandle {
    fn drop(&mut self) {
        // Host-owned handles (`HostValueCallable`, `HostValueOpaque`) are
        // *not* tracked in `HANDLE_TABLE` — their lifetime is managed
        // per-bridge via the host-value registry (see
        // [`crate::host_value::lookup_host_value`]). Releasing them here
        // would call `baml_handle_release` with a key that may
        // numerically collide with an unrelated engine-side entry (the
        // two keyspaces are disjoint by design but share the `u64` value
        // space), evicting it. Skip the table release for those
        // variants; the owners drop them through their own release path.
        // Mirrors `bridge_wasm/src/handle.rs`'s `Drop` for the same
        // reason.
        use bridge_ctypes::baml_bridge::cffi::BamlHandleType;
        let ht_i32 = i32::try_from(self.handle_type).unwrap_or(-1);
        let is_host_value = ht_i32 == BamlHandleType::HostValueCallable as i32
            || ht_i32 == BamlHandleType::HostValueOpaque as i32;
        let active = self.is_active();
        let runtime_state = self.transfer_state.take();
        if !is_host_value && active {
            let _ = unsafe { baml_handle_release(self.handle_key) };
        }
        // The last SDK wrapper can also own the last runtime Arc. Dropping it
        // may enqueue host releases after the table-release safepoint above.
        drop(runtime_state);
        bex_project::host_release_dispatch::drain();
    }
}

/// Test-only: seed a `FunctionRef` entry through the shared CFFI API,
/// returning `(key, handle_type)` so test code can construct a
/// `BamlPyHandle` or stage a wire `BamlHandle`.
#[gen_stub_pyfunction]
#[pyfunction]
pub fn _seed_function_ref_handle(global_index: u64) -> PyResult<(u64, u64)> {
    let mut key = 0;
    let mut handle_type = 0;
    match unsafe { __testonly_seed_function_ref(global_index, &mut key, &mut handle_type) } {
        BamlCffiStatus::Ok => Ok((key, handle_type as u64)),
        status => Err(status_to_pyerr("_seed_function_ref_handle", status)),
    }
}

/// Test-only: seed an `Adt(Media(generic))` entry through the shared CFFI API.
#[gen_stub_pyfunction]
#[pyfunction]
pub fn _seed_generic_media_handle() -> PyResult<(u64, u64)> {
    let mut key = 0;
    let mut handle_type = 0;
    match unsafe { __testonly_seed_generic_media(&mut key, &mut handle_type) } {
        BamlCffiStatus::Ok => Ok((key, handle_type as u64)),
        status => Err(status_to_pyerr("_seed_generic_media_handle", status)),
    }
}

/// Release a handle cloned for wire ownership when encoding aborts before the
/// engine can consume it.
#[gen_stub_pyfunction]
#[pyfunction]
pub fn _release_wire_handle(key: u64) -> PyResult<()> {
    release_wire_handle(key, "BamlPyHandle wire-encode rollback")
}

/// Test-only: return the number of live ordinary HANDLE_TABLE keys.
#[gen_stub_pyfunction]
#[pyfunction]
pub fn _live_handle_count() -> usize {
    bridge_cffi::handle::live_handle_count()
}
