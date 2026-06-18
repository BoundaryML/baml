use std::path::PathBuf;

use anyhow::Result;
use baml_lsp_server::run_server;
use clap::Args;

#[derive(Args, Debug)]
pub struct LanguageServerArgs {
    /// Workspace root to discover BAML projects from when running the LSP
    /// outside an editor client. May be passed more than once.
    #[clap(long, value_name = "PATH")]
    pub workspace: Vec<PathBuf>,
}

impl LanguageServerArgs {
    pub fn run(&self) -> Result<()> {
        run_server(self.workspace.clone())?;
        Ok(())
    }
}
