//! `baml cloud`: the Boundary Cloud resources a saved login manages (BEP-086).
//!
//! Resources are `<noun> <verb>`. `--org`, `--project` and `--env` pick what a
//! command acts on, by name or ID; their defaults come from `BOUNDARY_PROJECT`
//! and `[boundary].project` in `baml.toml`. Names become IDs through the
//! login's session, which lists every organization, project and environment
//! the user can reach.

use std::{
    cell::OnceCell,
    io::{self, Write},
};

use anyhow::{Context as _, Result, bail};
use bcs_api::cloud::organizations::{Environment, Organization, Project, Session};
use clap::Args;
use serde::Serialize;

use crate::{
    ExitCode,
    output::{Command, CommandContext, Outcome, Report, Style, run_command, table},
};

pub(crate) mod billing;
pub(crate) mod dashboards;
pub(crate) mod members;
pub(crate) mod organizations;

/// What a cloud command acts on. Defaults come from `BOUNDARY_PROJECT` and
/// `[boundary].project` in `baml.toml`.
#[derive(Debug, Args, Clone, Default)]
pub(crate) struct CloudScope {
    /// Boundary Cloud organization, by handle or ID.
    #[arg(long, global = true, value_name = "ORG")]
    pub org: Option<String>,

    /// Boundary Cloud project, by name or ID; `org_handle/project_name` also picks the organization.
    #[arg(long, global = true, value_name = "PROJECT")]
    pub project: Option<String>,

    /// Boundary Cloud environment, by name or ID.
    #[arg(long, global = true, value_name = "ENV")]
    pub env: Option<String>,
}

impl CloudScope {
    /// `--project` as `org_handle/project_name` when `--org` supplies the
    /// organization, for the routes that take the pair as one string.
    pub(crate) fn qualified_project(&self) -> Option<String> {
        match (&self.org, &self.project) {
            (Some(org), Some(project)) if !project.contains('/') => {
                Some(format!("{org}/{project}"))
            }
            (_, project) => project.clone(),
        }
    }

    /// The organization and project this scope names, falling back to
    /// `default_project` (`org_handle/project_name`) for what the flags leave
    /// out.
    fn selection(&self, default_project: Option<&str>) -> Result<Selection> {
        let split = |project: &str| match project.split_once('/') {
            Some((org, project)) => (Some(org.to_owned()), project.to_owned()),
            None => (None, project.to_owned()),
        };
        if let Some(project) = &self.project {
            let (project_org, project) = split(project);
            if let (Some(org), Some(project_org)) = (&self.org, &project_org)
                && org != project_org
            {
                bail!(
                    "--org {org} and --project {project_org}/{project} name different organizations"
                );
            }
            let org = self
                .org
                .clone()
                .or(project_org)
                .or_else(|| default_project.and_then(|default| split(default).0));
            return Ok(Selection {
                org,
                project: Some(project),
            });
        }
        let (default_org, default_project) = match default_project.map(split) {
            Some((org, project)) => (org, Some(project)),
            None => (None, None),
        };
        Ok(match &self.org {
            // The default project belongs to the default organization only.
            Some(org) if default_org.as_ref() != Some(org) => Selection {
                org: Some(org.clone()),
                project: None,
            },
            _ => Selection {
                org: self.org.clone().or(default_org),
                project: default_project,
            },
        })
    }
}

#[derive(Debug, PartialEq, Eq)]
struct Selection {
    org: Option<String>,
    project: Option<String>,
}

/// A logged-in connection to the control plane and what the command's scope
/// selects there.
pub(crate) struct Cloud<'a> {
    pub client: bcs_api::CloudClient,
    scope: &'a CloudScope,
    selection: Selection,
    session: OnceCell<Session>,
}

