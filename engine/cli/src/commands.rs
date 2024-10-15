use anyhow::Result;
use baml_runtime::cli::RuntimeCliDefaults;
use clap::{Parser, Subcommand};
use internal_baml_core::configuration::GeneratorOutputType;

use baml_runtime::BamlRuntime;

#[derive(Parser, Debug)]
#[command(author, version, about = "A CLI tool for working with the BAML runtime.", long_about = None)]
#[command(propagate_version = true)]
pub(crate) struct RuntimeCli {
    /// Specifies a subcommand to run.
    #[command(subcommand)]
    command: Commands,
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

    #[command(subcommand, about = "Authenticate with Boundary Cloud")]
    Auth(crate::auth::AuthCommands),

    #[command(about = "Login to Boundary Cloud (alias for `baml auth login`)")]
    Login(crate::auth::LoginArgs),

    #[command(about = "Deploy a BAML project to Boundary Cloud")]
    Deploy(crate::deploy::DeployArgs),
}

impl RuntimeCli {
    pub async fn run(&mut self, defaults: RuntimeCliDefaults) -> Result<()> {
        match &mut self.command {
            Commands::Generate(args) => {
                args.from = BamlRuntime::parse_baml_src_path(&args.from)?;
                args.run(defaults)
            }
            Commands::Init(args) => args.run(defaults),
            Commands::Serve(args) => {
                args.from = BamlRuntime::parse_baml_src_path(&args.from)?;
                args.run()
            }
            Commands::Dev(args) => {
                args.from = BamlRuntime::parse_baml_src_path(&args.from)?;
                args.run(defaults)
            }
            Commands::Auth(args) => args.run_async().await,
            Commands::Login(args) => args.run_async().await,
            Commands::Deploy(args) => {
                args.from = BamlRuntime::parse_baml_src_path(&args.from)?;
                args.run_async().await
            }
        }
    }
}
