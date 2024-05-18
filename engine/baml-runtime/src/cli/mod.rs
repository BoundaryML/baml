mod generate;

use anyhow::Result;
use clap::{Parser, Subcommand};

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
    Generate(generate::GenerateArgs),
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
        }
    }
}
