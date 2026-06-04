//! `BamlPyHandle` — Python wrapper for a shared CFFI handle-table row.
//!
//! Holds a `(handle_key, handle_type)` pair. The actual table entry is managed
//! through `bridge_cffi`'s checked handle API; Python keeps only the protobuf
//! handle payload and owns its key until drop.
//!
//! Lifecycle:
//!  - Construct (decode side): validate `(key, handle_type)` through
//!    `bridge_cffi`, then wrap. The Python object now owns the row.
//!  - `__copy__` / `__deepcopy__`: allocate a new row through
//!    `bridge_cffi::handle_clone_impl`. Sharing the same key between two
//!    `BamlPyHandle`s would double-release on drop.
//!  - Drop: release through `bridge_cffi::handle_release_impl`.
//!
//! There is no public `handle_type()` method. The wire transmits
//! `BamlHandle.handle_type`, the Python object stores it on construction,
//! and Rust-internal callers (`put_pyhandle_into_table`, media class
//! validation) read the field directly.

use pyo3::prelude::*;
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pyfunction, gen_stub_pymethods};

fn handle_type_to_i32(handle_type: u64) -> PyResult<i32> {
    i32::try_from(handle_type).map_err(|_| {
        pyo3::exceptions::PyValueError::new_err(format!(
            "BAML handle_type {handle_type} does not fit in int32"
        ))
    })
}

fn status_to_py_err(context: &str, key: Option<u64>, status: bridge_cffi::BamlCffiStatus) -> PyErr {
    let key_text = key.map(|key| format!(" for key {key}")).unwrap_or_default();
    let reason = match status {
        bridge_cffi::BAML_HANDLE_INVALID_HANDLE => "invalid handle",
        bridge_cffi::BAML_HANDLE_TYPE_MISMATCH => "handle type mismatch",
        bridge_cffi::BAML_HANDLE_UNSUPPORTED_HANDLE_TYPE => "unsupported handle type",
        bridge_cffi::BAML_HANDLE_INTERNAL_ERROR => "internal handle error",
        _ => "unknown handle error",
    };
    pyo3::exceptions::PyRuntimeError::new_err(format!("{context}{key_text}: {reason}"))
}

fn ensure_ok(context: &str, key: Option<u64>, status: bridge_cffi::BamlCffiStatus) -> PyResult<()> {
    if status == bridge_cffi::BAML_OK {
        Ok(())
    } else {
        Err(status_to_py_err(context, key, status))
    }
}

#[gen_stub_pyclass]
#[pyclass]
pub struct BamlPyHandle {
    pub(crate) handle_key: u64,
    /// `BamlHandleType` as u64 — same width as `handle_key` for uniformity
    /// across the CFFI surface. Set at construction from the wire field
    /// (decode path) or from the entry's intrinsic kind (encode/seed
    /// paths). Read by inbound encode (`put_pyhandle_into_table`) and by
    /// media `_from_pyhandle` validation.
    pub(crate) handle_type: u64,
}

#[gen_stub_pymethods]
#[pymethods]
impl BamlPyHandle {
    fn __copy__(&self) -> PyResult<Self> {
        let (new_key, handle_type) = _handle_clone(self.handle_key, self.handle_type)?;
        Ok(Self {
            handle_key: new_key,
            handle_type,
        })
    }

    fn __deepcopy__(&self, _memo: &Bound<'_, PyAny>) -> PyResult<Self> {
        self.__copy__()
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
        if let Ok(handle_type) = handle_type_to_i32(self.handle_type) {
            let _ = bridge_cffi::handle_release_impl(self.handle_key, handle_type);
        }
    }
}

#[gen_stub_pyfunction]
#[pyfunction]
pub fn _handle_validate(key: u64, handle_type: u64) -> PyResult<()> {
    let handle_type = handle_type_to_i32(handle_type)?;
    ensure_ok(
        "_handle_validate",
        Some(key),
        bridge_cffi::handle_validate_impl(key, handle_type),
    )
}

#[gen_stub_pyfunction]
#[pyfunction]
pub fn _handle_clone(key: u64, handle_type: u64) -> PyResult<(u64, u64)> {
    let handle_type = handle_type_to_i32(handle_type)?;
    let mut out_key = 0;
    let mut out_handle_type = 0;
    ensure_ok(
        "_handle_clone",
        Some(key),
        bridge_cffi::handle_clone_impl(
            key,
            handle_type,
            Some(&mut out_key),
            Some(&mut out_handle_type),
        ),
    )?;
    Ok((out_key, out_handle_type as u64))
}

#[gen_stub_pyfunction]
#[pyfunction]
pub fn _handle_release(key: u64, handle_type: u64) -> PyResult<()> {
    let handle_type = handle_type_to_i32(handle_type)?;
    ensure_ok(
        "_handle_release",
        Some(key),
        bridge_cffi::handle_release_impl(key, handle_type),
    )
}

#[gen_stub_pyfunction]
#[pyfunction]
pub fn _handle_type(key: u64) -> PyResult<u64> {
    bridge_cffi::handle_type_impl(key)
        .map(|handle_type| handle_type as u64)
        .map_err(|status| status_to_py_err("_handle_type", Some(key), status))
}

/// Wrap a `HANDLE_TABLE` key as a `BamlPyHandle`. Used by
/// `proto.py::_decode_handle`. Does **not** drain — the entry stays in
/// the table and is owned by the returned `BamlPyHandle`. Validates the
/// key exists so a malformed wire payload errors here rather than on
/// later use.
#[gen_stub_pyfunction]
#[pyfunction]
pub fn take_pyhandle_from_table(key: u64, handle_type: u64) -> PyResult<BamlPyHandle> {
    _handle_validate(key, handle_type)?;
    Ok(BamlPyHandle::new(key, handle_type))
}

/// Allocate a fresh `HANDLE_TABLE` row sharing the same `Arc` as
/// `pyhandle.handle_key`, return `(new_key, handle_type)`. The original
/// `BamlPyHandle` keeps its key and stays usable — Python may pass the
/// same handle to multiple calls.
#[gen_stub_pyfunction]
#[pyfunction]
pub fn put_pyhandle_into_table(pyhandle: &BamlPyHandle) -> PyResult<(u64, u64)> {
    _handle_clone(pyhandle.handle_key, pyhandle.handle_type)
}

/// Test-only: seed a `FunctionRef` entry directly into `HANDLE_TABLE`,
/// returning `(key, handle_type)` so test code can construct a
/// `BamlPyHandle` or stage a wire `BamlHandle`.
#[gen_stub_pyfunction]
#[pyfunction]
pub fn _seed_function_ref_handle(global_index: u64) -> (u64, u64) {
    let mut key = 0;
    let mut handle_type = 0;
    let status =
        bridge_cffi::baml_handle_test_seed_function_ref(global_index, &mut key, &mut handle_type);
    debug_assert_eq!(status, bridge_cffi::BAML_OK);
    (key, handle_type as u64)
}

/// Test-only: seed an `Adt(Media(generic))` entry directly into `HANDLE_TABLE`.
#[gen_stub_pyfunction]
#[pyfunction]
pub fn _seed_generic_media_handle() -> (u64, u64) {
    let mut key = 0;
    let mut handle_type = 0;
    let status = bridge_cffi::baml_handle_test_seed_generic_media(&mut key, &mut handle_type);
    debug_assert_eq!(status, bridge_cffi::BAML_OK);
    (key, handle_type as u64)
}
