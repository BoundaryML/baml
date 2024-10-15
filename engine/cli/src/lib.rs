pub(crate) mod api_client;
pub(crate) mod auth;
pub(crate) mod colordiff;
pub(crate) mod commands;
pub(crate) mod deploy;
pub(crate) mod propelauth;
pub(crate) mod tui;

use anyhow::Result;
use clap::Parser;

pub fn run_cli(argv: Vec<String>, caller_type: baml_runtime::RuntimeCliDefaults) -> Result<()> {
    let t = tokio::runtime::Runtime::new()?;
    let _ = t.enter();

    t.block_on(commands::RuntimeCli::parse_from(argv.into_iter()).run(caller_type))
}
