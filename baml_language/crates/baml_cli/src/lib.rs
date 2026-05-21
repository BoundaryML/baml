// This crate provides the BAML CLI, including the LSP server and
// standalone execution via `baml run`.
#![allow(
    dead_code,
    unreachable_pub,
    clippy::pedantic,
    clippy::nursery,
    clippy::empty_structs_with_brackets,
    clippy::exit
)]

pub(crate) mod commands;
pub(crate) mod describe_command;
#[cfg(test)]
mod describe_command_tests;
pub(crate) mod format;
pub(crate) mod generate;
pub(crate) mod grep_command;
pub(crate) mod init_command;
pub(crate) mod lsp;
pub(crate) mod pack_command;
pub(crate) mod project_load;
pub mod reporter;
pub(crate) mod run_command;
pub(crate) mod test_command;
pub(crate) mod test_filter;

// TODO: These modules are disabled for now as they depend on baml_runtime
// pub(crate) mod api_client;
// pub(crate) mod auth;
// pub(crate) mod colordiff;
// pub(crate) mod format;
// pub(crate) mod propelauth;
// pub(crate) mod tui;

use anyhow::Result;

#[derive(Debug, Clone)]
pub enum ExitCode {
    Success,
    InvalidArgs,
    Other,
    HumanEvalRequired,
    TestFailure,
    TestCancelled,
    NoTestsRun,
    // `baml run` / packed-binary target failures: unhandled user-code
    // errors or input-shape errors (bad `--json-args`, unknown function,
    // etc.). Mapped to `1` to match Unix CLI convention and the packed
    // runtime (`baml_pack_host` returns `ExitCode::FAILURE = 1` for the
    // same conditions); BEP-027 §"Exit codes" only mandates non-zero,
    // so we pick the conventional code and keep the two runtimes aligned.
    TargetError,
}

impl From<ExitCode> for i32 {
    fn from(exit_code: ExitCode) -> Self {
        match exit_code {
            // All tests passed
            ExitCode::Success => 0,
            // All tests completed, but some required human evaluation, OR
            // `baml run` / packed-binary target failure
            ExitCode::HumanEvalRequired | ExitCode::TargetError => 1,
            // Some tests failed
            ExitCode::TestFailure => 2,
            // Execution was interrupted
            ExitCode::TestCancelled => 3,
            // Some internal error occurred
            ExitCode::Other | ExitCode::InvalidArgs => 4,
            // No tests were found
            ExitCode::NoTestsRun => 5,
        }
    }
}

impl From<ExitCode> for u32 {
    fn from(exit_code: ExitCode) -> Self {
        match exit_code {
            // All tests passed
            ExitCode::Success => 0,
            // All tests completed, but some required human evaluation, OR
            // `baml run` / packed-binary target failure
            ExitCode::HumanEvalRequired | ExitCode::TargetError => 1,
            // Some tests failed
            ExitCode::TestFailure => 2,
            // Execution was interrupted
            ExitCode::TestCancelled => 3,
            // Some internal error occurred
            ExitCode::Other | ExitCode::InvalidArgs => 4,
            // No tests were found
            ExitCode::NoTestsRun => 5,
        }
    }
}

/// Run the CLI with the given arguments.
///
/// Dispatches to one of: `run`, `describe`, `generate`, `grep`, `test`,
/// `format`, or `language-server`. `baml run` is the top-level entry for
/// standalone execution.
pub fn run_cli(argv: Vec<String>) -> Result<ExitCode> {
    let cli = commands::RuntimeCli::parse_from_smart(argv);
    cli.run()
}

// TODO: Original run_cli that used RuntimeCliDefaults is commented out
// pub fn run_cli(
//     argv: Vec<String>,
//     caller_type: baml_runtime::RuntimeCliDefaults,
// ) -> Result<ExitCode> {
//     let mut cli = commands::RuntimeCli::parse_from_smart(argv);
//     if !matches!(cli.command, commands::Commands::Test(_)) {
//         // We only need to set the exit handlers if we're not running tests
//         // and the caller is Python.
//         if caller_type.output_type == baml_types::GeneratorOutputType::PythonPydantic {
//             set_exit_handlers();
//         }
//     }
//
//     let exit_code = cli.run(caller_type)?;
//
//     match exit_code {
//         ExitCode::Success => Ok(ExitCode::Success),
//         // Use the same exit code mechanism as Clap uses for invalid arguments (error.exit())
//         _ => std::process::exit(exit_code.into()),
//     }
// }

fn set_exit_handlers() {
    // SIGINT (Ctrl+C) Handling Implementation
    let (interrupt_send, interrupt_recv) = std::sync::mpsc::channel();

    ctrlc::set_handler(move || {
        #[allow(clippy::print_stderr)]
        {
            eprintln!("\nShutting Down BAML...");
        }
        interrupt_send.send(()).ok();
    })
    .expect("Error setting Ctrl-C handler");

    std::thread::spawn(move || {
        if interrupt_recv.recv().is_ok() {
            std::process::exit(130);
        }
    });
}

#[cfg(test)]
mod exit_code_tests {
    use super::*;

    /// `baml run` target failures and the packed runtime
    /// (`baml_pack_host`) must agree on the shell exit code. The packed
    /// host returns `std::process::ExitCode::FAILURE = 1`; the
    /// `TargetError` variant exists precisely so `baml run` lines up at
    /// `1` for the same condition. BEP-027 §"Exit codes" only mandates
    /// non-zero, so we pick the conventional `1` and keep both runtimes
    /// emitting it.
    #[test]
    fn target_error_maps_to_one() {
        assert_eq!(i32::from(ExitCode::TargetError), 1);
        assert_eq!(u32::from(ExitCode::TargetError), 1);
    }

    /// `Other` continues to mean "internal error" (used by describe /
    /// generate / format / test internal failures). It must not collide
    /// with the new `TargetError`.
    #[test]
    fn other_stays_at_four() {
        assert_eq!(i32::from(ExitCode::Other), 4);
    }
}
