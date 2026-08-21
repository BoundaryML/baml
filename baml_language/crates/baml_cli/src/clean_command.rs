use std::path::PathBuf;

use anyhow::{Context, Result, bail};
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
        let project_root = crate::project_load::find_project_root_from(self.from.as_deref())?
            .unwrap_or(std::env::current_dir().context("failed to resolve current directory")?);
        let profiles_root = project_root.join(".baml/profiles-v1");
        match bex_events::prof::backend::clean_profiles_v1(&profiles_root) {
            Ok(removed) => {
                let reporter = crate::reporter::Reporter::new();
                reporter.status(
                    if removed { "Removed" } else { "Clean" },
                    profiles_root.display().to_string(),
                );
                Ok(crate::ExitCode::Success)
            }
            Err(bex_events::prof::backend::CleanProfilesError::InUse) => {
                bail!("profiling store is in use: {}", profiles_root.display())
            }
            Err(error) => Err(anyhow::Error::new(error).context(format!(
                "failed to clean segmented profiler data at {}",
                profiles_root.display()
            ))),
        }
    }
}
