// Anonymous, best-effort PostHog telemetry for the BAML CLI.
//
// On every `baml <subcommand>` invocation we send a single `cli_invocation`
// event capturing the top-level subcommand plus coarse environment metadata
// (CLI version, release channel, OS, arch). Users are identified only by a
// random anonymous id persisted under the BAML home directory; we never
// collect emails or any project content.
//
// Design constraints:
//   - Never slow the CLI down noticeably: the request runs on a background
//     thread that overlaps with the command, and we wait at most `GRACE` for
//     it to finish once the command is done (see `InvocationGuard`).
//   - Never fail or interfere with a command: every error is swallowed.
//   - Respect opt-out: `DO_NOT_TRACK` and `BAML_TELEMETRY` (see `opt_out`).

use std::{sync::mpsc, time::Duration};

/// PostHog project API key (the public, write-only `phc_...` ingestion key).
///
/// Safe to embed in the client binary. Leave empty to disable telemetry at
/// build time.
const POSTHOG_API_KEY: &str = "phc_zgLi9FbzjkLLX6vDsUUixBnDsW6GbN93ohcdboSXSpGy";

/// PostHog ingestion host (US cloud).
const POSTHOG_HOST: &str = "https://us.i.posthog.com";

/// PostHog event name for a single CLI invocation.
const EVENT_NAME: &str = "cli_invocation";

/// Max time we wait, after the command finishes, for the in-flight telemetry
/// request to complete before letting the process exit.
const GRACE: Duration = Duration::from_secs(1);

/// Per-request network timeout for the telemetry POST.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(2);

/// Spawn a best-effort telemetry send for `command` (the top-level subcommand
/// name, e.g. `"test"`).
///
/// Returns a guard that, when dropped, waits up to [`GRACE`] for the request
/// to finish. Keep it alive for the duration of the command:
///
/// ```ignore
/// let _telemetry = telemetry::record_invocation(self.command.name());
/// ```
///
/// If telemetry is disabled (opt-out env var set, or no API key configured)
/// this spawns nothing and returns a no-op guard.
pub(crate) fn record_invocation(command: &str) -> InvocationGuard {
    if !enabled() {
        return InvocationGuard::noop();
    }

    // Own the name so it can move into the background thread.
    let command = command.to_string();
    let (tx, rx) = mpsc::channel();
    let spawned = std::thread::Builder::new()
        .name("baml-telemetry".to_string())
        .spawn(move || {
            // Errors are intentionally ignored: telemetry must never surface.
            let _ = try_send(&command);
            // Signal completion so the guard's grace wait can return early.
            let _ = tx.send(());
        });

    match spawned {
        Ok(_) => InvocationGuard { done: Some(rx) },
        Err(_) => InvocationGuard::noop(),
    }
}

/// RAII guard returned by [`record_invocation`]. On drop it waits up to
/// [`GRACE`] for the background send to complete, then lets the process
/// continue (and exit). Fast commands may occasionally drop an event if the
/// network is slower than the grace window; that is an accepted trade-off for
/// never delaying the CLI.
pub(crate) struct InvocationGuard {
    done: Option<mpsc::Receiver<()>>,
}

impl InvocationGuard {
    const fn noop() -> Self {
        Self { done: None }
    }
}

impl Drop for InvocationGuard {
    fn drop(&mut self) {
        if let Some(rx) = self.done.take() {
            let _ = rx.recv_timeout(GRACE);
        }
    }
}

/// Whether telemetry should run for this invocation: not opted out, and an
/// API key is configured.
fn enabled() -> bool {
    !opt_out() && api_key().is_some()
}

/// Telemetry is disabled if `DO_NOT_TRACK` is truthy (the cross-tool
/// convention from <https://consoledonottrack.com>) or `BAML_TELEMETRY` is
/// explicitly set to a falsy value.
fn opt_out() -> bool {
    env_is_truthy("DO_NOT_TRACK") || env_is_falsy("BAML_TELEMETRY")
}

/// The compiled-in PostHog API key, or `None` if it was left empty at build
/// time (which disables telemetry entirely).
fn api_key() -> Option<&'static str> {
    let key = POSTHOG_API_KEY.trim();
    if key.is_empty() { None } else { Some(key) }
}

/// The PostHog host, without a trailing slash.
fn host() -> &'static str {
    POSTHOG_HOST.trim_end_matches('/')
}

/// Build and send the PostHog capture request. Returns `None` (silently) on
/// any failure or if telemetry is not fully configured.
fn try_send(command: &str) -> Option<()> {
    let api_key = api_key()?;
    let distinct_id = anonymous_id()?;

    // Which environment this invocation came from. `.envrc` sets
    // `BAML_TELEMETRY_ENV=internal` for this repo (CI + local dev), so internal
    // traffic can be filtered out of product analytics by default. Unset
    // (i.e. a real user's machine) reports as "production".
    let environment = std::env::var("BAML_TELEMETRY_ENV")
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "production".to_string());

    // Non-human invocations: editor-spawned language servers and any CI run.
    // Flagged as `robot=1` so dashboards can exclude them by default. We also
    // forward the raw `CI` value, which build systems usually set (e.g.
    // "true", or a provider-specific string).
    let ci = std::env::var("CI").ok().filter(|v| !v.is_empty());
    let robot = command == "lsp" || env_is_truthy("CI");

    let body = serde_json::json!({
        "api_key": api_key,
        "event": EVENT_NAME,
        "distinct_id": distinct_id,
        "properties": {
            "command": command,
            "environment": environment,
            "robot": u8::from(robot),
            "ci": ci,
            "cli_version": baml_version::CANONICAL_VERSION,
            "channel": baml_version::CHANNEL,
            "os": std::env::consts::OS,
            "arch": std::env::consts::ARCH,
            // Anonymous events: don't create/merge a PostHog person profile.
            "$process_person_profile": false,
        },
    });

    let client = reqwest::blocking::Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .build()
        .ok()?;

    let _ = client
        .post(format!("{}/capture/", host()))
        .json(&body)
        .send();

    Some(())
}

/// Load the persisted anonymous id, generating and storing a new UUID v4 on
/// first use. Stored as a plain-text file at `<baml_home>/telemetry_id`.
fn anonymous_id() -> Option<String> {
    let path = baml_release::baml_home().join("telemetry_id");

    if let Ok(existing) = std::fs::read_to_string(&path) {
        let id = existing.trim();
        if !id.is_empty() {
            return Some(id.to_string());
        }
    }

    let id = uuid::Uuid::new_v4().to_string();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    // Best-effort persist. If the write fails the id won't be stable across
    // runs, but a single event is still better than none.
    let _ = std::fs::write(&path, &id);

    Some(id)
}

/// `true` if `name` is set to a truthy value (`1`/`true`/`yes`/`on`, or, per
/// the `DO_NOT_TRACK` convention, any non-empty value other than the explicit
/// falsy ones).
fn env_is_truthy(name: &str) -> bool {
    match std::env::var(name) {
        Ok(value) => {
            let v = value.trim().to_ascii_lowercase();
            !v.is_empty() && !matches!(v.as_str(), "0" | "false" | "no" | "off")
        }
        Err(_) => false,
    }
}

/// `true` if `name` is set to an explicit falsy value (`0`/`false`/`no`/`off`).
fn env_is_falsy(name: &str) -> bool {
    match std::env::var(name) {
        Ok(value) => matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "0" | "false" | "no" | "off"
        ),
        Err(_) => false,
    }
}
