use std::{
    path::{Path, PathBuf},
    process::Command,
};

use crate::errors::CliError;

use super::{
    run_test_with_forward::run_test_with_forward, run_test_with_watcher::run_test_with_watcher,
    test_state::RunState,
};

pub(crate) fn run_tests(
    state: RunState,
    shell_setup: Option<String>,
    baml_dir: &PathBuf,
    selected_tests: &Vec<(String, String, String)>,
    playground_port: Option<u16>,
) -> Result<(), CliError> {
    let (start_command, mut test_command) = match shell_setup {
        Some(setup) => {
            // Parse as cli args
            let mut args = shellwords::split(&setup)
                .map_err(|e| format!("Failed to parse shell setup command: {}", e.to_string()))?;
            let command = args.remove(0);
            args.push("pytest".into());
            (command, args)
        }
        None => ("pytest".into(), vec![]),
    };
    test_command.push("-v".into());
    test_command.push("-s".into());
    test_command.push("--color".into());
    test_command.push("yes".into());
    // Rootdir
    test_command.push("--rootdir".into());
    test_command.push(
        (Path::join(&baml_dir, "../baml_client"))
            .to_string_lossy()
            .to_string(),
    );
    test_command.push("baml_client".into());
    test_command.push("-s".into());
    selected_tests.iter().for_each(|(function, test, r#impl)| {
        test_command.push("--pytest-baml-include".into());
        test_command.push(format!("{}:{}:{}", function, r#impl, test));
    });

    let mut cmd = Command::new(start_command.clone());
    match playground_port {
        Some(port) => {
            cmd.args(test_command);
            run_test_with_forward(state, cmd, port)
        }
        None => {
            cmd.args(test_command);
            run_test_with_watcher(state, cmd)
        }
    }
}
