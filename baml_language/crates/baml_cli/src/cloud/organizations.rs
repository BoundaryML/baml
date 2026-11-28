//! `baml cloud org`, `baml cloud project` and `baml cloud env`.

use anyhow::{Result, bail};
use bcs_api::cloud::{
    hosting::{EnvironmentHosting, PlacementChoice},
    organizations::{
        Environment, NewEnvironment, NewEnvironmentKind, NewOrganization, Organization, Project,
    },
};
use clap::{Args, Subcommand, ValueEnum};

use super::{Cloud, CloudCommand, CloudScope, Detail, Fields, Listing, Row, names, optional, word};
use crate::{
    ExitCode,
    output::{CommandContext, Outcome},
};

#[derive(Subcommand, Debug)]
pub(crate) enum OrgCommands {
    /// List the organizations you belong to.
    #[command(after_long_help = "Examples:\n  List your organizations:\n    baml cloud org list")]
    List(OrgListArgs),
    /// Show one organization and its projects.
    #[command(
        after_long_help = "Examples:\n  Show the selected organization:\n    baml cloud org show\n\n  Show another one:\n    baml cloud org show --org acme"
    )]
    Show(OrgShowArgs),
    /// Create an organization.
    #[command(
        after_long_help = "Examples:\n  Create an organization:\n    baml cloud org create \"Acme Inc\" --handle acme"
    )]
    Create(OrgCreateArgs),
}

impl OrgCommands {
    pub(crate) fn run(&self, cx: &CommandContext, scope: &CloudScope) -> Result<ExitCode> {
        match self {
            Self::List(args) => super::run(args, cx, scope),
            Self::Show(args) => super::run(args, cx, scope),
            Self::Create(args) => super::run(args, cx, scope),
        }
    }
}

#[derive(Args, Debug)]
pub(crate) struct OrgListArgs {}

#[derive(Args, Debug)]
pub(crate) struct OrgShowArgs {}

#[derive(Args, Debug)]
pub(crate) struct OrgCreateArgs {
    /// The organization's display name.
    pub name: String,

    /// The handle that names the organization in `org_handle/project_name`.
    #[arg(long, value_name = "HANDLE")]
    pub handle: String,

    /// Let anyone with an email address at your domain join.
    #[arg(long)]
    pub allow_email_domain_joining: bool,
}

impl Row for Organization {
    const COLUMNS: &'static [&'static str] = &["HANDLE", "NAME", "ROLE", "PROJECTS", "ID"];

    fn cells(&self) -> Vec<String> {
        vec![
            optional(self.slug.as_deref()),
            self.name.clone(),
            word(&self.role),
            self.projects.len().to_string(),
            self.id.clone(),
        ]
    }
}

impl Fields for Organization {
    fn fields(&self) -> Vec<(&'static str, String)> {
        vec![
            ("Name", self.name.clone()),
            ("Handle", optional(self.slug.as_deref())),
            ("Role", word(&self.role)),
            ("ID", self.id.clone()),
            (
                "Projects",
                names(self.projects.iter().map(|project| project.name.as_str())),
            ),
        ]
    }
}

impl CloudCommand for OrgListArgs {
    type Output = Listing<Organization>;

    fn run(&self, cloud: &Cloud<'_>) -> Result<Outcome<Self::Output>> {
        Ok(Outcome::ok(Listing(cloud.session()?.organizations.clone())))
    }
}

impl CloudCommand for OrgShowArgs {
    type Output = Detail<Organization>;

    fn run(&self, cloud: &Cloud<'_>) -> Result<Outcome<Self::Output>> {
        Ok(Outcome::ok(Detail(cloud.organization()?.clone())))
    }
}

impl CloudCommand for OrgCreateArgs {
    type Output = Detail<Organization>;

    fn run(&self, cloud: &Cloud<'_>) -> Result<Outcome<Self::Output>> {
        let organization = cloud.client.create_organization(&NewOrganization {
            name: self.name.clone(),
            slug: self.handle.clone(),
            allow_email_domain_joining: self.allow_email_domain_joining,
        })?;
        Ok(Outcome::ok(Detail(organization)))
    }
}

