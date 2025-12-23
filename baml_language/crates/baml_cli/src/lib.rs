// TODO: This CLI has been simplified to only support the LSP command for now.
// Other commands that depend on baml_runtime are commented out.
#![allow(
    dead_code,
    unreachable_pub,
    clippy::pedantic,
    clippy::nursery,
    clippy::empty_structs_with_brackets,
    clippy::exit
)]

pub(crate) mod commands;
pub(crate) mod lsp;

// TODO: These modules are disabled for now as they depend on baml_runtime
// pub(crate) mod api_client;
// pub(crate) mod auth;
// pub(crate) mod colordiff;
// pub(crate) mod deploy;
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
}

impl From<ExitCode> for i32 {
    fn from(exit_code: ExitCode) -> Self {
        match exit_code {
            // All tests passed
            ExitCode::Success => 0,
            // All tests completed, but some required human evaluation
            ExitCode::HumanEvalRequired => 1,
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
            // All tests completed, but some required human evaluation
            ExitCode::HumanEvalRequired => 1,
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
/// This is a simplified version that only supports the LSP command.
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
