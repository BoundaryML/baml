use crate::test_executor::TestExecutor;
use crate::{test_executor::TestFilter, BamlRuntime};
use anyhow::Result;
use clap::{Args, Subcommand};
use std::path::PathBuf;

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
    Run {
        #[arg(long, help = "Number of tests to run in parallel", default_value_t = 10)]
        parallel: usize,

        #[arg(long, help = "Pass if no tests are selected", default_value_t = false)]
        pass_if_no_tests: bool,

        #[arg(long, help = "Fail if any tests need human evaluation", default_value_t = true)]
        require_human_eval: bool,

        #[arg(long, help = "Output format to use for test results", default_value_t = OutputFormat::Pretty)]
        output_format: OutputFormat,
    },
}

#[derive(Clone, Debug)]
pub enum OutputFormat {
    Pretty,
    Github,
}

impl std::fmt::Display for OutputFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OutputFormat::Pretty => write!(f, "pretty"),
            OutputFormat::Github => write!(f, "github"),
        }
    }
}

impl std::str::FromStr for OutputFormat {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "github" => Ok(OutputFormat::Github),
            "pretty" => Ok(OutputFormat::Pretty),
            _ => Err(format!("Invalid output format: {}", s)),
        }
    }
}

pub enum TestRunResult {
    Success,
    HumanEvalRequired,
    TestFailure,
    TestCancelled,
    NoTestsRun,
}

impl TestArgs {
    pub async fn run(&self) -> Result<TestRunResult> {
        let from = BamlRuntime::parse_baml_src_path(&self.from)?;

        let runtime = BamlRuntime::from_directory(&from, std::env::vars().collect())?;
        let runtime = std::sync::Arc::new(runtime);

        let test_execution_args = TestFilter::from(
            self.include.iter().map(|s| s.as_str()),
            self.exclude.iter().map(|s| s.as_str()),
        );

        match &self.command {
            Some(TestCommand::List) | None => {
                // Default to list if no subcommand is provided
                runtime.cli_list_tests(&test_execution_args)?;
            }
            Some(TestCommand::Run {
                parallel,
                pass_if_no_tests,
                require_human_eval,
                output_format,
            }) => {
                match runtime.cli_run_tests(&test_execution_args, *parallel).await {
                    crate::test_executor::TestRunStatus::NoTests => {
                        if *pass_if_no_tests {
                            return Ok(TestRunResult::Success)
                        } else {
                            return Ok(TestRunResult::NoTestsRun)
                        }
                    }
                    crate::test_executor::TestRunStatus::Passed => {                    }
                    crate::test_executor::TestRunStatus::NeedsEval => {
                        if *require_human_eval {
                            return Ok(TestRunResult::HumanEvalRequired)
                        }
                    }
                    crate::test_executor::TestRunStatus::Failed(_) => {
                        return Ok(TestRunResult::TestFailure)
                    }
                    crate::test_executor::TestRunStatus::Cancelled => {
                        return Ok(TestRunResult::TestCancelled)
                    }
                }
            }
        }

        Ok(TestRunResult::Success)
    }
}
