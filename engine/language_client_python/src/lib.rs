mod parse_py_type;
mod python_types;

use baml_runtime::{BamlRuntime, RuntimeContext, RuntimeInterface};
use parse_py_type::parse_py_type;
use pyo3::exceptions::{PyRuntimeError, PyTypeError};
use pyo3::prelude::{pyclass, pyfunction, pymethods, pymodule, PyModule, PyResult};

use pyo3::types::{IntoPyDict, PyType};
use pyo3::{create_exception, Py, PyAny, PyErr, PyObject, Python, ToPyObject};
use pythonize::depythonize;
use serde::de;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

create_exception!(baml_py, BamlError, pyo3::exceptions::PyException);

impl BamlError {
    fn from_anyhow(err: anyhow::Error) -> PyErr {
        PyErr::new::<BamlError, _>(format!("{:?}", err))
    }
}

#[pyclass]
struct BamlRuntimeFfi {
    internal: Arc<BamlRuntime>,
}

// Use this once we update pyo3. Current version doesn't support this struct enum.
// pub enum BamlImagePy {
//     // struct
//     Url { url: String },
//     Base64 { base64: String },
// }

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename = "Image")]
#[pyclass(name = "Image")]
struct BamlImagePy {
    url: Option<String>,
    base64: Option<String>,
}

// Implement constructor for BamlImage
#[pymethods]
impl BamlImagePy {
    #[new]
    fn new(url: Option<String>, base64: Option<String>) -> Self {
        BamlImagePy { url, base64 }
    }

    #[getter]
    fn get_url(&self) -> PyResult<Option<String>> {
        Ok(self.url.clone())
    }

    #[getter]
    fn get_base64(&self) -> PyResult<Option<String>> {
        Ok(self.base64.clone())
    }

    #[setter]
    fn set_url(&mut self, url: Option<String>) {
        self.url = url;
    }

    #[setter]
    fn set_base64(&mut self, base64: Option<String>) {
        self.base64 = base64;
    }

    fn __repr__(&self) -> String {
        let url_repr = match &self.url {
            Some(url) => format!("Optional(\"{}\")", url),
            None => "None".to_string(),
        };
        let base64_repr = match &self.base64 {
            Some(base64) => format!("Optional(\"{}\")", base64),
            None => "None".to_string(),
        };
        format!("Image(url={}, base64={})", url_repr, base64_repr)
    }

    // Makes it work with Pydantic
    #[classmethod]
    fn __get_pydantic_core_schema__(
        cls: &PyType,
        source_type: &PyAny,
        handler: &PyAny,
    ) -> PyResult<PyObject> {
        Python::with_gil(|py| {
            let code = r#"
from pydantic_core import core_schema

def get_schema():
    # No validation
    return core_schema.any_schema()

ret = get_schema()
    "#;
            // py.run(code, None, Some(ret_dict));
            let fun: Py<PyAny> = PyModule::from_code(py, code, "", "")
                .unwrap()
                .getattr("ret")
                .unwrap()
                .into();
            Ok(fun.to_object(py)) // Return the PyObject
        })
    }

    fn __eq__(&self, other: &Self) -> bool {
        self == other
    }
}

fn convert_to_hashmap(value: Value) -> Option<HashMap<String, Value>> {
    match value {
        Value::Object(map) => Some(map.into_iter().collect()),
        _ => None,
    }
}

#[pymethods]
impl BamlRuntimeFfi {
    #[staticmethod]
    fn from_directory(directory: PathBuf) -> PyResult<Self> {
        Ok(BamlRuntimeFfi {
            internal: Arc::new(
                BamlRuntime::from_directory(&directory).map_err(BamlError::from_anyhow)?,
            ),
        })
    }

    /// TODO: ctx should be optional
    #[pyo3(signature = (function_name, args, *, ctx))]
    fn call_function(
        &self,
        py: Python<'_>,
        function_name: String,
        args: PyObject,
        ctx: PyObject,
    ) -> PyResult<PyObject> {
        let args = parse_py_type(args.as_ref(py).to_object(py))?;
        let args_map = convert_to_hashmap(args);
        println!("call_function args {:#?}", args_map);

        let mut ctx: RuntimeContext = depythonize(ctx.as_ref(py))?;

        ctx.env = std::env::vars_os()
            .map(|(k, v)| {
                (
                    k.to_string_lossy().to_string(),
                    v.to_string_lossy().to_string(),
                )
            })
            .chain(ctx.env.into_iter())
            .collect();

        match args_map {
            None => Err(BamlError::new_err("Failed to parse args")),
            Some(args_map) => {
                let baml_runtime = Arc::clone(&self.internal);

                pyo3_asyncio::tokio::future_into_py(py, async move {
                    let result = baml_runtime
                        .call_function(function_name, args_map, &ctx)
                        .await
                        .map(python_types::FunctionResult::new)
                        .map_err(BamlError::from_anyhow);

                    result
                })
                .map(|f| f.into())
            }
        }
    }
}

#[pymodule]
fn baml_py(_: Python<'_>, m: &PyModule) -> PyResult<()> {
    if let Err(e) = env_logger::try_init_from_env(
        env_logger::Env::new()
            .filter("BAML_LOG")
            .write_style("BAML_LOG_STYLE"),
    ) {
        eprintln!("Failed to initialize BAML logger: {:#}", e);
    };

    m.add_class::<BamlRuntimeFfi>()?;
    m.add_class::<python_types::FunctionResult>()?;
    m.add_class::<BamlImagePy>()?;

    Ok(())
}
