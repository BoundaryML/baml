use anyhow::Result;
use baml_runtime::RuntimeCliDefaults;

fn main() -> Result<()> {
    baml_log::init()?;

    let argv: Vec<String> = std::env::args().collect();

    baml_cli::run_cli(
        argv,
        RuntimeCliDefaults {
            output_type: baml_types::GeneratorOutputType::OpenApi,
        },
    )?;
    Ok(())
}
