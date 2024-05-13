mod parse_py_type;
mod python_types;

use baml_runtime::{BamlRuntime, RuntimeContext, RuntimeInterface};
use indexmap::IndexMap;
use internal_baml_codegen::GeneratorArgs;
use parse_py_type::parse_py_type;
use pyo3::exceptions::{PyRuntimeError, PyTypeError};
use pyo3::prelude::{pyclass, pyfunction, pymethods, pymodule, PyModule, PyResult};

use pyo3::types::{IntoPyDict, PyType};
use pyo3::{
    create_exception, wrap_pyfunction, wrap_pymodule, Py, PyAny, PyErr, PyObject, Python,
    ToPyObject,
};
use python_types::BamlImagePy;
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

fn convert_to_hashmap(value: Value) -> Option<IndexMap<String, Value>> {
    match value {
        Value::Object(map) => Some(map.into_iter().collect()),
        _ => None,
    }
}

#[pymethods]
impl BamlRuntimeFfi {
    #[staticmethod]
    fn from_directory(py: Python<'_>, directory: PathBuf, ctx: PyObject) -> PyResult<Self> {
        let ctx: RuntimeContext = RuntimeContext::from_env().merge(Some(depythonize::<
            baml_runtime::RuntimeContext,
        >(ctx.as_ref(py))?));

        Ok(BamlRuntimeFfi {
            internal: Arc::new(
                BamlRuntime::from_directory(&directory, &ctx).map_err(BamlError::from_anyhow)?,
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
        log::debug!("pyo3 call_function parsed args into: {:#?}", args_map);
        let ctx = RuntimeContext::from_env().merge(Some(depythonize::<
            baml_runtime::RuntimeContext,
        >(ctx.as_ref(py))?));
        match args_map {
            None => Err(BamlError::new_err("Failed to parse args")),
            Some(args_map) => {
                let baml_runtime = self.internal.clone();

                pyo3_asyncio::tokio::future_into_py(py, async move {
                    let result = baml_runtime
                        .call_function(function_name, &args_map, &ctx)
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

#[pyfunction]
fn invoke_runtime_cli(py: Python) -> PyResult<()> {
    Ok(baml_runtime::BamlRuntime::run_cli(
        py.import("sys")?
            .getattr("argv")?
            .extract::<Vec<String>>()?,
        baml_runtime::CallerType::Python,
    )
    .map_err(BamlError::from_anyhow)?)
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
    m.add_class::<python_types::GenerateArgs>()?;

    m.add_wrapped(wrap_pyfunction!(invoke_runtime_cli))?;

    Ok(())
}
