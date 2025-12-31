use anyhow::Result;
use baml_language_server::run_server;
use clap::Args;

#[derive(Args, Debug)]
pub struct LanguageServerArgs {}

impl LanguageServerArgs {
    pub fn run(&self) -> Result<()> {
        run_server()?;
        Ok(())
    }
}
