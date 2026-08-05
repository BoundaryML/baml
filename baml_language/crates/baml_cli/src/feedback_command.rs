// `baml feedback` — report an issue or improvement to Boundary, and manage
// past reports (gh-issues style: `status`, `list`, `view`, `disable`).
//
// Identity is PostHog-first, and sending costs no interaction: with no
// session, reports go out anonymously under a locally generated PostHog
// distinct id (persisted in `~/.baml/creds.json`). After `baml auth login`,
// reports carry the verified email instead. No prompt either way.
//
// When an anonymous reporter later logs in, a `$identify` event merges the
// anonymous person into the identified one — PostHog's native person merge —
// so every previously filed report is retroactively attributed. There is no
// backfill job; the merge *is* the backfill.
//
// Every report is also recorded in `<BAML_HOME>/feedback.json`, which is
// what `status`/`list`/`view` read: the embedded PostHog key is write-only,
// so past reports cannot be queried back from the server. Records start as
// `open` and flip to `anonymous`/`reported` once delivered, so an offline
// send is saved locally and synced on a later run instead of failing.

// `println!` here is the primary UX of the command: data output (previews,
// confirmations, status/list/view) belongs on stdout. Diagnostics go
// through `crate::reporter`. The workspace-wide ban on `print*!` exists to
// catch stray debug prints, not to break intentional user-facing output.
#![allow(clippy::print_stdout)]

use anyhow::{Context, Result};
use clap::{Args, Subcommand, ValueEnum};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::auth::{self, Credentials};

const FEEDBACK_EVENT: &str = "baml_feedback";

/// Cap on deferred deliveries attempted per invocation, so a long-offline
/// backlog can't turn one command into hundreds of blocking posts. The
/// remainder syncs on later runs.
const MAX_SYNC_PER_RUN: usize = 25;

/// PostHog ingestion host, overridable for tests and self-hosted setups.
fn posthog_host() -> String {
    std::env::var("BAML_POSTHOG_HOST")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| crate::telemetry::posthog_host().to_string())
}

/// Wrapper existing only to fix clap's derived `update_from_arg_matches`,
/// which errors with "a subcommand is required" whenever the optional
/// subcommand is absent (so plain `baml feedback --title ...` would die in
/// `parse_from_smart`'s update pass). The manual impl replays a fresh parse
/// instead.
#[derive(Debug)]
pub(crate) struct FeedbackArgs(FeedbackInner);

impl FeedbackArgs {
    pub fn run(&self) -> Result<crate::ExitCode> {
        self.0.run()
    }
}

impl clap::FromArgMatches for FeedbackArgs {
    fn from_arg_matches(matches: &clap::ArgMatches) -> Result<Self, clap::Error> {
        FeedbackInner::from_arg_matches(matches).map(Self)
    }
    fn update_from_arg_matches(&mut self, matches: &clap::ArgMatches) -> Result<(), clap::Error> {
        self.0 = FeedbackInner::from_arg_matches(matches)?;
        Ok(())
    }
}

impl Args for FeedbackArgs {
    fn augment_args(cmd: clap::Command) -> clap::Command {
        FeedbackInner::augment_args(cmd)
    }
    fn augment_args_for_update(cmd: clap::Command) -> clap::Command {
        FeedbackInner::augment_args_for_update(cmd)
    }
}

#[derive(Args, Debug)]
#[command(args_conflicts_with_subcommands = true)]
#[command(after_long_help = "\
Examples:
  Report an issue:
    baml feedback --title \"Issue (parser): panics on nested unions\"

  Include a description with a minimum repro:
    baml feedback --title \"...\" --description \"Minimum repro: class A { ... }\"

  Submit a JSON report from standard input:
    echo '{\"title\": \"...\", \"description\": \"...\"}' | baml feedback -

  Attach files:
    baml feedback --title \"...\" --files screenshot.png --files repro.baml

  List undelivered reports:
    baml feedback list --status open

  View one report:
    baml feedback view a1b2c3d4")]
struct FeedbackInner {
    #[command(subcommand)]
    pub action: Option<FeedbackAction>,

    /// One line describing the issue, in the form `Issue (feature): description`.
    #[arg(long)]
    pub title: Option<String>,

