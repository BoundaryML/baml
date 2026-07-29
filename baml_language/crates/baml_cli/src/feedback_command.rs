// `baml feedback` — report a way to improve the BAML language to Boundary.
//
// Identity is PostHog-first. The reporter chooses:
//
// - **Anonymously**: the event is sent under a locally generated PostHog
//   distinct id, persisted in `~/.baml/creds.json` so future reports share
//   one anonymous person. No account, no server-side registration.
// - **Via email** (requires `baml login`): the event additionally carries
//   the verified email, and the reporter can be notified when a fix ships.
//
// When an anonymous reporter later logs in, a `$identify` event merges the
// anonymous person into the identified one — PostHog's native person merge —
// so every previously filed report is retroactively attributed. There is no
// backfill job; the merge *is* the backfill.

use anyhow::{Context, Result};
use clap::Args;
use serde_json::{Value, json};

use crate::auth::{self, Credentials};

/// Feedback events must stay comfortably inside PostHog's per-event limits.
const MAX_PAYLOAD_BYTES: usize = 256 * 1024;

const FEEDBACK_EVENT: &str = "baml_feedback";

/// How long a "Don't send" choice suppresses the prompt.
const DECLINE_COOLDOWN_SECS: u64 = 24 * 60 * 60;

/// PostHog ingestion host, overridable for tests and self-hosted setups.
fn posthog_host() -> String {
    std::env::var("BAML_POSTHOG_HOST")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| crate::telemetry::posthog_host().to_string())
}

/// Report an issue or improvement to Boundary.
///
/// Reports can be anonymous or associated with the email from `baml login`.
/// Without an identity flag, interactive sessions prompt before sending.
#[derive(Args, Debug)]
#[command(after_long_help = "\
Examples:
  Report an issue:
    baml feedback --issue \"parser panics on nested unions\"

  Submit an anonymous report with a reproduction:
    baml feedback --anonymous --issue \"...\" --repro \"class A { ... }\"

  Submit an anonymous JSON report from standard input:
    echo '{\"issue\": \"...\", \"repro\": \"...\"}' | baml feedback --anonymous -")]
pub(crate) struct FeedbackArgs {
    /// The issue or improvement you found.
    #[arg(long, help_heading = "Report options")]
    pub issue: Option<String>,

    /// Steps or a snippet that reproduces it.
    #[arg(long, help_heading = "Report options")]
    pub repro: Option<String>,

    /// Anything else useful (versions, environment, what you were doing).
    #[arg(long, help_heading = "Report options")]
    pub context: Option<String>,

    /// Advanced: supply the fields as JSON instead of flags (an inline
    /// object, a file path, or `-` for stdin).
    #[arg(value_name = "JSON", hide_short_help = true)]
    pub input: Option<String>,

    /// Report anonymously without prompting.
    #[arg(long, conflicts_with = "email", help_heading = "Identity options")]
    pub anonymous: bool,

    /// Report with your email without prompting (requires `baml login`).
    #[arg(long, help_heading = "Identity options")]
    pub email: bool,
}

