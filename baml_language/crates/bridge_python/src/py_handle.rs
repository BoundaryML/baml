//! `BamlPyHandle` — owning Python wrapper for a `CffiHandleTableEntry`.
//!
//! Replaces the previous `BamlHandle(key, handle_type)` (a wire-table token)
//! and `UnknownHandle` (a Python shim around it). The entry is held inline;
//! no global table indirection. The `HANDLE_TABLE` is only used to bridge
//! the FFI wire (insert on the encode side, drain on the decode side).

use bridge_ctypes::{CffiHandleTableEntry, HANDLE_TABLE};
use pyo3::prelude::*;

#[pyclass]
pub struct BamlPyHandle {
    pub(crate) entry: CffiHandleTableEntry,
}

#[pymethods]
impl BamlPyHandle {
    /// Derived host-side `BamlHandleType` tag, as i32. Source of truth
    /// for opaque-handle dispatch on the Python side; the wire field of
    /// the same name is redundant and ignored after decode.
    fn handle_type(&self) -> i32 {
        self.entry.handle_type() as i32
    }

    fn __copy__(&self) -> Self {
        Self {
            entry: self.entry.clone(),
        }
    }

    fn __deepcopy__(&self, _memo: &Bound<'_, PyAny>) -> Self {
        self.__copy__()
    }
}

impl BamlPyHandle {
    pub fn new(entry: CffiHandleTableEntry) -> Self {
        Self { entry }
    }
}

/// Drain `HANDLE_TABLE[key]` into a fresh `BamlPyHandle`. Used by
/// `proto.py::_decode_handle`. Raises `RuntimeError` if the key is
/// absent (which would mean the encoder failed to insert, or the same
/// key was decoded twice).
#[pyfunction]
pub fn take_pyhandle_from_table(key: u64) -> PyResult<BamlPyHandle> {
    let arc_entry = HANDLE_TABLE.drain(key).ok_or_else(|| {
        pyo3::exceptions::PyRuntimeError::new_err(format!(
            "BAML handle key {key} is not in HANDLE_TABLE"
        ))
    })?;
    Ok(BamlPyHandle {
        entry: (*arc_entry).clone(),
    })
}

/// Insert a clone of `pyhandle.entry` into `HANDLE_TABLE`, return
/// `(key, handle_type as i32)`. The original `BamlPyHandle` stays
/// usable — Python may pass the same handle to multiple calls.
#[pyfunction]
pub fn put_pyhandle_into_table(pyhandle: &BamlPyHandle) -> (u64, i32) {
    let entry = pyhandle.entry.clone();
    let ht = entry.handle_type() as i32;
    let key = HANDLE_TABLE.insert(entry);
    (key, ht)
}

/// Test-only: seed a `FunctionRef` entry directly into `HANDLE_TABLE`.
/// Used by pytest fixtures that need to exercise decoder paths for
/// handle kinds that no Python-facing API constructs.
#[pyfunction]
pub fn _seed_function_ref_handle(global_index: u64) -> u64 {
    let entry = CffiHandleTableEntry::FunctionRef {
        global_index: global_index as usize,
    };
    HANDLE_TABLE.insert(entry)
}

/// Test-only: seed an `Adt(Media(generic))` entry directly into `HANDLE_TABLE`.
#[pyfunction]
pub fn _seed_generic_media_handle() -> u64 {
    use bex_project::{BexExternalAdt, MediaKind, MediaValue};
    let media = MediaValue::from_url(MediaKind::Generic, "https://example.com/", None);
    let entry = CffiHandleTableEntry::Adt(BexExternalAdt::Media(media));
    HANDLE_TABLE.insert(entry)
}
