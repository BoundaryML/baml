use anyhow::Result;
use baml_lsp_server::run_server;
use clap::Args;

#[derive(Args, Debug)]
pub struct LanguageServerArgs {
    /// Open the playground in the system browser instead of sending
    /// an LSP notification to the client.
    #[clap(long)]
    pub playground_via_browser: bool,
}

impl LanguageServerArgs {
    pub fn run(&self) -> Result<()> {
        run_server(self.playground_via_browser)?;
        Ok(())
    }
}
