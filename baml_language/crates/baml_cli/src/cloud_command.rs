//! Manage cloud dashboards using the saved Boundary login.
use std::{
    io::{self, Write as _},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use clap::{Args, Subcommand};
use serde::Serialize;

use crate::{
    output::{Command, CommandContext, Outcome, Report, Style},
    recordings::CloudScope,
};

#[derive(Debug, Subcommand)]
pub(crate) enum DashboardCommands {
    #[command(
        about = "Create a dashboard from a self-contained HTML file",
        after_long_help = "Examples:\n  Create a dashboard:\n    baml cloud dashboard create dashboard.html --project acme/app --key overview --name Overview"
    )]
    Create(UploadArgs),
    #[command(
        about = "Publish a new dashboard version",
        after_long_help = "Examples:\n  Publish version 2:\n    baml cloud dashboard update page.html --key home --name Home --expected-version 1"
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
    /// Environment whose deployment receives the request (alias for --env).
    #[arg(long, conflicts_with = "env")]
    environment: Option<String>,
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

pub(crate) struct Dashboard<'a> {
    pub command: &'a DashboardCommands,
    pub scope: &'a CloudScope,
}

#[derive(Serialize)]
#[serde(untagged)]
pub(crate) enum DashboardReport {
    Metadata(bcs_api::dashboards::PublishedDashboard),
    Deleted {
        project: String,
        key: String,
        deleted: bool,
    },
}

impl Report for DashboardReport {
    fn text(&self, out: &mut dyn io::Write, _style: &Style) -> io::Result<()> {
        serde_json::to_writer_pretty(&mut *out, self)?;
        writeln!(out)
    }
}

impl Command for Dashboard<'_> {
    type Output = DashboardReport;

    fn run(&self, cx: &CommandContext) -> Result<Outcome<Self::Output>> {
        let result = match self.command {
            DashboardCommands::Create(args) => args.publish(cx, self.scope, None)?,
            DashboardCommands::Update(args) => {
                args.upload
                    .publish(cx, self.scope, Some(args.expected_version))?
            }
            DashboardCommands::Get(args) => {
                let (_, client, session) = args.selection.client(cx, self.scope)?;
                let result = client.get_dashboard(
                    &session,
                    &args.selection.lookup(cx, self.scope)?,
                    args.output.is_some(),
                )?;
                if let (Some(path), Some(html)) = (&args.output, result.html) {
                    save_html(path, &html)
                        .with_context(|| format!("Could not write dashboard {}", path.display()))?;
                }
                DashboardReport::Metadata(result.dashboard)
            }
            DashboardCommands::Delete(args) => {
                let (project, client, session) = args.client(cx, self.scope)?;
                client.delete_dashboard(&session, &args.lookup(cx, self.scope)?)?;
                crate::reporter::Reporter::new()
                    .finish("Deleted", format!("{project}/{}", args.key));
                DashboardReport::Deleted {
                    project,
                    key: args.key.clone(),
                    deleted: true,
                }
            }
        };
        Ok(Outcome::ok(result))
    }
}

fn save_html(path: &Path, html: &[u8]) -> io::Result<()> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        temporary
            .as_file()
            .set_permissions(std::fs::Permissions::from_mode(0o600))?;
    }
    temporary.write_all(html)?;
    // Replacing the inode keeps an old public file or hard link from exposing the new HTML.
    temporary
        .persist(path)
        .map(|_| ())
        .map_err(|error| error.error)
}

impl Selection {
    fn settings(&self, cx: &CommandContext) -> Result<crate::cloud_config::Boundary> {
        let root = crate::project_load::find_project_root_from(cx.project_dir.as_deref())?;
        Ok(root
            .as_deref()
            .map(crate::cloud_config::Boundary::read)
            .transpose()?
            .unwrap_or_default())
    }

