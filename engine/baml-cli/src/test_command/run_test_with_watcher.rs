use colored::*;
use std::{
    borrow::Borrow,
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

use crate::{
    errors::CliError, shell::build_shell_command, test_command::test_state::on_finish_test,
};

use super::{ipc_comms, run_tests::TestRunner, test_state::RunState};

async fn handle_connection(
    mut stream: TcpStream,
    state: Arc<Mutex<RunState>>,
) -> tokio::io::Result<()> {
    let mut buffer = Vec::new();
    let mut read_buf = [0u8; 1024]; // Adjust buffer size as needed

    loop {
        let n = stream.read(&mut read_buf).await?;
        if n == 0 {
            // End of stream
            break;
        }

        // Append this chunk to the buffer
        buffer.extend_from_slice(&read_buf[..n]);

        // Convert buffer to string for processing (assuming UTF-8 encoded data)
        let buffer_str = match String::from_utf8(buffer.clone()) {
            Ok(s) => s,
            Err(_) => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Stream data is not valid UTF-8",
                ));
            }
        };

        // Split based on the message separator and collect into a Vec for iteration
        let mut messages: Vec<&str> = buffer_str.split("<BAML_END_MSG>").collect();

        // If the last message is incomplete (doesn't end with our separator),
        // it will be processed in the next chunk. Remove it from processing now.
        let incomplete_message = if buffer_str.ends_with("<BAML_END_MSG>") {
            ""
        } else {
            messages.pop().unwrap_or("")
        };

        {
            // Lock state for update
            let mut state = state.lock().await;

            for message in messages {
                let message = message.trim();
                if message.is_empty() {
                    continue;
                }

                if let Some(message) = ipc_comms::handle_message(message) {
                    state.add_message(message);
                } else {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("Failed to parse message: {}", message),
                    ));
                }
            }
        }

        // Prepare buffer for next read, preserving the incomplete message if there is one
        buffer = incomplete_message.as_bytes().to_vec();
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

    let mut cmd = build_shell_command(shell_command.clone());

    println!("{}", format!("Running test with args: {:?}", &cmd).dimmed());

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
        .env("BOUNDARY_IPC_PORT", port.to_string())
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
    on_finish_test(
        output,
        state.lock().await.borrow(),
        stdout_file_path,
        stderr_file_path,
    )
    .await
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
    .map_err(|e| CliError::StringError(format!("Failed to run tests!\n{}", e)))?;

    Ok(())
}
