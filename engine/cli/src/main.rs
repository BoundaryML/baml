use anyhow::Result;
use baml_runtime::{BamlRuntime, RuntimeCliDefaults};
use indicatif::MultiProgress;

fn main() -> Result<()> {
    // We init here and not in run_cli because Python/Node FFI set run_cli on library import, not at argv parse time
    let logger = env_logger::Builder::from_env(
        env_logger::Env::new()
            .filter_or("BAML_LOG", "info")
            .write_style("BAML_LOG_STYLE"),
    )
    .build();
    let level = logger.filter();

    if let Err(e) = indicatif_log_bridge::LogWrapper::new(MultiProgress::new(), logger).try_init() {
        eprintln!("Failed to initialize BAML logger: {:#}", e);
    }
    log::set_max_level(level);

    let argv: Vec<String> = std::env::args().collect();

    baml_cli::run_cli(
        argv,
        RuntimeCliDefaults {
            output_type: baml_types::GeneratorOutputType::OpenApi,
        },
    )?;

    Ok(())
}
