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
        // Same resolution rule as the producer and `baml query`:
        // BAML_PROFILE_DIR wins, else the project store. Resolving this
        // differently would report "Clean" while leaving recorded data in
        // the directory the profiler actually wrote to.
        let profiles_root =
            bex_events::prof::backend::ProfilerSession::resolve_store_root(&project_root);
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
            // Cleanup refuses any root not shaped `.../.baml/profiles-v1`,
            // so it cannot delete an arbitrary BAML_PROFILE_DIR. Say so
            // instead of surfacing the bare `InvalidRoot`.
            Err(bex_events::prof::backend::CleanProfilesError::InvalidRoot)
                if std::env::var_os("BAML_PROFILE_DIR").is_some() =>
            {
                bail!(
                    "BAML_PROFILE_DIR points at {}, which is not a `.baml/profiles-v1` store; \
                     remove it manually",
                    profiles_root.display()
                )
            }
            Err(error) => Err(anyhow::Error::new(error).context(format!(
                "failed to clean segmented profiler data at {}",
                profiles_root.display()
            ))),
        }
    }
}
