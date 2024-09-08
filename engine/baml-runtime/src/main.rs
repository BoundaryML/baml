use anyhow::Result;
use baml_runtime::{BamlRuntime, RuntimeCliDefaults};

fn main() -> Result<()> {
    let argv: Vec<String> = std::env::args().collect();

    BamlRuntime::run_cli(
        argv,
        RuntimeCliDefaults {
            output_type: baml_types::GeneratorOutputType::OpenApi,
        },
    )?;

    Ok(())
}
