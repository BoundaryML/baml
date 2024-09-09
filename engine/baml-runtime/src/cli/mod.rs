mod dev;
mod generate;
mod init;
mod serve;

use anyhow::Result;
use clap::{Parser, Subcommand};
use internal_baml_core::configuration::GeneratorOutputType;

use crate::BamlRuntime;

#[derive(Parser, Debug)]
#[command(author, version, about = "A CLI tool for working with the BAML runtime.", long_about = None)]
#[command(propagate_version = true)]
pub(crate) struct RuntimeCli {
    /// Specifies a subcommand to run.
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    #[command(about = "Initialize a new BAML project.")]
    Init(init::InitArgs),
    #[command(about = "Runs all generators in the baml_src directory")]
    Generate(generate::GenerateArgs),
    #[command(about = "Starts a server that translates LLM responses to BAML responses")]
    Serve(serve::ServeArgs),
    #[command(about = "Starts a development server")]
    Dev(dev::DevArgs),
}

/// Default values for the CLI to use.
///
/// We ship different variants of the CLI today:
///
///   - `baml-cli` as bundled with the Python package
///   - `baml-cli` as bundled with the NPM package
///   - `baml-cli` as bundled with the Ruby gem
///
/// Each of these ship with different defaults, as appropriate for
/// the language that they're bundled with.
#[derive(Clone, Copy, Debug)]
pub struct RuntimeCliDefaults {
    pub output_type: GeneratorOutputType,
}

impl RuntimeCli {
    pub fn run(&mut self, defaults: RuntimeCliDefaults) -> Result<()> {
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
        }
    }
}