    /// Anything relevant to help the BAML team understand your
    /// problem/suggestion; include a minimum repro.
    ///
    /// Good descriptions cover: what I was doing, what went wrong, a
    /// minimum repro (include one whenever possible), what I want to
    /// happen, and potential syntax ideas.
    #[arg(long)]
    pub description: Option<String>,

    /// Attach a file (image, code, log).
    /// Repeatable: `--files a.png --files repro.baml`.
    #[arg(long = "files", value_name = "PATH", action = clap::ArgAction::Append)]
    pub files: Vec<std::path::PathBuf>,

    /// Advanced: supply the fields as JSON instead of flags (an inline
    /// object, a file path, or `-` for stdin).
    #[arg(value_name = "JSON", hide_short_help = true)]
    pub input: Option<String>,

    /// Report anonymously even while logged in.
    #[arg(long, conflicts_with = "email")]
    pub anonymous: bool,

    /// Report with your email (requires `baml auth login`).
    #[arg(long)]
    pub email: bool,
}

#[derive(Subcommand, Debug)]
pub(crate) enum FeedbackAction {
    /// Show whether feedback is enabled and the reports sent so far.
    Status,
    /// List past reports.
    List {
        /// Only show reports with this status.
        #[arg(long, value_enum)]
        status: Option<ReportStatus>,
        /// Show at most this many reports (newest last).
        #[arg(long)]
        limit: Option<usize>,
        /// Output the matching records as JSON.
        #[arg(long)]
        json: bool,
    },
    /// View one past report in full.
    View {
        /// The report id (from `baml feedback list`).
        id: String,
        /// Output the record as JSON.
        #[arg(long)]
        json: bool,
    },
    /// Disable sending feedback from this machine.
    Disable,
    /// Re-enable sending feedback.
    Enable,
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum ReportStatus {
    /// Recorded locally but not yet delivered (e.g. sent while offline).
    Open,
    /// Delivered to Boundary without an email.
    Anonymous,
    /// Delivered to Boundary with a verified email.
    Reported,
}

impl std::fmt::Display for ReportStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            ReportStatus::Open => "open",
            ReportStatus::Anonymous => "anonymous",
            ReportStatus::Reported => "reported",
        })
    }
}