impl<'a> Cloud<'a> {
    fn connect(cx: &CommandContext, scope: &'a CloudScope) -> Result<Self> {
        let root = crate::project_load::find_project_root_from(cx.project_dir.as_deref())?;
        let settings = match root.as_deref() {
            Some(root) => crate::cloud_config::Boundary::read(root)?,
            None => crate::cloud_config::Boundary::default(),
        };
        let api = settings.endpoint()?;
        let services = settings.services_endpoint(&api)?;
        let login = bcs_api::store(&api)
            .read()?
            .ok_or(bcs_api::Error::MissingCredentials)?;
        let default_project = baml_env::string_var("BOUNDARY_PROJECT")?.or(settings.project);
        Ok(Self {
            client: bcs_api::CloudClient::new(services, bcs_api::Client::new(api)?, login),
            scope,
            selection: scope.selection(default_project.as_deref())?,
            session: OnceCell::new(),
        })
    }

    /// Everything the login can reach. Fetched once per command.
    pub(crate) fn session(&self) -> Result<&Session> {
        if self.session.get().is_none() {
            // A second `set` cannot happen: nothing else holds `self` here.
            let _ = self.session.set(self.client.session()?);
        }
        Ok(self.session.get().expect("the session was just set"))
    }

    pub(crate) fn organization(&self) -> Result<&Organization> {
        select_organization(self.session()?, self.selection.org.as_deref())
    }

    pub(crate) fn project(&self) -> Result<&Project> {
        select_project(self.organization()?, self.selection.project.as_deref())
    }

    /// The environment `--env` names, if it was passed.
    pub(crate) fn environment(&self) -> Result<Option<&Environment>> {
        self.scope
            .env
            .as_deref()
            .map(|name| self.environment_named(name))
            .transpose()
    }

    /// An environment of the selected project, by name or ID.
    pub(crate) fn environment_named(&self, name: &str) -> Result<&Environment> {
        let project = self.project()?;
        project
            .environments
            .iter()
            .find(|environment| environment.id == name || environment.name == name)
            .with_context(|| {
                format!(
                    "project `{}` has no environment `{name}`; it has: {}",
                    project.name,
                    names(project.environments.iter().map(|e| e.name.as_str()))
                )
            })
    }
}

fn select_organization<'s>(session: &'s Session, name: Option<&str>) -> Result<&'s Organization> {
    let organizations = &session.organizations;
    let available = || {
        names(
            organizations
                .iter()
                .map(|org| org.slug.as_deref().unwrap_or(&org.id)),
        )
    };
    match name {
        Some(name) => organizations
            .iter()
            .find(|org| org.id == name || org.slug.as_deref() == Some(name))
            .with_context(|| {
                format!(
                    "you do not belong to an organization `{name}`; yours: {}",
                    available()
                )
            }),
        None => match organizations.as_slice() {
            [organization] => Ok(organization),
            [] => bail!(
                "you do not belong to an organization; create one with `baml cloud org create`"
            ),
            _ => bail!(
                "you belong to several organizations; pick one with --org: {}",
                available()
            ),
        },
    }
}

fn select_project<'s>(organization: &'s Organization, name: Option<&str>) -> Result<&'s Project> {
    let available = || names(organization.projects.iter().map(|p| p.name.as_str()));
    match name {
        Some(name) => organization
            .projects
            .iter()
            .find(|project| project.id == name || project.name == name)
            .with_context(|| {
                format!(
                    "organization `{}` has no project `{name}`; it has: {}",
                    organization.name,
                    available()
                )
            }),
        None => match organization.projects.as_slice() {
            [project] => Ok(project),
            [] => bail!(
                "organization `{}` has no projects; create one with `baml cloud project create`",
                organization.name
            ),
            _ => bail!(
                "organization `{}` has several projects; pick one with --project: {}",
                organization.name,
                available()
            ),
        },
    }
}

fn names<'n>(names: impl Iterator<Item = &'n str>) -> String {
    let names: Vec<&str> = names.collect();
    if names.is_empty() {
        "none".to_owned()
    } else {
        names.join(", ")
    }
}

