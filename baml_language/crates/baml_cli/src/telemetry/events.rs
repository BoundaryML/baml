//! Typed telemetry events. Direct analogue of Next.js's
//! `packages/next/src/telemetry/events/*.ts` — one constructor per event
//! type.
//!
//! ## Adding a new event
//!
//! 1. Add a constructor below (name + `json!` payload of coarse,
//!    non-identifying fields — never paths, file contents, or argument
//!    values).
//! 2. Add its name to the `event_names_snapshot` test at the bottom and a
//!    bullet to `TELEMETRY.md`'s "What is being collected?" section, in
//!    the same commit.
//! 3. Fire it from anywhere: `crate::telemetry::record(TelemetryEvent::my_event(...))`.
//!    Delivery, crash-safety, opt-out, and retries are handled for you.
//! 4. Validate with `BAML_TELEMETRY_DEBUG=1 cargo run -- <cmd>` — the
//!    payload prints to stderr instead of being sent.

use serde::Serialize;
use serde_json::json;

use super::storage::env_is_truthy;

/// One telemetry event. Kept intentionally minimal: an event name and a
/// JSON payload of arbitrary fields. Context / meta are added by the
/// transport ([`super::post`]).
#[derive(Debug, Clone, Serialize)]
pub(crate) struct TelemetryEvent {
    pub event_name: &'static str,
    pub payload: serde_json::Value,
}

impl TelemetryEvent {
    /// The single event fired on every `baml <cmd>` invocation. Carries
    /// the subcommand name and a couple of coarse environment flags.
    ///
    /// `command` is the clap-registered subcommand name (e.g. `"test"`,
    /// `"lsp"`, `"fmt"`). Never argument values.
    pub(crate) fn cli_invocation(command: &str) -> Self {
        // Which environment this invocation came from. `.envrc` sets
        // `BAML_TELEMETRY_ENV=internal` in this repo (CI + local dev), so
        // internal traffic filters out of product analytics by default.
        // Unset (a real user's machine) reports as `production`.
        let environment = std::env::var("BAML_TELEMETRY_ENV")
            .ok()
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| "production".to_string());

        // Non-human invocations: editor-spawned language servers and any
        // CI run. Flagged `robot=1` so dashboards can exclude them by
        // default.
        let robot = command == "lsp" || env_is_truthy("CI");

        Self {
            event_name: "cli_invocation",
            payload: json!({
                "command": command,
                "environment": environment,
                "robot": u8::from(robot),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The `cli_invocation` event carries the subcommand name verbatim in
    /// its payload — dashboards join on this string.
    #[test]
    fn cli_invocation_carries_command_name() {
        let e = TelemetryEvent::cli_invocation("fmt");
        assert_eq!(e.event_name, "cli_invocation");
        assert_eq!(e.payload["command"], "fmt");
    }

    /// The `lsp` subcommand is flagged `robot=1` so dashboards can filter
    /// out editor-spawned language-server chatter by default.
    #[test]
    fn lsp_marked_robot() {
        let e = TelemetryEvent::cli_invocation("lsp");
        assert_eq!(e.payload["robot"], 1);
    }

    /// A normal subcommand is not flagged robot — assuming the test
    /// environment isn't itself CI. We can't reliably assert `robot=0`
    /// on a CI runner where `CI=true` genuinely is set; instead, assert
    /// the payload shape and let the CI-branch check the flag flips.
    #[test]
    fn regular_command_has_robot_field() {
        let e = TelemetryEvent::cli_invocation("fmt");
        assert!(e.payload["robot"].is_number(), "payload: {:?}", e.payload);
    }

    /// Every event this build can emit must have a documented name. The
    /// list below is the contract with `TELEMETRY.md`'s "What is being
    /// collected?" section; when you add an event constructor, add its
    /// actual constructed value to `all_shipped_events()` and its name
    /// here (and the docs), in the same commit.
    ///
    /// This is not a tautology: it builds the real events via their
    /// constructors and checks the names they *actually* produce against
    /// the documented set, so renaming the wire name (e.g.
    /// `"cli_invocation"` → `"cli_invoke"`) without updating the list and
    /// docs fails the build.
    #[test]
    fn every_shipped_event_name_is_documented() {
        let documented: &[&str] = &["cli_invocation"];
        for event in all_shipped_events() {
            assert!(
                documented.contains(&event.event_name),
                "event {:?} is emitted but not in the documented set {documented:?} \
                 — add it here and to TELEMETRY.md",
                event.event_name,
            );
        }
    }

    /// One representative value per event constructor in this crate. Add a
    /// line here whenever you add a constructor to `TelemetryEvent`; the
    /// test above then holds you to documenting its name.
    fn all_shipped_events() -> Vec<TelemetryEvent> {
        vec![TelemetryEvent::cli_invocation("fmt")]
    }
}