impl FeedbackInner {
    /// Runs `baml feedback` and its subcommands.
    ///
    /// Without a subcommand, collects the payload and sends it: with the
    /// verified email when logged in (see `baml auth login`), anonymously
    /// under the persistent PostHog distinct id otherwise. Any invocation
    /// first retries `open` (undelivered) reports best-effort.
    ///
    /// Returns:
    /// - `ExitCode::Success` when the action completes; a send that cannot
    ///   reach PostHog still succeeds by saving the report as `open`.
    /// - `ExitCode::Other` (with guidance on stderr) when `--email` is used
    ///   without a login, when feedback is disabled, or when a `view` id is
    ///   unknown.
    ///
    /// Errors:
    /// - On a missing/malformed payload, an unreadable attachment, or a
    ///   corrupt local store.
    pub fn run(&self) -> Result<crate::ExitCode> {
        let mut store = FeedbackStore::load()?;

        // Best-effort delivery of anything still `open` before every action
        // except the enabled-flag flips: `status`/`list`/`view` should show
        // fresh state, and a send should not overtake older reports.
        if store.enabled
            && !matches!(
                self.action,
                Some(FeedbackAction::Disable) | Some(FeedbackAction::Enable)
            )
        {
            sync_open_reports(&mut store);
        }

        match &self.action {
            Some(FeedbackAction::Status) => return run_status(&store),
            Some(FeedbackAction::List {
                status,
                limit,
                json,
            }) => return run_list(&store, *status, *limit, *json),
            Some(FeedbackAction::View { id, json }) => return run_view(&store, id, *json),
            Some(FeedbackAction::Disable) => return run_set_enabled(&mut store, false),
            Some(FeedbackAction::Enable) => return run_set_enabled(&mut store, true),
            None => {}
        }

        if !store.enabled {
            crate::reporter::print_error(
                "feedback is disabled on this machine; run `baml feedback enable` to turn it back on",
            );
            return Ok(crate::ExitCode::Other);
        }

        let mut payload = self.payload()?;
        let attachments = encode_attachments(&self.files)?;
        if !attachments.is_empty() {
            payload["files"] = files_event_json(&attachments);
        }

        let mut creds = Credentials::read()?.unwrap_or_default();

        // Resolve identity: email when logged in, anonymous otherwise. No
        // prompt — reporting should never cost an interaction.
        let identified = if self.anonymous {
            false
        } else if creds.user_email.is_some() {
            true
        } else if self.email {
            // Non-interactive email mode without a session: instruct and
            // exit rather than trying to run an interactive login under a
            // flag that suggests automation.
            crate::reporter::print_error(
                "reporting via email requires a login; run `baml auth login` and re-run this command",
            );
            return Ok(crate::ExitCode::Other);
        } else {
            false
        };

        self.print_preview(&payload, identified, creds.user_email.as_deref());

        // A stable distinct id makes every report from this machine one
        // PostHog person — and is what a later login merges into the
        // identified person.
        //
        // Exception: choosing anonymous while a login exists. The stored id
        // has already been merged into the identified person, so sending
        // under it would attribute the report anyway. Honor the choice with
        // a one-shot, unpersisted id instead.
        let distinct_id = if !identified && creds.user_email.is_some() {
            uuid::Uuid::new_v4().to_string()
        } else {
            if creds.posthog_distinct_id.is_none() {
                creds.posthog_distinct_id = Some(uuid::Uuid::new_v4().to_string());
            }
            creds.write()?;
            creds.posthog_distinct_id.clone().expect("set above")
        };

        // Record first as `open`, then flip on delivery: an offline send is
        // saved rather than lost.
        let record = FeedbackRecord::new(
            &payload,
            attachments,
            identified,
            self.anonymous,
            creds.user_email.as_deref(),
        );
        let short_id = record.id.clone();
        let report_id = record.event_uuid;
        store.reports.push(record);
        store.save()?;

        match send_feedback(
            &creds,
            &distinct_id,
            &payload,
            identified,
            &report_id.to_string(),
        ) {
            Ok(()) => {
                let delivered_status = if identified {
                    ReportStatus::Reported
                } else {
                    ReportStatus::Anonymous
                };
                let delivered_email = identified.then(|| creds.user_email.clone()).flatten();
                if let Some(r) = store.reports.iter_mut().find(|r| r.event_uuid == report_id) {
                    r.mark_delivered(delivered_status, delivered_email.as_deref());
                }
                store.save()?;
                if identified {
                    println!(
                        "feedback {} sent as {}; you'll be notified when this is addressed",
                        short_id,
                        creds.user_email.as_deref().unwrap_or("your account")
                    );
                } else {
                    println!("feedback {short_id} sent anonymously");
                    if creds.user_email.is_none() {
                        println!(
                            "run `baml auth login` to be notified when this is \
                             fixed; past reports come with you"
                        );
                    }
                }
            }
            Err(_) => {
                println!(
                    "could not reach Boundary; feedback {short_id} saved locally \
                     and will sync on the next `baml feedback` run"
                );
            }
        }
        Ok(crate::ExitCode::Success)
    }

    /// Assembles `{title, description?}` from flags and/or JSON input.
    /// Flags win over JSON fields when both are given.
    fn payload(&self) -> Result<Value> {
        let mut from_json = match self.input.as_deref() {
            None => json!({}),
            Some("-") => {
                let mut buf = String::new();
                use std::io::Read as _;
                std::io::stdin()
                    .read_to_string(&mut buf)
                    .context("Failed to read stdin")?;
                parse_payload_json(&buf)?
            }
            Some(inline) if inline.trim_start().starts_with('{') => parse_payload_json(inline)?,
            Some(path) => {
                let content = std::fs::read_to_string(path)
                    .with_context(|| format!("Failed to read {path}"))?;
                parse_payload_json(&content)?
            }
        };

        let obj = from_json
            .as_object_mut()
            .expect("parse_payload_json returns an object");
        for (key, flag) in [("title", &self.title), ("description", &self.description)] {
            if let Some(value) = flag {
                obj.insert(key.to_string(), Value::String(value.clone()));
            }
        }

        // Whitelist: only the documented fields ship. Anything else in a
        // piped payload (stray script fields, PostHog-special keys like
        // `$set`) is rejected rather than silently transmitted. `files` is
        // deliberately NOT accepted from JSON: a piped payload naming local
        // paths would let untrusted payload content exfiltrate arbitrary
        // files. Attachments come only from the `--files` flag (trusted CLI
        // input).
        let unknown: Vec<&str> = obj
            .keys()
            .map(String::as_str)
            .filter(|k| !matches!(*k, "title" | "description"))
            .collect();
        if !unknown.is_empty() {
            return Err(anyhow::anyhow!(
                "unknown feedback field(s) {}; only \"title\" and \
                 \"description\" are sent (attach files with --files)",
                unknown.join(", ")
            ));
        }

        // Validate types, not just names: a non-string field would ship to
        // PostHog while the preview and the local record (which read via
        // `as_str`) silently dropped it.
        for key in ["title", "description"] {
            if let Some(value) = obj.get(key)
                && !value.is_string()
            {
                return Err(anyhow::anyhow!("feedback field \"{key}\" must be a string"));
            }
        }

        let title = obj.get("title").and_then(Value::as_str).unwrap_or("");
        if title.trim().is_empty() {
            return Err(anyhow::anyhow!(
                "feedback needs a title; pass --title \"...\" or a JSON payload \
                 with a \"title\" field"
            ));
        }
        Ok(from_json)
    }

