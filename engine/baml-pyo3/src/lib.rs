use internal_baml_jinja;
use pyo3::exceptions::{PyRuntimeError, PyTypeError};
use pyo3::prelude::{
    pyfunction, pymodule, wrap_pyfunction, Bound, PyAnyMethods, PyDictMethods, PyModule, PyResult,
};
use pyo3::types::{PyDict, PyList};
use pyo3::{PyObject, Python};
use serde_json::json;

struct SerializationError {
    position: Vec<String>,
    message: String,
}

fn pyobject_to_json(any: PyObject) -> Result<serde_json::Value, Vec<SerializationError>> {
    Python::with_gil(|py| {
        if let Ok(pydict) = any.downcast_bound::<PyDict>(py) {
            let mut map = serde_json::Map::new();

            let mut string_key_count = 0;
            let mut total_key_count = 0;
            let mut errors = Vec::<SerializationError>::new();

            for (key, value) in pydict.iter() {
                total_key_count += 1;

                if let Ok(key) = key.extract::<String>() {
                    string_key_count += 1;

                    match pyobject_to_json(value.into()) {
                        Ok(value) => {
                            map.insert(key, value);
                        }
                        Err(value_errors) => {
                            for mut error in value_errors {
                                error.position.insert(0, key.clone());
                                errors.push(error);
                            }
                        }
                    };
                }
            }

            if string_key_count < total_key_count {
                errors.push(SerializationError {
                    position: vec![],
                    message: format!(
                        "{} of {total_key_count} dict keys were not strings",
                        total_key_count - string_key_count
                    ),
                });
            }
            if !errors.is_empty() {
                return Err(errors);
            }

            return Ok(map.into());
        }

        if let Ok(pylist) = any.downcast_bound::<PyList>(py) {
            let Ok(iter) = pylist.iter() else {
                return Err(vec![SerializationError {
                    position: vec![],
                    message: "list was provided, but could not be iterated".to_string(),
                }]);
            };

            let mut list = Vec::<serde_json::Value>::new();
            let mut errors = Vec::<SerializationError>::new();

            for (i, item) in iter.enumerate() {
                let Ok(item) = item else {
                    errors.push(SerializationError {
                        position: vec![format!("{i}")],
                        message: "list item could not be dereferenced".to_string(),
                    });
                    continue;
                };
                match pyobject_to_json(item.into()) {
                    Ok(item) => {
                        list.push(item);
                    }
                    Err(item_errors) => {
                        for mut error in item_errors {
                            error.position.insert(0, format!("{i}"));
                            errors.push(error);
                        }
                    }
                }
            }

            if !errors.is_empty() {
                return Err(errors);
            }

            return Ok(list.into());
        }

        if let Ok(s) = any.extract::<String>(py) {
            return Ok(json!(s));
        }

        if let Ok(i) = any.extract::<i64>(py) {
            return Ok(json!(i));
        }

        if let Ok(f) = any.extract::<f64>(py) {
            return Ok(json!(f));
        }

        if let Ok(b) = any.extract::<bool>(py) {
            return Ok(json!(b));
        }

        if any.is_none(py) {
            return Ok(json!(null));
        }

        Err(vec![SerializationError {
            position: vec![],
            message: "unsupported type (must be dict, list, str, int, float, bool, or None)"
                .to_string(),
        }])
    })
}

#[pyfunction]
fn render_prompt(template: String, params: PyObject) -> PyResult<String> {
    let params = match pyobject_to_json(params) {
        Ok(params) => params,
        Err(errors) => {
            let messages = errors
                .into_iter()
                .map(|SerializationError { position, message }| {
                    format!("params.{}: {message}", position.join("."))
                })
                .collect::<Vec<String>>();

            if messages.len() == 1 {
                return Err(PyTypeError::new_err(messages[0].clone()));
            } else {
                return Err(PyTypeError::new_err(format!(
                    "{} errors\n{}",
                    messages.len(),
                    messages.join("\n")
                )));
            }
        }
    };
    let rendered = internal_baml_jinja::render_template(&template, &params);

    match rendered {
        Ok(s) => Ok(s),
        Err(err) => Err(PyRuntimeError::new_err(format!("{err:#}"))),
    }
}

#[pymodule]
fn baml_pyo3(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(render_prompt, m)?)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        assert_eq!(4, 4);
    }
}
