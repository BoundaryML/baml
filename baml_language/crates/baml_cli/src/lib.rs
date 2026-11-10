#![warn(clippy::disallowed_methods)]
// This crate provides the BAML CLI: project build/check/test commands and
// standalone execution via `baml run`.
#![allow(
    dead_code,
    unreachable_pub,
    clippy::pedantic,
    clippy::nursery,
    clippy::empty_structs_with_brackets,
    clippy::exit
)]

pub(crate) mod agent_command;
pub(crate) mod auth;
pub(crate) mod bytecode_cache;
#[cfg(test)]
mod cache_test_support;
pub(crate) mod check_command;
mod cloud_config;
pub(crate) mod codequality_command;
pub(crate) mod commands;
pub(crate) mod describe_command;
#[cfg(test)]
mod describe_command_tests;
pub(crate) mod diagnostics_cache;
#[cfg(test)]
mod diagnostics_cache_oracle;
mod embed_telemetry;
pub(crate) mod format;
pub(crate) mod generate;
pub(crate) mod help_command;
pub(crate) mod ide_command;
pub(crate) mod init_command;
pub(crate) mod log_output;
pub(crate) mod lsp;
pub(crate) mod optional_duration;
pub(crate) mod output;
pub(crate) mod pack_command;
pub(crate) mod paint;
pub(crate) mod playground_command;
pub(crate) mod project_load;
pub(crate) mod project_session;
pub(crate) mod recordings;
pub mod reporter;
pub(crate) mod run_command;
pub(crate) mod runtime_telemetry;
pub(crate) mod shutdown;
pub(crate) mod skill_check;
pub(crate) mod telemetry;
pub(crate) mod telemetry_command;
pub(crate) mod test_command;
pub(crate) mod test_filter;
pub(crate) mod util;

use anyhow::Result;

/// Process exit status. `0` success, `1` failure, `2` usage; higher codes are
/// per command and documented in that command's help.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitCode {
    Success,
    /// The command ran and failed: compile errors, an unformatted file, a
    /// failing `run` target, a network error.
    Failure,
    /// The invocation was wrong: bad flags or arguments.
    Usage,
    // `baml test`. Documented in `baml help test`.
    /// All tests completed, but some required human evaluation.
    HumanEvalRequired,
    TestFailure,
    TestCancelled,
    NoTestsRun,
    // `baml query`. Documented in `baml help query`; its `3` is a budget, not
    // a cancelled test run.
    /// The result is missing evidence: rows returned, completeness lost.
    QueryIncomplete,
    /// Invalid SQL, or a relation the caller is not authorized to read.
    QueryInvalid,
    /// A query budget (rows, wall-clock, decoded bytes) was exhausted.
    QueryBudgetExhausted,
    /// The query was cancelled before it completed.
    QueryCancelled,
    /// Any other query failure, including setup failures before execution.
    QueryFailed,
}

impl From<ExitCode> for u8 {
    fn from(exit_code: ExitCode) -> Self {
        match exit_code {
            ExitCode::Success => 0,
            ExitCode::Failure | ExitCode::HumanEvalRequired => 1,
            ExitCode::Usage => 2,
            ExitCode::TestFailure => 2,
            ExitCode::TestCancelled => 3,
            ExitCode::NoTestsRun => 5,
            ExitCode::QueryIncomplete => 1,
            ExitCode::QueryInvalid => 2,
            ExitCode::QueryBudgetExhausted => 3,
            ExitCode::QueryCancelled => 4,
            ExitCode::QueryFailed => 5,
        }
    }
}

impl From<ExitCode> for i32 {
    fn from(exit_code: ExitCode) -> Self {
        u8::from(exit_code).into()
    }
}

impl From<ExitCode> for u32 {
    fn from(exit_code: ExitCode) -> Self {
        u8::from(exit_code).into()
    }
}

/// Run the CLI with the given arguments.
///
/// Dispatches to one of: `run`, `check`, `generate`, `test`, `pack`,
/// `fmt`, ... `baml run` is the top-level entry for standalone execution.
pub fn run_cli(argv: Vec<String>) -> Result<ExitCode> {
    let cli = commands::RuntimeCli::parse_from_smart(argv);
    cli.run()
}

#[cfg(test)]
mod exit_code_tests {
    use super::*;

    /// `baml run` target failures and the packed runtime (`baml_pack_host`,
    /// which returns `std::process::ExitCode::FAILURE`) agree on `1`.
    #[test]
    fn failure_maps_to_one() {
        assert_eq!(i32::from(ExitCode::Failure), 1);
    }

    #[test]
    fn usage_matches_clap() {
        assert_eq!(i32::from(ExitCode::Usage), 2);
    }
}
