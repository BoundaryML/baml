//! Anonymous CLI telemetry — privacy story documented at repo-root
//! `TELEMETRY.md`; the rendered version the CLI links users at is
//! [`TELEMETRY_URL`].
//!
//! ## Adding a new event (the only thing most contributors need)
//!
//! 1. Add a constructor to [`events::TelemetryEvent`] (and a bullet to
//!    `TELEMETRY.md`'s "What is being collected?" — the snapshot test in
//!    `events.rs` reminds you).
//! 2. Call it from anywhere:
//!
//!    ```ignore
//!    crate::telemetry::record(TelemetryEvent::test_run(passed, failed, ms));
//!    ```
//!
//! That's it. Delivery, crash-safety, opt-out, retries, and rate concerns
//! are all handled underneath. Validate with
//! `BAML_TELEMETRY_DEBUG=1 cargo run -- <cmd>`, which prints the payload
//! to stderr instead of sending it.
//!
//! ## Architecture (for the curious)
//!
//! Modeled on Next.js's `packages/next/src/telemetry/`, with delivery
//! upgraded to a crash-safe disk queue:
//!
//! - [`storage`] — the `Telemetry` singleton: persistent config at
//!   `<baml_home>/telemetry.toml`, first-run notice, opt-out gate,
//!   `one_way_hash`, and the 10-minute rotation timer for long-running
//!   processes.
//! - [`queue`] — the disk queue at `<baml_home>/telemetry/`. `record()`
//!   appends the full request body to a `live_*.jsonl` file (one atomic
//!   write; survives panics and SIGKILL). On drop or rotation the file is
//!   sealed and a detached `baml __flush-telemetry` child POSTs it after
//!   the parent has already exited — zero added latency, automatic retry
//!   of any backlog, orphan recovery, and a 24h staleness purge.
//! - [`events`] — typed `TelemetryEvent` constructors. The only file most
//!   changes touch.
//! - [`post`] — builds PostHog request bodies at record time; POSTs only
//!   inside the flush child.
//! - [`anonymous_meta`] / [`project_id`] — coarse machine snapshot and the
//!   salted, irreversible project-root hash.
//!
//! Invariants:
//!
//!   - No HTTP on any user-visible code path, ever.
//!   - Never fail or interfere with a command: every error is swallowed.
//!   - Respect opt-out: `DO_NOT_TRACK`, `BAML_TELEMETRY_DISABLED`, and the
//!     persistent `enabled = false` in `telemetry.toml` (a flush child
//!     that finds opt-out deletes the backlog instead of sending it).

pub(crate) mod anonymous_meta;
pub(crate) mod events;
pub(crate) mod post;
pub(crate) mod project_id;
pub(crate) mod queue;
pub(crate) mod storage;

pub(crate) use events::TelemetryEvent;
pub(crate) use storage::{InvocationGuard, Telemetry};

/// Canonical URL the CLI points at for the full telemetry disclosure /
/// opt-out docs. The site owner sets this up as a redirect to
/// `TELEMETRY.md` on GitHub while the marketing page is being written; the
/// CLI never has to change even if that target moves later.
pub(crate) const TELEMETRY_URL: &str = "https://boundaryml.com/telemetry";

/// Record a telemetry event from anywhere in the crate. Fire-and-forget:
/// one atomic file append (~10µs), no HTTP, no guard to hold. See the
/// module docs for how to add a new event type.
pub(crate) fn record(event: TelemetryEvent) {
    Telemetry::global().record(event);
}

/// Per-invocation wiring for `commands.rs::run`: prints the first-run
/// notice if it's never been shown, records the `cli_invocation` event,
/// and returns a guard whose drop seals the queue file and spawns the
/// detached flush child (non-blocking, ~1–2ms).
pub(crate) fn record_invocation(command: &str) -> InvocationGuard {
    let telemetry = Telemetry::global().clone();
    telemetry.notify_once();
    telemetry.record(TelemetryEvent::cli_invocation(command));
    InvocationGuard::new(telemetry)
}

/// Entry point for the hidden `baml __flush-telemetry` subcommand — the
/// detached child spawned by [`storage::Telemetry::flush`]. Drains the
/// disk queue: sweeps stale/orphaned files, claims sealed ones, POSTs
/// them, and deletes (or restores for retry) accordingly.
pub(crate) fn run_flush_child() {
    // `load()`, not `global()`: the child needs no rotation timer, and
    // it must re-read the config so an opt-out that happened after the
    // events were queued is honored (backlog gets deleted, not sent).
    let telemetry = Telemetry::load();
    queue::drain(&queue::queue_dir(), !telemetry.is_enabled());
}

/// The PostHog project API key, shared with `baml feedback` (which sends
/// person-profile events, unlike telemetry). Public write-only `phc_...`
/// ingestion key; safe to embed.
pub(crate) fn posthog_api_key() -> &'static str {
    post::posthog_api_key()
}

/// The PostHog ingestion host, shared with `baml feedback`.
pub(crate) fn posthog_host() -> &'static str {
    post::posthog_host()
}
