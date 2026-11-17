//! `baml cloud hotswaps`: swap marked functions in running processes from
//! Boundary Cloud (BEP-087, stage 2).
//!
//! `create` makes the swap in BCS first, then follows its stored status
//! events while it diffs the working tree against the baseline with git,
//! compiles to check, and uploads the changed files. The result goes to
//! stdout; status goes to stderr. Ctrl-C is safe: the swap continues in BCS.

use std::{
    collections::BTreeMap,
    io::{IsTerminal as _, Read as _, Write as _},
    path::{Path, PathBuf},
};

use anyhow::{Context as _, bail};
use bcs_api::{
    Client,
    credentials::{Authentication, RequestAuthorization, Target},
    hotswaps::{CreateHotswap, Hotswap, HotswapEvent, HotswapPayload, SourceFile, SwapFunction},
};
use clap::{Args, Subcommand};
use serde::Serialize;

use crate::{
    ExitCode,
    output::{CommandContext, Outcome, Report, Style, run_command},
    paint::Painter,
    recordings::CloudScope,
};

#[derive(Subcommand, Debug)]
pub(crate) enum HotswapsCommands {
    /// Swap the working tree's changes to marked functions into running
    /// processes
    #[command(
        after_long_help = "The query selects processes in the project and environment. The baseline defaults to HEAD; only processes built from it install the swap.\n\nExamples:\n  Every process in production:\n    baml cloud --env production hotswaps create \"SELECT process_id FROM processes\"\n\n  Two processes by ID:\n    baml cloud hotswaps create --process 0193... --process 0194..."
    )]
    Create(CreateArgs),
    /// List the swaps in the project and environment
    #[command(
        after_long_help = "Examples:\n  List swaps in production:\n    baml cloud --env production hotswaps list"
    )]
    List,
    /// Show one swap, with its diff
    #[command(
        after_long_help = "Examples:\n  Show a swap:\n    baml cloud hotswaps show hs_7Kq2\n\n  With its full history:\n    baml cloud hotswaps show hs_7Kq2 --events"
    )]
    Show(ShowArgs),
    /// Approve, disable or enable a swap
    #[command(
        after_long_help = "Examples:\n  Approve a swap, as the dashboard does:\n    baml cloud hotswaps update hs_6Jp1 --approve\n\n  Stop delivering a swap:\n    baml cloud hotswaps update hs_7Kq2 --disable"
    )]
    Update(UpdateArgs),
    /// Delete a swap, returning its processes to the code as built
    #[command(
        after_long_help = "Examples:\n  Delete a swap:\n    baml cloud hotswaps delete hs_7Kq2"
    )]
    Delete(IdArgs),
}

#[derive(Args, Debug)]
pub(crate) struct CreateArgs {
    /// The SQL query that selects processes, or `-` to read it from stdin
    #[arg(value_name = "SQL | -")]
    pub query: Option<String>,
    /// The git commit the change is made against
    #[arg(long, value_name = "COMMIT", default_value = "HEAD")]
    pub baseline: String,
    /// Target this process instead of a query; repeatable
    #[arg(long = "process", value_name = "ID")]
    pub processes: Vec<String>,
    /// Retire this active swap and install the new one in its place
    #[arg(long, value_name = "ID")]
    pub replace: Option<String>,
    /// Return once the upload is done, without following the rollout
    #[arg(long)]
    pub no_wait: bool,
}

#[derive(Args, Debug)]
pub(crate) struct ShowArgs {
    pub id: String,
    /// Print every stored status event
    #[arg(long)]
    pub events: bool,
}

#[derive(Args, Debug)]
#[command(group(clap::ArgGroup::new("action").required(true).args(["approve", "disable", "enable"])))]
pub(crate) struct UpdateArgs {
    pub id: String,
    /// Approve the swap, as the dashboard does
    #[arg(long)]
    pub approve: bool,
    /// Stop delivering the swap; installed processes remove it
    #[arg(long)]
    pub disable: bool,
    /// Deliver a disabled swap again
    #[arg(long)]
    pub enable: bool,
}

