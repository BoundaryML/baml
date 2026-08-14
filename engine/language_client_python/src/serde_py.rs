//! Minimal, self-contained conversions between `serde_json::Value` and Python
//! objects, replacing the external `pythonize`/`depythonize` helpers.
//!
//! Only the two shapes actually used by the Python bindings are supported:
//! - `json_to_py`: a `serde_json::Value` (produced by `serde_json::to_value`) is
//!   materialized as plain Python `dict`/`list`/scalars.
//! - `py_to_json`: a Python object is walked into a `serde_json::Value`, which
//!   callers then feed to `serde_json::from_value` so existing serde
//!   `rename`/`flatten`/`untagged` attributes keep working unchanged.

use pyo3::{
    exceptions::PyTypeError,
    prelude::*,
    types::{PyBool, PyDict, PyList, PyTuple},
    IntoPyObjectExt,
};
use serde_json::Value;

/// Convert a `serde_json::Value` into an equivalent Python object.
pub fn json_to_py<'py>(py: Python<'py>, value: &Value) -> PyResult<Bound<'py, PyAny>> {
    match value {
        Value::Null => Ok(py.None().into_bound(py)),
        Value::Bool(b) => (*b).into_bound_py_any(py),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                i.into_bound_py_any(py)
            } else if let Some(u) = n.as_u64() {
                u.into_bound_py_any(py)
            } else if let Some(f) = n.as_f64() {
                f.into_bound_py_any(py)
            } else {
                Ok(py.None().into_bound(py))
            }
        }
        Value::String(s) => s.as_str().into_bound_py_any(py),
        Value::Array(arr) => {
            let list = PyList::empty(py);
            for item in arr {
                list.append(json_to_py(py, item)?)?;
            }
            Ok(list.into_any())
        }
        Value::Object(map) => {
            let dict = PyDict::new(py);
            for (k, v) in map {
                dict.set_item(k, json_to_py(py, v)?)?;
            }
            Ok(dict.into_any())
        }
    }
}

/// Convert a Python object into a `serde_json::Value`.
pub fn py_to_json(obj: &Bound<'_, PyAny>) -> PyResult<Value> {
    if obj.is_none() {
        return Ok(Value::Null);
    }
    // `bool` must be checked before the integer extractions because in Python
    // `bool` is a subclass of `int`.
    if let Ok(b) = obj.downcast::<PyBool>() {
        return Ok(Value::Bool(b.is_true()));
    }
    if let Ok(i) = obj.extract::<i64>() {
        return Ok(Value::Number(i.into()));
    }
    if let Ok(u) = obj.extract::<u64>() {
        return Ok(Value::Number(u.into()));
    }
    if let Ok(f) = obj.extract::<f64>() {
        return Ok(serde_json::Number::from_f64(f)
            .map(Value::Number)
            .unwrap_or(Value::Null));
    }
    if let Ok(s) = obj.extract::<String>() {
        return Ok(Value::String(s));
    }
    if let Ok(dict) = obj.downcast::<PyDict>() {
        let mut map = serde_json::Map::with_capacity(dict.len());
        for (k, v) in dict.iter() {
            map.insert(k.extract::<String>()?, py_to_json(&v)?);
        }
        return Ok(Value::Object(map));
    }
    if let Ok(list) = obj.downcast::<PyList>() {
        let mut arr = Vec::with_capacity(list.len());
        for item in list.iter() {
            arr.push(py_to_json(&item)?);
        }
        return Ok(Value::Array(arr));
    }
    if let Ok(tuple) = obj.downcast::<PyTuple>() {
        let mut arr = Vec::with_capacity(tuple.len());
        for item in tuple.iter() {
            arr.push(py_to_json(&item)?);
        }
        return Ok(Value::Array(arr));
    }
    Err(PyTypeError::new_err(format!(
        "Cannot convert Python object of type '{}' to JSON",
        obj.get_type().name()?,
    )))
}
