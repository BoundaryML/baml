//! `baml cloud alerts`: SQL that runs on a schedule in a Boundary environment and posts to Slack
//! or a webhook when something is wrong. Alerts are kept by Boundary's control API; this command
//! signs in with the saved `baml cloud auth login` session, and takes the project and environment
//! from `baml cloud`'s `--project` and `--env`, as `baml cloud recordings` does.

use std::{
    collections::HashMap,
    io::{self, Read as _},
    path::PathBuf,
};

use anyhow::{Context, Result, bail};
use bcs_api::alerts::{
    Alert, Client, Condition, Definition, Destination, Notify, Run, RunPage, Schedule,
};
use clap::{Args, Parser, Subcommand};
use serde::Serialize;

use crate::{
    output::{Command, CommandContext, Outcome, Report, Style},
    recordings::CloudScope,
};

const DAYS: [&str; 7] = ["mon", "tue", "wed", "thu", "fri", "sat", "sun"];

/// Manage alerts: SQL that runs on a schedule and posts to Slack or a webhook when it returns rows.
#[derive(Debug, Parser)]
#[command(after_long_help = "Examples:
  List an environment's alerts:
    baml cloud --project acme/app --env production alerts list

  Post to #ai-pipeline when errors.sql returns rows, checking every 15 minutes:
    baml cloud alerts create --name errors --every 15m --to ai-pipeline --sql-file errors.sql

  Check the SQL first with `baml cloud recordings query`.")]
pub struct AlertsArgs {
    #[command(subcommand)]
    pub action: Action,
}

#[derive(Debug, Subcommand)]
pub enum Action {
    /// List the environment's alerts.
    #[command(
        after_long_help = "Examples:\n  Every alert, and whether it is firing:\n    baml cloud alerts list\n\n  As JSON:\n    baml --output-format json cloud alerts list"
    )]
    List,
    /// Show one alert, with its SQL.
    #[command(
        after_long_help = "Examples:\n  By name:\n    baml cloud alerts show errors\n\n  As JSON:\n    baml --output-format json cloud alerts show errors"
    )]
    Show {
        /// The alert's name or ID.
        alert: String,
    },
    /// Create an alert.
    #[command(
        after_long_help = "Examples:\n  Every 15 minutes, when the query returns rows:\n    baml cloud alerts create --name errors --every 15m --to ai-pipeline --sql-file errors.sql\n\n  Weekdays at 09:00 UTC:\n    baml cloud alerts create --name daily --at 09:00 --days mon-fri --to ops --sql-file d.sql\n\n  When a number in the first row is over 5:\n    baml cloud alerts create --name slow --every 1h --to ops --when \"p95 > 5\" --sql-file p.sql"
    )]
    Create(CreateArgs),
    /// Save a new version of an alert; what is not given stays as it is.
    #[command(
        after_long_help = "Examples:\n  Check every 5 minutes instead:\n    baml cloud alerts update errors --every 5m\n\n  Remind every hour while it stays firing:\n    baml cloud alerts update errors --notify changes --remind 1h"
    )]
    Update(UpdateArgs),
    /// List the runs that posted, failed or could not deliver, newest first.
    #[command(
        after_long_help = "Examples:\n  The latest runs:\n    baml cloud alerts runs\n\n  The 20 runs before run 120:\n    baml cloud alerts runs --limit 20 --before 120"
    )]
    Runs {
        /// Runs to list.
        #[arg(long, default_value_t = 10)]
        limit: u32,
        /// Only runs logged before this run ID (a listing's last ID).
        #[arg(long)]
        before: Option<u64>,
    },
    /// Stop an alert's runs for a while, such as `24h`.
    #[command(after_long_help = "Examples:\n  For a day:\n    baml cloud alerts mute errors 24h")]
    Mute {
        /// The alert's name or ID.
        alert: String,
        /// How long, such as `30m`, `24h` or `7d`.
        duration: String,
    },
    /// End an alert's mute.
    #[command(after_long_help = "Examples:\n  Now:\n    baml cloud alerts unmute errors")]
    Unmute {
        /// The alert's name or ID.
        alert: String,
    },
    /// Delete an alert: it stops running.
    #[command(after_long_help = "Examples:\n  By name:\n    baml cloud alerts delete errors")]
    Delete {
        /// The alert's name or ID.
        alert: String,
    },
    /// List where the environment's alerts can post. Add destinations in Studio.
    #[command(
        after_long_help = "Examples:\n  Every destination:\n    baml cloud alerts destinations"
    )]
    Destinations,
}

