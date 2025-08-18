mod cli;
mod metadata;
mod proxy;
mod recorder;
mod subprocess;

use anyhow::Result;
use cli::{Cli, Commands};
use proxy::LspProxy;
use std::path::PathBuf;
use tracing_subscriber;

fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("lsp_proxy=info".parse()?)
        )
        .init();

    let cli = Cli::parse_args();

    match cli.command {
        Commands::Record { output, lsp_command } => {
            let output_file = output.unwrap_or_else(|| {
                let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
                PathBuf::from(format!("lsp-session-{}.jsonl", timestamp))
            });

            let proxy = LspProxy::new(lsp_command)?;
            proxy.run_record_mode(output_file)?;
        }

        Commands::Replay { session_file, lsp_command } => {
            let proxy = LspProxy::new(lsp_command)?;
            proxy.run_replay_mode(session_file)?;
        }
    }

    Ok(())
}