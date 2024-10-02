mod errors;
mod parse_py_type;
mod runtime;
mod types;

use pyo3::prelude::{pyfunction, pymodule, PyAnyMethods, PyModule, PyResult};
use pyo3::{wrap_pyfunction, Bound, Python};

#[pyfunction]
fn invoke_runtime_cli(py: Python) -> PyResult<()> {
    Ok(baml_runtime::BamlRuntime::run_cli(
        py.import_bound("sys")?
            .getattr("argv")?
            .extract::<Vec<String>>()?,
        baml_runtime::RuntimeCliDefaults {
            output_type: baml_types::GeneratorOutputType::PythonPydantic,
        },
    )
    .map_err(errors::BamlError::from_anyhow)?)
}

#[pymodule]
fn baml_py(m: Bound<'_, PyModule>) -> PyResult<()> {
    if let Err(e) = env_logger::try_init_from_env(
        env_logger::Env::new()
            .filter("BAML_LOG")
            .write_style("BAML_LOG_STYLE"),
    ) {
        eprintln!("Failed to initialize BAML logger: {:#}", e);
    };

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
    m.add("BamlError", m.py().get_type_bound::<errors::BamlError>())?;
    m.add(
        "BamlInvalidArgumentError",
        m.py().get_type_bound::<errors::BamlInvalidArgumentError>(),
    )?;
    m.add(
        "BamlClientError",
        m.py().get_type_bound::<errors::BamlClientError>(),
    )?;
    m.add(
        "BamlClientHttpError",
        m.py().get_type_bound::<errors::BamlClientHttpError>(),
    )?;

    // m.add(
    //     "BamlValidationError",
    //     m.py().get_type_bound::<errors::BamlValidationError>(),
    // )?;
    // m.add_class::<errors::BamlValidationError>()?;
    errors::errors_module(&m)?;

    Ok(())
}
