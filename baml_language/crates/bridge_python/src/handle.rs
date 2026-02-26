//! Python handle lifecycle — released via __del__.

use bridge_ctypes::HANDLE_TABLE;
use pyo3::prelude::*;

/// Base class for all opaque BAML handles.
///
/// When the Python garbage collector finalizes an instance, `__del__` releases
/// the corresponding entry from the global handle table.
#[pyclass(subclass)]
pub struct BamlHandle {
    key: u64,
    handle_type: i32,
}

#[pymethods]
impl BamlHandle {
    #[new]
    pub fn new(key: u64, handle_type: i32) -> Self {
        BamlHandle { key, handle_type }
    }

    #[getter]
    pub fn key(&self) -> u64 {
        self.key
    }

    #[getter]
    pub fn handle_type(&self) -> i32 {
        self.handle_type
    }

    pub fn __copy__(&self) -> PyResult<Self> {
        let new_key = HANDLE_TABLE.clone_handle(self.key).ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err("Handle is no longer valid")
        })?;
        Ok(BamlHandle {
            key: new_key,
            handle_type: self.handle_type,
        })
    }

    pub fn __deepcopy__(&self, _memo: &Bound<'_, PyAny>) -> PyResult<Self> {
        self.__copy__()
    }

    pub fn __del__(&mut self) {
        HANDLE_TABLE.release(self.key);
    }
}