#[derive(Args, Debug)]
pub(crate) struct IdArgs {
    pub id: String,
}

impl HotswapsCommands {
    pub(crate) fn run(&self, cx: &CommandContext, scope: &CloudScope) -> anyhow::Result<ExitCode> {
        let command = Hotswaps {
            command: self,
            scope,
        };
        run_command(&command, cx)
    }
}

struct Hotswaps<'a> {
    command: &'a HotswapsCommands,
    scope: &'a CloudScope,
}

/// What every hot swap command prints: one swap's row, a list of them, or
/// one in full.
#[derive(Debug, Serialize)]
#[serde(untagged)]
pub(crate) enum HotswapReport {
    Row(Row),
    List(Vec<Row>),
    Shown {
        #[serde(flatten)]
        hotswap: Box<Hotswap>,
        #[serde(skip_serializing_if = "Vec::is_empty")]
        events: Vec<HotswapEvent>,
        #[serde(skip)]
        diffs: Vec<(String, String, String)>,
    },
}

/// A swap as `create`, `update`, `delete` and `list` print it.
#[derive(Debug, Serialize)]
pub(crate) struct Row {
    pub id: String,
    pub status: String,
    pub functions: Vec<String>,
    pub processes: RowProcesses,
}

#[derive(Debug, Serialize)]
pub(crate) struct RowProcesses {
    pub matched: u64,
    pub installed: u64,
    pub rejected: u64,
}

impl From<&Hotswap> for Row {
    fn from(hotswap: &Hotswap) -> Self {
        Self {
            id: hotswap.id.clone(),
            status: hotswap.status.clone(),
            functions: hotswap.functions.iter().map(|f| f.name.clone()).collect(),
            processes: RowProcesses {
                matched: hotswap.processes.matched,
                installed: hotswap.processes.installed,
                rejected: hotswap.processes.rejected,
            },
        }
    }
}

impl Row {
    fn cells(&self) -> [String; 4] {
        [
            self.id.clone(),
            self.status.clone(),
            if self.functions.is_empty() {
                "-".into()
            } else {
                self.functions.join(", ")
            },
            format!("{}/{}", self.processes.installed, self.processes.matched),
        ]
    }
}

fn table(out: &mut dyn std::io::Write, rows: &[[String; 4]]) -> std::io::Result<()> {
    let mut widths = [0usize; 4];
    for row in rows {
        for (width, cell) in widths.iter_mut().zip(row) {
            *width = (*width).max(cell.chars().count());
        }
    }
    for row in rows {
        let line = row
            .iter()
            .zip(widths)
            .enumerate()
            .map(|(index, (cell, width))| {
                if index == 3 {
                    cell.clone()
                } else {
                    format!("{cell:width$}")
                }
            })
            .collect::<Vec<_>>()
            .join("  ");
        writeln!(out, "{line}")?;
    }
    Ok(())
}

