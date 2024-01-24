use std::{
    fs::{self, File},
    io::{self, Write},
    process::{Command, Stdio},
    sync::Arc,
};

use tokio::{
    io::AsyncReadExt,
    net::{TcpListener, TcpStream},
    sync::Mutex,
    time,
};

use crate::errors::CliError;

use super::{ipc_comms, test_state::RunState};

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

async fn run_pytest_and_update_state(
    state: Arc<Mutex<RunState>>,
    mut cmd: Command,
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

    // Append args to pytest to enable IPC
    // println!("Running pytest with args: {:?}", cmd);
    // cmd.arg("--pytest-baml-ipc");
    // cmd.arg(format!("{}", port));
    cmd.args(["--pytest-baml-ipc", &format!("{}", port)]);

    println!("Running pytest with args: {:?}", cmd);

    // Create a directory in the temp folder
    let temp_dir = std::env::temp_dir();
    let baml_tests_dir = temp_dir.join("baml/tests");
    fs::create_dir_all(&baml_tests_dir)?;

    // Create files for stdout and stderr
    let human_readable_time = chrono::Local::now().format("%Y-%m-%d-%H-%M-%S");
    let stdout_file_path = baml_tests_dir.join(format!("{}-stdout.log", human_readable_time));
    let stderr_file_path = baml_tests_dir.join(format!("{}-stderr.log", human_readable_time));

    println!("Verbose logs available at: {}", stdout_file_path.display());

    let stdout_file = File::create(&stdout_file_path)?;
    let stderr_file = File::create(stderr_file_path)?;

    let mut child = cmd
        .stdout(Stdio::from(stdout_file))
        .stderr(Stdio::from(stderr_file))
        .spawn()
        .expect("failed to spawn pytest");

    // Print state every 2 seconds while pytest is running
    let mut interval = time::interval(time::Duration::from_millis(500));
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
    println!("Pytest finished with status: {}", output.status);
    println!("Verbose logs available at: {}", stdout_file_path.display());

    Ok(())
}

pub(crate) fn run_test_with_watcher(state: RunState, cmd: Command) -> Result<(), CliError> {
    let rt = tokio::runtime::Runtime::new().unwrap();

    let state = Arc::new(Mutex::new(state));
    rt.block_on(run_pytest_and_update_state(state, cmd))
        .map_err(|e| format!("Failed to run pytest: {}", e).into())
}
