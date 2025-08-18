use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "cli-proxy")]
#[command(about = "A CLI proxy tool for recording and replaying stdin interactions")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    #[command(about = "Record stdin to a JSONL file while forwarding to subprocess")]
    Record {
        #[arg(help = "JSONL file to write recorded stdin to")]
        file: String,
        #[arg(last = true, help = "Command and arguments to run")]
        command: Vec<String>,
    },
    #[command(about = "Replay stdin from a JSONL file then forward live stdin to subprocess")]
    Replay {
        #[arg(help = "JSONL file to read recorded stdin from")]
        file: String,
        #[arg(last = true, help = "Command and arguments to run")]
        command: Vec<String>,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Record { file, command } => {
            if command.is_empty() {
                anyhow::bail!("No command provided after --");
            }
            cli_proxy::record(&file, &command)
        }
        Commands::Replay { file, command } => {
            if command.is_empty() {
                anyhow::bail!("No command provided after --");
            }
            cli_proxy::replay(&file, &command)
        }
    }
}