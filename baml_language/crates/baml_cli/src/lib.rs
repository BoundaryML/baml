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
pub(crate) mod hot_reload;
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

/// Process exit status. Every command uses the same three; anything finer is
/// in the command's output. `baml run` alone can also exit with the code the
/// program chose.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitCode {
    Success,
    /// The command ran and failed: compile errors, failing tests, an
    /// unformatted file, a failed query, a network error.
    Failure,
    /// The invocation was wrong: bad flags or arguments.
    Usage,
}

impl ExitCode {
    /// The status an error that escaped a command exits with.
    pub fn for_error(error: &anyhow::Error) -> Self {
        if error.is::<UsageError>() {
            Self::Usage
        } else {
            Self::Failure
        }
    }
}

impl From<ExitCode> for u8 {
    fn from(exit_code: ExitCode) -> Self {
        match exit_code {
            ExitCode::Success => 0,
            ExitCode::Failure => 1,
            ExitCode::Usage => 2,
        }
    }
}

/// An invocation that is wrong in a way clap cannot express, such as flags
/// that conflict only in combination. Exits like clap's own errors.
#[derive(Debug)]
pub struct UsageError(String);

impl UsageError {
    pub(crate) fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

impl std::fmt::Display for UsageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for UsageError {}

macro_rules! usage_bail {
    ($($arg:tt)*) => {
        return Err($crate::UsageError::new(format!($($arg)*)).into())
    };
}
pub(crate) use usage_bail;

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
