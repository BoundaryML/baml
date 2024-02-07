use std::path::PathBuf;

use crate::errors::CliError;

use super::{
    run_test_with_forward::run_test_with_forward, run_test_with_watcher::run_test_with_watcher,
    test_state::RunState,
};

pub(crate) fn run_tests<T: AsRef<str>>(
    state: RunState,
    output_path: &PathBuf,
    test_command: T,
    selected_tests: &Vec<(String, String, String)>,
    playground_port: Option<u16>,
) -> Result<(), CliError> {
    let mut test_command = shellwords::split(test_command.as_ref())
        .map_err(|e| CliError::StringError(format!("Failed to parse test command: {}", e)))?;

    ["-v", "-s", "--color=yes", "baml_client", "-s", "--rootdir"]
        .iter()
        .for_each(|arg| test_command.push(arg.to_string()));

    test_command.push(output_path.to_string_lossy().to_string());

    selected_tests.iter().for_each(|(function, test, r#impl)| {
        test_command.push("--pytest-baml-include".into());
        test_command.push(format!("{}:{}:{}", function, r#impl, test));
    });

    match playground_port {
        Some(port) => run_test_with_forward(state, test_command, port),
        None => run_test_with_watcher(state, test_command),
    }
}
