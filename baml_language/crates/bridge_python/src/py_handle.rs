//! `BamlPyHandle` — owning Python wrapper for a `CffiHandleTableEntry`.
//!
//! Replaces the previous `BamlHandle(key, handle_type)` (a wire-table token)
//! and `UnknownHandle` (a Python shim around it). The entry is held inline;
//! no global table indirection. The `HANDLE_TABLE` is only used to bridge
//! the FFI wire (insert on the encode side, drain on the decode side).

use bridge_ctypes::CffiHandleTableEntry;
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
