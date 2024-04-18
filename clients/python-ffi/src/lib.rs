mod parse_py_type;

use anyhow::{bail, Result};
use internal_baml_jinja;
use parse_py_type::parse_py_type;
use pyo3::exceptions::{PyRuntimeError, PyTypeError};
use pyo3::prelude::{
    pyclass, pyfunction, pymethods, pymodule, wrap_pyfunction, Bound, PyAnyMethods, PyModule,
    PyResult,
};
use pyo3::types::{PyDict, PyList, PyTuple};
use pyo3::{Py, PyObject, Python, ToPyObject};
use pythonize::{depythonize_bound, pythonize};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use std::fmt::format;

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
        template_string_macros: &Bound<'_, PyList>,
    ) -> PyResult<Self> {
        Ok(RenderData {
            args,
            ctx,
            template_string_macros: template_string_macros
                .as_gil_ref()
                .iter()
                .map(|item| item.extract::<TemplateStringMacro>())
                .collect::<PyResult<Vec<TemplateStringMacro>>>()?,
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
    let render_args = parse_py_type(args)?;
    let serde_json::Value::Object(render_args) = render_args else {
        return Err(PyTypeError::new_err(
            "args must be convertible to a JSON object",
        ));
    };

    let rendered = internal_baml_jinja::render_prompt(
        &template,
        &render_args,
        &internal_baml_jinja::RenderContext {
            client: internal_baml_jinja::RenderContext_Client {
                name: ctx.client.name,
                provider: ctx.client.provider,
            },
            output_schema: ctx.output_schema,
            env: ctx.env,
        },
        &template_string_macros
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
