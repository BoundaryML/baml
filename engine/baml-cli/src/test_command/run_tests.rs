use std::{
    collections::HashMap,
    fmt::format,
    io::{self, BufRead, BufReader, Read},
    net::TcpListener,
    ops::Deref,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    },
    thread,
    time::Duration,
};
use tokio::sync::Mutex;

use log::{debug, info};
use serde::de;

use crate::errors::CliError;

use super::{
    ipc_comms::handle_message, run_test_with_forward::run_test_with_forward,
    run_test_with_watcher::run_test_with_watcher, test_state::RunState,
};

fn start_server() -> std::io::Result<(TcpListener, u16)> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    listener.set_nonblocking(true)?;
    let port = listener.local_addr()?.port();
    Ok((listener, port))
}

fn run_tests_local(mut cmd: Command) -> Result<(), CliError> {
    let mut child = match cmd.spawn() {
        Ok(child) => child,
        Err(e) => {
            eprintln!("Failed to start pytest: {}", e);
            return Err("Failed to run pytest".into());
        }
    };

    let status = child.wait().map_err(|e| {
        eprintln!("Failed to wait on pytest: {}", e);
        "Failed to run pytest"
    })?;
    if !status.success() {
        return Err("pytest failed".into());
    }
    Ok(())
}

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
