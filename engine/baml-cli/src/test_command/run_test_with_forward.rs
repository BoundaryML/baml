use colored::*;
use std::{
    fs::{self, File},
    process::Stdio,
    sync::Arc,
};

use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::Mutex,
    time,
};

use crate::{errors::CliError, shell::build_shell_command};

use super::{ipc_comms, test_state::RunState};

async fn handle_connection(
    mut stream: TcpStream,
    state: Arc<Mutex<RunState>>,
    forward_port: u16,
) -> tokio::io::Result<()> {
    let mut buffer = String::new();
    // Read message from Python test
    let n = stream.read_to_string(&mut buffer).await?;
    if n == 0 {
        return Ok(());
    }
    let promise = forward_to_port(forward_port, &buffer);

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
            _ = promise.await;
            return Err(tokio::io::Error::new(
                tokio::io::ErrorKind::InvalidData,
                format!("Failed to parse message: {}", message),
            ));
        }
    }
    _ = promise.await;

    Ok(())
}

async fn start_server() -> std::io::Result<(TcpListener, u16)> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let port = listener.local_addr()?.port();
    Ok((listener, port))
}

async fn run_pytest_and_update_state(
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

    // Append args to pytest to enable IPC
    // println!("Running pytest with args: {:?}", cmd);
    // cmd.arg("--pytest-baml-ipc");
    // cmd.arg(format!("{}", port));
    shell_command.push("--pytest-baml-ipc".into());
    shell_command.push(format!("{}", port));

    let mut cmd = build_shell_command(shell_command);

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

    let mut child = cmd
        .stdout(Stdio::from(stdout_file))
        .stderr(Stdio::from(stderr_file))
        .spawn()
        .expect("failed to spawn pytest");

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
            if let Some(code) = output.status.code() {
                if ![0, 1].contains(&code) {
                    println!("{}",
                        format!("Testing failed with exit code {}. Open the output logs below for more details", code).bright_red().bold()
                    );
                    println!(
                        "{}\n{}",
                        stdout_file_path.display().to_string().dimmed(),
                        stderr_file_path.display().to_string().dimmed()
                    );
                    // exit the process
                    std::process::exit(code);
                }
            }
            output
        }
        Err(e) => {
            eprintln!("Failed to execute command: {}", e);
            return Err(e);
        }
    };

    Ok(())
}

pub(crate) fn run_test_with_forward(
    state: RunState,
    shell_command: Vec<String>,
    forward_port: u16,
) -> Result<(), CliError> {
    let rt = tokio::runtime::Runtime::new().unwrap();

    let state = Arc::new(Mutex::new(state));

    rt.block_on(run_pytest_and_update_state(
        state,
        shell_command,
        forward_port,
    ))
    .map_err(|e| format!("Failed to run pytest: {}", e).into())
}

async fn forward_to_port(port: u16, message: &String) -> tokio::io::Result<()> {
    const HOST: &str = "127.0.0.1";
    // Forward message to the port.
    let mut stream = TcpStream::connect(format!("{}:{}", HOST, port)).await?;
    stream.write_all(message.as_bytes()).await
}
