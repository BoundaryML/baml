//! Anonymous CLI telemetry — architecture, module layout, and privacy story
//! documented at repo-root `TELEMETRY.md`. The rendered version the CLI links
//! users at is [`TELEMETRY_URL`].
//!
//! Layout mirrors Next.js's `packages/next/src/telemetry/`:
//!
//! - [`storage`]  — the `Telemetry` singleton: persistent config file at
//!   `<baml_home>/telemetry.toml`, first-run notice, opt-out gate, the
//!   background event queue, and the `one_way_hash` helper.
//! - [`anonymous_meta`] — memoized OS / arch / CPU / CI snapshot attached to
//!   every event.
//! - [`project_id`] — SHA-256(salt || project_root) so aggregate dashboards
//!   can dedupe "same project seen twice" without ever seeing the path.
//! - [`events`] — typed `TelemetryEvent` values and their constructors.
//! - [`post`] — PostHog HTTP transport, plus the `[telemetry] {...}` stderr
//!   dry-run used by `BAML_TELEMETRY_DEBUG=1`.
//!
//! Design constraints (unchanged from the previous single-file version):
//!
//!   - Never slow the CLI down noticeably: events are POSTed on background
//!     threads and only awaited briefly on drop of an `InvocationGuard`.
//!   - Never fail or interfere with a command: every error is swallowed.
//!   - Respect opt-out: `DO_NOT_TRACK`, `BAML_TELEMETRY_DISABLED`, and the
//!     persistent `enabled = false` in `telemetry.toml`.

pub(crate) mod anonymous_meta;
pub(crate) mod events;
pub(crate) mod post;
pub(crate) mod project_id;
pub(crate) mod storage;

pub(crate) use events::TelemetryEvent;
pub(crate) use storage::{InvocationGuard, Telemetry};

/// Canonical URL the CLI points at for the full telemetry disclosure /
/// opt-out docs. The site owner sets this up as a redirect to
/// `TELEMETRY.md` on GitHub while the marketing page is being written; the
/// CLI never has to change even if that target moves later.
pub(crate) const TELEMETRY_URL: &str = "https://boundaryml.com/telemetry";

/// Spawn a best-effort telemetry send for `command` (the top-level
/// subcommand name, e.g. `"test"`). Keeps the current call-site shape
/// (`let _telemetry = record_invocation(...)`) so wiring into
/// `commands.rs::run` doesn't have to change every time we add a new event.
///
/// Loads the [`Telemetry`] singleton, prints the first-run notice if we
/// haven't already, records a [`TelemetryEvent::cli_invocation`], and
/// returns a guard that waits briefly on drop for the request to complete.
pub(crate) fn record_invocation(command: &str) -> InvocationGuard {
    let telemetry = Telemetry::load();
    telemetry.notify_once();
    telemetry.record(TelemetryEvent::cli_invocation(command));
    InvocationGuard::new(telemetry)
}
