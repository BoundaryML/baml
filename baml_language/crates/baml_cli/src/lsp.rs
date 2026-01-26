use anyhow::Result;
use baml_lsp::run_server;
use clap::Args;

#[derive(Args, Debug)]
pub struct LanguageServerArgs {}

impl LanguageServerArgs {
    pub fn run(&self) -> Result<()> {
        run_server()?;
        Ok(())
    }
}
