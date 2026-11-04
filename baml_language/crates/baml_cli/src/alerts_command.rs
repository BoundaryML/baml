//! `baml alerts`: SQL that runs on a schedule in a Boundary environment and posts to Slack or a
//! webhook when it returns rows. Alerts are kept by Boundary's control API; this command signs
//! in with the saved `baml auth login` session and names the project and environment as
//! `baml query` does.
#![allow(
    clippy::print_stdout,
    reason = "listings and saved alerts are the command's output"
)]

use std::{io::Read as _, path::PathBuf};

use anyhow::{Context, Result, bail};
use bcs_api::alerts::{Alert, Client, Definition, Destination, Run, Schedule};
use clap::{Args, Parser, Subcommand};

use crate::ExitCode;

const DAYS: [&str; 7] = ["mon", "tue", "wed", "thu", "fri", "sat", "sun"];

/// Manage alerts: SQL that runs on a schedule and posts to Slack or a webhook when it returns rows.
#[derive(Debug, Parser)]
#[command(after_long_help = "Examples:
  List an environment's alerts:
    baml alerts list --project acme/ai-pipeline --environment production

  Post to #ai-pipeline when FindFacts failed in the last 15 minutes, checking every 15 minutes:
    baml alerts create --name \"FindFacts errors\" --every 15m --to \"#ai-pipeline\" \\
      --sql \"SELECT span_id, error_value FROM spans WHERE span_name = 'FindFacts'
             AND error_value IS NOT NULL AND start_time > now() - INTERVAL '15 minutes'\"

  Check a query first with `baml query --project ... --environment ...`.")]
pub struct AlertsArgs {
    #[command(subcommand)]
    pub action: Action,

    /// Boundary cloud project in org_handle/project_name form.
    #[arg(
        long,
        id = "boundary_project",
        value_name = "ORG/PROJECT",
        global = true
    )]
    pub project: Option<String>,

    /// Boundary cloud environment; defaults to your personal environment.
    #[arg(long, global = true)]
    pub environment: Option<String>,

    /// Print JSON instead of a table.
    #[arg(long, global = true)]
    pub json: bool,
}

