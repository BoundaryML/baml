mod generate;

use anyhow::Result;
use clap::{Args, Parser, Subcommand, ValueEnum};
use std::fmt;

#[derive(Parser, Debug)]
#[command(author, version, about = "A CLI tool for working with the BAML runtime.", long_about = None)]
#[command(propagate_version = true)]
pub(crate) struct RuntimeCli {
    /// Specifies a subcommand to run.
    #[command(subcommand)]
    pub(crate) command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    Generate(generate::GenerateArgs),
}

impl RuntimeCli {
    pub fn run(&self) -> Result<()> {
        match self.command {
            Commands::Generate(ref args) => args.run(),
        }
    }
}
