//! `baml telemetry` — inspect and change the CLI's telemetry preference.
//! Direct rewrite of Next.js's `packages/next/src/cli/next-telemetry.ts`:
//! same subcommand shape (`status` / `enable` / `disable`), same color
//! palette (green for Enabled, red for Disabled, cyan for the docs URL),
//! same output structure.

use anyhow::Result;
use baml_shell::{Shell, ThemeStyle};
use clap::{Args, ValueEnum};

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
    /// What to do. Omit to just show current status.
    #[arg(value_enum, default_value_t = TelemetryAction::Status)]
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
        let mut shell = Shell::new();
        let t = telemetry::Telemetry::load();
        // Snapshot the pre-change state so `disable` can distinguish
        // "already off" from "just turned off" (matches Next's UX).
        let was_enabled = t.is_enabled();

        match self.action {
            TelemetryAction::Status => {}
            TelemetryAction::Enable => {
                let path = t.set_enabled(true);
                shell.writeln_out_styled(ThemeStyle::Good, "Success!")?;
                if let Some(path) = &path {
                    writeln!(
                        shell.out(),
                        "Your preference has been saved to {}.",
                        path.display()
                    )?;
                }
            }
            TelemetryAction::Disable => {
                let path = t.set_enabled(false);
                if was_enabled {
                    shell.write_out_styled(ThemeStyle::Good, "Success!")?;
                    if let Some(path) = &path {
                        writeln!(
                            shell.out(),
                            " Your preference has been saved to {}.",
                            path.display()
                        )?;
                    } else {
                        writeln!(shell.out())?;
                    }
                } else {
                    shell.warn("BAML CLI telemetry is already disabled.")?;
                }
            }
        }

        // Re-load so the printed status reflects the post-change value.
        let t = telemetry::Telemetry::load();
        print_status(&mut shell, &t)?;
        Ok(ExitCode::Success)
    }
}

/// Print the "Status: Enabled/Disabled" block plus a link to the docs page.
/// Format matches Next.js's `nextTelemetry`: bold header, colored status,
/// short body, cyan "Learn more" URL.
fn print_status(shell: &mut Shell, t: &telemetry::Telemetry) -> Result<()> {
    let is_enabled = t.is_enabled();
    writeln!(shell.out())?;
    shell.writeln_out_styled(ThemeStyle::Heading, "BAML CLI Telemetry")?;
    write!(shell.out(), "\nStatus: ")?;
    if is_enabled {
        shell.writeln_out_styled(ThemeStyle::Good, "Enabled")?;
    } else {
        shell.writeln_out_styled(ThemeStyle::Bad, "Disabled")?;
    }
    write!(shell.out(), "Config: ")?;
    shell.writeln_out_styled(ThemeStyle::Dim, t.config_path().display())?;

    if is_enabled {
        writeln!(
            shell.out(),
            "\nBAML telemetry is completely anonymous. Thank you for participating!"
        )?;
    } else {
        writeln!(
            shell.out(),
            "\nYou have opted out of BAML's anonymous telemetry program.\n\
             No data will be collected from your machine."
        )?;
    }

    write!(shell.out(), "\nLearn more: ")?;
    shell.writeln_out_styled(ThemeStyle::Note, telemetry::TELEMETRY_URL)?;
    Ok(())
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
