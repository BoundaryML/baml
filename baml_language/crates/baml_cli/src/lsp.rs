use std::path::PathBuf;

use anyhow::Result;
use baml_lsp_server::run_server;
use clap::Args;

/// Start a BAML language server over standard input and output.
///
/// Editor integrations normally start this command automatically. Use one or
/// more `--workspace` paths when launching it outside an editor client.
#[derive(Args, Debug)]
#[command(after_long_help = "\
Examples:
  Start a language server:
    baml lsp

  Add a workspace root:
    baml lsp --workspace ./my-project")]
pub struct LanguageServerArgs {
    /// Workspace root to discover BAML projects from when running the LSP
    /// outside an editor client. May be passed more than once.
    #[clap(long, value_name = "PATH", help_heading = "Workspace options")]
    pub workspace: Vec<PathBuf>,
}

impl LanguageServerArgs {
    pub fn run(&self) -> Result<()> {
        run_server(self.workspace.clone())?;
        Ok(())
    }
}
