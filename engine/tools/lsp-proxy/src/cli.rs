use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "lsp-proxy")]
#[command(about = "LSP proxy for recording and replaying LSP sessions")]
#[command(version)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Record an LSP session to a file
    Record {
        /// Output file for recorded session (defaults to timestamped file)
        #[arg(short, long)]
        output: Option<PathBuf>,
        
        /// LSP command to execute (e.g., "baml-cli lsp" or "rust-analyzer")
        #[arg(last = true, required = true)]
        lsp_command: Vec<String>,
    },
    /// Replay a recorded LSP session
    Replay {
        /// Recorded session file to replay
        session_file: PathBuf,
        
        /// LSP command to execute against replayed messages
        #[arg(last = true, required = true)]
        lsp_command: Vec<String>,
    },
}

impl Cli {
    pub fn parse_args() -> Self {
        Self::parse()
    }
}