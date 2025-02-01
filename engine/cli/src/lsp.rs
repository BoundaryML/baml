use anyhow::Result;
use clap::Args;
use language_server::run_server;

#[derive(Args, Debug)]
pub struct LanguageServerArgs {
    #[arg(
        long,
        help = "port to expose language server on",
        default_value = "2025"
    )]
    port: u16,
}

impl LanguageServerArgs {
    pub fn run(&self) -> Result<()> {
        run_server()?;
        Ok(())
    }
}