impl Report for HotswapReport {
    fn text(&self, out: &mut dyn std::io::Write, _style: &Style) -> std::io::Result<()> {
        match self {
            Self::Row(row) => table(out, &[row.cells()]),
            Self::List(rows) => {
                let mut cells = vec![[
                    "ID".to_string(),
                    "STATUS".to_string(),
                    "FUNCTIONS".to_string(),
                    "PROCESSES".to_string(),
                ]];
                cells.extend(rows.iter().map(Row::cells));
                table(out, &cells)
            }
            Self::Shown {
                hotswap,
                events,
                diffs,
            } => {
                let by = |who: &Option<String>, at: &Option<String>| match (who, at) {
                    (Some(who), Some(at)) => format!("{who}, {at}"),
                    (Some(who), None) => who.clone(),
                    (None, Some(at)) => at.clone(),
                    (None, None) => "-".into(),
                };
                let target = match (&hotswap.query, &hotswap.process_ids) {
                    (Some(query), _) => ("query", query.clone()),
                    (None, Some(ids)) => ("processes", ids.join(", ")),
                    (None, None) => ("query", "-".into()),
                };
                let mut lines = vec![
                    ("id", hotswap.id.clone()),
                    ("status", hotswap.status.clone()),
                    ("baseline", hotswap.baseline.clone()),
                    target,
                    (
                        "published",
                        by(&hotswap.published_by, &hotswap.published_at),
                    ),
                    ("approved", by(&hotswap.approved_by, &hotswap.approved_at)),
                    (
                        "renewed",
                        hotswap.renewed_at.clone().unwrap_or_else(|| "-".into()),
                    ),
                    (
                        "processes",
                        format!(
                            "{} installed, {} rejected",
                            hotswap.processes.installed, hotswap.processes.rejected
                        ),
                    ),
                ];
                if let Some(replaced) = &hotswap.replaces {
                    lines.insert(3, ("replaces", replaced.clone()));
                }
                if let Some(rejection) = &hotswap.rejection {
                    lines.push(("rejected", rejection.reason.clone()));
                }
                for (key, value) in lines {
                    writeln!(out, "{key:10} {value}")?;
                }
                for (summary, (name, old, new)) in hotswap.functions.iter().zip(diffs) {
                    debug_assert_eq!(&summary.name, name);
                    writeln!(out)?;
                    writeln!(out, "{name} (rule: {})", summary.rule)?;
                    for line in diff_lines(old, new) {
                        writeln!(out, "{line}")?;
                    }
                }
                if !events.is_empty() {
                    writeln!(out)?;
                    for event in events {
                        writeln!(out, "{}", describe_event(event))?;
                    }
                }
                Ok(())
            }
        }
    }
}

/// One event as `show --events` prints it.
fn describe_event(event: &HotswapEvent) -> String {
    let mut parts = vec![event.at.clone()];
    if let Some(process) = &event.process {
        parts.push(format!("process {process}"));
    }
    parts.push(event.status.clone());
    for (key, value) in [
        ("by", event.by.clone()),
        ("baseline", event.baseline.clone()),
        ("processes", event.processes.map(|n| n.to_string())),
        ("skipped", event.skipped.map(|n| n.to_string())),
        ("installed", event.installed.map(|n| n.to_string())),
        ("rejected", event.rejected.map(|n| n.to_string())),
        ("reason", event.reason.clone()),
    ] {
        if let Some(value) = value {
            parts.push(format!("{key}={value}"));
        }
    }
    parts.join("  ")
}

/// The lines of `old` and `new` that differ, `-` and `+`, by a longest
/// common subsequence of lines. Unchanged lines are left out.
fn diff_lines(old: &str, new: &str) -> Vec<String> {
    let (a, b): (Vec<&str>, Vec<&str>) = (old.lines().collect(), new.lines().collect());
    let mut lcs = vec![vec![0usize; b.len() + 1]; a.len() + 1];
    for i in (0..a.len()).rev() {
        for j in (0..b.len()).rev() {
            lcs[i][j] = if a[i] == b[j] {
                lcs[i + 1][j + 1] + 1
            } else {
                lcs[i + 1][j].max(lcs[i][j + 1])
            };
        }
    }
    let (mut i, mut j, mut out) = (0, 0, Vec::new());
    while i < a.len() || j < b.len() {
        if i < a.len() && j < b.len() && a[i] == b[j] {
            i += 1;
            j += 1;
        } else if i < a.len() && (j == b.len() || lcs[i + 1][j] >= lcs[i][j + 1]) {
            out.push(format!("-{}", a[i]));
            i += 1;
        } else {
            out.push(format!("+{}", b[j]));
            j += 1;
        }
    }
    out
}

