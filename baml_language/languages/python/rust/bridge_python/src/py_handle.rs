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
//!    `HANDLE_TABLE.clone_handle(key)` and wrap. Sharing the same key
//!    between two `BamlPyHandle`s would double-release on drop.
//!  - Drop: `HANDLE_TABLE.release(key)`. Without this every handle leaks
//!    one row.
//!
//! There is no public `handle_type()` method. The wire transmits
//! `BamlHandle.handle_type`, the Python object stores it on construction,
//! and Rust-internal callers (`put_pyhandle_into_table`, media class
//! validation) read the field directly.

use bridge_ctypes::{CffiHandleTableEntry, HANDLE_TABLE};
use pyo3::prelude::*;
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pyfunction, gen_stub_pymethods};

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
        let new_key = HANDLE_TABLE.clone_handle(self.handle_key).ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err(format!(
                "BamlPyHandle.__copy__: key {} not in HANDLE_TABLE",
                self.handle_key
            ))
        })?;
        Ok(Self {
            handle_key: new_key,
            handle_type: self.handle_type,
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
        HANDLE_TABLE.release(self.handle_key);
    }
}

/// Wrap a `HANDLE_TABLE` key as a `BamlPyHandle`. Used by
/// `proto.py::_decode_handle`. Does **not** drain — the entry stays in
/// the table and is owned by the returned `BamlPyHandle`. Validates the
/// key exists so a malformed wire payload errors here rather than on
/// later use.
#[gen_stub_pyfunction]
#[pyfunction]
pub fn take_pyhandle_from_table(key: u64, handle_type: u64) -> PyResult<BamlPyHandle> {
    if HANDLE_TABLE.resolve(key).is_none() {
        return Err(pyo3::exceptions::PyRuntimeError::new_err(format!(
            "BAML handle key {key} is not in HANDLE_TABLE"
        )));
    }
    Ok(BamlPyHandle::new(key, handle_type))
}

/// Allocate a fresh `HANDLE_TABLE` row sharing the same `Arc` as
/// `pyhandle.handle_key`, return `(new_key, handle_type)`. The original
/// `BamlPyHandle` keeps its key and stays usable — Python may pass the
/// same handle to multiple calls.
#[gen_stub_pyfunction]
#[pyfunction]
pub fn put_pyhandle_into_table(pyhandle: &BamlPyHandle) -> PyResult<(u64, u64)> {
    let new_key = HANDLE_TABLE
        .clone_handle(pyhandle.handle_key)
        .ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err(format!(
                "BamlPyHandle key {} is not in HANDLE_TABLE",
                pyhandle.handle_key
            ))
        })?;
    Ok((new_key, pyhandle.handle_type))
}

/// Test-only: seed a `FunctionRef` entry directly into `HANDLE_TABLE`,
/// returning `(key, handle_type)` so test code can construct a
/// `BamlPyHandle` or stage a wire `BamlHandle`.
#[gen_stub_pyfunction]
#[pyfunction]
pub fn _seed_function_ref_handle(global_index: u64) -> (u64, u64) {
    let entry = CffiHandleTableEntry::FunctionRef {
        global_index: global_index as usize,
    };
    let ht = entry.handle_type() as u64;
    let key = HANDLE_TABLE.insert(entry);
    (key, ht)
}

/// Test-only: seed an `Adt(Media(generic))` entry directly into `HANDLE_TABLE`.
#[gen_stub_pyfunction]
#[pyfunction]
pub fn _seed_generic_media_handle() -> (u64, u64) {
    use bex_project::{BexExternalAdt, MediaKind, MediaValue};
    let media = MediaValue::from_url(MediaKind::Generic, "https://example.com/", None);
    let entry = CffiHandleTableEntry::Adt(BexExternalAdt::Media(media));
    let ht = entry.handle_type() as u64;
    let key = HANDLE_TABLE.insert(entry);
    (key, ht)
}
