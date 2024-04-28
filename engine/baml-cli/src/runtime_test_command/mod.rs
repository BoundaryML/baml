use anyhow::Result;

mod filter;
mod run_state;
use std::path::PathBuf;

use filter::FilterArgs;

use crate::{runtime_test_command::run_state::TestCommand, TestArgs};

use baml_runtime::{internal::WithInternal, BamlRuntime};

fn check_supported(runtime: &BamlRuntime) -> anyhow::Result<()> {
    let features = runtime.features();
    if features.v1_functions {
        return Err(anyhow::anyhow!("Legacy functions are not supported. Please migrate to the new function format. See https://docs.boundaryml.com"));
    }

    if features.class_getters {
        return Err(anyhow::anyhow!("Legacy @get is not supported. Please remove them from your code. See https://docs.boundaryml.com"));
    }

    Ok(())
}

pub fn run(command: &TestArgs) -> Result<()> {
    let filter_args = FilterArgs::from_command(command)?;

    // Now find the right directory
    let baml_dir = crate::builder::get_baml_src(&command.baml_dir)?;

    // Load the runtime.
    let runtime = std::sync::Arc::from(BamlRuntime::from_directory(&baml_dir)?);
    check_supported(&runtime)?;

    let test_command = TestCommand::new(&runtime, &filter_args);

    let env_vars = std::env::vars().collect();

    let fut = test_command.run_parallel(4, runtime.clone(), &env_vars);

    let rt = tokio::runtime::Runtime::new()?;
    let test_summary = rt.block_on(fut)?;

    Ok(())
}
