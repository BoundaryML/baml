//! Convert Python objects to BexExternalValue.

use std::collections::HashMap;

use bex_external_types::BexExternalValue;
use indexmap::IndexMap;
use pyo3::{
    exceptions::PyTypeError,
    prelude::PyAnyMethods,
    types::{PyBool, PyBoolMethods, PyDict, PyDictMethods, PyList, PyListMethods, PyTypeMethods},
    PyObject, PyResult, Python,
};

/// Convert a Python object to a `BexExternalValue`.
///
/// This performs recursive conversion of Python primitives, lists, and dicts
/// into the corresponding `BexExternalValue` variants. Since Python doesn't
/// carry static type info, container type parameters use placeholder `Ty` values.
pub fn py_to_bex_value(py: Python<'_>, obj: &PyObject) -> PyResult<BexExternalValue> {
    let any = obj.bind(py);

    // None
    if any.is_none() {
        return Ok(BexExternalValue::Null);
    }

    // Bool (must be checked before int, since bool is a subclass of int in Python)
    if let Ok(b) = any.downcast::<PyBool>() {
        return Ok(BexExternalValue::Bool(b.is_true()));
    }

    // Int
    if let Ok(i) = any.extract::<i64>() {
        return Ok(BexExternalValue::Int(i));
    }

    // Float
    if let Ok(f) = any.extract::<f64>() {
        return Ok(BexExternalValue::Float(f));
    }

    // String
    if let Ok(s) = any.extract::<String>() {
        return Ok(BexExternalValue::String(s));
    }

    // List
    if let Ok(list) = any.downcast::<PyList>() {
        let len = list.len();
        let mut items = Vec::with_capacity(len);
        for idx in 0..len {
            let item = list.get_item(idx)?;
            items.push(py_to_bex_value(py, &item.unbind())?);
        }
        return Ok(BexExternalValue::Array {
            element_type: baml_type::Ty::Null,
            items,
        });
    }

    // Dict
    if let Ok(dict) = any.downcast::<PyDict>() {
        let mut entries = IndexMap::new();
        for (key, value) in dict.iter() {
            let key_str = key
                .extract::<String>()
                .map_err(|_| PyTypeError::new_err("Dict keys must be strings"))?;
            entries.insert(key_str, py_to_bex_value(py, &value.unbind())?);
        }
        return Ok(BexExternalValue::Map {
            key_type: baml_type::Ty::String,
            value_type: baml_type::Ty::Null,
            entries,
        });
    }

    // Fallback: unsupported type
    let type_name = any.get_type().name()?;
    Err(PyTypeError::new_err(format!(
        "Unsupported Python type for BAML: {type_name}"
    )))
}

/// Parse a Python kwargs dict into a `HashMap<String, PyObject>`.
pub fn parse_py_kwargs(
    py: Python<'_>,
    args: &PyObject,
) -> PyResult<HashMap<String, PyObject>> {
    let any = args.bind(py);
    let dict = any
        .downcast::<PyDict>()
        .map_err(|_| PyTypeError::new_err("Expected a dict for function arguments"))?;

    let mut result = HashMap::new();
    for (key, value) in dict.iter() {
        let key_str = key
            .extract::<String>()
            .map_err(|_| PyTypeError::new_err("Argument keys must be strings"))?;
        result.insert(key_str, value.unbind());
    }
    Ok(result)
}
