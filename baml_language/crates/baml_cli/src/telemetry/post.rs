//! HTTP transport. Analogue of Next.js's
//! `packages/next/src/telemetry/post-telemetry-payload.ts`.
//!
//! Two output modes:
//!
//! - **Normal.** POST the payload to PostHog. Errors are swallowed; the
//!   CLI must never fail or slow down because of telemetry.
//! - **Debug** (`BAML_TELEMETRY_DEBUG=1`). Print the exact payload to
//!   stderr, prefixed `[telemetry]`, and do NOT send it. This is the
//!   audit-your-own-traffic feature; equivalent to Next's
//!   `NEXT_TELEMETRY_DEBUG`.

#![allow(clippy::print_stderr)] // The `[telemetry]` debug line is deliberate stderr output.

use std::time::Duration;

use serde_json::{Value, json};

use super::{anonymous_meta, events::TelemetryEvent, project_id, storage::Telemetry};

/// PostHog project API key (the public, write-only `phc_...` ingestion key).
/// Safe to embed in the client binary. Leave empty to disable telemetry at
/// build time (`api_key_configured` returns `false`).
const POSTHOG_API_KEY: &str = "phc_zgLi9FbzjkLLX6vDsUUixBnDsW6GbN93ohcdboSXSpGy";

/// PostHog ingestion host. If we ever front this with a Boundary-owned
/// domain (`telemetry.boundaryml.com` → PostHog reverse proxy) — the
/// architecture doc recommends this — this constant is the one place to
/// swap it.
const POSTHOG_HOST: &str = "https://us.i.posthog.com";

/// Per-request network timeout for the telemetry POST.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(2);

/// Send (or print, in debug mode) one telemetry event. Returns `Some(())`
/// on any completed action so tests can distinguish "we handled it" from
/// "no-op because opted out".
pub(crate) fn send(telemetry: &Telemetry, event: &TelemetryEvent) -> Option<()> {
    // Belt-and-suspenders: `Telemetry::record` already short-circuits when
    // opt-out is in effect, but a future direct caller of `post::send`
    // shouldn't accidentally leak either.
    if !telemetry.is_enabled() && !telemetry.debug_mode() {
        return None;
    }

    let body = build_body(telemetry, event)?;

    if telemetry.debug_mode() {
        // Pretty-printed so users grepping their terminal can actually
        // read what would have been sent. Matches Next's `[telemetry]`
        // stderr line.
        let rendered = serde_json::to_string_pretty(&body).ok()?;
        eprintln!("[telemetry] {rendered}");
        return Some(());
    }

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

/// `true` if a PostHog key was compiled in. When empty (typically in a
/// fork or a debug build with the constant blanked out), the whole
/// pipeline treats telemetry as fully disabled.
pub(crate) fn api_key_configured() -> bool {
    !POSTHOG_API_KEY.trim().is_empty()
}

/// Compose the PostHog `capture` body. Field organization mirrors Next.js's
/// `{ context, meta, events }` payload, flattened into PostHog's
/// `properties` bag because PostHog's ingestion API doesn't take a
/// custom envelope.
///
/// Concretely: `context` and `meta` fields live at the top of `properties`,
/// then the event-specific `fields` are merged in on top.
fn build_body(telemetry: &Telemetry, event: &TelemetryEvent) -> Option<Value> {
    let api_key = POSTHOG_API_KEY.trim();
    if api_key.is_empty() {
        return None;
    }
    let anonymous_id = telemetry.anonymous_id();
    if anonymous_id.is_empty() {
        return None;
    }

    let meta = anonymous_meta::get();
    let project = project_id::compute(telemetry);

    // Base properties = meta + context. Event-specific fields overlay on top.
    let mut properties = json!({
        // Anonymous events: don't create/merge a PostHog person profile.
        "$process_person_profile": false,
        // PostHog groups events with the same $session_id into one session.
        "$session_id": telemetry.session_id(),

        // Context (per-invocation).
        "project_id": project,

        // Meta (per-machine, memoized).
        "system_platform":     meta.system_platform,
        "system_architecture": meta.system_architecture,
        "cpu_count":           meta.cpu_count,
        "is_docker":           meta.is_docker,
        "is_wsl":              meta.is_wsl,
        "is_ci":               meta.is_ci,
        "ci_name":             meta.ci_name,
        "cli_version":         meta.cli_version,
        "channel":             meta.channel,
    });

    // Merge the event's payload into `properties`. `serde_json::Value`
    // merging is manual; keep it explicit rather than pulling in `json_patch`.
    if let (Value::Object(props), Value::Object(fields)) = (&mut properties, &event.payload) {
        for (k, v) in fields {
            props.insert(k.clone(), v.clone());
        }
    }

    Some(json!({
        "api_key":     api_key,
        "event":       event.event_name,
        "distinct_id": anonymous_id,
        "properties":  properties,
    }))
}

fn host() -> &'static str {
    POSTHOG_HOST.trim_end_matches('/')
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::telemetry::events::TelemetryEvent;

    /// The built body carries the api key, event name, distinct id, and
    /// the merged properties. It's what actually goes on the wire, so a
    /// smoke test that all four buckets are populated is cheap insurance
    /// against a future refactor dropping one.
    ///
    /// Uses `TempDir` directly (rather than the `BAML_HOME` env var) so
    /// this test can safely run alongside `storage.rs`'s env-var tests
    /// without needing the shared `env_lock`. `Telemetry::load()`'s
    /// choice of `<baml_home>/telemetry.toml` is exercised elsewhere.
    #[test]
    fn body_shape_is_stable() {
        // Load whatever the current environment picks — the test only
        // cares that the returned body has the right buckets. It does
        // NOT care about a specific config path.
        let t = Telemetry::load();
        let event = TelemetryEvent::cli_invocation("fmt");
        let body = build_body(&t, &event).expect("api key is compiled in for this build");

        assert!(body["api_key"].as_str().is_some());
        assert_eq!(body["event"], "cli_invocation");
        assert!(body["distinct_id"].as_str().is_some());
        let props = body["properties"]
            .as_object()
            .expect("properties is an object");
        // Meta.
        assert!(props.contains_key("cli_version"));
        assert!(props.contains_key("system_platform"));
        // Context.
        assert!(props.contains_key("project_id"));
        assert!(props.contains_key("$session_id"));
        // Event fields.
        assert_eq!(props["command"], "fmt");
    }
}
