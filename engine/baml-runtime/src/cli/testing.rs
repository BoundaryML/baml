use anyhow::Result;
use crate::{test_executor::TestFilter, BamlRuntime};
use clap::{Args, Subcommand};
use std::path::PathBuf;
use crate::test_executor::TestExecutor;

#[derive(Args, Clone, Debug)]
pub struct TestArgs {
    #[arg(long, help = "path/to/baml_src", default_value = ".", global = true)]
    pub from: PathBuf,

    #[arg(
        long,
        help = "Specific functions or tests to include tests for. If none provided, runs all tests",
        global = true
    )]
    pub include: Vec<String>,

    #[arg(
        long,
        help = "Specific functions or tests to exclude tests for. Takes precedence over --include. If none provided, runs all tests",
        global = true
    )]
    pub exclude: Vec<String>,

    #[command(subcommand)]
    pub command: Option<TestCommand>,
}

#[derive(Subcommand, Clone, Debug)]
pub enum TestCommand {
    /// List all available tests
    List,
    /// Run specified tests
    Run
    // Run {
    //     #[arg(long, help = "Output format to use for test results")]
    //     output_format: OutputFormat,
    // },
}

#[derive(Clone, Debug)]
pub enum OutputFormat {
    Github,
}

impl std::str::FromStr for OutputFormat {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "github" => Ok(OutputFormat::Github),
            _ => Err(format!("Invalid output format: {}", s)),
        }
    }
}

impl TestArgs {
    pub async fn run(&self) -> Result<()> {
        let from = BamlRuntime::parse_baml_src_path(&self.from)?;

        let runtime = BamlRuntime::from_directory(&from, std::env::vars().collect())?;
        let runtime = std::sync::Arc::new(runtime);

        let test_execution_args = TestFilter::from(self.include.iter().map(|s| s.as_str()), self.exclude.iter().map(|s| s.as_str()));

        match &self.command {
            Some(TestCommand::List) | None => {
                // Default to list if no subcommand is provided
                runtime.cli_list_tests(&test_execution_args)?;
            }
            Some(TestCommand::Run { .. }) => {
                runtime.cli_run_tests(&test_execution_args).await?;
            }
        }

        Ok(())
    }
}
