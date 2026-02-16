//! FunctionResult - wraps the result of a BAML function call.

use pyo3::{PyObject, PyResult, Python, prelude::pymethods, pyclass, types::PyAnyMethods};

/// Result of a BAML function call.
///
/// Contains the parsed Python object returned by the function.
#[pyclass]
pub struct FunctionResult {
    value: PyObject,
}

impl FunctionResult {
    pub fn new(value: PyObject) -> Self {
        Self { value }
    }
}

#[pymethods]
impl FunctionResult {
    /// Construct a FunctionResult from a Python value.
    #[new]
    pub fn py_new(value: PyObject) -> Self {
        Self { value }
    }

    /// Get the result value.
    fn result(&self, py: Python<'_>) -> PyResult<PyObject> {
        Ok(self.value.clone_ref(py))
    }

    fn __str__(&self, py: Python<'_>) -> PyResult<String> {
        let repr = self.value.bind(py).repr()?;
        Ok(format!("FunctionResult({})", repr))
    }

    fn __repr__(&self, py: Python<'_>) -> PyResult<String> {
        self.__str__(py)
    }
}