/// The SQL and schedule options of a save.
#[derive(Debug, Args)]
pub struct Spec {
    /// The SQL: one read-only SELECT that returns rows only when something is wrong, with its own
    /// time window. `-` reads stdin.
    #[arg(long, conflicts_with = "sql_file")]
    pub sql: Option<String>,
    /// A file holding the SQL.
    #[arg(long, value_name = "PATH")]
    pub sql_file: Option<PathBuf>,
    /// Run every interval, counted from the save, such as `15m`, `1h` or `1d`.
    #[arg(long, conflicts_with = "at")]
    pub every: Option<String>,
    /// Run at a local time, `HH:MM`, 24-hour.
    #[arg(long)]
    pub at: Option<String>,
    /// The time zone of `--at`, from the IANA database, such as `Europe/Madrid`.
    #[arg(long, requires = "at")]
    pub time_zone: Option<String>,
    /// The days of `--at`: `daily`, `weekdays`, a range such as `mon-fri`, or a list such as
    /// `mon,wed,fri`.
    #[arg(long, requires = "at")]
    pub days: Option<String>,
    /// Where to post: a destination's name or ID; repeat for more.
    #[arg(long = "to", value_name = "DESTINATION")]
    pub to: Vec<String>,
    /// Result rows each message carries, 0 to 20.
    #[arg(long)]
    pub rows: Option<u32>,
    /// Run once as soon as it is saved, and count the schedule from then.
    #[arg(long)]
    pub run_now: bool,
    /// When a check is bad: `rows` (the query returns any, the default), or a number in the first
    /// row against a threshold, such as `"errors > 5"` or `"> 0.2"` (the first column).
    #[arg(long, value_name = "CONDITION")]
    pub when: Option<String>,
    /// Bad checks in a row before it fires, 1 to 60 (default 1).
    #[arg(long, value_name = "N")]
    pub after: Option<u32>,
    /// When it posts: `changes` (when it fires and when it resolves, the default) or
    /// `every-check` (on every bad check).
    #[arg(long, value_name = "WHEN")]
    pub notify: Option<String>,
    /// With `--notify changes`, remind while it stays firing, such as `1h`; `never` stops it.
    #[arg(long, value_name = "INTERVAL")]
    pub remind: Option<String>,
}

#[derive(Debug, Args)]
pub struct CreateArgs {
    /// The alert's name.
    #[arg(long)]
    pub name: String,
    #[command(flatten)]
    pub spec: Spec,
}

#[derive(Debug, Args)]
pub struct UpdateArgs {
    /// The alert's name or ID.
    pub alert: String,
    /// A new name.
    #[arg(long)]
    pub name: Option<String>,
    #[command(flatten)]
    pub spec: Spec,
}

