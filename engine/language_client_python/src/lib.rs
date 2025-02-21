mod errors;
mod parse_py_type;
mod runtime;
mod types;

use pyo3::prelude::{pyfunction, pymodule, PyAnyMethods, PyModule, PyResult};
use pyo3::types::PyModuleMethods;
use pyo3::{wrap_pyfunction, Bound, Python};
use tracing_subscriber::{self, EnvFilter};

#[pyfunction]
fn invoke_runtime_cli(py: Python) -> PyResult<()> {
    baml_cli::run_cli(
        py.import("sys")?
            .getattr("argv")?
            .extract::<Vec<String>>()?,
        baml_runtime::RuntimeCliDefaults {
            output_type: baml_types::GeneratorOutputType::PythonPydantic,
        },
    )
    .map_err(errors::BamlError::from_anyhow)
}

pub(crate) const MODULE_NAME: &str = "baml_py.baml_py";

#[pymodule]
fn baml_py(m: Bound<'_, PyModule>) -> PyResult<()> {
    let use_json = match std::env::var("BAML_LOG_JSON") {
        Ok(val) => val.trim().eq_ignore_ascii_case("true") || val.trim() == "1",
        Err(_) => false,
    };

    if use_json {
        // JSON formatting
        tracing_subscriber::fmt()
            .with_target(false)
            .with_file(false)
            .with_line_number(false)
            .json()
            .with_env_filter(
                EnvFilter::try_from_env("BAML_LOG").unwrap_or_else(|_| EnvFilter::new("info")),
            )
            .flatten_event(true)
            .with_current_span(false)
            .with_span_list(false)
            .init();
    } else {
        // Regular formatting
        if let Err(e) = env_logger::try_init_from_env(
            env_logger::Env::new()
                .filter("BAML_LOG")
                .write_style("BAML_LOG_STYLE"),
        ) {
            eprintln!("Failed to initialize BAML logger: {:#}", e);
        }
    }

    m.add_class::<runtime::BamlRuntime>()?;

    m.add_class::<types::FunctionResult>()?;
    m.add_class::<types::FunctionResultStream>()?;
    m.add_class::<types::SyncFunctionResultStream>()?;
    m.add_class::<types::BamlImagePy>()?;
    m.add_class::<types::BamlAudioPy>()?;
    m.add_class::<types::RuntimeContextManager>()?;
    m.add_class::<types::BamlSpan>()?;
    m.add_class::<types::TypeBuilder>()?;
    m.add_class::<types::EnumBuilder>()?;
    m.add_class::<types::ClassBuilder>()?;
    m.add_class::<types::EnumValueBuilder>()?;
    m.add_class::<types::ClassPropertyBuilder>()?;
    m.add_class::<types::FieldType>()?;
    m.add_class::<types::ClientRegistry>()?;

    m.add_class::<runtime::BamlLogEvent>()?;
    m.add_class::<runtime::LogEventMetadata>()?;

    m.add_wrapped(wrap_pyfunction!(invoke_runtime_cli))?;

    errors::errors(&m)?;

    Ok(())
}
