//! `baml cloud dashboards`: a project's dashboards. A dashboard is one HTML
//! file; every save is a new version.

use std::path::PathBuf;

use anyhow::{Context as _, Result};
use bcs_api::cloud::{
    Page,
    dashboards::{Dashboard, DashboardVersion, NewDashboard},
};
use clap::{Args, Subcommand};

use super::{Cloud, CloudCommand, CloudScope, Detail, Fields, Listing, Row, time};
use crate::{
    ExitCode,
    output::{CommandContext, Outcome},
    reporter::Reporter,
};

#[derive(Subcommand, Debug)]
pub(crate) enum DashboardsCommands {
    /// List the project's dashboards.
    #[command(
        after_long_help = "Examples:\n  List the dashboards of the selected project:\n    baml cloud dashboards list\n\n  List another project's:\n    baml cloud dashboards list --project acme/app"
    )]
    List(DashboardsListArgs),
    /// Show one dashboard.
    #[command(
        after_long_help = "Examples:\n  Show a dashboard by the ID `baml cloud dashboards list` shows:\n    baml cloud dashboards show dsh_123"
    )]
    Show(DashboardsShowArgs),
    /// Create a dashboard from an HTML file.
    #[command(
        after_long_help = "Examples:\n  Create a dashboard that queries your personal environment:\n    baml cloud dashboards create latency.html --key latency --name \"Latency\"\n\n  Create one that queries production:\n    baml cloud dashboards create latency.html --key latency --env production"
    )]
    Create(DashboardsCreateArgs),
    /// Rename a dashboard or save a new version of it.
    #[command(
        after_long_help = "Examples:\n  Rename a dashboard:\n    baml cloud dashboards update dsh_123 --name \"Latency by model\"\n\n  Save a new version:\n    baml cloud dashboards update dsh_123 --file latency.html"
    )]
    Update(DashboardsUpdateArgs),
    /// Delete a dashboard and its versions.
    #[command(
        after_long_help = "Examples:\n  Delete a dashboard:\n    baml cloud dashboards delete dsh_123"
    )]
    Delete(DashboardsDeleteArgs),
    /// List a dashboard's versions, newest first.
    #[command(
        after_long_help = "Examples:\n  List a dashboard's versions:\n    baml cloud dashboards history dsh_123"
    )]
    History(DashboardsHistoryArgs),
}

impl DashboardsCommands {
    pub(crate) fn run(&self, cx: &CommandContext, scope: &CloudScope) -> Result<ExitCode> {
        match self {
            Self::List(args) => super::run(args, cx, scope),
            Self::Show(args) => super::run(args, cx, scope),
            Self::Create(args) => super::run(args, cx, scope),
            Self::Update(args) => super::run(args, cx, scope),
            Self::Delete(args) => super::run(args, cx, scope),
            Self::History(args) => super::run(args, cx, scope),
        }
    }
}

#[derive(Args, Debug)]
pub(crate) struct DashboardsListArgs {}

#[derive(Args, Debug)]
pub(crate) struct DashboardsShowArgs {
    /// The dashboard's ID.
    pub id: String,
}

#[derive(Args, Debug)]
pub(crate) struct DashboardsCreateArgs {
    /// The dashboard's HTML file.
    pub file: PathBuf,

    /// The dashboard's key, unique in the project.
    #[arg(long, value_name = "KEY")]
    pub key: String,

    /// The dashboard's display name; defaults to the key.
    #[arg(long, value_name = "NAME")]
    pub name: Option<String>,
}

#[derive(Args, Debug)]
pub(crate) struct DashboardsUpdateArgs {
    /// The dashboard's ID.
    pub id: String,

    /// The dashboard's new display name.
    #[arg(long, value_name = "NAME", required_unless_present = "file")]
    pub name: Option<String>,

    /// An HTML file to save as the dashboard's next version.
    #[arg(long, value_name = "PATH")]
    pub file: Option<PathBuf>,
}

#[derive(Args, Debug)]
pub(crate) struct DashboardsDeleteArgs {
    /// The dashboard's ID.
    pub id: String,
}

#[derive(Args, Debug)]
pub(crate) struct DashboardsHistoryArgs {
    /// The dashboard's ID.
    pub id: String,
}

impl Row for Dashboard {
    const COLUMNS: &'static [&'static str] = &["KEY", "NAME", "VERSION", "SAVED", "ID"];

    fn cells(&self) -> Vec<String> {
        vec![
            self.key.clone(),
            self.latest.name.clone(),
            self.latest.version.to_string(),
            time(self.latest.created_at),
            self.id.clone(),
        ]
    }
}

