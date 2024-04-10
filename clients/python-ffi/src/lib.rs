use internal_baml_jinja;
use pyo3::exceptions::{PyRuntimeError, PyTypeError};
use pyo3::prelude::{
    pyclass, pyfunction, pymethods, pymodule, wrap_pyfunction, Bound, PyAnyMethods, PyDictMethods,
    PyModule, PyResult,
};
use pyo3::types::{PyDict, PyList, PyTuple};
use pyo3::{Py, PyObject, Python};
use pythonize::{depythonize_bound, pythonize};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;

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

#[derive(Clone, Debug, Serialize)]
#[pyclass]
#[allow(non_camel_case_types)]
struct RenderData_Client {
    name: String,
    provider: String,
}

#[derive(Clone, Debug, Serialize)]
#[pyclass]
#[allow(non_camel_case_types)]
struct RenderData_Context {
    client: RenderData_Client,
    output_schema: String,
    env: HashMap<String, String>,
}

#[pymethods]
impl RenderData_Context {
    fn set_env(&mut self, key: String, value: String) {
        self.env.insert(key, value);
    }
}

#[derive(Clone, Debug, Deserialize)]
#[pyclass]
struct TemplateStringMacro {
    name: String,
    args: Vec<(String, String)>,
    template: String,
}

#[derive(Clone, Debug)]
#[pyclass]
struct RenderData {
    args: PyObject,
    ctx: RenderData_Context,
    template_string_macros: Vec<TemplateStringMacro>,
}

#[pymethods]
impl RenderData {
    #[new]
    fn new(
        args: PyObject,
        ctx: RenderData_Context,
        template_string_macros: &PyList,
    ) -> PyResult<Self> {
        Python::with_gil(|py| {
            Ok(RenderData {
                args: args,
                ctx: ctx,
                template_string_macros: template_string_macros
                    .iter()
                    .map(|item| item.extract::<TemplateStringMacro>())
                    .collect::<PyResult<Vec<TemplateStringMacro>>>()?,
            })
        })
    }

    #[staticmethod]
    fn ctx(
        client: RenderData_Client,
        output_schema: String,
        env: PyObject,
    ) -> PyResult<RenderData_Context> {
        Python::with_gil(|py| {
            Ok(RenderData_Context {
                client: client,
                output_schema: output_schema,
                env: depythonize_bound(env.into_bound(py))?,
            })
        })
    }

    #[staticmethod]
    fn client(name: String, provider: String) -> RenderData_Client {
        RenderData_Client {
            name: name,
            provider: provider,
        }
    }

    #[staticmethod]
    fn template_string_macro(
        name: String,
        args: PyObject,
        template: String,
    ) -> PyResult<TemplateStringMacro> {
        Python::with_gil(|py| {
            Ok(TemplateStringMacro {
                name: name,
                args: args.extract(py)?,
                template: template,
            })
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[pyclass]
struct RenderedChatMessage {
    #[pyo3(get)]
    role: String,

    #[pyo3(get)]
    message: String,
}

// Implemented for testing purposes
#[pymethods]
impl RenderedChatMessage {
    #[new]
    fn new(role: String, message: String) -> Self {
        RenderedChatMessage { role, message }
    }

    fn __repr__(&self) -> String {
        format!("{self:#?}")
    }

    fn __eq__(&self, other: &Self) -> bool {
        self == other
    }
}

impl From<internal_baml_jinja::RenderedChatMessage> for RenderedChatMessage {
    fn from(chat_message: internal_baml_jinja::RenderedChatMessage) -> Self {
        RenderedChatMessage {
            role: chat_message.role,
            message: chat_message.message,
        }
    }
}

#[pyfunction]
fn render_prompt(template: String, context: RenderData) -> PyResult<PyObject> {
    let RenderData {
        args,
        ctx,
        template_string_macros,
    } = context;
    let render_args = match pyobject_to_json(args) {
        Ok(render_args) => render_args,
        Err(errors) => {
            let messages = errors
                .into_iter()
                .map(|SerializationError { position, message }| {
                    format!("args.{}: {message}", position.join("."))
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
    let serde_json::Value::Object(render_args) = render_args else {
        return Err(PyTypeError::new_err(
            "args must be convertible to a JSON object",
        ));
    };

    let rendered = internal_baml_jinja::render_prompt(
        &template,
        render_args,
        internal_baml_jinja::RenderContext {
            client: internal_baml_jinja::RenderContext_Client {
                name: ctx.client.name,
                provider: ctx.client.provider,
            },
            output_schema: ctx.output_schema,
            env: ctx.env,
        },
        template_string_macros
            .into_iter()
            .map(|t| internal_baml_jinja::TemplateStringMacro {
                name: t.name,
                args: t.args,
                template: t.template,
            })
            .collect::<Vec<_>>(),
    );

    match rendered {
        Ok(internal_baml_jinja::RenderedPrompt::Completion(s)) => {
            Ok(Python::with_gil(|py| pythonize(py, &("completion", s)))?)
        }
        Ok(internal_baml_jinja::RenderedPrompt::Chat(messages)) => Python::with_gil(|py| {
            let messages = messages
                .into_iter()
                .map(|message| Py::new(py, RenderedChatMessage::from(message)))
                .collect::<PyResult<Vec<_>>>()?;
            Ok(PyTuple::new_bound(
                py,
                vec![
                    pythonize(py, "chat")?,
                    PyList::new_bound(py, messages).into(),
                ],
            )
            .into())
        }),
        Err(err) => Err(PyRuntimeError::new_err(format!("{err:#}"))),
    }
}

#[pymodule]
fn baml_core_ffi(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(render_prompt, m)?)?;
    m.add_class::<RenderData>()?;
    m.add_class::<RenderData_Client>()?;
    m.add_class::<RenderData_Context>()?;
    m.add_class::<RenderedChatMessage>()?;
    m.add_class::<TemplateStringMacro>()?;
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