impl AlertsArgs {
    /// Runs the action in the environment `scope` names: `baml cloud`'s `--project` and `--env`.
    fn report(&self, scope: &CloudScope) -> Result<AlertsReport> {
        let context = AlertContext::open(scope)?;
        Ok(match &self.action {
            Action::List => AlertsReport::Alerts(Listing {
                environment: context.environment_name.clone(),
                alerts: context.client.list(&context.environment)?,
            }),
            Action::Show { alert } => {
                let alert = context.alert(alert)?;
                let definition = context.client.definition(&alert.id)?;
                AlertsReport::Shown(Shown {
                    destination_names: context.destination_names()?,
                    alert,
                    definition,
                })
            }
            Action::Create(args) => {
                let schedule = args
                    .spec
                    .schedule()?
                    .context("give a schedule: --every 15m, or --at 09:00 --time-zone ...")?;
                let sql = args
                    .spec
                    .sql()?
                    .context("give the SQL: --sql, or --sql-file")?;
                anyhow::ensure!(
                    !args.spec.to.is_empty(),
                    "give where to post: --to <destination>; `baml cloud alerts destinations` lists them"
                );
                let definition = Definition {
                    name: args.name.clone(),
                    sql,
                    destinations: context.destinations(&args.spec.to)?,
                    message_rows: args.spec.rows.unwrap_or(5),
                    condition: args.spec.condition()?.unwrap_or_default(),
                    after: args.spec.after.unwrap_or(1),
                    notify: args.spec.notify(Notify::Changes {
                        remind_seconds: None,
                    })?,
                };
                AlertsReport::Saved(Saved {
                    alert: context.client.create(
                        &context.environment,
                        &definition,
                        &schedule,
                        args.spec.run_now,
                    )?,
                })
            }
            Action::Update(args) => {
                let alert = context.alert(&args.alert)?;
                let current = context.client.definition(&alert.id)?;
                let definition = Definition {
                    name: args.name.clone().unwrap_or(current.name),
                    sql: args.spec.sql()?.unwrap_or(current.sql),
                    destinations: if args.spec.to.is_empty() {
                        current.destinations
                    } else {
                        context.destinations(&args.spec.to)?
                    },
                    message_rows: args.spec.rows.unwrap_or(current.message_rows),
                    condition: args.spec.condition()?.unwrap_or(current.condition),
                    after: args.spec.after.unwrap_or(current.after),
                    notify: args.spec.notify(current.notify)?,
                };
                let schedule = args.spec.schedule()?.unwrap_or(alert.schedule.clone());
                AlertsReport::Saved(Saved {
                    alert: context.client.update(
                        &alert.id,
                        alert.version,
                        &definition,
                        &schedule,
                        args.spec.run_now,
                    )?,
                })
            }
            Action::Runs { limit, before } => {
                AlertsReport::Runs(context.client.runs(&context.environment, *limit, *before)?)
            }
            Action::Mute { alert, duration } => {
                let alert = context.alert(alert)?;
                let until = now() + u64::from(parse_interval(duration)?);
                AlertsReport::Saved(Saved {
                    alert: context.client.mute(&alert.id, Some(until))?,
                })
            }
            Action::Unmute { alert } => {
                let alert = context.alert(alert)?;
                AlertsReport::Saved(Saved {
                    alert: context.client.mute(&alert.id, None)?,
                })
            }
            Action::Delete { alert } => {
                let alert = context.alert(alert)?;
                context.client.delete(&alert.id)?;
                AlertsReport::Deleted(Deleted {
                    deleted: alert.id,
                    name: alert.name,
                })
            }
            Action::Destinations => AlertsReport::Destinations(Destinations {
                destinations: context.client.destinations(&context.environment)?,
            }),
        })
    }
}

/// `baml cloud alerts`: an action in the environment `baml cloud`'s scope names.
pub(crate) struct CloudAlerts<'a> {
    pub args: &'a AlertsArgs,
    pub scope: &'a CloudScope,
}

impl Command for CloudAlerts<'_> {
    type Output = AlertsReport;

    fn run(&self, _cx: &CommandContext) -> Result<Outcome<AlertsReport>> {
        Ok(Outcome::ok(self.args.report(self.scope)?))
    }
}

