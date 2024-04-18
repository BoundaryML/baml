use colored::*;
use std::{
    borrow::Borrow,
    fs::{self, File},
    process::Stdio,
    sync::Arc,
};

use tokio::{
    io::{self, AsyncReadExt, AsyncWriteExt},
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
    forward_port: u16,
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
                let promise = forward_to_port(forward_port, message);

                if let Some(message) = ipc_comms::handle_message(message) {
                    state.add_message(message);
                    _ = promise.await;
                } else {
                    _ = promise.await;
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
    forward_port: u16,
) -> tokio::io::Result<()> {
    let (listener, port) = start_server().await?;

    // Spawn a separate task for the TCP server
    let server_state = state.clone();
    tokio::spawn(async move {
        loop {
            let (socket, _) = listener.accept().await.unwrap();
            let state = server_state.clone();
            tokio::spawn(handle_connection(socket, state, forward_port));
        }
    });

    runner.add_ipc_to_command(&mut shell_command, port);

    let mut cmd = build_shell_command(shell_command.clone());

    // We don't need this - too noisy. We can append to stdout logs later or print it if there was an error.
    // println!(
    //     "{}",
    //     format!("Running pytest with args: {:?}", cmd).dimmed()
    // );
    // Create a directory in the temp folder
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
        "Verbose logs available at: ".dimmed(),
        stdout_file_path.display().to_string().dimmed(),
        stderr_file_path.display().to_string().dimmed()
    );

    let stdout_file = File::create(&stdout_file_path)?;
    let stderr_file = File::create(&stderr_file_path)?;

    println!("{}", format!("Running tests using: {:?}", &cmd,));
    let mut child = cmd
        .envs(runner.env_vars().into_iter())
        .current_dir(language_root_dir)
        .env("BOUNDARY_IPC_PORT", port.to_string())
        .stdout(Stdio::from(stdout_file))
        .stderr(Stdio::from(stderr_file))
        .spawn()
        .unwrap_or_else(|_| panic!("failed to spawn child process {}", shell_command.join(" ")));

    // Print state every 2 seconds while pytest is running
    let mut interval = time::interval(time::Duration::from_millis(500));
    while child.try_wait()?.is_none() {
        interval.tick().await;
        let mut state = state.lock().await;
        if let Some(message) = state.sync() {
            println!("{}", message);
        }
    }

    {
        let mut state = state.lock().await;
        state.sync();

        // Create a symlink to the baml directory
        println!("{}", state.to_string())
    }

    // exit also with the same status only if the exit codes are
    // 2, 3, 4 https://docs.pytest.org/en/latest/reference/exit-codes.html
    match child.wait_with_output() {
        Ok(output) => {
            on_finish_test(
                output,
                state.lock().await.borrow(),
                stdout_file_path,
                stderr_file_path,
            )
            .await
        }
        Err(e) => {
            eprintln!("Failed to execute command: {}", e);
            Err(e)
        }
    }
}

pub(crate) fn run_test_with_forward(
    runner: TestRunner,
    language_root_dir: std::path::PathBuf,
    state: RunState,
    shell_command: Vec<String>,
    forward_port: u16,
) -> Result<(), CliError> {
    let rt = tokio::runtime::Runtime::new().unwrap();

    let state = Arc::new(Mutex::new(state));

    rt.block_on(run_and_update_state(
        runner,
        language_root_dir,
        state,
        shell_command,
        forward_port,
    ))
    .map_err(|e| format!("Failed to run tests!\n{}", e).into())
}

async fn forward_to_port(port: u16, message: &str) -> tokio::io::Result<()> {
    const HOST: &str = "127.0.0.1";
    // Forward message to the port.
    let mut stream = TcpStream::connect(format!("{}:{}", HOST, port)).await?;
    stream.write_all(message.as_bytes()).await?;
    stream.write_all(b"<BAML_END_MSG>\n").await?;
    stream.flush().await
}