#[derive(Subcommand, Debug)]
pub(crate) enum ProjectCommands {
    /// List the organization's projects.
    #[command(
        after_long_help = "Examples:\n  List the projects of the selected organization:\n    baml cloud project list\n\n  List another organization's:\n    baml cloud project list --org acme"
    )]
    List(ProjectListArgs),
    /// Show one project and its environments.
    #[command(
        after_long_help = "Examples:\n  Show the selected project:\n    baml cloud project show\n\n  Show another one:\n    baml cloud project show --project acme/app"
    )]
    Show(ProjectShowArgs),
    /// Create a project in the organization.
    #[command(
        after_long_help = "Examples:\n  Create a project:\n    baml cloud project create app --org acme"
    )]
    Create(ProjectCreateArgs),
}

impl ProjectCommands {
    pub(crate) fn run(&self, cx: &CommandContext, scope: &CloudScope) -> Result<ExitCode> {
        match self {
            Self::List(args) => super::run(args, cx, scope),
            Self::Show(args) => super::run(args, cx, scope),
            Self::Create(args) => super::run(args, cx, scope),
        }
    }
}

#[derive(Args, Debug)]
pub(crate) struct ProjectListArgs {}

#[derive(Args, Debug)]
pub(crate) struct ProjectShowArgs {}

#[derive(Args, Debug)]
pub(crate) struct ProjectCreateArgs {
    /// The project's name, the second half of `org_handle/project_name`.
    pub name: String,
}

impl Row for Project {
    const COLUMNS: &'static [&'static str] = &["NAME", "ENVIRONMENTS", "ID"];

    fn cells(&self) -> Vec<String> {
        vec![
            self.name.clone(),
            names(self.environments.iter().map(|e| e.name.as_str())),
            self.id.clone(),
        ]
    }
}

impl Fields for Project {
    fn fields(&self) -> Vec<(&'static str, String)> {
        vec![
            ("Name", self.name.clone()),
            ("ID", self.id.clone()),
            (
                "Environments",
                names(self.environments.iter().map(|e| e.name.as_str())),
            ),
        ]
    }
}

impl CloudCommand for ProjectListArgs {
    type Output = Listing<Project>;

    fn run(&self, cloud: &Cloud<'_>) -> Result<Outcome<Self::Output>> {
        Ok(Outcome::ok(Listing(cloud.organization()?.projects.clone())))
    }
}

impl CloudCommand for ProjectShowArgs {
    type Output = Detail<Project>;

    fn run(&self, cloud: &Cloud<'_>) -> Result<Outcome<Self::Output>> {
        Ok(Outcome::ok(Detail(cloud.project()?.clone())))
    }
}

impl CloudCommand for ProjectCreateArgs {
    type Output = Detail<Project>;

    fn run(&self, cloud: &Cloud<'_>) -> Result<Outcome<Self::Output>> {
        let project = cloud
            .client
            .create_project(&cloud.organization()?.id, &self.name)?;
        Ok(Outcome::ok(Detail(project)))
    }
}

#[derive(Subcommand, Debug)]
pub(crate) enum EnvCommands {
    /// List the project's environments.
    #[command(
        after_long_help = "Examples:\n  List the environments of the selected project:\n    baml cloud env list\n\n  List another project's:\n    baml cloud env list --project acme/app"
    )]
    List(EnvListArgs),
    /// Show one environment.
    #[command(
        after_long_help = "Examples:\n  Show an environment:\n    baml cloud env show --env staging\n\n  Show your personal environment, creating it if you have none:\n    baml cloud env show --personal"
    )]
    Show(EnvShowArgs),
    /// Create an environment in the project.
    #[command(
        after_long_help = "Examples:\n  Create a custom environment:\n    baml cloud env create preview\n\n  Create the staging environment:\n    baml cloud env create staging --kind staging"
    )]
    Create(EnvCreateArgs),
    /// Show where an environment's data is hosted.
    #[command(
        after_long_help = "Examples:\n  Show where production is hosted:\n    baml cloud env hosting production"
    )]
    Hosting(EnvHostingArgs),
}

impl EnvCommands {
    pub(crate) fn run(&self, cx: &CommandContext, scope: &CloudScope) -> Result<ExitCode> {
        match self {
            Self::List(args) => super::run(args, cx, scope),
            Self::Show(args) => super::run(args, cx, scope),
            Self::Create(args) => super::run(args, cx, scope),
            Self::Hosting(args) => super::run(args, cx, scope),
        }
    }
}

