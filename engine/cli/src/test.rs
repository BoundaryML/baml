use anyhow::Result;
use baml_runtime::BamlRuntime;
use clap::Args;
use std::path::PathBuf;

use baml_runtime::test_executor::TestExecutor;

#[derive(Args, Debug)]
pub struct TestArgs {
    #[arg(long, help = "path/to/baml_src", default_value = ".")]
    pub from: PathBuf,

    #[arg(
        long,
        help = "Specific test files or test names to run. If none provided, runs all tests"
    )]
    pub include: Vec<String>,

    #[arg(
        long,
        help = "Specific test files or test names to run. If none provided, runs all tests"
    )]
    pub exclude: Vec<String>,
}

impl TestArgs {
    pub async fn run(&self) -> Result<()> {
        let from = BamlRuntime::parse_baml_src_path(&self.from)?;

        let runtime = BamlRuntime::from_directory(&from, std::env::vars().collect())?;

        runtime.run_all_tests().await?;

        Ok(())
    }
}