/// What an action reports: text, or with `--output-format json` one JSON object.
#[derive(Serialize)]
#[serde(untagged)]
pub(crate) enum AlertsReport {
    Alerts(Listing),
    Shown(Shown),
    Saved(Saved),
    Runs(RunPage),
    Deleted(Deleted),
    Destinations(Destinations),
}

#[derive(Serialize)]
pub(crate) struct Listing {
    environment: String,
    alerts: Vec<Alert>,
}

#[derive(Serialize)]
pub(crate) struct Shown {
    alert: Alert,
    definition: Definition,
    /// Destination names by ID, for the text view.
    #[serde(skip)]
    destination_names: HashMap<String, String>,
}

#[derive(Serialize)]
pub(crate) struct Saved {
    alert: Alert,
}

#[derive(Serialize)]
pub(crate) struct Deleted {
    deleted: String,
    name: String,
}

#[derive(Serialize)]
pub(crate) struct Destinations {
    destinations: Vec<Destination>,
}

impl Report for AlertsReport {
    fn text(&self, out: &mut dyn io::Write, _style: &Style) -> io::Result<()> {
        match self {
            Self::Alerts(listing) if listing.alerts.is_empty() => {
                writeln!(out, "No alerts in {}.", listing.environment)
            }
            Self::Alerts(listing) => listing
                .alerts
                .iter()
                .try_for_each(|alert| writeln!(out, "{}", line(alert))),
            Self::Shown(shown) => {
                let definition = &shown.definition;
                writeln!(out, "{}", line(&shown.alert))?;
                let to: Vec<&str> = definition
                    .destinations
                    .iter()
                    .map(|id| {
                        shown
                            .destination_names
                            .get(id)
                            .map_or(id.as_str(), String::as_str)
                    })
                    .collect();
                writeln!(out, "Posts to: {}", to.join(", "))?;
                writeln!(
                    out,
                    "Fires when: {}{}",
                    describe_condition(&definition.condition),
                    match definition.after {
                        1 => String::new(),
                        n => format!(", {n} checks in a row"),
                    }
                )?;
                writeln!(out, "Posts: {}", describe_notify(definition.notify))?;
                writeln!(out, "Rows per message: {}", definition.message_rows)?;
                writeln!(out, "\n{}", definition.sql.trim())
            }
            Self::Saved(saved) => writeln!(out, "{}", line(&saved.alert)),
            Self::Runs(page) if page.runs.is_empty() => {
                writeln!(out, "No runs have posted or failed yet.")
            }
            Self::Runs(page) => {
                for run in &page.runs {
                    writeln!(out, "{}", run_line(run))?;
                }
                if let Some(before) = page.next_before {
                    writeln!(out, "More: baml cloud alerts runs --before {before}")?;
                }
                Ok(())
            }
            Self::Deleted(deleted) => {
                writeln!(out, "Deleted {} ({}).", deleted.name, deleted.deleted)
            }
            Self::Destinations(list) if list.destinations.is_empty() => writeln!(
                out,
                "No destinations yet. Add a Slack channel or a webhook in Studio."
            ),
            Self::Destinations(list) => list.destinations.iter().try_for_each(|destination| {
                writeln!(
                    out,
                    "{}  {}  {}",
                    destination.id, destination.kind, destination.name
                )
            }),
        }
    }
}

/// The signed-in client and the environment the command works in.
struct AlertContext {
    client: Client,
    environment: String,
    environment_name: String,
}

