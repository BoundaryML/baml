use anyhow::Result;
use baml_runtime::{BamlRuntime, RuntimeCliDefaults};

fn main() -> Result<()> {
    // We init here and not in run_cli because Python/Node FFI set run_cli on library import, not at argv parse time
    if let Err(e) = env_logger::Builder::from_env(
        env_logger::Env::new()
            .filter_or("BAML_LOG", "info")
            .write_style("BAML_LOG_STYLE"),
    )
    // .format_target(false)
    .try_init()
    {
        eprintln!("Failed to initialize BAML logger: {:#}", e);
    };

    let argv: Vec<String> = std::env::args().collect();

    BamlRuntime::run_cli(
        argv,
        RuntimeCliDefaults {
            output_type: baml_types::GeneratorOutputType::OpenApi,
        },
    )?;

    Ok(())
}