impl crate::output::Command for Hotswaps<'_> {
    type Output = HotswapReport;

    fn run(&self, cx: &CommandContext) -> anyhow::Result<Outcome<HotswapReport>> {
        let root = crate::project_load::find_project_root_from(cx.project_dir.as_deref())?
            .unwrap_or_else(|| cx.project_dir.clone().unwrap_or_else(|| PathBuf::from(".")));
        let (client, authorization) = connect(&root, self.scope)?;
        let row = |hotswap: &Hotswap| Outcome::ok(HotswapReport::Row(Row::from(hotswap)));
        match self.command {
            HotswapsCommands::Create(args) => create(&root, &client, &authorization, args),
            HotswapsCommands::List => {
                let rows = client
                    .list_hotswaps(&authorization)?
                    .iter()
                    .map(Row::from)
                    .collect();
                Ok(Outcome::ok(HotswapReport::List(rows)))
            }
            HotswapsCommands::Show(args) => {
                let hotswap = client.hotswap(&authorization, &args.id)?;
                let mut events = Vec::new();
                if args.events {
                    client.hotswap_events(&authorization, &args.id, 0, false, |event| {
                        events.push(event);
                        Ok(true)
                    })?;
                }
                let diffs = hotswap
                    .functions
                    .iter()
                    .map(|function| {
                        (
                            function.name.clone(),
                            function.old_source.clone().unwrap_or_default(),
                            function.new_source.clone().unwrap_or_default(),
                        )
                    })
                    .collect();
                Ok(Outcome::ok(HotswapReport::Shown {
                    hotswap: Box::new(hotswap),
                    events,
                    diffs,
                }))
            }
            HotswapsCommands::Update(args) => {
                let action = if args.approve {
                    "approve"
                } else if args.disable {
                    "disable"
                } else {
                    "enable"
                };
                Ok(row(&client.update_hotswap(
                    &authorization,
                    &args.id,
                    action,
                )?))
            }
            HotswapsCommands::Delete(args) => {
                Ok(row(&client.delete_hotswap(&authorization, &args.id)?))
            }
        }
    }
}

/// The client and credential for the project and environment, as
/// `baml cloud recordings query` selects them.
fn connect(root: &Path, scope: &CloudScope) -> anyhow::Result<(Client, RequestAuthorization)> {
    let settings = crate::cloud_config::Boundary::read(root)?;
    let endpoint = settings.endpoint()?;
    let credential = match baml_env::string_var("BOUNDARY_API_KEY")? {
        Some(key) if key == bcs_api::credentials::LOCAL_API_KEY => {
            bail!("BOUNDARY_API_KEY=local has no Boundary Cloud; unset it to swap")
        }
        Some(key) => bcs_api::Secret::new(key),
        None => bcs_api::Store::new(&endpoint)
            .and_then(|store| store.read())?
            .map(|session| session.refresh_token)
            .context("not logged in to Boundary Cloud: run `baml cloud auth login`")?,
    };
    let project = match scope.project.clone() {
        Some(project) => Some(project),
        None => baml_env::string_var("BOUNDARY_PROJECT")?.or(settings.project),
    };
    let authorization = RequestAuthorization {
        authentication: Authentication::shared(endpoint.as_str(), credential),
        target: Some(Target {
            project,
            environment: scope.env.clone(),
            ..Target::default()
        }),
    };
    Ok((Client::new(endpoint)?, authorization))
}

/// One cargo-style status line on stderr, as `baml run --hot` prints them.
#[allow(clippy::print_stderr, reason = "status goes to stderr")]
fn status(verb: &str, message: impl std::fmt::Display) {
    if crate::reporter::quiet() {
        return;
    }
    let painter = Painter::stderr();
    eprintln!("{} {message}", painter.accent(format!("{verb:>12}")));
}

#[allow(clippy::print_stderr, reason = "status goes to stderr")]
fn detail(message: impl std::fmt::Display) {
    if !crate::reporter::quiet() {
        eprintln!("{:12} {message}", "");
    }
}