impl AlertContext {
    fn open(scope: &CloudScope) -> Result<Self> {
        let root =
            crate::project_load::find_project_root_from(None)?.unwrap_or(std::env::current_dir()?);
        let settings = crate::cloud_config::Boundary::read(&root)?;
        let gateway = settings.endpoint()?;
        let stored = bcs_api::Store::new(&gateway)?
            .read()?
            .context("alerts need your Boundary login: run `baml cloud auth login`")?;
        let session = bcs_api::Client::new(gateway)?.refresh(&stored)?;
        let control = bcs_api::alerts::control_endpoint(settings.control_api_url.as_deref())?;
        let client = Client::new(control, session.access_token)?;
        let project = match scope.project.clone() {
            Some(project) => Some(project),
            None => baml_env::string_var("BOUNDARY_PROJECT")?.or(settings.project),
        }
        .context("give the project: --project org_handle/project_name")?;
        let found = client
            .environment(&project, scope.env.as_deref())
            .with_context(|| match &scope.env {
                Some(name) => format!("no environment {name} in {project} that you can reach"),
                None => format!("no personal environment of yours in {project}"),
            })?;
        Ok(Self {
            client,
            environment: found.environment_id,
            environment_name: found.environment_name,
        })
    }

    /// The alert a name or ID names.
    fn alert(&self, reference: &str) -> Result<Alert> {
        let alerts = self.client.list(&self.environment)?;
        let mut matching = alerts
            .into_iter()
            .filter(|alert| alert.id == reference || alert.name == reference);
        match (matching.next(), matching.next()) {
            (Some(alert), None) => Ok(alert),
            (Some(_), Some(_)) => bail!("more than one alert is named {reference}; use its ID"),
            (None, _) => bail!("no alert {reference} in {}", self.environment_name),
        }
    }

    /// The destination IDs names or IDs name.
    fn destinations(&self, references: &[String]) -> Result<Vec<String>> {
        let destinations: Vec<Destination> = self.client.destinations(&self.environment)?;
        references
            .iter()
            .map(|reference| {
                destinations
                    .iter()
                    .find(|destination| {
                        destination.id == *reference || destination.name == *reference
                    })
                    .map(|destination| destination.id.clone())
                    .with_context(|| {
                        let known: Vec<&str> = destinations
                            .iter()
                            .map(|destination| destination.name.as_str())
                            .collect();
                        format!(
                            "no destination {reference} in {}; it has: {}",
                            self.environment_name,
                            if known.is_empty() {
                                "none (add one in Studio)".to_owned()
                            } else {
                                known.join(", ")
                            }
                        )
                    })
            })
            .collect()
    }

    fn destination_names(&self) -> Result<HashMap<String, String>> {
        Ok(self
            .client
            .destinations(&self.environment)?
            .into_iter()
            .map(|destination| (destination.id, destination.name))
            .collect())
    }
}

impl Spec {
    fn condition(&self) -> Result<Option<Condition>> {
        self.when.as_deref().map(parse_condition).transpose()
    }

    /// The notify policy the flags give, from `current` for what they leave out.
    fn notify(&self, current: Notify) -> Result<Notify> {
        let mode = match self.notify.as_deref() {
            None => current,
            Some("changes") => match current {
                Notify::Changes { .. } => current,
                Notify::EveryCheck => Notify::Changes {
                    remind_seconds: None,
                },
            },
            Some("every-check") => Notify::EveryCheck,
            Some(other) => bail!("--notify takes changes or every-check, not {other}"),
        };
        match (&self.remind, mode) {
            (None, mode) => Ok(mode),
            (Some(_), Notify::EveryCheck) => {
                bail!("--remind needs --notify changes: every-check posts on every bad check")
            }
            (Some(remind), Notify::Changes { .. }) => Ok(Notify::Changes {
                remind_seconds: match remind.as_str() {
                    "never" => None,
                    interval => Some(parse_interval(interval)?),
                },
            }),
        }
    }

    fn sql(&self) -> Result<Option<String>> {
        let sql = match (&self.sql, &self.sql_file) {
            (Some(sql), _) if sql == "-" => {
                let mut sql = String::new();
                std::io::stdin().read_to_string(&mut sql)?;
                sql
            }
            (Some(sql), _) => sql.clone(),
            (None, Some(path)) => std::fs::read_to_string(path)
                .with_context(|| format!("could not read {}", path.display()))?,
            (None, None) => return Ok(None),
        };
        anyhow::ensure!(!sql.trim().is_empty(), "the SQL is empty");
        Ok(Some(sql))
    }