/// A cloud command: it gets the connection instead of the dispatcher's context.
pub(crate) trait CloudCommand {
    type Output: Report;
    fn run(&self, cloud: &Cloud<'_>) -> Result<Outcome<Self::Output>>;
}

struct Scoped<'a, C> {
    command: &'a C,
    scope: &'a CloudScope,
}

impl<C: CloudCommand> Command for Scoped<'_, C> {
    type Output = C::Output;

    fn run(&self, cx: &CommandContext) -> Result<Outcome<C::Output>> {
        self.command.run(&Cloud::connect(cx, self.scope)?)
    }
}

/// Runs one cloud command, wording a Boundary failure for the user.
pub(crate) fn run<C: CloudCommand>(
    command: &C,
    cx: &CommandContext,
    scope: &CloudScope,
) -> Result<ExitCode> {
    use bcs_api::diagnostics::{Context, CredentialSource, Operation, Outcome as Failure};
    run_command(&Scoped { command, scope }, cx).map_err(|error| {
        match error.downcast::<bcs_api::Error>() {
            Ok(source) => Context {
                endpoint: crate::cloud_config::login_endpoint(cx.project_dir.as_deref()).ok(),
                source: CredentialSource::SavedLogin,
                operation: Operation::Management,
            }
            .report(source, Failure::CloudCommandFailed)
            .into(),
            Err(error) => error,
        }
    })
}

/// One row of a [`Listing`].
pub(crate) trait Row: Serialize {
    const COLUMNS: &'static [&'static str];
    fn cells(&self) -> Vec<String>;
}

/// Resources of one kind: a table as text, Boundary's own JSON otherwise.
#[derive(Debug, Serialize)]
#[serde(transparent)]
pub(crate) struct Listing<T>(pub Vec<T>);

impl<T: Row> Report for Listing<T> {
    fn text(&self, out: &mut dyn Write, _style: &Style) -> io::Result<()> {
        let rows: Vec<Vec<String>> = self.0.iter().map(Row::cells).collect();
        table::write(out, T::COLUMNS, &rows)
    }
}

