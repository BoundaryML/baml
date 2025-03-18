use anyhow::Result;
use baml_runtime::cli::RuntimeCliDefaults;
use clap::{Parser, Subcommand};

use baml_runtime::BamlRuntime;

#[derive(Parser, Debug)]
#[command(author, version, about = "A CLI tool for working with BAML. Learn more at https://docs.boundaryml.com.", long_about = None)]
#[command(styles = clap_cargo::style::CLAP_STYLING)]
#[command(propagate_version = true)]
pub(crate) struct RuntimeCli {
    /// Specifies a subcommand to run.
    #[command(subcommand)]
    pub(crate) command: Commands,
}

#[derive(Subcommand, Debug)]
pub(crate) enum Commands {
    #[command(about = "Initialize a new BAML project.")]
    Init(baml_runtime::cli::init::InitArgs),

    #[command(about = "Runs all generators in the baml_src directory")]
    Generate(baml_runtime::cli::generate::GenerateArgs),

    #[command(about = "Starts a server that translates LLM responses to BAML responses")]
    Serve(baml_runtime::cli::serve::ServeArgs),

    #[command(about = "Starts a development server")]
    Dev(baml_runtime::cli::dev::DevArgs),

    #[command(subcommand, about = "Authenticate with Boundary Cloud", hide = true)]
    Auth(crate::auth::AuthCommands),

    #[command(about = "Login to Boundary Cloud (alias for `baml auth login`)", hide = true)]
    Login(crate::auth::LoginArgs),

    #[command(about = "Deploy a BAML project to Boundary Cloud", hide = true)]
    Deploy(crate::deploy::DeployArgs),

    #[command(about = "Format BAML source files", name = "fmt", hide = true)]
    Format(crate::format::FormatArgs),

    #[command(about = "Run BAML tests")]
    Test(baml_runtime::cli::testing::TestArgs),
}

impl RuntimeCli {
    pub fn run(&mut self, defaults: RuntimeCliDefaults) -> Result<crate::ExitCode> {
        // NB: we spawn a runtime here but block_on inside the match arms
        // because 'baml-cli dev' and 'baml-cli serve' cannot block_on
        let t = tokio::runtime::Runtime::new()?;
        let _ = t.enter();

        match &mut self.command {
            Commands::Generate(args) => {
                args.from = BamlRuntime::parse_baml_src_path(&args.from)?;
                match args.run(defaults) {
                    Ok(()) => Ok(crate::ExitCode::Success),
                    Err(_) => Ok(crate::ExitCode::Other),
                }
            }
            Commands::Init(args) => {
                match args.run(defaults) {
                    Ok(()) => Ok(crate::ExitCode::Success),
                    Err(_) => Ok(crate::ExitCode::Other),
                }
            },
            Commands::Serve(args) => {
                args.from = BamlRuntime::parse_baml_src_path(&args.from)?;
                match args.run() {
                    Ok(()) => Ok(crate::ExitCode::Success),
                    Err(_) => Ok(crate::ExitCode::Other),
                }
            }
            Commands::Dev(args) => {
                args.from = BamlRuntime::parse_baml_src_path(&args.from)?;
                match args.run(defaults) {
                    Ok(()) => Ok(crate::ExitCode::Success),
                    Err(_) => Ok(crate::ExitCode::Other),
                }
            }
            Commands::Auth(args) => {
                match t.block_on(async { args.run_async().await }) {
                    Ok(()) => Ok(crate::ExitCode::Success),
                    Err(_) => Ok(crate::ExitCode::Other),
                }
            }
            Commands::Login(args) => {
                match t.block_on(async { args.run_async().await }) {
                    Ok(()) => Ok(crate::ExitCode::Success),
                    Err(_) => Ok(crate::ExitCode::Other),
                }
            }
            Commands::Deploy(args) => {
                args.from = BamlRuntime::parse_baml_src_path(&args.from)?;
                match t.block_on(async { args.run_async().await }) {
                    Ok(()) => Ok(crate::ExitCode::Success),
                    Err(_) => Ok(crate::ExitCode::Other),
                }
            }
            Commands::Format(args) => {
                // We deliberately don't apply parse_baml_src_path here
                // see format.rs for more details
                // args.from = BamlRuntime::parse_baml_src_path(&args.from)?;
                match args.run() {
                    Ok(()) => Ok(crate::ExitCode::Success),
                    Err(_) => Ok(crate::ExitCode::Other),
                }
            }
            Commands::Test(args) => {
                let res = t.block_on(async { args.run().await })?;
                match res {
                    baml_runtime::cli::testing::TestRunResult::Success => Ok(crate::ExitCode::Success),
                    baml_runtime::cli::testing::TestRunResult::HumanEvalRequired => Ok(crate::ExitCode::HumanEvalRequired),
                    baml_runtime::cli::testing::TestRunResult::TestFailure => Ok(crate::ExitCode::TestFailure),
                    baml_runtime::cli::testing::TestRunResult::TestCancelled => Ok(crate::ExitCode::TestCancelled),
                    baml_runtime::cli::testing::TestRunResult::NoTestsRun => Ok(crate::ExitCode::NoTestsRun),
                }
            },
        }
    }
}