fn git(dir: &Path, args: &[&str]) -> anyhow::Result<Vec<u8>> {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .context("hot swapping needs git, and `git` did not run")?;
    if !output.status.success() {
        bail!(
            "git {}: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(output.stdout)
}

fn git_text(dir: &Path, args: &[&str]) -> anyhow::Result<String> {
    Ok(String::from_utf8(git(dir, args)?)?.trim().to_string())
}

/// What the working tree changes relative to the baseline: the swap's
/// functions and the changed files to upload.
struct LocalChange {
    functions: Vec<SwapFunction>,
    /// Functions changed only through a changed client.
    affects: Vec<String>,
    files: Vec<(SourceFile, Vec<u8>)>,
}

/// Diff the working tree against `baseline` with git, and refuse anything
/// but a change to marked functions (and the clients they use).
fn local_change(root: &Path, baseline: &str) -> anyhow::Result<LocalChange> {
    let repo = PathBuf::from(git_text(root, &["rev-parse", "--show-toplevel"])?);
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let repo = repo.canonicalize().unwrap_or(repo);
    let current =
        crate::hot_reload::read_sources(&crate::hot_reload::HotFiles::Project(root.clone()))
            .context("failed to read the project's sources")?;
    // Project-relative path to repository-relative path.
    let in_repo = |path: &str| {
        root.join(path)
            .strip_prefix(&repo)
            .map(|path| path.to_string_lossy().replace('\\', "/"))
            .unwrap_or_else(|_| path.to_string())
    };
    let mut before = BTreeMap::new();
    let mut changed_files = Vec::new();
    for (path, text) in &current {
        let built = git(&repo, &["show", &format!("{baseline}:{}", in_repo(path))]).ok();
        match built.map(String::from_utf8) {
            Some(Ok(built)) => {
                if &built != text {
                    changed_files.push(path.clone());
                }
                before.insert(path.clone(), built);
            }
            _ => bail!(
                "`{path}` is new since {}, and only swappable functions can change without a deploy",
                short(baseline)
            ),
        }
    }
    // A file deleted since the baseline is a deleted declaration.
    let listed = git_text(
        &repo,
        &["ls-tree", "-r", "--name-only", baseline, "--", &{
            let scope = in_repo(".");
            let scope = scope.trim_end_matches('.').trim_end_matches('/');
            if scope.is_empty() {
                ".".to_string()
            } else {
                scope.to_string()
            }
        }],
    )?;
    for path in listed.lines().filter(|path| path.ends_with(".baml")) {
        let absolute = repo.join(path);
        if let Ok(relative) = absolute.strip_prefix(&root) {
            let relative = relative.to_string_lossy().replace('\\', "/");
            if !current.contains_key(&relative) {
                bail!(
                    "`{relative}` was deleted since {}, which needs a deploy",
                    short(baseline)
                );
            }
        }
    }
    let as_iter = |files: &BTreeMap<String, String>| {
        files
            .iter()
            .map(|(path, text)| (path.clone(), text.clone()))
            .collect::<Vec<_>>()
    };
    let (before_files, after_files) = (as_iter(&before), as_iter(&current));
    let (old_fingerprints, old) = bex_project::hotswap_fingerprint::declarations(
        before_files.iter().map(|(p, t)| (p.as_str(), t.as_str())),
    );
    let (new_fingerprints, new) = bex_project::hotswap_fingerprint::declarations(
        after_files.iter().map(|(p, t)| (p.as_str(), t.as_str())),
    );
    let config = crate::runtime_telemetry::hotswap_config(&root)?;
    let is_function = |key: &str| {
        key.starts_with("fn ") || key.starts_with("method ") || key.starts_with("implmethod ")
    };
    let marked = |text: &str| matches!(bex_project::hotswap_rules::mark_of(text), Some(Ok(_)));
    let label = |key: &str| {
        key.split_once(' ')
            .map_or(key, |(_, name)| name)
            .to_string()
    };
    let mut functions = Vec::new();
    let keys: std::collections::BTreeSet<&String> = old_fingerprints
        .decls
        .keys()
        .chain(new_fingerprints.decls.keys())
        .collect();
    for key in keys {
        if old_fingerprints.decls.get(key) == new_fingerprints.decls.get(key) {
            continue;
        }
        let name = label(key);
        let refused = || {
            anyhow::anyhow!(
                "`{name}` changed, and only swappable functions can change without a deploy"
            )
        };
        let (Some(originals), Some(replacements)) = (old.get(key), new.get(key)) else {
            return Err(refused());
        };
        if originals.len() != replacements.len() {
            return Err(refused());
        }
        // Implementations can share a key: each changed one is its own entry.
        for (original, replacement) in originals.iter().zip(replacements) {
            if original.text == replacement.text {
                continue;
            }
            // A client's configuration counts as a change to each function
            // that uses it; the processes check those functions' marks and rules.
            let rule = if key.starts_with("client ") {
                String::new()
            } else if is_function(key) && marked(&original.text) {
                match bex_project::hotswap_rules::mark_of(&original.text) {
                    Some(Ok(mark)) => mark
                        .rule
                        .or_else(|| config.default_rule.clone())
                        .map(|rule| rule.name().to_string())
                        .unwrap_or_default(),
                    _ => String::new(),
                }
            } else {
                return Err(refused());
            };
            functions.push(SwapFunction {
                name: name.clone(),
                file: original.file.clone(),
                rule,
                old_source: original.text.clone(),
                new_source: replacement.text.clone(),
            });
        }
    }
    // Every function a changed client reaches must be swappable too.
    let mut affects = Vec::new();
    for name in bex_hotswap::hotswap_changes(&old_fingerprints, &new_fingerprints) {
        let swappable = old.iter().any(|(key, sources)| {
            label(key) == name
                && is_function(key)
                && sources.iter().all(|source| marked(&source.text))
        });
        if !swappable {
            bail!("`{name}` changed, and only swappable functions can change without a deploy");
        }
        if functions.iter().all(|function| function.name != name) {
            affects.push(name);
        }
    }
    if functions.is_empty() {
        bail!(
            "nothing to swap: no marked function changed since {}",
            short(baseline)
        );
    }
    let files = changed_files
        .into_iter()
        .map(|path| {
            let bytes = current[&path].clone().into_bytes();
            let sha256 = sha256_hex(&bytes);
            let size = bytes.len() as u64;
            (SourceFile { path, sha256, size }, bytes)
        })
        .collect();
    Ok(LocalChange {
        functions,
        affects,
        files,
    })
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest as _, Sha256};
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn short(commit: &str) -> &str {
    &commit[..commit.len().min(7)]
}

/// Compile the working tree, only to catch errors before anything uploads.
fn compile_check(root: &Path) -> anyhow::Result<()> {
    let session = crate::project_session::ProjectSession::open(
        Some(root),
        crate::project_session::CacheUse::Off,
    )?;
    let diagnostics = baml_db::ProjectDiagnostics::collect(&session.db);
    if baml_db::CheckedWorld::check(&diagnostics, session.package).is_err() {
        let rendered = crate::check_command::render_project_diagnostics(&session.db, &diagnostics);
        #[allow(clippy::print_stderr, reason = "compile errors go to stderr")]
        {
            eprintln!("{rendered}");
        }
        bail!("the change does not compile");
    }
    Ok(())
}

/// Follow `id`'s events after `after` until `stop` says so; returns the
/// sequence of the last one read.
fn follow(
    client: &Client,
    authorization: &RequestAuthorization,
    id: &str,
    after: u64,
    mut stop: impl FnMut(&HotswapEvent) -> bool,
) -> anyhow::Result<u64> {
    let mut last = after;
    // A stream that ends early, such as on a dropped connection, resumes
    // after the last event read.
    loop {
        let mut stopped = false;
        client.hotswap_events(authorization, id, last, true, |event| {
            last = event.sequence;
            stopped = stop(&event);
            Ok(!stopped)
        })?;
        if stopped {
            return Ok(last);
        }
        std::thread::sleep(std::time::Duration::from_millis(250));
    }
}

fn create(
    root: &Path,
    client: &Client,
    authorization: &RequestAuthorization,
    args: &CreateArgs,
) -> anyhow::Result<Outcome<HotswapReport>> {
    let query = match args.query.as_deref() {
        Some("-") => {
            let mut text = String::new();
            std::io::stdin()
                .read_to_string(&mut text)
                .context("failed to read the query from stdin")?;
            Some(text.trim().to_string())
        }
        Some(query) => Some(query.to_string()),
        None => None,
    };
    match (&query, args.processes.is_empty()) {
        (Some(_), false) => bail!("pass a query or --process IDs, not both"),
        (None, true) => bail!("pass a query that selects processes, or --process IDs"),
        _ => {}
    }
    let baseline = git_text(
        root,
        &[
            "rev-parse",
            "--verify",
            &format!("{}^{{commit}}", args.baseline),
        ],
    )
    .with_context(|| format!("`{}` is no git commit", args.baseline))?;
    let config = crate::runtime_telemetry::hotswap_config(root)?;
    let created = client.create_hotswap(
        authorization,
        &CreateHotswap {
            baseline: baseline.clone(),
            query,
            process_ids: (!args.processes.is_empty()).then(|| args.processes.clone()),
            replaces: args.replace.clone(),
            require_approval: config.require_approval,
        },
    )?;
    let id = created.id.clone();
    status("Created", &id);
    detail("Ctrl-C is safe: the swap continues without this command");

    // Check locally while BCS runs the query. A swap the CLI refuses is
    // deleted, so nothing is left to approve.
    let local = local_change(root, &baseline).and_then(|change| {
        compile_check(root)?;
        Ok(change)
    });
    let change = match local {
        Ok(change) => change,
        Err(error) => {
            let _ = client.delete_hotswap(authorization, &id);
            return Err(error);
        }
    };

    let mut after = follow(client, authorization, &id, 0, |event| {
        if event.status == "matched" {
            status(
                "Matched",
                format!(
                    "{} processes on {}, skipped {} on other commits",
                    event.processes.unwrap_or(0),
                    short(&baseline),
                    event.skipped.unwrap_or(0)
                ),
            );
            return true;
        }
        matches!(event.status.as_str(), "deleted" | "rejected")
    })?;
    for function in &change.functions {
        if function.rule.is_empty() {
            status("Changed", &function.name);
        } else {
            status(
                "Changed",
                format!("{}, rule {}", function.name, function.rule),
            );
        }
    }
    status("Checked", format!("against {}", short(&baseline)));

    let sources: Vec<SourceFile> = change.files.iter().map(|(file, _)| file.clone()).collect();
    let prepared = client.prepare_hotswap_sources(authorization, &id, &sources)?;
    let (mut uploaded, mut bytes) = (0usize, 0u64);
    for (file, content) in &change.files {
        let Some(target) = prepared.files.iter().find(|p| p.sha256 == file.sha256) else {
            continue;
        };
        if let (Some(url), "upload") = (&target.upload_url, target.disposition.as_str()) {
            client.upload_hotswap_source(authorization, url, content)?;
            uploaded += 1;
            bytes += file.size;
        }
    }
    let available = sources.len() - uploaded;
    status(
        "Uploaded",
        format!(
            "{uploaded} file{} ({:.1} KB), {available} already in BCS",
            if uploaded == 1 { "" } else { "s" },
            bytes as f64 / 1024.0
        ),
    );
    let submitted = client.submit_hotswap_payload(
        authorization,
        &id,
        &HotswapPayload {
            functions: change.functions,
            files: sources,
            affects: change.affects,
        },
    )?;
    if let Some(rejection) = &submitted.rejection {
        crate::reporter::print_error(&rejection.reason);
        if let Some(conflict) = rejection.conflicts.first() {
            #[allow(clippy::print_stderr, reason = "hints go to stderr")]
            {
                eprintln!(
                    "hint: merge its change into yours, then pass --replace {}",
                    conflict.hotswap_id
                );
            }
        }
        return Ok(Outcome::new(
            HotswapReport::Row(Row::from(&submitted)),
            ExitCode::Failure,
        ));
    }
    if args.no_wait {
        return Ok(Outcome::ok(HotswapReport::Row(Row::from(&submitted))));
    }

    // Follow the rollout until every matched process has reported.
    let terminal = std::io::stderr().is_terminal() && !crate::reporter::quiet();
    let (mut installed, mut rejected, mut matched) = (0u64, 0u64, submitted.processes.matched);
    let mut reasons: Vec<String> = Vec::new();
    let progress = |installed: u64, matched: u64| {
        if terminal {
            let painter = Painter::stderr();
            #[allow(clippy::print_stderr, reason = "status goes to stderr")]
            {
                eprint!(
                    "\r{} {installed}/{matched} processes",
                    painter.accent(format!("{:>12}", "Installing"))
                );
                let _ = std::io::stderr().flush();
            }
        }
    };
    let mut done = false;
    after = follow(client, authorization, &id, after, |event| {
        match (event.process.is_some(), event.status.as_str()) {
            (false, "pending_approval") => status("Waiting", "for approval"),
            (false, "approved") => match &event.by {
                Some(by) => status("Approved", format!("by {by}")),
                None => status("Approved", "without review, as baml.toml allows"),
            },
            (false, "matched") => matched = event.processes.unwrap_or(matched),
            (true, "installed") => {
                installed += 1;
                progress(installed, matched);
            }
            (true, "rejected") => {
                rejected += 1;
                if let (Some(process), Some(reason)) = (&event.process, &event.reason)
                    && !reasons.contains(reason)
                {
                    let _ = process;
                    reasons.push(reason.clone());
                }
            }
            (false, "installed" | "rejected") => {
                installed = event.installed.unwrap_or(installed);
                rejected = event.rejected.unwrap_or(rejected);
                done = true;
                return true;
            }
            (false, "disabled" | "deleted") => {
                done = true;
                return true;
            }
            _ => {}
        }
        false
    })?;
    let _ = after;
    if terminal {
        #[allow(clippy::print_stderr, reason = "status goes to stderr")]
        {
            eprint!("\r");
        }
    }
    let final_state = client.hotswap(authorization, &id)?;
    let total = final_state.processes.matched;
    if rejected > 0 {
        status("Rejected", format!("{rejected}/{total} processes"));
        for reason in &reasons {
            for line in reason.lines() {
                detail(line);
            }
        }
    }
    if installed > 0 || rejected == 0 {
        status("Installed", format!("{installed}/{total} processes"));
    }
    let failed = !done || rejected > 0 || final_state.status != "installed";
    Ok(Outcome::new(
        HotswapReport::Row(Row::from(&final_state)),
        if failed {
            ExitCode::Failure
        } else {
            ExitCode::Success
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::diff_lines;

    #[test]
    fn a_diff_lists_only_the_changed_lines() {
        let old = "function S(doc: string) -> Summary {\n  client: Writer\n  prompt: `a ${doc}`\n}";
        let new = old.replace("`a ${doc}`", "`b ${doc}`");
        assert_eq!(
            diff_lines(old, &new),
            ["-  prompt: `a ${doc}`", "+  prompt: `b ${doc}`"]
        );
    }
}