    /// Shows exactly what a report contains and how it is attributed.
    fn print_preview(&self, payload: &Value, identified: bool, email: Option<&str>) {
        if identified {
            println!(
                "reporting to Boundary as {}:",
                email.unwrap_or("your account")
            );
        } else {
            println!("reporting to Boundary anonymously:");
        }
        println!();
        for (label, key) in [("Title", "title"), ("Description", "description")] {
            if let Some(text) = payload.get(key).and_then(Value::as_str) {
                println!("  {label}: {text}");
            }
        }
        if let Some(files) = payload.get("files").and_then(Value::as_array) {
            let listed: Vec<String> = files
                .iter()
                .map(|f| {
                    format!(
                        "{} ({} bytes)",
                        f["name"].as_str().unwrap_or("?"),
                        f["size_bytes"].as_u64().unwrap_or(0)
                    )
                })
                .collect();
            println!("  Files: {}", listed.join(", "));
        }
        println!(
            "  (plus: cli version {}, {}/{})",
            baml_version::CANONICAL_VERSION,
            std::env::consts::OS,
            std::env::consts::ARCH
        );
        println!();
    }
}

/// Reads attachments into [`StoredFile`]s (raw bytes; base64 happens at
/// serialization time, both for the event payload and the local store).
fn encode_attachments(paths: &[std::path::PathBuf]) -> Result<Vec<StoredFile>> {
    let mut files = Vec::new();
    for path in paths {
        let bytes = std::fs::read(path)
            .with_context(|| format!("failed to read attachment {}", path.display()))?;
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.display().to_string());
        // mime_guess knows the registry; for extensions it doesn't (code
        // files like .baml), fall back on the content: valid UTF-8 is text.
        let mime = mime_guess::from_path(path).first().unwrap_or_else(|| {
            if std::str::from_utf8(&bytes).is_ok() {
                mime_guess::mime::TEXT_PLAIN
            } else {
                mime_guess::mime::APPLICATION_OCTET_STREAM
            }
        });
        files.push(StoredFile {
            name,
            mime: mime.essence_str().to_string(),
            size_bytes: bytes.len() as u64,
            content: Some(bytes),
        });
    }
    Ok(files)
}

/// The `files` array as it appears in the PostHog event payload.
fn files_event_json(files: &[StoredFile]) -> Value {
    use base64::Engine as _;
    json!(
        files
            .iter()
            .map(|f| json!({
                "name": f.name,
                "mime": f.mime,
                "size_bytes": f.size_bytes,
                "content_base64": base64::engine::general_purpose::STANDARD
                    .encode(f.content.as_deref().unwrap_or_default()),
            }))
            .collect::<Vec<_>>()
    )
}

fn parse_payload_json(raw: &str) -> Result<Value> {
    let value: Value = serde_json::from_str(raw).context("feedback payload is not valid JSON")?;
    if !value.is_object() {
        return Err(anyhow::anyhow!(
            "feedback payload must be a JSON object like {{\"title\": \"...\"}}"
        ));
    }
    Ok(value)
}

