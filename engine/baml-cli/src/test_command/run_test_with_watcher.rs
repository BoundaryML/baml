use colored::*;
use std::{
    fs::{self, File},
    io::{self, Write},
    process::Stdio,
    sync::Arc,
};

use tokio::{
    io::AsyncReadExt,
    net::{TcpListener, TcpStream},
    sync::Mutex,
    time,
};

use crate::{errors::CliError, shell::build_shell_command};

use super::{ipc_comms, run_tests::TestRunner, test_state::RunState};

async fn handle_connection(
    mut stream: TcpStream,
    state: Arc<Mutex<RunState>>,
) -> tokio::io::Result<()> {
    let mut buffer = String::new();
    // Read message from Python test
    let n = stream.read_to_string(&mut buffer).await?;
    if n == 0 {
        return Ok(());
    }
    // buffer may have multiple messages in it, so we need to split it
    // on the message separator <END_MSG>\n
    let messages = buffer.split("<END_MSG>\n");
    let mut state = state.lock().await;
    for message in messages {
        if message.is_empty() {
            continue;
        }
        if let Some(message) = ipc_comms::handle_message(message) {
            state.add_message(message);
        } else {
            return Err(tokio::io::Error::new(
                tokio::io::ErrorKind::InvalidData,
                format!("Failed to parse message: {}", message),
            ));
        }
    }
    Ok(())
}

async fn start_server() -> std::io::Result<(TcpListener, u16)> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let port = listener.local_addr()?.port();
    Ok((listener, port))
}

async fn run_and_update_state(
    runner: TestRunner,
    language_root_dir: std::path::PathBuf,
    state: Arc<Mutex<RunState>>,
    mut shell_command: Vec<String>,
) -> tokio::io::Result<()> {
    let (listener, port) = start_server().await?;

    // Spawn a separate task for the TCP server
    let server_state = state.clone();
    tokio::spawn(async move {
        loop {
            let (socket, _) = listener.accept().await.unwrap();
            let state = server_state.clone();
            tokio::spawn(handle_connection(socket, state));
        }
    });

    // Add the port to the shell command
    runner.add_ipc_to_command(&mut shell_command, port);

    let mut cmd = build_shell_command(shell_command);

    println!("{}", format!("Running test with args: {:?}", cmd).dimmed());

    // Create a directory in the temp folder
    // Load from environment variable (BAML_TEST_LOGS) if set or use temp_dir
    let baml_tests_dir = match std::env::var("BAML_TEST_LOGS") {
        Ok(dir) => std::path::PathBuf::from(dir),
        Err(_) => std::env::temp_dir().join("baml/tests"),
    };
    fs::create_dir_all(&baml_tests_dir)?;

    // Create files for stdout and stderr
    let human_readable_time = chrono::Local::now().format("%Y-%m-%d-%H-%M-%S");
    let stdout_file_path = baml_tests_dir.join(format!("{}-stdout.log", human_readable_time));
    let stderr_file_path = baml_tests_dir.join(format!("{}-stderr.log", human_readable_time));

    println!(
        "{}\n{}\n{}",
        "Verbose logs available at:".dimmed(),
        stdout_file_path.display().to_string().dimmed(),
        stderr_file_path.display().to_string().dimmed()
    );

    let stdout_file = File::create(&stdout_file_path)?;
    let stderr_file = File::create(&stderr_file_path)?;

    let mut child = cmd
        .envs(runner.env_vars().into_iter())
        .env("BAML_IPC_PORT", port.to_string())
        .current_dir(language_root_dir)
        .stdout(Stdio::from(stdout_file))
        .stderr(Stdio::from(stderr_file))
        .spawn()
        .expect("failed to spawn test process");

    // Print state every 2 seconds while pytest/jest is running
    let mut interval = time::interval(time::Duration::from_millis(1000));
    let mut last_print_lines = 0;
    while child.try_wait()?.is_none() {
        interval.tick().await;
        let (state_string, extra_message) = {
            let mut state = state.lock().await;
            let message = state.sync();
            (state.to_string(), message)
        };

        // Clear previous print characters
        for _ in 0..last_print_lines {
            print!("\x1B[A\x1B[2K"); // ANSI escape code: Move up and clear line
        }
        print!("\r");
        match extra_message {
            Some(message) => print!("{}\n{}", message, state_string),
            None => print!("{}", state_string),
        }

        // Update the length of the last printed state
        last_print_lines = state_string.lines().count();

        // Flush stdout to ensure immediate output
        let _ = io::stdout().flush();
    }

    // Optionally, you can handle the output after the subprocess has finished
    let output = child.wait_with_output()?;
    if let Some(code) = output.status.code() {
        // Open the stderr file and check if it has any content
        let stderr_content = tokio::fs::read_to_string(&stderr_file_path).await?;
        let stdout_content = tokio::fs::read_to_string(&stdout_file_path).await?;
        // Pytest exits with 1 even if it ran fine but had some tests failing (we should suppress this via a pytest plugin) so we dont mark as failure
        // But we could also get exit code 1 from other things like infisical CLI being absent, or python not being found.
        // so check the stderr for any other issues.
        if ![0, 1].contains(&code) {
            println!("\n####### STDOUT Logs ########\n{}", stdout_content);
            if !stderr_content.is_empty() {
                println!("\n####### STDERR Logs ########\n{}", stderr_content);
            }

            println!(
                "{}",
                format!(
                    "Testing failed with exit code {}. Output logs were printed above this line.",
                    code
                )
            );
            println!(
                "{}\n{}",
                stdout_file_path.display().to_string().dimmed(),
                stderr_file_path.display().to_string().dimmed()
            );
        }

        // if stderr is not empty and exit code is 1, we also have an issue
        if code == 1 && (!stderr_content.is_empty() || !stdout_content.is_empty()) {
            // Don't say the test failed since the exit code 1 may just be pytest saying some tests failed.
            // print stdout
            println!(
                "\n####### STDOUT Logs for this test ########\n{}",
                stdout_content
            );
            println!(
                "{}",
                "Some tests failed or there was a problem running the tests."
                    .bright_red()
                    .bold()
            );
            println!("{}", stderr_content.bright_red().bold());

            println!(
                "\n{}\n{}",
                stdout_file_path.display().to_string().dimmed(),
                stderr_file_path.display().to_string().dimmed()
            );
        }
    }

    Ok(())
}

pub(crate) fn run_test_with_watcher(
    runner: TestRunner,
    language_root_dir: std::path::PathBuf,
    state: RunState,
    shell_command: Vec<String>,
) -> Result<(), CliError> {
    let rt = tokio::runtime::Runtime::new().unwrap();

    let state = Arc::new(Mutex::new(state));
    rt.block_on(run_and_update_state(
        runner,
        language_root_dir,
        state,
        shell_command,
    ))
    .map_err(|e| format!("Failed to run tests: {}", e).into())
}