    fn schedule(&self) -> Result<Option<Schedule>> {
        if let Some(every) = &self.every {
            return Ok(Some(Schedule::Every {
                seconds: parse_interval(every)?,
            }));
        }
        let Some(time) = &self.at else {
            return Ok(None);
        };
        Ok(Some(Schedule::At {
            time: parse_time(time)?,
            time_zone: self.time_zone.clone().unwrap_or_else(|| "UTC".to_owned()),
            days: parse_days(self.days.as_deref().unwrap_or("daily"))?,
        }))
    }
}

/// Seconds in an interval such as `90s`, `15m`, `1h`, `1d` or `1w`.
pub(crate) fn parse_interval(value: &str) -> Result<u32> {
    let value = value.trim();
    let split = value
        .find(|character: char| !character.is_ascii_digit())
        .unwrap_or(value.len());
    let (count, unit) = value.split_at(split);
    let count: u32 = count
        .parse()
        .with_context(|| format!("{value} is not an interval such as 15m or 1h"))?;
    let unit = match unit {
        "s" => 1,
        "" | "m" => 60,
        "h" => 3_600,
        "d" => 86_400,
        "w" => 604_800,
        _ => bail!("{value} is not an interval such as 15m or 1h"),
    };
    let seconds = count
        .checked_mul(unit)
        .with_context(|| format!("{value} is too long"))?;
    anyhow::ensure!(seconds > 0, "an interval must be longer than zero");
    Ok(seconds)
}

/// A condition as `--when` takes it: `rows`, or `[column] <op> <number>`.
pub(crate) fn parse_condition(value: &str) -> Result<Condition> {
    let value = value.trim();
    if value.eq_ignore_ascii_case("rows") {
        return Ok(Condition::Rows);
    }
    let (at, op) = [">=", "<=", ">", "<"]
        .iter()
        .filter_map(|op| value.find(op).map(|at| (at, *op)))
        .min_by_key(|(at, op)| (*at, std::cmp::Reverse(op.len())))
        .with_context(|| {
            format!("{value} is not a condition such as rows, \"errors > 5\" or \"> 0.2\"")
        })?;
    let column = value[..at].trim();
    let threshold: f64 = value[at + op.len()..]
        .trim()
        .parse()
        .with_context(|| format!("{value}: the threshold must be a number"))?;
    anyhow::ensure!(
        threshold.is_finite(),
        "{value}: the threshold must be a number"
    );
    Ok(Condition::Value {
        column: (!column.is_empty()).then(|| column.to_owned()),
        op: op.to_owned(),
        threshold,
    })
}

fn describe_condition(condition: &Condition) -> String {
    match condition {
        Condition::Rows => "the query returns rows".to_owned(),
        Condition::Value {
            column,
            op,
            threshold,
        } => format!(
            "{} {op} {threshold}",
            column.as_deref().unwrap_or("the first column")
        ),
    }
}

fn describe_notify(notify: Notify) -> String {
    match notify {
        Notify::EveryCheck => "on every bad check, and when it resolves".to_owned(),
        Notify::Changes {
            remind_seconds: None,
        } => "when it fires and when it resolves".to_owned(),
        Notify::Changes {
            remind_seconds: Some(seconds),
        } => format!(
            "when it fires, {} while it stays firing, and when it resolves",
            describe(&Schedule::Every { seconds })
        ),
    }
}

/// `HH:MM`, 24-hour, as the control API takes it.
pub(crate) fn parse_time(value: &str) -> Result<String> {
    let (hours, minutes) = value
        .split_once(':')
        .with_context(|| format!("{value} is not a time such as 09:00"))?;
    let (hours, minutes): (u32, u32) = (hours.parse()?, minutes.parse()?);
    anyhow::ensure!(
        hours < 24 && minutes < 60,
        "{value} is not a time such as 09:00"
    );
    Ok(format!("{hours:02}:{minutes:02}"))
}