/// The labeled values a [`Detail`] prints.
pub(crate) trait Fields: Serialize {
    fn fields(&self) -> Vec<(&'static str, String)>;
}

/// One resource: `Label: value` lines as text, Boundary's own JSON otherwise.
#[derive(Debug, Serialize)]
#[serde(transparent)]
pub(crate) struct Detail<T>(pub T);

impl<T: Fields> Report for Detail<T> {
    fn text(&self, out: &mut dyn Write, _style: &Style) -> io::Result<()> {
        for (label, value) in self.0.fields() {
            writeln!(out, "{label}: {value}")?;
        }
        Ok(())
    }
}

/// How Boundary spells an enum value, such as `admin` for a role.
pub(crate) fn word(value: &impl Serialize) -> String {
    match serde_json::to_value(value) {
        Ok(serde_json::Value::String(word)) => word,
        Ok(other) => other.to_string(),
        Err(_) => "?".to_owned(),
    }
}

/// A time in seconds since the Unix epoch, as RFC 3339.
pub(crate) fn time(unix_seconds: u64) -> String {
    i64::try_from(unix_seconds)
        .ok()
        .and_then(|seconds| jiff::Timestamp::from_second(seconds).ok())
        .map_or_else(|| unix_seconds.to_string(), |time| time.to_string())
}

pub(crate) fn optional(value: Option<&str>) -> String {
    value.unwrap_or("-").to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scope(org: Option<&str>, project: Option<&str>) -> CloudScope {
        CloudScope {
            org: org.map(str::to_owned),
            project: project.map(str::to_owned),
            env: None,
        }
    }

    fn selected(org: Option<&str>, project: Option<&str>) -> Selection {
        Selection {
            org: org.map(str::to_owned),
            project: project.map(str::to_owned),
        }
    }

    #[test]
    fn flags_override_the_default_project_piecewise() {
        for (org, project, default, expected) in [
            (None, None, None, selected(None, None)),
            (
                None,
                None,
                Some("acme/app"),
                selected(Some("acme"), Some("app")),
            ),
            (
                Some("acme"),
                None,
                Some("acme/app"),
                selected(Some("acme"), Some("app")),
            ),
            // Another organization does not inherit the default's project.
            (
                Some("beta"),
                None,
                Some("acme/app"),
                selected(Some("beta"), None),
            ),
            (
                None,
                Some("web"),
                Some("acme/app"),
                selected(Some("acme"), Some("web")),
            ),
            (
                None,
                Some("beta/web"),
                Some("acme/app"),
                selected(Some("beta"), Some("web")),
            ),
            (
                Some("beta"),
                Some("web"),
                Some("acme/app"),
                selected(Some("beta"), Some("web")),
            ),
            (
                Some("beta"),
                Some("beta/web"),
                None,
                selected(Some("beta"), Some("web")),
            ),
        ] {
            assert_eq!(
                scope(org, project).selection(default).unwrap(),
                expected,
                "--org {org:?} --project {project:?} with default {default:?}"
            );
        }
    }

    #[test]
    fn org_and_qualified_project_must_agree() {
        let error = scope(Some("acme"), Some("beta/web"))
            .selection(None)
            .unwrap_err();
        assert_eq!(
            error.to_string(),
            "--org acme and --project beta/web name different organizations"
        );
    }

    #[test]
    fn qualified_project_joins_the_org_flag() {
        assert_eq!(
            scope(Some("acme"), Some("app"))
                .qualified_project()
                .as_deref(),
            Some("acme/app")
        );
        assert_eq!(
            scope(Some("acme"), Some("beta/app"))
                .qualified_project()
                .as_deref(),
            Some("beta/app")
        );
        assert_eq!(
            scope(None, Some("app")).qualified_project().as_deref(),
            Some("app")
        );
        assert_eq!(scope(Some("acme"), None).qualified_project(), None);
    }

    fn session() -> Session {
        serde_json::from_value(serde_json::json!({
            "user": {"id": "user_1", "email": "a@example.test"},
            "customerHosting": false,
            "organizations": [
                {"id": "org_1", "name": "Acme", "slug": "acme", "role": "admin", "projects": [
                    {"id": "proj_1", "name": "app", "environments": []},
                    {"id": "proj_2", "name": "web", "environments": []},
                ]},
                {"id": "org_2", "name": "Beta", "slug": null, "role": "member", "projects": [
                    {"id": "proj_3", "name": "only", "environments": []},
                ]},
            ],
        }))
        .unwrap()
    }

    #[test]
    fn organizations_and_projects_resolve_by_name_or_id() {
        let session = session();
        let acme = select_organization(&session, Some("acme")).unwrap();
        assert_eq!(acme.id, "org_1");
        assert_eq!(
            select_organization(&session, Some("org_2")).unwrap().name,
            "Beta"
        );
        assert_eq!(select_project(acme, Some("web")).unwrap().id, "proj_2");
        assert_eq!(select_project(acme, Some("proj_1")).unwrap().name, "app");
    }

    #[test]
    fn an_unnamed_selection_must_be_the_only_candidate() {
        let session = session();
        assert_eq!(
            select_organization(&session, None).unwrap_err().to_string(),
            "you belong to several organizations; pick one with --org: acme, org_2"
        );
        assert_eq!(
            select_organization(&session, Some("gamma"))
                .unwrap_err()
                .to_string(),
            "you do not belong to an organization `gamma`; yours: acme, org_2"
        );
        let [acme, beta] = session.organizations.as_slice() else {
            panic!("two organizations");
        };
        assert_eq!(
            select_project(acme, None).unwrap_err().to_string(),
            "organization `Acme` has several projects; pick one with --project: app, web"
        );
        assert_eq!(select_project(beta, None).unwrap().name, "only");
    }
}
