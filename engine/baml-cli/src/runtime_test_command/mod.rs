use anyhow::Result;

mod filter;
mod run_state;

use filter::FilterArgs;

use crate::{runtime_test_command::run_state::TestCommand, TestArgs};

use baml_runtime::{internal::WithInternal, BamlRuntime};

fn check_supported(runtime: &BamlRuntime) -> anyhow::Result<()> {
    let features = runtime.features();
    features.err_if_legacy()
}

pub fn run(command: &TestArgs) -> Result<()> {
    let filter_args = FilterArgs::from_command(command)?;

    // Now find the right directory
    let baml_dir = crate::builder::get_baml_src(&command.baml_dir)?;

    // Load the runtime.
    let runtime = std::sync::Arc::from(BamlRuntime::from_directory(&baml_dir)?);
    check_supported(&runtime)?;

    let test_command = TestCommand::new(&runtime, &filter_args);

    match command.action {
        crate::TestAction::Run => {
            let env_vars = std::env::vars().collect();
            let fut = test_command.run_parallel(4, runtime.clone(), &env_vars);
            let rt = tokio::runtime::Runtime::new()?;
            let test_summary = rt.block_on(fut)?;
        }
        crate::TestAction::List => test_command.print_as_list(true),
    }

    Ok(())
}
