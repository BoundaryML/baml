//! `baml telemetry` — inspect and change the CLI's telemetry preference.
//! Direct rewrite of Next.js's `packages/next/src/cli/next-telemetry.ts`:
//! same subcommand shape (`status` / `enable` / `disable`), same color
//! palette (green for Enabled, red for Disabled, cyan for the docs URL),
//! same output structure.

use std::{
    io::{self, Write},
    path::PathBuf,
};

use anyhow::Result;
use clap::{Args, ValueEnum};
use console::style;
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
/// Run without an action to see the current status, config file path, and
/// docs link. Use `baml telemetry disable` to opt out, or
/// `baml telemetry enable` to opt back in. See <https://boundaryml.com/telemetry>.
//
// Impl note: modeled as a positional `ValueEnum` rather than a nested
// subcommand so `baml telemetry` (no action) works cleanly as "show status"
// without fighting clap 4's default `subcommand_required = true` on
// tuple-variant subcommands. UX matches Next.js's
// `next telemetry [enable|disable|status]`.
#[derive(Args, Debug)]
pub(crate) struct TelemetryArgs {
    #[arg(
        value_enum,
        value_name = "ACTION",
        help = "Telemetry action [default: status] [possible values: status, enable, disable]",
        hide_default_value = true,
        hide_possible_values = true,
        default_value_t = TelemetryAction::Status
    )]
    pub action: TelemetryAction,
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TelemetryAction {
    /// Show the current telemetry preference.
    Status,
    /// Enable BAML CLI telemetry.
    Enable,
    /// Disable BAML CLI telemetry.
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
    fn text(&self, out: &mut dyn Write, _style: &Style) -> io::Result<()> {
        writeln!(out, "{}", style("BAML CLI Telemetry").bold())?;
        let status_word = if self.enabled {
            style("Enabled").green().bold()
        } else {
            style("Disabled").red().bold()
        };
        writeln!(out, "\nStatus: {status_word}")?;
        writeln!(out, "Config: {}", style(self.config.display()).dim())?;
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
        writeln!(
            out,
            "\nLearn more: {}",
            style(telemetry::TELEMETRY_URL).cyan()
        )
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
        let saved = |path: Option<PathBuf>| match path {
            Some(path) => reporter.status("Saved", path.display().to_string()),
            None => reporter.warning("could not save the telemetry preference"),
        };
        match self.action {
            TelemetryAction::Status => {}
            TelemetryAction::Enable => saved(t.set_enabled(true)),
            TelemetryAction::Disable if was_enabled => saved(t.set_enabled(false)),
            TelemetryAction::Disable => {
                t.set_enabled(false);
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

    /// `baml telemetry` with no action defaults to `Status`.
    #[test]
    fn no_action_parses_and_defaults_to_status() {
        let cli = TestCli::try_parse_from(Vec::<String>::new()).unwrap();
        assert_eq!(cli.args.action, TelemetryAction::Status);
    }

    /// Explicit actions parse to their respective variants.
    #[test]
    fn actions_parse() {
        let cli = TestCli::try_parse_from(["enable"]).unwrap();
        assert_eq!(cli.args.action, TelemetryAction::Enable);

        let cli = TestCli::try_parse_from(["disable"]).unwrap();
        assert_eq!(cli.args.action, TelemetryAction::Disable);

        let cli = TestCli::try_parse_from(["status"]).unwrap();
        assert_eq!(cli.args.action, TelemetryAction::Status);
    }
}
