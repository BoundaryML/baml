//! Publish a self-contained dashboard HTML file with the saved Boundary login.
use anyhow::{Context, Result};
use clap::{Args, Subcommand};
use std::path::PathBuf;

#[derive(Debug, Subcommand)]
pub(crate) enum CloudCommands {
    #[command(
        about = "Upload a dashboard HTML file",
        after_long_help = "Examples:\n  baml cloud upload-dashboard dashboard.html --key overview --name Overview --project acme/app\n\n  Update version 3:\n  baml cloud upload-dashboard dashboard.html --key overview --name Overview --project acme/app --expected-version 3"
    )]
    UploadDashboard(UploadDashboardArgs),
}

#[derive(Debug, Args)]
pub(crate) struct UploadDashboardArgs {
    /// A self-contained HTML dashboard bundle.
    pub file: PathBuf,
    /// Stable key within the cloud project. Omit --expected-version to create a new dashboard.
    #[arg(long)]
    pub key: String,
    /// Display name for the published version.
    #[arg(long)]
    pub name: String,
    /// Cloud project in organization/project form; otherwise BOUNDARY_PROJECT or baml.toml.
    #[arg(long)]
    pub project: Option<String>,
    /// Environment whose deployment receives a new dashboard.
    #[arg(long, default_value = "production")]
    pub environment: String,
    /// Update an existing dashboard only if it is still at this version.
    #[arg(long)]
    pub expected_version: Option<u64>,
    #[arg(skip)]
    pub from: Option<PathBuf>,
}

impl CloudCommands {
    pub fn apply_project(&mut self, source: PathBuf) {
        match self {
            Self::UploadDashboard(args) => args.from = Some(source),
        }
    }
    pub fn run(&self) -> Result<crate::ExitCode> {
        match self {
            Self::UploadDashboard(args) => args.run(),
        }
    }
}
impl UploadDashboardArgs {
    pub fn run(&self) -> Result<crate::ExitCode> {
        let root = crate::project_load::find_project_root_from(self.from.as_deref())?;
        let settings = root
            .as_deref()
            .map(crate::cloud_config::Boundary::read)
            .transpose()?
            .unwrap_or_default();
        let endpoint = settings.endpoint()?;
        let project = self.project.clone().or(baml_env::string_var("BOUNDARY_PROJECT")?).or(settings.project)
            .context("Select a cloud project with --project, BOUNDARY_PROJECT or [boundary].project in baml.toml")?;
        anyhow::ensure!(
            project
                .split_once('/')
                .is_some_and(|(org, name)| !org.is_empty()
                    && !name.is_empty()
                    && !name.contains('/')),
            "Cloud project must be organization/project"
        );
        let html = std::fs::read(&self.file)
            .with_context(|| format!("Could not read dashboard {}", self.file.display()))?;
        let stored = bcs_api::Store::new(&endpoint)?
            .read()?
            .context("Dashboard uploads require a saved login; run baml auth login")?;
        let client = bcs_api::Client::new(endpoint)?;
        let published = client.publish_dashboard(
            &stored.refresh_token,
            &bcs_api::dashboards::Publication {
                target: bcs_api::credentials::Target {
                    project: Some(project.clone()),
                    environment: Some(self.environment.clone()),
                    ..Default::default()
                },
                key: self.key.clone(),
                name: self.name.clone(),
                expected_version: self.expected_version,
            },
            html,
        )?;
        crate::reporter::Reporter::new().finish(
            "Uploaded",
            format!(
                "{project}/{} version {}",
                published.key, published.latest.version
            ),
        );
        Ok(crate::ExitCode::Success)
    }
}

#[cfg(test)]
mod tests {
    use crate::commands::{Commands, RuntimeCli};
    use clap::FromArgMatches;
    #[test]
    fn cloud_upload_parses_a_cloud_project_independently_of_the_source_project() {
        let matches = RuntimeCli::command()
            .try_get_matches_from([
                "baml",
                "--project",
                "source",
                "cloud",
                "upload-dashboard",
                "dashboard.html",
                "--key",
                "overview",
                "--name",
                "Overview",
                "--project",
                "acme/app",
                "--expected-version",
                "3",
            ])
            .unwrap();
        let cli = RuntimeCli::from_arg_matches(&matches).unwrap();
        assert_eq!(
            cli.global.project.as_deref(),
            Some(std::path::Path::new("source"))
        );
        let Commands::Cloud(super::CloudCommands::UploadDashboard(args)) = cli.command else {
            panic!("upload command")
        };
        assert_eq!(args.project.as_deref(), Some("acme/app"));
        assert_eq!(args.environment, "production");
        assert_eq!(args.expected_version, Some(3));
    }
}
