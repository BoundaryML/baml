//! `baml telemetry` — inspect and change the CLI's telemetry preference.
//! Direct rewrite of Next.js's `packages/next/src/cli/next-telemetry.ts`:
//! same subcommand shape (`status` / `enable` / `disable`), same color
//! palette (green for Enabled, red for Disabled, cyan for the docs URL),
//! same output structure.

use std::{
    io::{self, Write},
    path::PathBuf,
};

use anyhow::{Context, Result};
use clap::{Args, Subcommand};
use serde::Serialize;

use crate::{
    ExitCode,
    output::{Command, CommandContext, Outcome, Report, Style},
    telemetry,
};

/// `baml __flush-telemetry` — the hidden entry point for the detached
/// child process that drains the on-disk telemetry queue (see
/// `telemetry::queue`). Spawned automatically on command exit and on the
/// 10-minute rotation timer; never invoked by humans. Takes no arguments:
/// the child sweeps the whole queue directory, so it also drains any
/// backlog left by earlier invocations whose sends failed.
#[derive(Args, Debug)]
pub(crate) struct FlushTelemetryArgs {}

impl FlushTelemetryArgs {
    pub(crate) fn run(&self) -> Result<ExitCode> {
        telemetry::run_flush_child();
        Ok(ExitCode::Success)
    }
}

/// Show or change BAML CLI telemetry preferences.
///
/// Run without a subcommand to see the current status, config file path, and
/// docs link. Use `baml telemetry disable` to opt out, or
/// `baml telemetry enable` to opt back in. See <https://boundaryml.com/telemetry>.
#[derive(Args, Debug)]
pub(crate) struct TelemetryArgs {
    #[command(subcommand)]
    pub command: Option<TelemetryCommand>,
}

#[derive(Subcommand, Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TelemetryCommand {
    #[command(
        about = "Show the current telemetry preference",
        after_long_help = "Examples:\n  Show the current setting:\n    baml telemetry status"
    )]
    Status,
    #[command(
        about = "Enable BAML CLI telemetry",
        after_long_help = "Examples:\n  Opt in:\n    baml telemetry enable"
    )]
    Enable,
    #[command(
        about = "Disable BAML CLI telemetry",
        after_long_help = "Examples:\n  Opt out:\n    baml telemetry disable"
    )]
    Disable,
}

/// The telemetry preference after the action ran.
#[derive(Debug, Serialize)]
pub(crate) struct TelemetryStatus {
    pub enabled: bool,
    pub config: PathBuf,
}

/// Format matches Next.js's `nextTelemetry`: bold header, colored status,
/// short body, cyan "Learn more" URL.
impl Report for TelemetryStatus {
    fn text(&self, out: &mut dyn Write, style: &Style) -> io::Result<()> {
        writeln!(out, "{}", style.bold("BAML CLI Telemetry"))?;
        let status_word = if self.enabled {
            style.good("Enabled")
        } else {
            style.bad("Disabled")
        };
        writeln!(out, "\nStatus: {status_word}")?;
        writeln!(out, "Config: {}", style.dim(self.config.display()))?;
        if self.enabled {
            writeln!(
                out,
                "\nBAML telemetry is completely anonymous. Thank you for participating!"
            )?;
        } else {
            writeln!(
                out,
                "\nYou have opted out of BAML's anonymous telemetry program.\n\
                 No data will be collected from your machine."
            )?;
        }
        writeln!(out, "\nLearn more: {}", style.url(telemetry::TELEMETRY_URL))
    }
}

impl Command for TelemetryArgs {
    type Output = TelemetryStatus;

    fn run(&self, _cx: &CommandContext) -> Result<Outcome<TelemetryStatus>> {
        let t = telemetry::Telemetry::load();
        let reporter = crate::reporter::Reporter::new();
        // Snapshot the pre-change state so `disable` can distinguish
        // "already off" from "just turned off" (matches Next's UX).
        let was_enabled = t.is_enabled();
        let save = |enabled: bool| {
            t.set_enabled(enabled).with_context(|| {
                format!(
                    "could not save the telemetry preference to {}",
                    t.config_path().display()
                )
            })
        };
        match self.command.unwrap_or(TelemetryCommand::Status) {
            TelemetryCommand::Status => {}
            TelemetryCommand::Enable => {
                reporter.status("Saved", save(true)?.display().to_string());
            }
            TelemetryCommand::Disable if was_enabled => {
                reporter.status("Saved", save(false)?.display().to_string());
            }
            // `DO_NOT_TRACK` can mask an enabled preference, so the opt-out
            // is still written.
            TelemetryCommand::Disable => {
                save(false)?;
                reporter.status("Unchanged", "BAML CLI telemetry is already disabled");
            }
        }

        // Re-load so the reported status reflects the post-change value.
        let t = telemetry::Telemetry::load();
        Ok(Outcome::ok(TelemetryStatus {
            enabled: t.is_enabled(),
            config: t.config_path().to_path_buf(),
        }))
    }
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::*;

    /// A convenient wrapper so we can `TelemetryArgs::parse_from(...)`
    /// without spinning up the whole top-level clap tree.
    #[derive(Parser, Debug)]
    #[command(no_binary_name = true)]
    struct TestCli {
        #[command(flatten)]
        args: TelemetryArgs,
    }

    /// `baml telemetry` with no subcommand shows the status.
    #[test]
    fn no_subcommand_parses() {
        let cli = TestCli::try_parse_from(Vec::<String>::new()).unwrap();
        assert_eq!(cli.args.command, None);
    }

    #[test]
    fn subcommands_parse() {
        for (name, expected) in [
            ("enable", TelemetryCommand::Enable),
            ("disable", TelemetryCommand::Disable),
            ("status", TelemetryCommand::Status),
        ] {
            let cli = TestCli::try_parse_from([name]).unwrap();
            assert_eq!(cli.args.command, Some(expected));
        }
    }
}
