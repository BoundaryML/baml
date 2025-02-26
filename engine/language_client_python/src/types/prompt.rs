use crate::errors::BamlError;
use pyo3::{
    prelude::{pyclass, pymethods, PyResult, Python},
    types::{PyDict, PyDictMethods, PyList, PyListMethods},
    IntoPyObjectExt,
};

/// Wrapper around serde JSON map that can be converted to a Python dict.
#[pyclass]
pub struct PyPrompt {
    body: serde_json::Map<String, serde_json::Value>,
}

impl From<serde_json::Map<String, serde_json::Value>> for PyPrompt {
    fn from(body: serde_json::Map<String, serde_json::Value>) -> Self {
        Self { body }
    }
}

#[pymethods]
impl PyPrompt {
    /// Convert the prompt to a Python dict.
    fn to_dict<'py>(&self, py: Python<'py>) -> PyResult<pyo3::Bound<'py, PyDict>> {
        let dict = PyDict::new(py);
        for (key, value) in &self.body {
            dict.set_item(key, serde_value_to_py_any(value, py)?)?;
        }

        Ok(dict)
    }
}

/// Convert a [`serde_json::Value`] to a [`pyo3::PyAny`].
fn serde_value_to_py_any<'py>(
    v: &serde_json::Value,
    py: Python<'py>,
) -> PyResult<pyo3::Py<pyo3::PyAny>> {
    match v {
        serde_json::Value::Null => Ok(py.None()),
        serde_json::Value::Bool(b) => b.into_py_any(py),
        serde_json::Value::String(s) => s.into_py_any(py),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                i.into_py_any(py)
            } else if let Some(f) = n.as_f64() {
                f.into_py_any(py)
            } else {
                Err(BamlError::new_err(format!(
                    "Can't convert '{n}' to a Python number"
                )))
            }
        }
        serde_json::Value::Array(a) => {
            let list = PyList::empty(py);
            for item in a {
                list.append(serde_value_to_py_any(item, py)?)?;
            }
            Ok(list.into())
        }
        serde_json::Value::Object(o) => {
            let dict = PyDict::new(py);
            for (key, value) in o {
                dict.set_item(key, serde_value_to_py_any(value, py)?)?;
            }
            Ok(dict.into())
        }
    }
}