/// The days `daily`, `weekdays`, `mon-fri` or `mon,wed,fri` name, in week order.
pub(crate) fn parse_days(value: &str) -> Result<Vec<String>> {
    let index = |day: &str| {
        DAYS.iter()
            .position(|known| known.eq_ignore_ascii_case(day.trim()))
            .with_context(|| format!("{day} is not a day: use mon, tue, wed, thu, fri, sat or sun"))
    };
    let mut chosen = [false; 7];
    match value.trim().to_ascii_lowercase().as_str() {
        "daily" | "every-day" | "all" => chosen = [true; 7],
        "weekdays" => chosen[..5].fill(true),
        "weekends" => chosen[5..].fill(true),
        list => {
            for part in list.split(',') {
                match part.split_once('-') {
                    Some((from, to)) => {
                        let (from, to) = (index(from)?, index(to)?);
                        anyhow::ensure!(from <= to, "{part} runs backwards; write it as mon-fri");
                        chosen[from..=to].fill(true);
                    }
                    None => chosen[index(part)?] = true,
                }
            }
        }
    }
    Ok(DAYS
        .iter()
        .zip(chosen)
        .filter(|(_, chosen)| *chosen)
        .map(|(day, _)| (*day).to_owned())
        .collect())
}

fn describe(schedule: &Schedule) -> String {
    match schedule {
        Schedule::Every { seconds } => {
            let units = [(604_800, "w"), (86_400, "d"), (3_600, "h"), (60, "m")];
            let (size, unit) = units
                .iter()
                .find(|(size, _)| seconds % size == 0)
                .copied()
                .unwrap_or((1, "s"));
            format!("every {}{unit}", seconds / size)
        }
        Schedule::At {
            time,
            time_zone,
            days,
        } => format!("at {time} {time_zone}, {}", days.join(",")),
    }
}

fn line(alert: &Alert) -> String {
    let schedule = match (alert.muted_until, alert.next_run_at) {
        (Some(until), _) if until > now() => format!("muted until {}", timestamp(until)),
        (_, Some(next)) => format!("next run {}", timestamp(next)),
        _ => "not scheduled".to_owned(),
    };
    let state = if alert.state.firing {
        "FIRING".to_owned()
    } else if alert.state.failing {
        "QUERY FAILING".to_owned()
    } else if alert.state.breaching > 0 {
        format!("bad for {} checks", alert.state.breaching)
    } else {
        "ok".to_owned()
    };
    format!(
        "{}  {}  v{}  {}  {state}  {schedule}",
        alert.id,
        alert.name,
        alert.version,
        describe(&alert.schedule),
    )
}

fn run_line(run: &Run) -> String {
    let details = match &run.error {
        Some(error) => error.clone(),
        None => run
            .deliveries
            .iter()
            .map(|delivery| {
                let name = delivery
                    .destination_name
                    .clone()
                    .unwrap_or_else(|| delivery.destination_id.clone());
                let name = if delivery.deleted {
                    format!("{name} (deleted)")
                } else {
                    name
                };
                if delivery.delivered {
                    format!("sent to {name}")
                } else {
                    format!(
                        "not sent to {name}: {}",
                        delivery.reason.as_deref().unwrap_or("refused")
                    )
                }
            })
            .collect::<Vec<_>>()
            .join("; "),
    };
    format!(
        "{}  {}  {}  {}  {details}",
        run.id,
        timestamp(run.run_at),
        run.alert_name,
        run.outcome
    )
}