// ---------------------------------------------------------------------------
// Local report store: <BAML_HOME>/feedback.json
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, Serialize)]
struct FeedbackStore {
    /// Whether `baml feedback` sends anything from this machine.
    #[serde(default = "default_true")]
    enabled: bool,
    /// Reports from this machine, oldest first.
    #[serde(default)]
    reports: Vec<FeedbackRecord>,
}

fn default_true() -> bool {
    true
}

impl Default for FeedbackStore {
    fn default() -> Self {
        Self {
            enabled: true,
            reports: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct FeedbackRecord {
    /// Short id shown in `list`/`view` (a prefix of `event_uuid`).
    id: String,
    /// Full uuid, sent as the `report_id` event property so server-side
    /// state can be joined to this record later.
    event_uuid: uuid::Uuid,
    title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    status: ReportStatus,
    /// The user explicitly asked for anonymity (`--anonymous`). A deferred
    /// delivery must honor it even if a login exists by sync time.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    forced_anonymous: bool,
    /// The verified email the report was (or will be) attributed to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    email: Option<String>,
    /// Attachments. Content is kept while `open` (a deferred delivery must
    /// ship it) and dropped once delivered, leaving only the
    /// name/mime/size metadata for `view`.
    #[serde(default)]
    files: Vec<StoredFile>,
    /// Unix seconds.
    created_at: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct StoredFile {
    name: String,
    mime: String,
    size_bytes: u64,
    /// Raw bytes in memory; serialized as base64 (field `content_base64`)
    /// so the store stays valid JSON.
    #[serde(
        default,
        rename = "content_base64",
        skip_serializing_if = "Option::is_none",
        with = "opt_base64"
    )]
    content: Option<Vec<u8>>,
}

/// Serde adapter: `Option<Vec<u8>>` <-> optional base64 string.
mod opt_base64 {
    use base64::Engine as _;
    use serde::{Deserialize as _, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(v: &Option<Vec<u8>>, s: S) -> Result<S::Ok, S::Error> {
        match v {
            Some(bytes) => {
                s.serialize_str(&base64::engine::general_purpose::STANDARD.encode(bytes))
            }
            None => s.serialize_none(),
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Vec<u8>>, D::Error> {
        let raw: Option<String> = Option::deserialize(d)?;
        raw.map(|text| {
            base64::engine::general_purpose::STANDARD
                .decode(text)
                .map_err(serde::de::Error::custom)
        })
        .transpose()
    }
}

impl FeedbackRecord {
    fn new(
        payload: &Value,
        files: Vec<StoredFile>,
        identified: bool,
        forced_anonymous: bool,
        email: Option<&str>,
    ) -> Self {
        let event_uuid = uuid::Uuid::new_v4();
        Self {
            id: event_uuid.to_string()[..8].to_string(),
            event_uuid,
            title: payload
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            description: payload
                .get("description")
                .and_then(Value::as_str)
                .map(str::to_string),
            status: ReportStatus::Open,
            forced_anonymous,
            email: if identified {
                email.map(str::to_string)
            } else {
                None
            },
            files,
            created_at: auth::now_unix(),
        }
    }

    /// Rebuilds the event payload for a deferred (`open`) delivery.
    fn payload(&self) -> Value {
        let mut payload = json!({ "title": self.title });
        if let Some(description) = &self.description {
            payload["description"] = json!(description);
        }
        if !self.files.is_empty() {
            payload["files"] = files_event_json(&self.files);
        }
        payload
    }

    /// Marks the record delivered: sets the final status/email and drops
    /// attachment bytes (the content has shipped; only name/mime/size
    /// metadata stays for `view`).
    fn mark_delivered(&mut self, status: ReportStatus, email: Option<&str>) {
        self.status = status;
        self.email = email.map(str::to_string);
        for file in &mut self.files {
            file.content = None;
        }
    }
}

impl FeedbackStore {
    fn path() -> std::path::PathBuf {
        baml_release::baml_home().join("feedback.json")
    }

    fn load() -> Result<Self> {
        let path = Self::path();
        match std::fs::read_to_string(&path) {
            Ok(raw) => serde_json::from_str(&raw)
                .with_context(|| format!("Malformed feedback store in {}", path.display())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e).with_context(|| format!("Failed to read {}", path.display())),
        }
    }

    fn save(&self) -> Result<()> {
        // Owner-only, like creds.json: delivered records carry the verified
        // email, and descriptions often carry environment details.
        auth::write_owner_only(&Self::path(), &serde_json::to_string(self)?)
    }
}

/// Attempts to deliver every `open` report with the current identity
/// (email when logged in, the persistent anonymous id otherwise).
/// Best-effort: failures leave the record `open` for the next run.
fn sync_open_reports(store: &mut FeedbackStore) {
    if !store.reports.iter().any(|r| r.status == ReportStatus::Open) {
        return;
    }
    let Ok(mut creds) = Credentials::read().map(Option::unwrap_or_default) else {
        return;
    };
    let identified = creds.user_email.is_some();
    if creds.posthog_distinct_id.is_none() {
        creds.posthog_distinct_id = Some(uuid::Uuid::new_v4().to_string());
        if creds.write().is_err() {
            return;
        }
    }
    let distinct_id = creds.posthog_distinct_id.clone().expect("set above");

    let mut changed = false;
    let mut delivered = 0usize;
    for record in &mut store.reports {
        if record.status != ReportStatus::Open {
            continue;
        }
        if delivered >= MAX_SYNC_PER_RUN {
            break;
        }
        // A record filed with --anonymous stays anonymous even if a login
        // exists by sync time: one-shot id, no email (mirrors the live send
        // path's post-login anonymity exception).
        let (record_identified, record_distinct_id) = if record.forced_anonymous {
            (false, uuid::Uuid::new_v4().to_string())
        } else {
            (identified, distinct_id.clone())
        };
        if send_feedback(
            &creds,
            &record_distinct_id,
            &record.payload(),
            record_identified,
            &record.event_uuid.to_string(),
        )
        .is_err()
        {
            // Still unreachable; keep it open and stop hammering.
            break;
        }
        let status = if record_identified {
            ReportStatus::Reported
        } else {
            ReportStatus::Anonymous
        };
        let email = record_identified
            .then(|| creds.user_email.clone())
            .flatten();
        record.mark_delivered(status, email.as_deref());
        changed = true;
        delivered += 1;
    }
    if changed && let Err(e) = store.save() {
        // Delivered statuses were lost: the same reports will re-send on the
        // next run (server-side dedupe is possible via report_id). Surface it
        // rather than looping silently forever.
        crate::reporter::print_warning(format!("failed to update the feedback store: {e:#}"));
    }
}

fn run_status(store: &FeedbackStore) -> Result<crate::ExitCode> {
    println!("BAML Feedback");
    println!();
    println!(
        "Status: {}",
        if store.enabled { "Enabled" } else { "Disabled" }
    );
    println!("Store: {}", FeedbackStore::path().display());
    println!();
    if store.reports.is_empty() {
        println!("no reports sent from this machine yet");
    } else {
        println!("Reports:");
        for r in &store.reports {
            println!("  [{}] {}: {}", r.status, r.id, r.title);
        }
    }
    Ok(crate::ExitCode::Success)
}

fn run_list(
    store: &FeedbackStore,
    status: Option<ReportStatus>,
    limit: Option<usize>,
    json: bool,
) -> Result<crate::ExitCode> {
    let matching: Vec<&FeedbackRecord> = store
        .reports
        .iter()
        .filter(|r| status.is_none_or(|s| r.status == s))
        .collect();
    let start = limit.map_or(0, |l| matching.len().saturating_sub(l));
    let matching = &matching[start..];

    if json {
        println!("{}", serde_json::to_string_pretty(&matching)?);
        return Ok(crate::ExitCode::Success);
    }
    if matching.is_empty() {
        println!("no matching reports");
        return Ok(crate::ExitCode::Success);
    }
    for r in matching {
        println!("{}\t{}\t{}\t{}", r.id, r.status, r.created_at, r.title);
    }
    Ok(crate::ExitCode::Success)
}

fn run_view(store: &FeedbackStore, id: &str, json: bool) -> Result<crate::ExitCode> {
    let Some(r) = store.reports.iter().find(|r| r.id == id) else {
        crate::reporter::print_error(format!("no report with id {id}; see `baml feedback list`"));
        return Ok(crate::ExitCode::Other);
    };
    if json {
        println!("{}", serde_json::to_string_pretty(r)?);
        return Ok(crate::ExitCode::Success);
    }
    println!("Id:          {}", r.id);
    println!("Title:       {}", r.title);
    if let Some(description) = &r.description {
        println!("Description: {description}");
    }
    if !r.files.is_empty() {
        let listed: Vec<String> = r
            .files
            .iter()
            .map(|f| format!("{} ({}, {} bytes)", f.name, f.mime, f.size_bytes))
            .collect();
        println!("Files:       {}", listed.join(", "));
    }
    println!("Status:      {}", r.status);
    if let Some(email) = &r.email {
        println!("Email:       {email}");
    }
    println!("Sent:        {} (unix seconds)", r.created_at);
    Ok(crate::ExitCode::Success)
}

fn run_set_enabled(store: &mut FeedbackStore, enabled: bool) -> Result<crate::ExitCode> {
    let changed = store.enabled != enabled;
    store.enabled = enabled;
    store.save()?;
    match (enabled, changed) {
        (false, true) => println!("feedback disabled; `baml feedback` will not send anything"),
        (false, false) => println!("feedback is already disabled"),
        (true, true) => println!("feedback enabled"),
        (true, false) => println!("feedback is already enabled"),
    }
    Ok(crate::ExitCode::Success)
}

/// Sends the feedback event to PostHog.
///
/// Unlike CLI telemetry (which sets `$process_person_profile: false`),
/// feedback events deliberately create person profiles — person merging on
/// `$identify` is what makes retroactive attribution work.
fn send_feedback(
    creds: &Credentials,
    distinct_id: &str,
    payload: &Value,
    identified: bool,
    report_id: &str,
) -> Result<()> {
    let mut properties = json!({
        "cli_version": baml_version::CANONICAL_VERSION,
        "system_platform": std::env::consts::OS,
        "system_architecture": std::env::consts::ARCH,
        "report_id": report_id,
    });
    if let (Value::Object(props), Value::Object(fields)) = (&mut properties, payload) {
        for (k, v) in fields {
            props.insert(k.clone(), v.clone());
        }
    }
    if identified {
        if let Some(email) = creds.user_email.as_deref() {
            properties["email"] = json!(email);
            properties["$set"] = json!({ "email": email });
        }
    }

    let body = json!({
        "api_key": crate::telemetry::posthog_api_key(),
        "event": FEEDBACK_EVENT,
        "distinct_id": distinct_id,
        "properties": properties,
    });
    post_event(&body).context("Failed to send feedback")
}

/// Sends the `$identify` event that merges the anonymous person into the
/// identified one. Called from `baml auth login`; best-effort by design —
/// the caller ignores failures, and the next login retries the merge.
pub(crate) fn identify(creds: &Credentials) {
    let (Some(anon_id), Some(email)) = (
        creds.posthog_distinct_id.as_deref(),
        creds.user_email.as_deref(),
    ) else {
        return;
    };
    let identified_id = creds.user_id.as_deref().unwrap_or(email);
    let body = json!({
        "api_key": crate::telemetry::posthog_api_key(),
        "event": "$identify",
        "distinct_id": identified_id,
        "properties": {
            "$anon_distinct_id": anon_id,
            "$set": { "email": email },
        },
    });
    let _ = post_event(&body);
}

fn post_event(body: &Value) -> Result<()> {
    let api_key = body["api_key"].as_str().unwrap_or("");
    if api_key.trim().is_empty() {
        anyhow::bail!("This build has no PostHog key configured.");
    }
    let resp = auth::http_client()
        .post(format!("{}/capture/", posthog_host().trim_end_matches('/')))
        .json(body)
        .send()
        .context("Failed to reach PostHog")?;
    let status = resp.status();
    if !status.is_success() {
        anyhow::bail!("PostHog returned {status}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn send_args(
        title: Option<&str>,
        description: Option<&str>,
        input: Option<&str>,
    ) -> FeedbackInner {
        FeedbackInner {
            action: None,
            title: title.map(str::to_string),
            description: description.map(str::to_string),
            input: input.map(str::to_string),
            files: Vec::new(),
            anonymous: true,
            email: false,
        }
    }

    #[test]
    fn payload_requires_title() {
        let args = send_args(None, Some("d"), None);
        let err = args.payload().unwrap_err().to_string();
        assert!(err.contains("needs a title"), "{err}");
    }

    #[test]
    fn flags_override_json_fields() {
        let args = send_args(
            Some("flag title"),
            None,
            Some(r#"{"title": "json title", "description": "json description"}"#),
        );
        let payload = args.payload().unwrap();
        assert_eq!(payload["title"], "flag title");
        assert_eq!(payload["description"], "json description");
    }

    #[test]
    fn long_titles_and_descriptions_are_accepted() {
        // No content limits: only the total payload ceiling applies.
        let long_title = "word ".repeat(60);
        let long_description = "word ".repeat(2000);
        let args = send_args(Some(long_title.trim()), Some(long_description.trim()), None);
        assert!(args.payload().is_ok());
    }

    #[test]
    fn attachments_encode_inline() {
        let dir = tempfile::tempdir().unwrap();
        let img = dir.path().join("shot.png");
        std::fs::write(&img, b"\x89PNG fake").unwrap();

        let files = encode_attachments(std::slice::from_ref(&img)).unwrap();
        assert_eq!(files[0].name, "shot.png");
        assert_eq!(files[0].mime, "image/png");
        assert_eq!(files[0].size_bytes, 9);
        assert_eq!(
            files[0].content.as_deref(),
            Some(b"\x89PNG fake".as_slice())
        );

        // The event json carries the same bytes base64-encoded, and the
        // store round-trips them through the serde adapter.
        use base64::Engine as _;
        let event = files_event_json(&files);
        assert_eq!(
            base64::engine::general_purpose::STANDARD
                .decode(event[0]["content_base64"].as_str().unwrap())
                .unwrap(),
            b"\x89PNG fake"
        );
        let json = serde_json::to_string(&files[0]).unwrap();
        assert!(json.contains("content_base64"), "{json}");
        let back: StoredFile = serde_json::from_str(&json).unwrap();
        assert_eq!(back.content.as_deref(), Some(b"\x89PNG fake".as_slice()));

        // Unknown extensions: UTF-8 content is text, binary is octet-stream.
        let code = dir.path().join("repro.baml");
        std::fs::write(&code, "class A { x int }").unwrap();
        let files = encode_attachments(std::slice::from_ref(&code)).unwrap();
        assert_eq!(files[0].mime, "text/plain");
        let blob = dir.path().join("dump.bamlbin");
        std::fs::write(&blob, [0xFFu8, 0xFE, 0x00, 0x80]).unwrap();
        let files = encode_attachments(std::slice::from_ref(&blob)).unwrap();
        assert_eq!(files[0].mime, "application/octet-stream");

        // Missing file is a clean error.
        let err = encode_attachments(&[dir.path().join("nope.txt")])
            .unwrap_err()
            .to_string();
        assert!(err.contains("failed to read attachment"), "{err}");
    }

    #[test]
    fn rejects_non_string_field_types() {
        let args = send_args(None, None, Some(r#"{"title":"x","description":{"a":1}}"#));
        let err = args.payload().unwrap_err().to_string();
        assert!(err.contains("\"description\" must be a string"), "{err}");

        let args = send_args(None, None, Some(r#"{"title":5}"#));
        let err = args.payload().unwrap_err().to_string();
        assert!(err.contains("\"title\" must be a string"), "{err}");
    }

    #[test]
    fn rejects_non_object_json() {
        assert!(parse_payload_json("[1,2]").is_err());
        assert!(parse_payload_json("\"str\"").is_err());
        assert!(parse_payload_json(r#"{"title":"x"}"#).is_ok());
    }

    #[test]
    fn json_files_field_is_rejected() {
        // Attachment paths must come from the trusted --files flag, never
        // from a piped payload (which could name arbitrary local files).
        let args = send_args(None, None, Some(r#"{"title":"x","files":["/etc/passwd"]}"#));
        let err = args.payload().unwrap_err().to_string();
        assert!(err.contains("unknown feedback field"), "{err}");
        assert!(err.contains("--files"), "{err}");
    }

    #[test]
    fn rejects_unknown_fields_including_old_names() {
        let args = send_args(
            None,
            None,
            Some(r#"{"title":"x","issue":"old","repro":"old"}"#),
        );
        let err = args.payload().unwrap_err().to_string();
        assert!(err.contains("unknown feedback field"), "{err}");
        assert!(err.contains("issue"), "{err}");
    }
}
