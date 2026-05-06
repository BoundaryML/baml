//! FunctionResult - wraps the result of a BAML function call.

use pyo3::{
    Py, PyResult, Python,
    prelude::pymethods,
    pyclass,
    types::{PyAny, PyAnyMethods},
};
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pymethods};

/// Result of a BAML function call.
///
/// Contains the parsed Python object returned by the function.
#[gen_stub_pyclass]
#[pyclass]
pub struct FunctionResult {
    value: Py<PyAny>,
}

impl FunctionResult {
    pub fn new(value: Py<PyAny>) -> Self {
        Self { value }
    }
}

#[gen_stub_pymethods]
#[pymethods]
impl FunctionResult {
    /// Construct a FunctionResult from a Python value.
    #[new]
    pub fn py_new(value: Py<PyAny>) -> Self {
        Self { value }
    }

    /// Get the result value.
    fn result(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
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