impl FeedbackArgs {
    /// Runs `baml feedback`.
    ///
    /// Collects the payload, resolves how to report (flag, or interactive
    /// prompt offering anonymous vs email — the email path runs `baml
    /// login` inline when needed), ensures a persistent PostHog distinct
    /// id, and sends the event.
    ///
    /// Returns:
    /// - `ExitCode::Success` when the event is accepted by PostHog.
    /// - `ExitCode::Other` (with guidance on stderr) when `--email` is used
    ///   non-interactively without a login.
    ///
    /// Errors:
    /// - On a missing/oversized/malformed payload or a failed send.
    #[allow(clippy::print_stdout, clippy::print_stderr)]
    pub fn run(&self) -> Result<crate::ExitCode> {
        let payload = self.payload()?;

        let mut creds = Credentials::read()?.unwrap_or_default();

        // Resolve identity mode.
        let identified = if self.anonymous {
            false
        } else if creds.user_email.is_some() {
            // Already logged in: report with the account, no prompt needed.
            true
        } else if self.email {
            // Non-interactive email mode without a session: instruct and
            // exit rather than trying to run an interactive login under a
            // flag that suggests automation.
            eprintln!(
                "Reporting via email requires a login. Run `baml login`, then \
                 re-run this command."
            );
            return Ok(crate::ExitCode::Other);
        } else if creds.feedback_anonymous {
            // A prior report was sent anonymously; honor that choice without
            // re-prompting until a login replaces it.
            false
        } else if creds
            .feedback_declined_at
            .is_some_and(|at| auth::now_unix() < at.saturating_add(DECLINE_COOLDOWN_SECS))
        {
            // "Don't send" was chosen within the last day: decline quietly
            // instead of re-asking. Explicit --anonymous/--email (handled
            // above) still send.
            println!(
                "Nothing sent (you declined recently). Pass --anonymous or \
                 --email to send anyway."
            );
            return Ok(crate::ExitCode::Success);
        } else {
            println!("I found an issue with BAML. I can report it to Boundary.");
            println!();
            self.print_preview(&payload);
            match self.prompt_for_mode()? {
                ReportMode::Anonymous => false,
                ReportMode::Email => {
                    creds = auth::device_login(false, creds)?;
                    true
                }
                ReportMode::DontSend => {
                    creds.feedback_declined_at = Some(auth::now_unix());
                    creds.write()?;
                    println!("Nothing sent.");
                    return Ok(crate::ExitCode::Success);
                }
            }
        };

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
            if !identified {
                // Remember the anonymous choice so later runs don't re-prompt
                // (until a login resets it).
                creds.feedback_anonymous = true;
            }
            // A sent report supersedes any earlier "Don't send" cooldown.
            creds.feedback_declined_at = None;
            creds.write()?;
            creds.posthog_distinct_id.clone().expect("set above")
        };

        send_feedback(&creds, &distinct_id, payload, identified)?;

        if identified {
            println!(
                "Feedback sent as {}. Thank you! You'll be notified when this is addressed.",
                creds.user_email.as_deref().unwrap_or("your account")
            );
        } else {
            println!("Feedback sent anonymously. Thank you!");
            if creds.user_email.is_none() {
                println!(
                    "(Run `baml login` any time to attach your email and get notified of fixes. Past reports come with you.)"
                );
            }
        }
        Ok(crate::ExitCode::Success)
    }

    /// Assembles `{issue, repro?, context?}` from flags and/or JSON input.
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
        for (key, flag) in [
            ("issue", &self.issue),
            ("repro", &self.repro),
            ("context", &self.context),
        ] {
            if let Some(value) = flag {
                obj.insert(key.to_string(), Value::String(value.clone()));
            }
        }

        // Whitelist: only the documented fields ship. Anything else in a
        // piped payload (stray script fields, PostHog-special keys like
        // `$set`) is rejected rather than silently transmitted.
        let unknown: Vec<&str> = obj
            .keys()
            .map(String::as_str)
            .filter(|k| !matches!(*k, "issue" | "repro" | "context"))
            .collect();
        if !unknown.is_empty() {
            anyhow::bail!(
                "Unknown feedback field(s): {}. Only \"issue\", \"repro\", and \
                 \"context\" are sent.",
                unknown.join(", ")
            );
        }

        if obj
            .get("issue")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim()
            .is_empty()
        {
            anyhow::bail!(
                "Feedback needs an issue. Pass --issue \"...\" or a JSON payload \
                 with an \"issue\" field."
            );
        }
        let size = serde_json::to_string(&from_json)
            .map(|s| s.len())
            .unwrap_or(0);
        if size > MAX_PAYLOAD_BYTES {
            anyhow::bail!(
                "Feedback payload is {size} bytes; the limit is {MAX_PAYLOAD_BYTES}. \
                 Trim the repro/context (a link to a gist works well)."
            );
        }
        Ok(from_json)
    }

    /// Shows exactly what a report will contain before the user decides.
    #[allow(clippy::print_stdout)]
    fn print_preview(&self, payload: &Value) {
        println!("Here's what will be sent:");
        println!();
        for (label, key) in [
            ("Issue", "issue"),
            ("Repro", "repro"),
            ("Context", "context"),
        ] {
            if let Some(text) = payload.get(key).and_then(Value::as_str) {
                println!("  {label}: {text}");
            }
        }
        println!(
            "  (plus: cli version {}, {}/{})",
            baml_version::CANONICAL_VERSION,
            std::env::consts::OS,
            std::env::consts::ARCH
        );
        println!();
    }

    /// The interactive choice.
    #[allow(clippy::print_stdout)]
    fn prompt_for_mode(&self) -> Result<ReportMode> {
        println!("How should I report it?");
        println!("  1) Anonymously");
        println!(
            "  2) Via your email (requires `baml login`; you'll be notified when a fix ships)"
        );
        println!("  3) Don't send");
        loop {
            let choice = auth::prompt("Choose [1/2/3]: ")?;
            match choice.as_str() {
                "1" => return Ok(ReportMode::Anonymous),
                "2" => return Ok(ReportMode::Email),
                "3" | "n" | "no" => return Ok(ReportMode::DontSend),
                _ => println!("Please enter 1, 2, or 3."),
            }
        }
    }
}

