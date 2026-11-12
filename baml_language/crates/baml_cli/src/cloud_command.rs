//! Manage cloud dashboards using the saved Boundary login.
use std::{
    io::{self, Write as _},
    path::PathBuf,
};

use anyhow::{Context, Result};
use clap::{Args, Subcommand};

#[derive(Debug, Subcommand)]
pub(crate) enum CloudCommands {
    #[command(
        subcommand,
        about = "Create, update, get or delete dashboards",
        after_long_help = "Examples:\n  Create a dashboard:\n    baml cloud dashboard create dashboard.html --project acme/app --key overview --name Overview\n\n  Download a dashboard:\n    baml cloud dashboard get --project acme/app --key overview --output dashboard.html"
    )]
    Dashboard(DashboardCommands),
}

#[derive(Debug, Subcommand)]
pub(crate) enum DashboardCommands {
    #[command(
        about = "Create a dashboard from a self-contained HTML file",
        after_long_help = "Examples:\n  Create a dashboard:\n    baml cloud dashboard create dashboard.html --project acme/app --key overview --name Overview"
    )]
    Create(UploadArgs),
    #[command(
        about = "Publish a new dashboard version",
        after_long_help = "Examples:\n  Publish version 2:\n    baml cloud dashboard update dashboard.html --project acme/app --key overview --name Overview --expected-version 1"
    )]
    Update(UpdateArgs),
    #[command(
        about = "Get dashboard metadata, optionally saving its HTML",
        after_long_help = "Examples:\n  Print dashboard metadata:\n    baml cloud dashboard get --project acme/app --key overview\n\n  Save the HTML bundle:\n    baml cloud dashboard get --project acme/app --key overview --output dashboard.html"
    )]
    Get(GetArgs),
    #[command(
        about = "Delete a dashboard",
        after_long_help = "Examples:\n  Delete a dashboard:\n    baml cloud dashboard delete --project acme/app --key overview"
    )]
    Delete(Selection),
}

#[derive(Debug, Args)]
pub(crate) struct Selection {
    /// Stable dashboard key within the cloud project.
    #[arg(long)]
    key: String,
    /// Cloud project in organization/project form; otherwise BOUNDARY_PROJECT or baml.toml.
    #[arg(long)]
    project: Option<String>,
    /// Environment whose deployment receives the request.
    #[arg(long, default_value = "production")]
    environment: String,
    #[arg(skip)]
    from: Option<PathBuf>,
}

#[derive(Debug, Args)]
pub(crate) struct UploadArgs {
    /// A self-contained HTML dashboard bundle.
    file: PathBuf,
    /// Display name for the published version.
    #[arg(long)]
    name: String,
    #[command(flatten)]
    selection: Selection,
}

#[derive(Debug, Args)]
pub(crate) struct UpdateArgs {
    #[command(flatten)]
    upload: UploadArgs,
    /// Update only if the dashboard is still at this version.
    #[arg(long)]
    expected_version: u64,
}

#[derive(Debug, Args)]
pub(crate) struct GetArgs {
    #[command(flatten)]
    selection: Selection,
    /// Save the verified HTML bundle to this file; metadata is printed as JSON.
    #[arg(long)]
    output: Option<PathBuf>,
}

impl CloudCommands {
    pub fn apply_project(&mut self, source: PathBuf) {
        let Self::Dashboard(command) = self;
        command.selection_mut().from = Some(source);
    }

    pub fn run(&self) -> Result<crate::ExitCode> {
        let Self::Dashboard(command) = self;
        match command {
            DashboardCommands::Create(args) => args.publish(None)?,
            DashboardCommands::Update(args) => args.upload.publish(Some(args.expected_version))?,
            DashboardCommands::Get(args) => {
                let (_, client, session) = args.selection.client()?;
                let result = client.get_dashboard(
                    &session,
                    &args.selection.lookup()?,
                    args.output.is_some(),
                )?;
                if let (Some(path), Some(html)) = (&args.output, result.html) {
                    std::fs::write(path, html)
                        .with_context(|| format!("Could not write dashboard {}", path.display()))?;
                }
                let mut output = io::stdout().lock();
                serde_json::to_writer_pretty(&mut output, &result.dashboard)?;
                writeln!(output)?;
            }
            DashboardCommands::Delete(args) => {
                let (project, client, session) = args.client()?;
                client.delete_dashboard(&session, &args.lookup()?)?;
                crate::reporter::Reporter::new()
                    .finish("Deleted", format!("{project}/{}", args.key));
            }
        }
        Ok(crate::ExitCode::Success)
    }
}

impl DashboardCommands {
    fn selection_mut(&mut self) -> &mut Selection {
        match self {
            Self::Create(args) => &mut args.selection,
            Self::Update(args) => &mut args.upload.selection,
            Self::Get(args) => &mut args.selection,
            Self::Delete(args) => args,
        }
    }
}

