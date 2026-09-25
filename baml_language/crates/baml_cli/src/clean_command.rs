use std::path::PathBuf;

use anyhow::{Result, bail};
use clap::Args;

#[derive(Args, Clone, Debug, Default)]
#[command(
    after_long_help = "Examples:\n  Clean the nearest project's profiler data:\n    baml clean\n\n  Clean a specific project:\n    baml clean --project ./my-project"
)]
pub struct CleanArgs {
    /// Deprecated alias for `--project`.
    #[arg(long, value_name = "PATH", hide = true)]
    pub from: Option<PathBuf>,
}

impl CleanArgs {
    pub fn run(&self) -> Result<crate::ExitCode> {
        bail!("profiling cleanup is unavailable: the old runtime tracing pipeline has been removed")
    }
}