enum ReportMode {
    Anonymous,
    Email,
    /// Declined at the prompt: nothing is sent, nothing is persisted.
    DontSend,
}

fn parse_payload_json(raw: &str) -> Result<Value> {
    let value: Value = serde_json::from_str(raw).context("Feedback payload is not valid JSON")?;
    if !value.is_object() {
        anyhow::bail!("Feedback payload must be a JSON object like {{\"issue\": \"...\"}}");
    }
    Ok(value)
}

/// Sends the feedback event to PostHog.
///
/// Unlike CLI telemetry (which sets `$process_person_profile: false`),
/// feedback events deliberately create person profiles — person merging on
/// `$identify` is what makes retroactive attribution work.
fn send_feedback(
    creds: &Credentials,
    distinct_id: &str,
    payload: Value,
    identified: bool,
) -> Result<()> {
    let mut properties = json!({
        "cli_version": baml_version::CANONICAL_VERSION,
        "system_platform": std::env::consts::OS,
        "system_architecture": std::env::consts::ARCH,
    });
    if let (Value::Object(props), Value::Object(fields)) = (&mut properties, &payload) {
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
/// identified one. Called from `baml login`; best-effort by design — the
/// caller ignores failures, and the next login retries the merge.
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

    #[test]
    fn payload_requires_issue() {
        let args = FeedbackArgs {
            issue: None,
            repro: Some("r".into()),
            context: None,
            input: None,
            anonymous: true,
            email: false,
        };
        let err = args.payload().unwrap_err().to_string();
        assert!(err.contains("needs an issue"), "{err}");
    }

    #[test]
    fn flags_override_json_fields() {
        let args = FeedbackArgs {
            issue: Some("flag issue".into()),
            repro: None,
            context: None,
            input: Some(r#"{"issue": "json issue", "repro": "json repro"}"#.into()),
            anonymous: true,
            email: false,
        };
        let payload = args.payload().unwrap();
        assert_eq!(payload["issue"], "flag issue");
        assert_eq!(payload["repro"], "json repro");
    }

    #[test]
    fn rejects_non_object_json() {
        assert!(parse_payload_json("[1,2]").is_err());
        assert!(parse_payload_json("\"str\"").is_err());
        assert!(parse_payload_json(r#"{"issue":"x"}"#).is_ok());
    }
}
