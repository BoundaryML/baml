use anyhow::Result;
use baml_lsp_server::run_server;
use clap::Args;
use std::path::PathBuf;

#[derive(Args, Debug)]
pub struct LanguageServerArgs {
    /// Open the playground in the system browser instead of sending
    /// an LSP notification to the client.
    #[clap(long)]
    pub playground_via_browser: bool,

    /// Workspace root to discover BAML projects from when running the LSP
    /// outside an editor client. May be passed more than once.
    #[clap(long, value_name = "PATH")]
    pub workspace: Vec<PathBuf>,
}

impl LanguageServerArgs {
    pub fn run(&self) -> Result<()> {
        run_server(self.playground_via_browser, self.workspace.clone())?;
        Ok(())
    }
}
