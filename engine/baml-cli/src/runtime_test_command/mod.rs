use anyhow::Result;

mod filter;
mod run_state;

use baml_runtime::{load_runtime_from_dir, InternalRuntimeInterface};
use filter::FilterArgs;

use crate::{runtime_test_command::run_state::TestCommand, TestArgs};

pub fn run(command: &TestArgs) -> Result<()> {
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(run_async(command))
}

async fn run_async(command: &TestArgs) -> Result<()> {
    let filter_args = FilterArgs::from_command(command)?;

    // Now find the right directory
    let baml_dir = crate::builder::get_baml_src(&command.baml_dir)?;

    // Load the runtime.
    let runtime = load_runtime_from_dir(&baml_dir)?;
    runtime.features().err_if_legacy()?;

    let test_command = TestCommand::new(runtime, filter_args);

    match command.action {
        crate::TestAction::Run => {
            let env_vars = std::env::vars().collect();
            let response = test_command.run_parallel(4, &env_vars).await?;
        }
        crate::TestAction::List => test_command.print_as_list(true).await,
    }

    Ok(())
}