fn timestamp(seconds: u64) -> String {
    i64::try_from(seconds)
        .ok()
        .and_then(|seconds| jiff::Timestamp::from_second(seconds).ok())
        .map_or_else(
            || seconds.to_string(),
            |time| time.strftime("%Y-%m-%d %H:%M UTC").to_string(),
        )
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intervals_times_and_days_read_as_people_write_them() {
        assert_eq!(parse_interval("15m").unwrap(), 900);
        assert_eq!(parse_interval("1h").unwrap(), 3_600);
        assert_eq!(parse_interval("2d").unwrap(), 172_800);
        assert_eq!(parse_interval("90s").unwrap(), 90);
        assert_eq!(
            parse_interval("30").unwrap(),
            1_800,
            "bare numbers are minutes"
        );
        for refused in ["", "0m", "15x", "m15", "-1h"] {
            assert!(parse_interval(refused).is_err(), "{refused:?}");
        }
        assert_eq!(parse_time("9:05").unwrap(), "09:05");
        assert!(parse_time("24:00").is_err());
        assert_eq!(
            parse_days("weekdays").unwrap(),
            ["mon", "tue", "wed", "thu", "fri"]
        );
        assert_eq!(parse_days("fri,mon").unwrap(), ["mon", "fri"]);
        assert_eq!(
            parse_days("Mon-Wed,sun").unwrap(),
            ["mon", "tue", "wed", "sun"]
        );
        assert_eq!(parse_days("daily").unwrap().len(), 7);
        assert!(parse_days("fri-mon").is_err());
        assert!(parse_days("funday").is_err());
    }

    #[test]
    fn conditions_and_notify_read_as_people_write_them() {
        assert_eq!(parse_condition("rows").unwrap(), Condition::Rows);
        assert_eq!(
            parse_condition("errors > 5").unwrap(),
            Condition::Value {
                column: Some("errors".into()),
                op: ">".into(),
                threshold: 5.0
            }
        );
        assert_eq!(
            parse_condition("p95_ms>=1500").unwrap(),
            Condition::Value {
                column: Some("p95_ms".into()),
                op: ">=".into(),
                threshold: 1500.0
            }
        );
        assert_eq!(
            parse_condition("< 0.2").unwrap(),
            Condition::Value {
                column: None,
                op: "<".into(),
                threshold: 0.2
            }
        );
        for refused in ["errors", "errors > many", "> NaN", ""] {
            assert!(parse_condition(refused).is_err(), "{refused:?}");
        }
        let spec = |notify: Option<&str>, remind: Option<&str>| Spec {
            sql: None,
            sql_file: None,
            every: None,
            at: None,
            time_zone: None,
            days: None,
            to: Vec::new(),
            rows: None,
            run_now: false,
            when: None,
            after: None,
            notify: notify.map(str::to_owned),
            remind: remind.map(str::to_owned),
        };
        let changes = Notify::Changes {
            remind_seconds: None,
        };
        assert_eq!(spec(None, None).notify(changes).unwrap(), changes);
        assert_eq!(
            spec(None, Some("1h")).notify(changes).unwrap(),
            Notify::Changes {
                remind_seconds: Some(3_600)
            }
        );
        assert_eq!(
            spec(Some("every-check"), None).notify(changes).unwrap(),
            Notify::EveryCheck
        );
        assert!(
            spec(Some("every-check"), Some("1h"))
                .notify(changes)
                .is_err()
        );
        assert!(spec(Some("sometimes"), None).notify(changes).is_err());
    }

    #[test]
    fn schedules_read_back_as_they_were_given() {
        assert_eq!(describe(&Schedule::Every { seconds: 900 }), "every 15m");
        assert_eq!(describe(&Schedule::Every { seconds: 86_400 }), "every 1d");
        assert_eq!(describe(&Schedule::Every { seconds: 90 }), "every 90s");
        assert_eq!(
            describe(&Schedule::At {
                time: "09:00".into(),
                time_zone: "Europe/Madrid".into(),
                days: vec!["mon".into(), "fri".into()],
            }),
            "at 09:00 Europe/Madrid, mon,fri"
        );
    }
}
