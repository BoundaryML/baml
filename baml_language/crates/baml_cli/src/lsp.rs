use anyhow::Result;
use clap::Args;
use lsp_server::run_server;

#[derive(Args, Debug)]
pub struct LanguageServerArgs {}

impl LanguageServerArgs {
    pub fn run(&self) -> Result<()> {
        run_server()?;
        Ok(())
    }
}