impl Fields for Dashboard {
    fn fields(&self) -> Vec<(&'static str, String)> {
        vec![
            ("Name", self.latest.name.clone()),
            ("Key", self.key.clone()),
            ("ID", self.id.clone()),
            ("Version", self.latest.version.to_string()),
            ("Size", format!("{} bytes", self.latest.size)),
            ("Saved", time(self.latest.created_at)),
            ("Saved by", self.latest.created_by.clone()),
            ("SHA-256", self.latest.sha256.clone()),
        ]
    }
}

impl Row for DashboardVersion {
    const COLUMNS: &'static [&'static str] =
        &["VERSION", "NAME", "BYTES", "SAVED", "SAVED BY", "ID"];

    fn cells(&self) -> Vec<String> {
        vec![
            self.version.to_string(),
            self.name.clone(),
            self.size.to_string(),
            time(self.created_at),
            self.created_by.clone(),
            self.id.clone(),
        ]
    }
}

fn read_html(path: &std::path::Path) -> Result<Vec<u8>> {
    std::fs::read(path).with_context(|| format!("could not read {}", path.display()))
}

/// The dashboard as it is now. An update names the version it replaces.
fn current(cloud: &Cloud<'_>, id: &str) -> Result<Dashboard> {
    let page = Page {
        cursor: None,
        limit: Some(1),
    };
    Ok(cloud.client.dashboard_history(id, &page)?.dashboard)
}

impl CloudCommand for DashboardsListArgs {
    type Output = Listing<Dashboard>;

    fn run(&self, cloud: &Cloud<'_>) -> Result<Outcome<Self::Output>> {
        let project = &cloud.project()?.id;
        let mut dashboards = Vec::new();
        let mut page = Page::default();
        loop {
            let list = cloud.client.list_dashboards(project, &page)?;
            dashboards.extend(list.dashboards);
            match list.next_cursor {
                Some(cursor) => page.cursor = Some(cursor),
                None => return Ok(Outcome::ok(Listing(dashboards))),
            }
        }
    }
}

impl CloudCommand for DashboardsShowArgs {
    type Output = Detail<Dashboard>;

    fn run(&self, cloud: &Cloud<'_>) -> Result<Outcome<Self::Output>> {
        Ok(Outcome::ok(Detail(current(cloud, &self.id)?)))
    }
}

impl CloudCommand for DashboardsCreateArgs {
    type Output = Detail<Dashboard>;

    fn run(&self, cloud: &Cloud<'_>) -> Result<Outcome<Self::Output>> {
        let html = read_html(&self.file)?;
        let project = &cloud.project()?.id;
        let environment = match cloud.environment()? {
            Some(environment) => environment.id.clone(),
            None => cloud.client.personal_environment(project)?.id,
        };
        let dashboard = cloud.client.create_dashboard(
            project,
            &NewDashboard {
                key: &self.key,
                name: self.name.as_deref().unwrap_or(&self.key),
                environment: &environment,
            },
            &html,
        )?;
        Ok(Outcome::ok(Detail(dashboard)))
    }
}

impl CloudCommand for DashboardsUpdateArgs {
    type Output = Detail<Dashboard>;

    fn run(&self, cloud: &Cloud<'_>) -> Result<Outcome<Self::Output>> {
        let html = self.file.as_deref().map(read_html).transpose()?;
        let latest = current(cloud, &self.id)?.latest;
        let name = self.name.as_deref().unwrap_or(&latest.name);
        let dashboard = match html {
            Some(html) => cloud
                .client
                .save_dashboard(&self.id, name, latest.version, &html)?,
            None => cloud
                .client
                .rename_dashboard(&self.id, name, latest.version)?,
        };
        Ok(Outcome::ok(Detail(dashboard)))
    }
}

impl CloudCommand for DashboardsDeleteArgs {
    type Output = ();

    fn run(&self, cloud: &Cloud<'_>) -> Result<Outcome<()>> {
        cloud.client.delete_dashboard(&self.id)?;
        Reporter::new().status("Deleted", format!("dashboard {}", self.id));
        Ok(Outcome::ok(()))
    }
}

impl CloudCommand for DashboardsHistoryArgs {
    type Output = Listing<DashboardVersion>;

    fn run(&self, cloud: &Cloud<'_>) -> Result<Outcome<Self::Output>> {
        let mut versions = Vec::new();
        let mut page = Page::default();
        loop {
            let history = cloud.client.dashboard_history(&self.id, &page)?;
            versions.extend(history.versions);
            match history.next_cursor {
                Some(cursor) => page.cursor = Some(cursor),
                None => return Ok(Outcome::ok(Listing(versions))),
            }
        }
    }
}
