//! HTTP transport. Analogue of Next.js's
//! `packages/next/src/telemetry/post-telemetry-payload.ts`.
//!
//! [`build_body`] composes the complete PostHog request body at
//! `record()` time (in the process that owns the event, so session id and
//! metadata are correct); [`send_body`] POSTs one such body and runs only
//! inside the detached flush child (see [`super::queue`]) — never on a
//! user-visible code path. The `BAML_TELEMETRY_DEBUG=1` stderr dry-run
//! lives in [`super::storage::Telemetry::record`].

use std::time::Duration;

use baml_http::outbound::{self, Destination};
use serde_json::{Value, json};

use super::{anonymous_meta, events::TelemetryEvent, project_id, storage::Telemetry};

/// PostHog project API key (the public, write-only `phc_...` ingestion key).
/// Safe to embed in the client binary. Leave empty to disable telemetry at
/// build time (`api_key_configured` returns `false`).
const POSTHOG_API_KEY: &str = "phc_zgLi9FbzjkLLX6vDsUUixBnDsW6GbN93ohcdboSXSpGy";

/// Per-request network timeout for the telemetry POST.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(2);

/// POST one pre-built request body to PostHog. Returns `true` on an HTTP
/// 2xx so the flush child knows whether to delete the queue file or put
/// it back for retry. Only ever runs in the detached child, so blocking
/// on the network here is fine.
pub(super) fn send_body(body: &Value) -> bool {
    let Some(host) = host() else {
        return false;
    };
    let Ok(payload) = serde_json::to_vec(body) else {
        return false;
    };
    let request = baml_http::Request::post(format!("{host}/capture/"))
        .header("content-type", "application/json")
        .body(payload)
        .timeout(REQUEST_TIMEOUT);
    outbound::send_blocking(Destination::AnonymousTelemetry, request, 64 * 1024)
        .is_ok_and(|response| response.is_success())
}

/// `true` if a PostHog key was compiled in and this build may send
/// telemetry at all. When empty (typically in a fork or a debug build with
/// the constant blanked out) or in a `no-phone-home` build, the whole
/// pipeline treats telemetry as fully disabled.
pub(crate) fn api_key_configured() -> bool {
    !POSTHOG_API_KEY.trim().is_empty() && Destination::AnonymousTelemetry.enabled()
}

/// Compose the PostHog `capture` body. Field organization mirrors Next.js's
/// `{ context, meta, events }` payload, flattened into PostHog's
/// `properties` bag because PostHog's ingestion API doesn't take a
/// custom envelope.
///
/// Concretely: `context` and `meta` fields live at the top of `properties`,
/// then the event-specific `fields` are merged in on top.
pub(super) fn build_body(telemetry: &Telemetry, event: &TelemetryEvent) -> Option<Value> {
    if !api_key_configured() {
        return None;
    }
    let api_key = POSTHOG_API_KEY.trim();
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

/// The PostHog ingestion host, from the outbound registry (`baml_http`).
fn host() -> Option<&'static str> {
    Destination::AnonymousTelemetry
        .default_url()
        .map(|host| host.trim_end_matches('/'))
}

/// Crate-visible accessor for the PostHog key (used by `baml feedback`).
pub(crate) fn posthog_api_key() -> &'static str {
    POSTHOG_API_KEY
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
