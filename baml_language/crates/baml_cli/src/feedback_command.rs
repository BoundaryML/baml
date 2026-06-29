// `baml feedback` opens a pre-filled GitHub issue on boundaryml/baml so users
// can share feedback. The issue title is "feedback: <summary>" and the body
// (via the 04-feedback.yml issue form template) is pre-populated with details
// about the user's machine and BAML environment.
#![allow(clippy::print_stdout, clippy::print_stderr)]

use anyhow::{Context, Result};
use clap::Args;

/// New-issue endpoint for the public BAML repo.
const FEEDBACK_NEW_ISSUE_URL: &str = "https://github.com/boundaryml/baml/issues/new";
/// Issue form template that renders the pre-filled fields
/// (see .github/ISSUE_TEMPLATE/04-feedback.yml).
const FEEDBACK_TEMPLATE: &str = "04-feedback.yml";

#[derive(Args, Debug)]
pub struct FeedbackArgs {}

impl FeedbackArgs {
    pub fn run(&self) -> Result<crate::ExitCode> {
        // No prompt: go straight to a pre-populated GitHub issue with a fixed,
        // version-stamped title and the environment details filled in.
        let environment = environment_details();
        let url =
            feedback_issue_url(&environment).context("Failed to build the feedback issue URL")?;

        println!("Thanks for sharing feedback on BAML! 🐶");
        println!();
        println!("The following environment details will be included:");
        for line in environment.lines() {
            println!("  {line}");
        }
        println!();

        match webbrowser::open(url.as_str()) {
            Ok(()) => {
                println!("Opening GitHub in your browser to finish filing the issue...");
            }
            Err(e) => {
                eprintln!("Could not open your browser automatically ({e}).");
                println!("Open this URL to submit your feedback:");
                println!();
                println!("  {url}");
            }
        }

        Ok(crate::ExitCode::Success)
    }
}

/// Builds the pre-filled GitHub new-issue URL. The title is
/// "[feedback] baml <version>"; the environment block is passed to the
/// `environment` field of the 04-feedback.yml issue form.
fn feedback_issue_url(environment: &str) -> Result<reqwest::Url> {
    let title = format!("[feedback] baml {}", baml_version::CANONICAL_VERSION);
    let url = reqwest::Url::parse_with_params(
        FEEDBACK_NEW_ISSUE_URL,
        &[
            ("template", FEEDBACK_TEMPLATE),
            ("title", title.as_str()),
            ("environment", environment),
        ],
    )?;
    Ok(url)
}

/// Collects machine and BAML environment details to pre-populate the issue.
fn environment_details() -> String {
    let mut lines = Vec::new();
    lines.push(format!(
        "BAML version: {} ({} channel)",
        baml_version::CANONICAL_VERSION,
        baml_version::CHANNEL
    ));
    lines.push(format!("OS: {}", std::env::consts::OS));
    lines.push(format!("Architecture: {}", std::env::consts::ARCH));
    lines.push(format!("OS family: {}", std::env::consts::FAMILY));
    // Best-effort contact email so we can follow up. Left out entirely when
    // neither git nor jj has a configured user email.
    if let Some(email) = contact_email() {
        lines.push(format!("Contact email: {email}"));
    }
    lines.join("\n")
}

/// Returns the local VCS-configured user email, if available. Tries
/// `git config user.email` first, then falls back to `jj config get user.email`
/// (jj stores its own user identity and is common in this project). Best-effort:
/// returns `None` when neither tool is installed or has the value configured.
fn contact_email() -> Option<String> {
    vcs_email("git", &["config", "--get", "user.email"])
        .or_else(|| vcs_email("jj", &["config", "get", "user.email"]))
}

/// Runs `<program> <args...>` and returns its trimmed, non-empty stdout on a
/// successful exit, or `None` otherwise.
fn vcs_email(program: &str, args: &[&str]) -> Option<String> {
    let output = std::process::Command::new(program)
        .args(args)
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let email = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if email.is_empty() { None } else { Some(email) }
}
