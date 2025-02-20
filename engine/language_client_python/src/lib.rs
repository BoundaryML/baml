mod errors;
mod parse_py_type;
mod runtime;
mod types;

use ctrlc;
use pyo3::prelude::{pyfunction, pymodule, PyAnyMethods, PyModule, PyResult};
use pyo3::types::PyModuleMethods;
use pyo3::{wrap_pyfunction, Bound, Python};
use tracing_subscriber::{self, EnvFilter};

#[pyfunction]
fn invoke_runtime_cli(py: Python) -> PyResult<()> {
    // SIGINT (Ctrl+C) Handling Implementation, an approach from @revidious
    //
    // Background:
    // When running BAML through Python, we face a challenge where Python's default SIGINT handling
    // can interfere with graceful shutdown. This is because:
    // 1. Python has its own signal handlers that may conflict with Rust's
    // 2. The PyO3 runtime can sometimes mask or delay interrupt signals
    // 3. We need to ensure clean shutdown across the Python/Rust boundary
    //
    // Solution:
    // We implement a custom signal handling mechanism using Rust's ctrlc crate that:
    // 1. Bypasses Python's signal handling entirely
    // 2. Provides consistent behavior across platforms
    // 3. Ensures graceful shutdown with proper exit codes
    // Note: While eliminating the root cause of SIGINT handling conflicts would be ideal,
    // the source appears to be deeply embedded in BAML's architecture and PyO3's runtime.
    // A proper fix would require extensive changes to how BAML handles signals across the
    // Python/Rust boundary. For now, this workaround provides reliable interrupt handling
    // without requiring major architectural changes but welp, this is a hacky solution.

    // Create a channel for communicating between the signal handler and main thread
    // This is necessary because signal handlers run in a separate context and
    // need a safe way to communicate with the main program
    let (interrupt_send, interrupt_recv) = std::sync::mpsc::channel();

    // Install our custom Ctrl+C handler
    // This will run in a separate thread when SIGINT is received
    ctrlc::set_handler(move || {
        println!("\nShutting Down BAML...");
        // Notify the main thread through the channel
        // Using ok() to ignore send errors if the receiver is already dropped
        interrupt_send.send(()).ok();
    })
    .expect("Error setting Ctrl-C handler");

    // Monitor for interrupt signals in a separate thread
    // This is necessary because we can't directly exit from the signal handler.

    std::thread::spawn(move || {
        if interrupt_recv.recv().is_ok() {
            // Exit with code 130 (128 + SIGINT's signal number 2)
            // This is the standard Unix convention for processes terminated by SIGINT
            std::process::exit(130);
        }
    });

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
    m.add_class::<types::Collector>()?;
    m.add_class::<types::FunctionLog>()?;
    m.add_class::<types::LLMCall>()?;
    m.add_class::<types::Timing>()?;
    m.add_class::<types::Usage>()?;
    m.add_wrapped(wrap_pyfunction!(invoke_runtime_cli))?;

    errors::errors(&m)?;

    Ok(())
}
