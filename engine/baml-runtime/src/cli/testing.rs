use std::{collections::HashMap, path::PathBuf, sync::Arc};

use anyhow::{Context, Result};
use clap::{Args, Subcommand};

use crate::{
    cli::dotenv,
    test_executor::{TestExecutor, TestFilter},
    BamlRuntime,
};

#[derive(Args, Clone, Debug)]
pub struct TestArgs {
    #[arg(long, help = "path/to/baml_src", default_value = ".", global = true)]
    pub from: PathBuf,

    /// Only list selected tests
    #[arg(long, default_value_t = false)]
    list: bool,

    #[arg(long, global = true, short = 'i')]
    /// Specific functions or tests to include tests for. If none provided, runs all tests
    ///
    /// Can chain multiple include filters together
    ///
    /// Examples:
    ///
    /// -i "wild_card*" will match any functions or tests that start with "wild_card"
    ///
    /// -i "FunctionName::TestName" will match the specific test "TestName" in the function "FunctionName"
    ///
    /// -i "FunctionName::" will run all tests in the function "FunctionName"
    ///
    /// -i "::TestName" will run the test "TestName" in any function
    ///
    /// -i "Get*::*Bar" will match any functions that start with "Get" and have a test that ends with "Bar"
    ///
    /// -i "Foo::" -i "Bar::" will run all tests in the functions "Foo" and "Bar"
    pub include: Vec<String>,

    #[arg(long, global = true, short = 'x')]
    /// Specific functions or tests to exclude tests for. Takes precedence over --include. If none provided, runs all tests
    ///
    /// Uses the same syntax as --include
    pub exclude: Vec<String>,

    #[arg(
        long,
        help = "Number of tests to run in parallel",
        default_value_t = 10
    )]
    parallel: usize,

    #[arg(long, help = "Pass if no tests are selected", default_value_t = false)]
    pass_if_no_tests: bool,

    #[arg(
        long,
        help = "Fail if any tests need human evaluation",
        default_value_t = true
    )]
    require_human_eval: bool,

    #[arg(long, help = "Output format to use for test results", default_value_t = OutputFormat::Pretty, hide = true)]
    output_format: OutputFormat,

    #[arg(
        long,
        help = "Output JUnit XML results",
        default_value_t = false,
        hide = true
    )]
    junit: bool,

    #[arg(long, help = "JUnit XML output file, example: --junit-path=junit-report.xml", default_value_t = String::from("junit-report.xml"), hide = true)]
    junit_path: String,

    #[command(flatten)]
    dotenv: dotenv::DotenvArgs,
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
            _ => Err(format!("Invalid output format: {s}")),
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
    /// Creates a runtime to run tests with baml-cli tests.
    ///
    /// This has to be created outside of async contexts because the runtime
    /// creation calls `blocking_send` on the publisher channel.
    ///
    /// `blocking_send` panics inside of async contexts.
    pub fn create_cli_testing_runtime(
        &self,
        feature_flags: internal_baml_core::feature_flags::FeatureFlags,
    ) -> Result<(Arc<BamlRuntime>, HashMap<String, String>)> {
        let from = BamlRuntime::parse_baml_src_path(&self.from)?;

        self.dotenv.load()?;

        let env_vars = std::env::vars().collect::<HashMap<String, String>>();

        let runtime = Arc::new(BamlRuntime::from_directory(
            &from,
            env_vars.clone(),
            feature_flags,
        )?);

        Ok((runtime, env_vars))
    }

    pub async fn run(
        &self,
        feature_flags: internal_baml_core::feature_flags::FeatureFlags,
        runtime: Arc<BamlRuntime>,
        env_vars: HashMap<String, String>,
    ) -> Result<TestRunResult> {
        let test_execution_args = TestFilter::from(
            self.include.iter().map(|s| s.as_str()),
            self.exclude.iter().map(|s| s.as_str()),
        );

        if self.list {
            runtime.cli_list_tests(&test_execution_args)?;
        } else {
            let TestArgs {
                parallel,
                pass_if_no_tests,
                require_human_eval,
                output_format,
                junit,
                junit_path,
                ..
            } = self;

            match runtime
                .cli_run_tests(
                    &test_execution_args,
                    *parallel,
                    output_format,
                    if *junit { Some(junit_path) } else { None },
                    &env_vars,
                )
                .await
            {
                crate::test_executor::TestRunStatus::NoTests => {
                    if *pass_if_no_tests {
                        return Ok(TestRunResult::Success);
                    } else {
                        return Ok(TestRunResult::NoTestsRun);
                    }
                }
                crate::test_executor::TestRunStatus::Passed => {}
                crate::test_executor::TestRunStatus::NeedsEval => {
                    if *require_human_eval {
                        return Ok(TestRunResult::HumanEvalRequired);
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

        Ok(TestRunResult::Success)
    }
}
