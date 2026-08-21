use std::{
    io::{self, Write},
    path::PathBuf,
};

use anyhow::Result;
use clap::Args;

use crate::ExitCode;

const MAIN_GUIDE: &str = include_str!("../../../../skill/guides/main.md");

#[derive(Args, Clone, Debug)]
pub(crate) struct AgentArgs {
    #[command(subcommand)]
    pub command: AgentCommand,
}

#[derive(clap::Subcommand, Clone, Debug)]
pub(crate) enum AgentCommand {
    #[command(about = "Print the agent guide bundled with this BAML toolchain")]
    Guide(AgentGuideArgs),

    #[command(about = "Install or refresh the BAML agent bootstrap in this project")]
    Install(AgentInstallArgs),
}

#[derive(Args, Clone, Debug)]
#[command(after_long_help = "\
Examples:
  Print the default guide:
    baml agent guide

  Print the main guide explicitly:
    baml agent guide main")]
pub(crate) struct AgentGuideArgs {
    /// Guide to print.
    #[arg(value_name = "GUIDE", default_value = "main")]
    pub guide: String,
}

#[derive(Args, Clone, Debug)]
#[command(after_long_help = "\
Examples:
  Install the BAML agent bootstrap:
    baml agent install

  Install the bootstrap in a specific project:
    baml agent install --project ./my-project")]
pub(crate) struct AgentInstallArgs {
    /// Deprecated alias for `--project`.
    #[arg(long, value_name = "PATH", hide = true)]
    pub dir: Option<PathBuf>,
}

impl AgentArgs {
    pub fn run(&self) -> Result<ExitCode> {
        match &self.command {
            AgentCommand::Guide(args) => args.run(),
            AgentCommand::Install(_) => {
                anyhow::bail!("`baml agent install` is provided by the BAML wrapper")
            }
        }
    }
}

impl AgentGuideArgs {
    fn run(&self) -> Result<ExitCode> {
        let guide = match self.guide.as_str() {
            "main" => MAIN_GUIDE,
            name => anyhow::bail!("unknown agent guide `{name}`; available guides: main"),
        };
        write_stdout(guide)?;
        Ok(ExitCode::Success)
    }
}

fn write_stdout(content: &str) -> Result<()> {
    let mut stdout = io::stdout().lock();
    stdout.write_all(content.as_bytes())?;
    if !content.ends_with('\n') {
        stdout.write_all(b"\n")?;
    }
    Ok(())
}
