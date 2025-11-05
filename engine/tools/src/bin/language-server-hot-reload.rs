use std::{
    collections::VecDeque,
    io::{self, Write},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{mpsc, Arc, Mutex},
    time::{Duration, SystemTime},
};

use anyhow::Result;
use notify_debouncer_full::{
    new_debouncer,
    notify::{RecursiveMode, Watcher},
    DebounceEventResult, Debouncer, FileIdMap,
};
use tokio::{
    io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWriteExt, BufReader},
    process::{Child as TokioChild, Command as TokioCommand},
    sync::watch,
};
use tracing::{info, warn};
use tracing_subscriber::{EnvFilter, FmtSubscriber};

const MAX_STDIN_BUFFER_SIZE: usize = 1000;

#[derive(Clone, Debug)]
struct StdinMessage {
    timestamp: SystemTime,
    data: Vec<u8>,
}

struct HotReloader {
    binary_path: PathBuf,
    current_process: Option<TokioChild>,
    shutdown_tx: watch::Sender<bool>,
    stdin_buffer: Arc<Mutex<VecDeque<StdinMessage>>>,
}

impl HotReloader {
    fn new() -> Self {
        let (shutdown_tx, _) = watch::channel(false);
        Self {
            binary_path: PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .unwrap()
                .join("target/debug/baml-cli"),
            current_process: None,
            shutdown_tx,
            stdin_buffer: Arc::new(Mutex::new(VecDeque::new())),
        }
    }

    fn record_stdin(&self, data: Vec<u8>) {
        let mut buffer = self.stdin_buffer.lock().unwrap();
        let message = StdinMessage {
            timestamp: SystemTime::now(),
            data,
        };

        buffer.push_back(message);

        // Keep buffer size under control
        while buffer.len() > MAX_STDIN_BUFFER_SIZE {
            buffer.pop_front();
        }
    }

    async fn replay_stdin(&self, process: &mut TokioChild) -> Result<()> {
        if let Some(stdin) = process.stdin.as_mut() {
            // Clone messages to avoid holding lock across await
            let messages: Vec<StdinMessage> = {
                let buffer = self.stdin_buffer.lock().unwrap();
                info!("Replaying {} stdin messages", buffer.len());
                buffer.iter().cloned().collect()
            };

            for message in messages.iter() {
                if let Err(e) = stdin.write_all(&message.data).await {
                    warn!("Failed to replay stdin message: {}", e);
                    break;
                }
            }
        }
        Ok(())
    }

    async fn start_process(&mut self, args: Vec<String>) -> Result<()> {
        if let Some(mut child) = self.current_process.take() {
            let _ = child.kill().await;
            let _ = child.wait().await;
        }

        let mut cmd = TokioCommand::new(&self.binary_path);
        cmd.args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit());

        let mut child = cmd.spawn()?;

        // Replay recorded stdin messages
        self.replay_stdin(&mut child).await?;

        // Start stdin forwarding task
        self.start_stdin_forwarding(&mut child).await?;

        self.current_process = Some(child);
        Ok(())
    }

    async fn start_stdin_forwarding(&self, child: &mut TokioChild) -> Result<()> {
        if let Some(child_stdin) = child.stdin.take() {
            let stdin_buffer = Arc::clone(&self.stdin_buffer);

            tokio::spawn(async move {
                let mut stdin = tokio::io::stdin();
                let mut child_stdin = child_stdin;
                let mut buffer = [0u8; 8192];

                loop {
                    match stdin.read(&mut buffer).await {
                        Ok(0) => break, // EOF
                        Ok(n) => {
                            let data = buffer[0..n].to_vec();

                            // Record the input
                            {
                                let stdin_buffer_clone = Arc::clone(&stdin_buffer);
                                let mut buf = stdin_buffer_clone.lock().unwrap();
                                let message = StdinMessage {
                                    timestamp: SystemTime::now(),
                                    data: data.clone(),
                                };
                                buf.push_back(message);

                                while buf.len() > MAX_STDIN_BUFFER_SIZE {
                                    buf.pop_front();
                                }
                            }

                            // Forward to child
                            if let Err(e) = child_stdin.write_all(&data).await {
                                warn!("Failed to write to child stdin: {}", e);
                                break;
                            }

                            if let Err(e) = child_stdin.flush().await {
                                warn!("Failed to flush child stdin: {}", e);
                                break;
                            }
                        }
                        Err(e) => {
                            warn!("Failed to read from stdin: {}", e);
                            break;
                        }
                    }
                }
            });
        }

        Ok(())
    }

    async fn stop_process(&mut self) -> Result<()> {
        if let Some(mut child) = self.current_process.take() {
            let _ = child.kill().await;
            let _ = child.wait().await;
        }
        Ok(())
    }

    async fn run(&mut self, args: Vec<String>) -> Result<()> {
        let (tx, rx) = mpsc::channel();

        let mut debouncer = new_debouncer(
            Duration::from_millis(250),
            None,
            move |result: DebounceEventResult| {
                if let Err(e) = tx.send(result) {
                    warn!("Failed to send watch event: {}", e);
                }
            },
        )?;

        let binary_path = Path::new(&self.binary_path);
        let parent_dir = binary_path.parent().ok_or_else(|| {
            anyhow::anyhow!(
                "Binary path {} has no parent directory",
                self.binary_path.display()
            )
        })?;

        debouncer
            .watcher()
            .watch(parent_dir, RecursiveMode::NonRecursive)?;

        info!("Starting hot-reload for {}", self.binary_path.display());
        self.start_process(args.clone()).await?;

        loop {
            match rx.try_recv() {
                Ok(Ok(events)) => {
                    let binary_path = Path::new(&self.binary_path);
                    for event in events {
                        // Check if the event is for our target binary
                        if event.paths.iter().any(|path| path == binary_path) {
                            info!("Binary changed, reloading...");
                            self.start_process(args.clone()).await?;
                            break;
                        }
                    }
                }
                Ok(Err(errors)) => {
                    for error in errors {
                        warn!("Watch error: {}", error);
                    }
                }
                Err(mpsc::TryRecvError::Empty) => {
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    break;
                }
            }

            if let Some(ref mut child) = self.current_process {
                if let Some(status) = child.try_wait()? {
                    info!("Process exited with status: {}", status);
                    if !status.success() {
                        info!("Process failed, waiting for binary update...");
                        self.current_process = None;
                    }
                }
            }
        }

        self.stop_process().await?;
        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let subscriber = FmtSubscriber::builder()
        .with_env_filter(
            EnvFilter::from_default_env().add_directive("language_server_hot_reload=info".parse()?),
        )
        .finish();

    tracing::subscriber::set_global_default(subscriber)?;

    let args: Vec<String> = std::env::args().skip(1).collect();

    let mut reloader = HotReloader::new();
    reloader.run(args).await?;

    Ok(())
}