#[derive(Args, Debug)]
pub(crate) struct EnvListArgs {}

#[derive(Args, Debug)]
pub(crate) struct EnvShowArgs {
    /// Show your personal environment in the project, creating it if you have none.
    #[arg(long, conflicts_with = "env")]
    pub personal: bool,
}

#[derive(ValueEnum, Clone, Copy, Debug, Default)]
pub(crate) enum EnvKind {
    Production,
    Staging,
    #[default]
    Custom,
}

#[derive(Args, Debug)]
pub(crate) struct EnvCreateArgs {
    /// The environment's name.
    pub name: String,

    /// What the environment is for.
    #[arg(long, value_enum, default_value_t)]
    pub kind: EnvKind,
}

#[derive(Args, Debug)]
pub(crate) struct EnvHostingArgs {
    /// The environment, by name or ID.
    #[arg(value_name = "ENV")]
    pub environment: String,
}

impl Row for Environment {
    const COLUMNS: &'static [&'static str] = &["NAME", "KIND", "READINESS", "ID"];

    fn cells(&self) -> Vec<String> {
        vec![
            self.name.clone(),
            word(&self.kind),
            word(&self.readiness),
            self.id.clone(),
        ]
    }
}

impl Fields for Environment {
    fn fields(&self) -> Vec<(&'static str, String)> {
        let mut fields = vec![
            ("Name", self.name.clone()),
            ("Kind", word(&self.kind)),
            ("Readiness", word(&self.readiness)),
            ("ID", self.id.clone()),
        ];
        if let Some(owner) = &self.personal_owner {
            fields.push(("Owner", owner.name.clone().unwrap_or(owner.id.clone())));
        }
        fields
    }
}

impl Fields for EnvironmentHosting {
    fn fields(&self) -> Vec<(&'static str, String)> {
        let mut fields = vec![
            ("Name", self.name.clone()),
            ("Hosting", word(&self.hosting)),
            ("Readiness", word(&self.readiness)),
            ("ID", self.environment_id.clone()),
        ];
        if let Some(plane) = &self.data_plane {
            fields.push(("Data plane", format!("{} ({})", plane.name, plane.id)));
            if let Some(url) = &plane.url {
                fields.push(("Data plane URL", url.clone()));
            }
        }
        fields
    }
}

impl CloudCommand for EnvListArgs {
    type Output = Listing<Environment>;

    fn run(&self, cloud: &Cloud<'_>) -> Result<Outcome<Self::Output>> {
        Ok(Outcome::ok(Listing(cloud.project()?.environments.clone())))
    }
}

impl CloudCommand for EnvShowArgs {
    type Output = Detail<Environment>;

    fn run(&self, cloud: &Cloud<'_>) -> Result<Outcome<Self::Output>> {
        if self.personal {
            let environment = cloud.client.personal_environment(&cloud.project()?.id)?;
            return Ok(Outcome::ok(Detail(environment)));
        }
        match cloud.environment()? {
            Some(environment) => Ok(Outcome::ok(Detail(environment.clone()))),
            None => bail!("pick an environment with --env, or pass --personal for your own"),
        }
    }
}

impl CloudCommand for EnvCreateArgs {
    type Output = Detail<Environment>;

    fn run(&self, cloud: &Cloud<'_>) -> Result<Outcome<Self::Output>> {
        let environment = cloud.client.create_environment(
            &cloud.project()?.id,
            &NewEnvironment {
                name: self.name.clone(),
                kind: match self.kind {
                    EnvKind::Production => NewEnvironmentKind::Production,
                    EnvKind::Staging => NewEnvironmentKind::Staging,
                    EnvKind::Custom => NewEnvironmentKind::Custom,
                },
                placement: PlacementChoice::Default,
            },
        )?;
        Ok(Outcome::ok(Detail(environment)))
    }
}

impl CloudCommand for EnvHostingArgs {
    type Output = Detail<EnvironmentHosting>;

    fn run(&self, cloud: &Cloud<'_>) -> Result<Outcome<Self::Output>> {
        let environment = cloud.environment_named(&self.environment)?;
        Ok(Outcome::ok(Detail(
            cloud.client.environment_hosting(&environment.id)?,
        )))
    }
}