#[derive(Debug, Subcommand)]
pub enum Action {
    /// List the environment's alerts.
    List,
    /// Show one alert, with its SQL.
    Show {
        /// The alert's name or ID.
        alert: String,
    },
    /// Create an alert.
    Create(CreateArgs),
    /// Save a new version of an alert; what is not given stays as it is.
    Update(UpdateArgs),
    /// List the runs that posted, failed or could not deliver, newest first.
    Runs {
        /// Runs to list.
        #[arg(long, default_value_t = 10)]
        limit: u32,
        /// Only runs logged before this run ID (a listing's last ID).
        #[arg(long)]
        before: Option<u64>,
    },
    /// Stop an alert's runs for a while, such as `24h`.
    Mute {
        /// The alert's name or ID.
        alert: String,
        /// How long, such as `30m`, `24h` or `7d`.
        duration: String,
    },
    /// End an alert's mute.
    Unmute {
        /// The alert's name or ID.
        alert: String,
    },
    /// Delete an alert: it stops running.
    Delete {
        /// The alert's name or ID.
        alert: String,
    },
    /// List where the environment's alerts can post. Add destinations in Studio.
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
    pub fn run(&self) -> Result<ExitCode> {
        let context = AlertContext::open(self)?;
        match &self.action {
            Action::List => {
                let alerts = context.client.list(&context.environment)?;
                if self.json {
                    print_json(&alerts)?;
                } else if alerts.is_empty() {
                    println!("No alerts in {}.", context.environment_name);
                } else {
                    for alert in &alerts {
                        println!("{}", line(alert));
                    }
                }
            }
            Action::Show { alert } => {
                let alert = context.alert(alert)?;
                let definition = context.client.definition(&alert.id)?;
                if self.json {
                    print_json(&serde_json::json!({"alert": alert, "definition": definition}))?;
                } else {
                    let names = context.destination_names()?;
                    println!("{}", line(&alert));
                    let to: Vec<&str> = definition
                        .destinations
                        .iter()
                        .map(|id| names.get(id).map_or(id.as_str(), String::as_str))
                        .collect();
                    println!("Posts to: {}", to.join(", "));
                    println!("Rows per message: {}", definition.message_rows);
                    println!("\n{}", definition.sql.trim());
                }
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
                    "give where to post: --to <destination>; `baml alerts destinations` lists them"
                );
                let definition = Definition {
                    name: args.name.clone(),
                    sql,
                    destinations: context.destinations(&args.spec.to)?,
                    message_rows: args.spec.rows.unwrap_or(5),
                };
                let saved = context.client.create(
                    &context.environment,
                    &definition,
                    &schedule,
                    args.spec.run_now,
                )?;
                context.print_saved(&saved, self.json)?;
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
                };
                let schedule = args.spec.schedule()?.unwrap_or(alert.schedule.clone());
                let saved = context.client.update(
                    &alert.id,
                    alert.version,
                    &definition,
                    &schedule,
                    args.spec.run_now,
                )?;
                context.print_saved(&saved, self.json)?;
            }
            Action::Runs { limit, before } => {
                let page = context.client.runs(&context.environment, *limit, *before)?;
                if self.json {
                    print_json(&page)?;
                } else if page.runs.is_empty() {
                    println!("No runs have posted or failed yet.");
                } else {
                    for run in &page.runs {
                        println!("{}", run_line(run));
                    }
                    if let Some(before) = page.next_before {
                        println!("More: baml alerts runs --before {before}");
                    }
                }
            }
            Action::Mute { alert, duration } => {
                let alert = context.alert(alert)?;
                let until = now() + u64::from(parse_interval(duration)?);
                let muted = context.client.mute(&alert.id, Some(until))?;
                context.print_saved(&muted, self.json)?;
            }
            Action::Unmute { alert } => {
                let alert = context.alert(alert)?;
                let unmuted = context.client.mute(&alert.id, None)?;
                context.print_saved(&unmuted, self.json)?;
            }
            Action::Delete { alert } => {
                let alert = context.alert(alert)?;
                context.client.delete(&alert.id)?;
                if self.json {
                    print_json(&serde_json::json!({"deleted": alert.id}))?;
                } else {
                    println!("Deleted {} ({}).", alert.name, alert.id);
                }
            }
            Action::Destinations => {
                let destinations = context.client.destinations(&context.environment)?;
                if self.json {
                    print_json(&destinations)?;
                } else if destinations.is_empty() {
                    println!("No destinations yet. Add a Slack channel or a webhook in Studio.");
                } else {
                    for destination in &destinations {
                        println!(
                            "{}  {}  {}",
                            destination.id, destination.kind, destination.name
                        );
                    }
                }
            }
        }
        Ok(ExitCode::Success)
    }
}

/// The signed-in client and the environment the command works in.
struct AlertContext {
    client: Client,
    environment: String,
    environment_name: String,
}

impl AlertContext {
    fn open(args: &AlertsArgs) -> Result<Self> {
        let root =
            crate::project_load::find_project_root_from(None)?.unwrap_or(std::env::current_dir()?);
        let settings = crate::cloud_config::Boundary::read(&root)?;
        let gateway = settings.endpoint()?;
        let stored = bcs_api::Store::new(&gateway)?
            .read()?
            .context("alerts need your Boundary login: run `baml auth login`")?;
        let session = bcs_api::Client::new(gateway)?.refresh(&stored)?;
        let control = bcs_api::alerts::control_endpoint(settings.control_api_url.as_deref())?;
        let client = Client::new(control, session.access_token)?;
        let project = match args.project.clone() {
            Some(project) => Some(project),
            None => baml_env::string_var("BOUNDARY_PROJECT")?.or(settings.project),
        }
        .context("give the project: --project org_handle/project_name")?;
        let found = client
            .environment(&project, args.environment.as_deref())
            .with_context(|| match &args.environment {
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

    fn destination_names(&self) -> Result<std::collections::HashMap<String, String>> {
        Ok(self
            .client
            .destinations(&self.environment)?
            .into_iter()
            .map(|destination| (destination.id, destination.name))
            .collect())
    }

    fn print_saved(&self, alert: &Alert, json: bool) -> Result<()> {
        if json {
            print_json(alert)
        } else {
            println!("{}", line(alert));
            Ok(())
        }
    }
}

impl Spec {
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
    let state = match (alert.muted_until, alert.next_run_at) {
        (Some(until), _) if until > now() => format!("muted until {}", timestamp(until)),
        (_, Some(next)) => format!("next run {}", timestamp(next)),
        _ => "not scheduled".to_owned(),
    };
    format!(
        "{}  {}  v{}  {}  {}",
        alert.id,
        alert.name,
        alert.version,
        describe(&alert.schedule),
        state
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

fn print_json<T: serde::Serialize + ?Sized>(value: &T) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
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
