pub(crate) mod api_client;
pub(crate) mod auth;
pub(crate) mod colordiff;
pub(crate) mod commands;
pub(crate) mod deploy;
pub(crate) mod format;
pub(crate) mod lsp;
pub(crate) mod propelauth;
pub(crate) mod tui;
use anyhow::Result;
use clap::Parser;

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

pub fn run_cli(
    argv: Vec<String>,
    caller_type: baml_runtime::RuntimeCliDefaults,
) -> Result<ExitCode> {
    let mut cli = commands::RuntimeCli::parse_from(argv);
    if !matches!(cli.command, commands::Commands::Test(_)) {
        // We only need to set the exit handlers if we're not running tests
        // and the caller is Python.
        if caller_type.output_type == baml_types::GeneratorOutputType::PythonPydantic {
            set_exit_handlers();
        }
    }

    let exit_code = cli.run(caller_type)?;

    match exit_code {
        ExitCode::Success => Ok(ExitCode::Success),
        // Use the same exit code mechanism as Clap uses for invalid arguments (error.exit())
        _ => std::process::exit(exit_code.into()),
    }
}

fn set_exit_handlers() {
    // SIGINT (Ctrl+C) Handling Implementation, an approach from @revidious
    //
    // Background:
    // When running BAML through Python, we face a challenge where Python's default SIGINT handling
    // can interfere with graceful shutdown. This is because:
    // 1. Python has its own signal handlers that may conflict with Rust's
    // 2. The PyO3 runtime can sometimes mask or delay interrupt signals
    // 3. We need to ensure clean shutdown across the Python/Rust boundary
    //
    // Solution:
    // We implement a custom signal handling mechanism using Rust's ctrlc crate that:
    // 1. Bypasses Python's signal handling entirely
    // 2. Provides consistent behavior across platforms
    // 3. Ensures graceful shutdown with proper exit codes
    // Note: While eliminating the root cause of SIGINT handling conflicts would be ideal,
    // the source appears to be deeply embedded in BAML's architecture and PyO3's runtime.
    // A proper fix would require extensive changes to how BAML handles signals across the
    // Python/Rust boundary. For now, this workaround provides reliable interrupt handling
    // without requiring major architectural changes but welp, this is a hacky solution.

    // Create a channel for communicating between the signal handler and main thread
    // This is necessary because signal handlers run in a separate context and
    // need a safe way to communicate with the main program
    let (interrupt_send, interrupt_recv) = std::sync::mpsc::channel();

    // Install our custom Ctrl+C handler
    // This will run in a separate thread when SIGINT is received
    ctrlc::set_handler(move || {
        println!("\nShutting Down BAML...");
        // Notify the main thread through the channel
        // Using ok() to ignore send errors if the receiver is already dropped
        interrupt_send.send(()).ok();
    })
    .expect("Error setting Ctrl-C handler");

    // Monitor for interrupt signals in a separate thread
    // This is necessary because we can't directly exit from the signal handler.

    std::thread::spawn(move || {
        if interrupt_recv.recv().is_ok() {
            // Exit with code 130 (128 + SIGINT's signal number 2)
            // This is the standard Unix convention for processes terminated by SIGINT
            std::process::exit(130);
        }
    });
}
