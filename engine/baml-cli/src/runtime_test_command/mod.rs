use anyhow::Result;

mod filter;
mod run_state;
mod tracing_helper;

use baml_runtime::{BamlRuntime, InternalRuntimeInterface};
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

    let ctx = baml_runtime::RuntimeContext::from_env();

    // Load the runtime.
    let runtime = BamlRuntime::from_directory(&baml_dir, &ctx)?;
    runtime.internal().features().err_if_legacy()?;

    let test_command = TestCommand::new(runtime, filter_args);

    match command.action {
        crate::TestAction::Run => {
            let response = test_command.run_parallel(4, &ctx).await?;

            println!("{}", response);
        }
        crate::TestAction::List => test_command.print_as_list(true).await,
    }

    Ok(())
}
