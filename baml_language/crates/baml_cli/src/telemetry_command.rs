//! `baml telemetry` — inspect and change the CLI's telemetry preference.
//! Direct rewrite of Next.js's `packages/next/src/cli/next-telemetry.ts`:
//! same subcommand shape (`status` / `enable` / `disable`), same color
//! palette (green for Enabled, red for Disabled, cyan for the docs URL),
//! same output structure.

// `println!` here is the primary UX of the subcommand: printing the current
// status and confirmation of a state change. The workspace-wide ban on
// `print*!` exists to catch stray debug prints, not to break intentional
// user-facing output.
#![allow(clippy::print_stdout)]

use anyhow::Result;
use clap::{Args, ValueEnum};
use console::style;

use crate::{ExitCode, telemetry};

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

impl TelemetryArgs {
    pub(crate) fn run(&self) -> Result<ExitCode> {
        let t = telemetry::Telemetry::load();
        // Snapshot the pre-change state so `disable` can distinguish
        // "already off" from "just turned off" (matches Next's UX).
        let was_enabled = t.is_enabled();

        match self.action {
            TelemetryAction::Status => {}
            TelemetryAction::Enable => {
                let path = t.set_enabled(true);
                println!("{}", style("Success!").cyan());
                if let Some(path) = &path {
                    println!("Your preference has been saved to {}.", path.display());
                }
            }
            TelemetryAction::Disable => {
                let path = t.set_enabled(false);
                if was_enabled {
                    if let Some(path) = &path {
                        println!(
                            "{} Your preference has been saved to {}.",
                            style("Success!").cyan(),
                            path.display()
                        );
                    } else {
                        println!("{}", style("Success!").cyan());
                    }
                } else {
                    println!(
                        "{}",
                        style("BAML CLI telemetry is already disabled.").yellow()
                    );
                }
            }
        }

        // Re-load so the printed status reflects the post-change value.
        let t = telemetry::Telemetry::load();
        print_status(&t);
        Ok(ExitCode::Success)
    }
}

/// Print the "Status: Enabled/Disabled" block plus a link to the docs page.
/// Format matches Next.js's `nextTelemetry`: bold header, colored status,
/// short body, cyan "Learn more" URL.
fn print_status(t: &telemetry::Telemetry) {
    let is_enabled = t.is_enabled();
    println!();
    println!("{}", style("BAML CLI Telemetry").bold());
    let status_word = if is_enabled {
        style("Enabled").green().bold().to_string()
    } else {
        style("Disabled").red().bold().to_string()
    };
    println!("\nStatus: {status_word}");
    println!("Config: {}", style(t.config_path().display()).dim());

    if is_enabled {
        println!("\nBAML telemetry is completely anonymous. Thank you for participating!");
    } else {
        println!(
            "\nYou have opted out of BAML's anonymous telemetry program.\n\
             No data will be collected from your machine."
        );
    }

    println!("\nLearn more: {}", style(telemetry::TELEMETRY_URL).cyan());
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
