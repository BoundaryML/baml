use std::collections::HashMap;

use anyhow::{bail, Result};
use pyo3::{
    exceptions::PyRuntimeError,
    types::{PyAnyMethods, PyDict, PyList},
    PyErr, PyObject, PyResult, Python, ToPyObject,
};
use serde_json::json;

pub struct SerializationError {
    position: Vec<String>,
    message: String,
}

struct Errors {
    errors: Vec<SerializationError>,
}

impl Errors {
    fn push(&mut self, error: SerializationError) {
        self.errors.push(error);
    }
}

impl Into<PyErr> for Errors {
    fn into(self) -> PyErr {
        let errs = self.errors;
        match errs.len() {
            0 => {
                PyRuntimeError::new_err("Unexpected BAML error! Report this bug to the developers!")
            }
            1 => PyRuntimeError::new_err(errs.get(0).unwrap().message.as_str().to_string()),
            _ => {
                let mut message = format!("{} errors occurred:\n", errs.len());
                for error in errs {
                    message.push_str(&format!("{}\n", error.message));
                }
                PyRuntimeError::new_err(message)
            }
        }
    }
}

enum MappedPyType {
    Enum(String, String),
    Class(String, HashMap<String, PyObject>),
    List(Vec<PyObject>),
    String(String),
    Int(i64),
    Float(f64),
    Bool(bool),
    None,
    Unsupported(String),
}

pub fn parse_py_type(any: PyObject) -> PyResult<serde_json::Value> {
    Python::with_gil(|py| {
        let enum_type = py.import_bound("enum").and_then(|m| m.getattr("Enum"))?;
        let base_model = py
            .import_bound("pydantic")
            .and_then(|m| m.getattr("BaseModel"))?;

        let mut get_type = |py: Python<'_>, any: PyObject| -> Result<MappedPyType> {
            // Call the type() function on the object

            let t = any.bind_borrowed(py).get_type();
            let t = t.as_gil_ref();
            if t.is_subclass(enum_type.as_gil_ref()).unwrap_or(false) {
                let name = t
                    .name()
                    .map(|n| n.to_string())
                    .unwrap_or("<UnnamedEnum>".to_string());
                let value = any.getattr(py, "value")?;
                let value = value.extract::<String>(py)?;
                Ok(MappedPyType::Enum(name, value))
            } else if t.is_subclass(base_model.as_gil_ref()).unwrap_or(false) {
                let name = t
                    .name()
                    .map(|n| n.to_string())
                    .unwrap_or("<UnnamedBaseModel>".to_string());
                let fields = match t
                    .getattr("model_fields")?
                    .extract::<HashMap<String, PyObject>>()
                {
                    Ok(fields) => fields
                        .keys()
                        .filter_map(|k| {
                            let v = any.getattr(py, k.as_str());
                            if let Ok(v) = v {
                                Some((k.clone(), v))
                            } else {
                                None
                            }
                        })
                        .collect::<HashMap<_, _>>(),
                    Err(_) => {
                        bail!("model_fields is not a dict")
                    }
                };
                Ok(MappedPyType::Class(name, fields))
            } else if let Ok(list) = any.downcast_bound::<PyList>(py) {
                let mut items = vec![];
                let len = list.len()?;
                for idx in 0..len {
                    items.push(list.get_item(idx)?.to_object(py));
                }
                Ok(MappedPyType::List(items))
            } else if let Ok(dict) = any.downcast_bound::<PyDict>(py) {
                let kv = dict.extract()?;
                Ok(MappedPyType::Class("<UnnamedDict>".to_string(), kv))
            } else if let Ok(s) = any.extract::<String>(py) {
                Ok(MappedPyType::String(s))
            } else if let Ok(i) = any.extract::<i64>(py) {
                Ok(MappedPyType::Int(i))
            } else if let Ok(f) = any.extract::<f64>(py) {
                Ok(MappedPyType::Float(f))
            } else if let Ok(b) = any.extract::<bool>(py) {
                Ok(MappedPyType::Bool(b))
            } else if any.is_none(py) {
                Ok(MappedPyType::None)
            } else {
                Ok(MappedPyType::Unsupported(format!("{:?}", t)))
            }
        };

        match pyobject_to_json(any, py, &mut get_type) {
            Ok(v) => Ok(v),
            Err(errors) => Err((Errors { errors }).into()),
        }
    })
}

fn pyobject_to_json<'py, F>(
    any: PyObject,
    py: Python<'py>,
    to_type: &mut F,
) -> Result<serde_json::Value, Vec<SerializationError>>
where
    F: FnMut(Python<'py>, PyObject) -> Result<MappedPyType>,
{
    let infered = match to_type(py, any) {
        Ok(infered) => infered,
        Err(e) => {
            return Err(vec![SerializationError {
                position: vec![],
                message: format!("Failed to parse type: {}", e),
            }])
        }
    };
    match infered {
        MappedPyType::Enum(_, values) => Ok(json!(values)),
        MappedPyType::Class(_, kvs) => {
            let mut obj = serde_json::Map::new();
            for (k, v) in kvs {
                let v = pyobject_to_json(v, py, to_type)?;
                obj.insert(k, v);
            }
            Ok(serde_json::Value::Object(obj))
        }
        MappedPyType::List(items) => {
            let mut arr = vec![];
            for item in items {
                arr.push(pyobject_to_json(item, py, to_type)?);
            }
            Ok(serde_json::Value::Array(arr))
        }
        MappedPyType::String(v) => Ok(json!(v)),
        MappedPyType::Int(v) => Ok(json!(v)),
        MappedPyType::Float(v) => Ok(json!(v)),
        MappedPyType::Bool(v) => Ok(json!(v)),
        MappedPyType::None => Ok(serde_json::Value::Null),
        MappedPyType::Unsupported(r#type) => Err(vec![SerializationError {
            position: vec![],
            message: format!("Unsupported type: {}", r#type),
        }]),
    }
}
