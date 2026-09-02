//! `BamlPyHandle` — Python wrapper for a `HANDLE_TABLE` row.
//!
//! Holds a `(handle_key, handle_type)` pair. The actual
//! `CffiHandleTableEntry` lives in `bridge_ctypes::HANDLE_TABLE`; the
//! Python object is just a token. This matches what Go/Rust client
//! bindings will look like — they can only see `u64` handles across the
//! CFFI boundary, so Python uses the same model for uniformity.
//!
//! Lifecycle:
//!  - Construct (decode side): `BamlPyHandle::new(key, ht)`. The wire
//!    encoder is responsible for having inserted under `key`. The Python
//!    object now owns the row; nobody else may drain it.
//!  - `__copy__` / `__deepcopy__`: allocate a new row via
//!    `baml_handle_clone(key, out_key)` and wrap. Sharing the same key
//!    between two `BamlPyHandle`s would double-release on drop.
//!  - Drop: `baml_handle_release(key)`. Without this every handle leaks
//!    one row.
//!
//! There is no public `handle_type()` method. The wire transmits
//! `BamlHandle.handle_type`, the Python object stores it on construction,
//! and Rust-internal callers (media class validation) read the field directly.

use bridge_cffi::{
    __testonly_seed_function_ref, __testonly_seed_generic_media, BamlCffiStatus, baml_handle_clone,
    baml_handle_release,
};
use pyo3::prelude::*;
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pyfunction, gen_stub_pymethods};

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
}

#[gen_stub_pymethods]
#[pymethods]
impl BamlPyHandle {
    #[new]
    fn py_new(handle_key: u64, handle_type: u64) -> Self {
        Self::new(handle_key, handle_type)
    }

    fn __copy__(&self) -> PyResult<Self> {
        let new_key = handle_clone(self.handle_key, "BamlPyHandle.__copy__")?;
        Ok(Self {
            handle_key: new_key,
            handle_type: self.handle_type,
        })
    }

    fn __deepcopy__(&self, _memo: &Bound<'_, PyAny>) -> PyResult<Self> {
        self.__copy__()
    }

    /// Clone this handle for inbound wire ownership.
    fn _clone_key_for_wire(&self) -> PyResult<(u64, u64)> {
        let new_key = handle_clone(self.handle_key, "BamlPyHandle._clone_key_for_wire")?;
        Ok((new_key, self.handle_type))
    }

    /// Borrow this handle key as a call target without transferring ownership.
    fn _key_for_call(&self) -> PyResult<u64> {
        use bridge_ctypes::baml_bridge::cffi::BamlHandleType;

        if self.handle_type != BamlHandleType::FunctionRef as u64 {
            return Err(pyo3::exceptions::PyTypeError::new_err(
                "handle does not reference a BAML callable",
            ));
        }
        Ok(self.handle_key)
    }
}

impl BamlPyHandle {
    pub fn new(handle_key: u64, handle_type: u64) -> Self {
        Self {
            handle_key,
            handle_type,
        }
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
        if !is_host_value {
            let _ = unsafe { baml_handle_release(self.handle_key) };
        }
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
