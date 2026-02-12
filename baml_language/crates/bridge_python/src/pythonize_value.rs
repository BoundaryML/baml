//! Convert BexExternalValue to Python objects.

use bex_external_types::BexExternalValue;
use pyo3::{
    exceptions::PyRuntimeError,
    types::{PyDict, PyDictMethods, PyList},
    IntoPyObjectExt, PyErr, PyObject, PyResult, Python,
};

/// Convert a `BexExternalValue` to a Python object.
pub fn bex_value_to_py(py: Python<'_>, value: BexExternalValue) -> PyResult<PyObject> {
    match value {
        BexExternalValue::Null => Ok(py.None()),

        BexExternalValue::Int(i) => i.into_py_any(py),

        BexExternalValue::Float(f) => f.into_py_any(py),

        BexExternalValue::Bool(b) => b.into_py_any(py),

        BexExternalValue::String(s) => s.into_py_any(py),

        BexExternalValue::Array { items, .. } => {
            let py_items: Vec<PyObject> = items
                .into_iter()
                .map(|item| bex_value_to_py(py, item))
                .collect::<PyResult<_>>()?;
            let list = PyList::new(py, py_items)?;
            list.into_py_any(py)
        }

        BexExternalValue::Map { entries, .. } => {
            let dict = PyDict::new(py);
            for (key, val) in entries {
                let py_val = bex_value_to_py(py, val)?;
                dict.set_item(key, py_val)?;
            }
            dict.into_py_any(py)
        }

        BexExternalValue::Instance { fields, .. } => {
            // MVP: return instances as dicts
            let dict = PyDict::new(py);
            for (key, val) in fields {
                let py_val = bex_value_to_py(py, val)?;
                dict.set_item(key, py_val)?;
            }
            dict.into_py_any(py)
        }

        BexExternalValue::Variant { variant_name, .. } => {
            // MVP: return enum variants as strings
            variant_name.into_py_any(py)
        }

        BexExternalValue::Union { value, .. } => {
            // Unwrap the inner value
            bex_value_to_py(py, *value)
        }

        BexExternalValue::Resource(_) => Err(PyErr::new::<PyRuntimeError, _>(
            "Resource values cannot be converted to Python objects",
        )),

        BexExternalValue::FunctionRef { .. } => Err(PyErr::new::<PyRuntimeError, _>(
            "FunctionRef values cannot be converted to Python objects",
        )),

        BexExternalValue::Handle(_) => Err(PyErr::new::<PyRuntimeError, _>(
            "Handle values cannot be converted to Python objects",
        )),

        BexExternalValue::Adt(_) => Err(PyErr::new::<PyRuntimeError, _>(
            "ADT values cannot be converted to Python objects yet",
        )),
    }
}
