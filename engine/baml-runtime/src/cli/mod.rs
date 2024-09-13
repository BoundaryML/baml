pub mod generate;
mod init;

use anyhow::Result;
use clap::{Parser, Subcommand};
use internal_baml_core::configuration::GeneratorOutputType;

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
}

#[derive(clap::ValueEnum, Clone, Debug)]
pub enum LanguageClientType {
    #[clap(name = "python/pydantic")]
    PythonPydantic,

    #[clap(name = "ruby")]
    Ruby,

    #[clap(name = "typescript")]
    Typescript,
}

impl Into<GeneratorOutputType> for &LanguageClientType {
    fn into(self) -> GeneratorOutputType {
        match self {
            LanguageClientType::PythonPydantic => GeneratorOutputType::PythonPydantic,
            LanguageClientType::Ruby => GeneratorOutputType::RubySorbet,
            LanguageClientType::Typescript => GeneratorOutputType::Typescript,
        }
    }
}

impl Into<GeneratorOutputType> for CallerType {
    fn into(self) -> GeneratorOutputType {
        match self {
            CallerType::Python => GeneratorOutputType::PythonPydantic,
            CallerType::Ruby => GeneratorOutputType::RubySorbet,
            CallerType::Typescript => GeneratorOutputType::Typescript,
        }
    }
}

pub enum CallerType {
    Python,
    Ruby,
    Typescript,
}

impl RuntimeCli {
    pub fn run(&self, caller_type: CallerType) -> Result<()> {
        match self.command {
            Commands::Generate(ref args) => args.run(caller_type),
            Commands::Init(ref args) => args.run(caller_type),
        }
    }
}