impl Selection {
    fn settings(&self) -> Result<crate::cloud_config::Boundary> {
        let root = crate::project_load::find_project_root_from(self.from.as_deref())?;
        Ok(root
            .as_deref()
            .map(crate::cloud_config::Boundary::read)
            .transpose()?
            .unwrap_or_default())
    }

    fn project(&self) -> Result<String> {
        let project = self.project.clone().or(baml_env::string_var("BOUNDARY_PROJECT")?).or(self.settings()?.project)
            .context("Select a cloud project with --project, BOUNDARY_PROJECT or [boundary].project in baml.toml")?;
        anyhow::ensure!(
            project
                .split_once('/')
                .is_some_and(|(org, name)| !org.is_empty()
                    && !name.is_empty()
                    && !name.contains('/')),
            "Cloud project must be organization/project"
        );
        Ok(project)
    }

    fn lookup(&self) -> Result<bcs_api::dashboards::DashboardLookup> {
        Ok(bcs_api::dashboards::DashboardLookup {
            target: bcs_api::credentials::Target {
                project: Some(self.project()?),
                environment: Some(self.environment.clone()),
                ..Default::default()
            },
            key: self.key.clone(),
        })
    }

    fn client(&self) -> Result<(String, bcs_api::Client, bcs_api::Secret)> {
        let endpoint = self.settings()?.endpoint()?;
        let project = self.project()?;
        let stored = bcs_api::Store::new(&endpoint)?
            .read()?
            .context("Dashboard operations require a saved login; run baml auth login")?;
        Ok((
            project,
            bcs_api::Client::new(endpoint)?,
            stored.refresh_token,
        ))
    }
}

impl UploadArgs {
    fn publish(&self, expected_version: Option<u64>) -> Result<()> {
        let (project, client, session) = self.selection.client()?;
        let lookup = self.selection.lookup()?;
        let html = std::fs::read(&self.file)
            .with_context(|| format!("Could not read dashboard {}", self.file.display()))?;
        let published = client.publish_dashboard(
            &session,
            &bcs_api::dashboards::Publication {
                target: lookup.target,
                key: lookup.key,
                name: self.name.clone(),
                expected_version,
            },
            html,
        )?;
        crate::reporter::Reporter::new().finish(
            if expected_version.is_some() {
                "Updated"
            } else {
                "Created"
            },
            format!(
                "{project}/{} version {}",
                published.key, published.latest.version
            ),
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use clap::FromArgMatches;

    use crate::commands::{Commands, RuntimeCli};

    #[test]
    fn dashboard_update_separates_cloud_project_from_source_and_requires_version() {
        let matches = RuntimeCli::command()
            .try_get_matches_from([
                "baml",
                "--project",
                "source",
                "cloud",
                "dashboard",
                "update",
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
        let Commands::Cloud(super::CloudCommands::Dashboard(super::DashboardCommands::Update(
            args,
        ))) = cli.command
        else {
            panic!("dashboard update")
        };
        assert_eq!(args.upload.selection.project.as_deref(), Some("acme/app"));
        assert_eq!(args.upload.selection.environment, "production");
        assert_eq!(args.expected_version, 3);
        assert!(
            RuntimeCli::command()
                .try_get_matches_from([
                    "baml",
                    "cloud",
                    "dashboard",
                    "update",
                    "dashboard.html",
                    "--key",
                    "overview",
                    "--name",
                    "Overview",
                ])
                .is_err()
        );
    }

    #[test]
    fn dashboard_commands_have_distinct_inputs_and_the_old_command_is_removed() {
        for action in ["get", "delete"] {
            assert!(
                RuntimeCli::command()
                    .try_get_matches_from([
                        "baml",
                        "cloud",
                        "dashboard",
                        action,
                        "--key",
                        "overview"
                    ])
                    .is_ok()
            );
        }
        assert!(
            RuntimeCli::command()
                .try_get_matches_from([
                    "baml",
                    "cloud",
                    "dashboard",
                    "get",
                    "--key",
                    "overview",
                    "--output",
                    "dashboard.html",
                ])
                .is_ok()
        );
        assert!(
            RuntimeCli::command()
                .try_get_matches_from([
                    "baml",
                    "cloud",
                    "dashboard",
                    "create",
                    "dashboard.html",
                    "--key",
                    "overview",
                    "--name",
                    "Overview",
                    "--expected-version",
                    "1",
                ])
                .is_err()
        );
        assert!(
            RuntimeCli::command()
                .try_get_matches_from(["baml", "cloud", "upload-dashboard"])
                .is_err()
        );
    }
}