    fn project(&self, cx: &CommandContext, scope: &CloudScope) -> Result<String> {
        let project = scope.project.clone().or(baml_env::string_var("BOUNDARY_PROJECT")?).or(self.settings(cx)?.project)
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

    fn lookup(
        &self,
        cx: &CommandContext,
        scope: &CloudScope,
    ) -> Result<bcs_api::dashboards::DashboardLookup> {
        Ok(bcs_api::dashboards::DashboardLookup {
            target: bcs_api::credentials::Target {
                project: Some(self.project(cx, scope)?),
                environment: Some(
                    self.environment
                        .clone()
                        .or_else(|| scope.env.clone())
                        .unwrap_or_else(|| "production".into()),
                ),
                ..Default::default()
            },
            key: self.key.clone(),
        })
    }

    fn client(
        &self,
        cx: &CommandContext,
        scope: &CloudScope,
    ) -> Result<(String, bcs_api::Client, bcs_api::Secret)> {
        let endpoint = self.settings(cx)?.endpoint()?;
        let project = self.project(cx, scope)?;
        let stored = bcs_api::Store::new(&endpoint)?
            .read()?
            .context("Dashboard operations require a saved login; run baml cloud auth login")?;
        Ok((
            project,
            bcs_api::Client::new(endpoint)?,
            stored.refresh_token,
        ))
    }
}

impl UploadArgs {
    fn publish(
        &self,
        cx: &CommandContext,
        scope: &CloudScope,
        expected_version: Option<u64>,
    ) -> Result<DashboardReport> {
        let (project, client, session) = self.selection.client(cx, scope)?;
        let lookup = self.selection.lookup(cx, scope)?;
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
        Ok(DashboardReport::Metadata(published))
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
                "--project-dir",
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
            cli.global.project_dir.as_deref(),
            Some(std::path::Path::new("source"))
        );
        let Commands::Cloud(cloud) = cli.command else {
            panic!("cloud command")
        };
        assert_eq!(cloud.scope.project.as_deref(), Some("acme/app"));
        let crate::commands::CloudCommands::Dashboard(super::DashboardCommands::Update(args)) =
            cloud.command
        else {
            panic!("dashboard update")
        };
        assert_eq!(args.upload.selection.environment, None);
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

    #[test]
    fn dashboard_uses_cloud_scope_and_keeps_production_as_the_default() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(
            root.path().join("baml.toml"),
            r#"[boundary]
project = "config/project"
"#,
        )
        .unwrap();
        let cx = crate::output::CommandContext::for_test(Some(root.path()));
        for (flags, expected) in [
            (vec![], "production"),
            (vec!["--env", "staging"], "staging"),
            (vec!["--environment", "staging"], "staging"),
        ] {
            let mut argv = vec![
                "baml",
                "cloud",
                "dashboard",
                "get",
                "--key",
                "overview",
                "--project",
                "acme/app",
            ];
            argv.extend(flags);
            let matches = RuntimeCli::command().try_get_matches_from(argv).unwrap();
            let cli = RuntimeCli::from_arg_matches(&matches).unwrap();
            let Commands::Cloud(cloud) = cli.command else {
                panic!("cloud command")
            };
            let crate::commands::CloudCommands::Dashboard(super::DashboardCommands::Get(args)) =
                cloud.command
            else {
                panic!("dashboard get")
            };
            let lookup = args.selection.lookup(&cx, &cloud.scope).unwrap();
            assert_eq!(lookup.target.project.as_deref(), Some("acme/app"));
            assert_eq!(lookup.target.environment.as_deref(), Some(expected));
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
                    "--env",
                    "staging",
                    "--environment",
                    "production"
                ])
                .is_err()
        );
    }

    #[test]
    fn metadata_report_preserves_json_without_an_output_wrapper() {
        let dashboard: bcs_api::dashboards::PublishedDashboard =
            serde_json::from_value(serde_json::json!({
                "id": "dashboard", "projectId": "project", "key": "overview", "latest": {
                    "id": "version", "name": "Overview", "createdBy": "user", "createdAt": 1,
                    "version": 2, "sha256": "hash", "size": 10
                }
            }))
            .unwrap();
        let report = super::DashboardReport::Metadata(dashboard);
        let text: serde_json::Value =
            serde_json::from_str(&crate::output::text_of(&report)).unwrap();
        assert_eq!(text, serde_json::to_value(&report).unwrap());
        assert_eq!(text["latest"]["version"], 2);
        assert_eq!(text["key"], "overview");
    }

    #[cfg(unix)]
    #[test]
    fn downloaded_html_replaces_public_files_with_private_files() {
        use std::os::unix::fs::PermissionsExt as _;
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("dashboard.html");
        std::fs::write(&path, b"old content").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        let previous = directory.path().join("previous.html");
        std::fs::hard_link(&path, &previous).unwrap();
        super::save_html(&path, b"private dashboard").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"private dashboard");
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(std::fs::read(&previous).unwrap(), b"old content");
        let new = directory.path().join("new.html");
        super::save_html(&new, b"new private dashboard").unwrap();
        assert_eq!(
            std::fs::metadata(new).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
}
