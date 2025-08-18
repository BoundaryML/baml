use anyhow::{Context, Result};
use language_server::recording::MessageReplayer;
use lsp_server::{Connection, Message};
use std::path::Path;
use std::process::{Command, Stdio};

use crate::metadata::SessionMetadata;
use crate::recorder::ProxyRecorder;

pub struct LspProxy {
    lsp_command: Vec<String>,
}

impl LspProxy {
    pub fn new(lsp_command: Vec<String>) -> Result<Self> {
        if lsp_command.is_empty() {
            anyhow::bail!("No LSP command provided");
        }
        Ok(Self { lsp_command })
    }

    /// Run the proxy in recording mode
    pub fn run_record_mode<P: AsRef<Path>>(&self, output_file: P) -> Result<()> {
        // Write metadata as first line
        let metadata = SessionMetadata::new(self.lsp_command.clone());
        std::fs::write(&output_file, metadata.to_json_line()?)?;

        // Create recorder (this will append to the existing file)
        let recorder = ProxyRecorder::new(&output_file)
            .context("Failed to create message recorder")?;

        // Spawn LSP process
        let mut lsp_process = Command::new(&self.lsp_command[0])
            .args(&self.lsp_command[1..])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .with_context(|| format!("Failed to spawn LSP command: {:?}", self.lsp_command))?;

        let lsp_stdin = lsp_process.stdin.take().context("Failed to get LSP stdin")?;
        let lsp_stdout = lsp_process.stdout.take().context("Failed to get LSP stdout")?;

        tracing::info!("Starting LSP proxy in record mode, output: {:?}", output_file.as_ref());

        // Create connection to client (stdin/stdout)
        let (client_connection, client_io_threads) = Connection::stdio();

        // Create connection to LSP process
        let (lsp_connection, lsp_io_threads) = Connection::stdio_with(lsp_stdout, lsp_stdin);

        // Message forwarding loop
        loop {
            crossbeam_channel::select! {
                // Forward client -> LSP messages
                recv(client_connection.receiver) -> msg => {
                    match msg {
                        Ok(message) => {
                            // Record incoming message
                            if let Err(e) = recorder.record_incoming(&message) {
                                tracing::error!("Failed to record incoming message: {}", e);
                            }

                            // Forward to LSP
                            if lsp_connection.sender.send(message).is_err() {
                                break;
                            }
                        }
                        Err(_) => break, // Client disconnected
                    }
                }

                // Forward LSP -> client messages  
                recv(lsp_connection.receiver) -> msg => {
                    match msg {
                        Ok(message) => {
                            // Record outgoing message
                            if let Err(e) = recorder.record_outgoing(&message) {
                                tracing::error!("Failed to record outgoing message: {}", e);
                            }

                            // Forward to client
                            if client_connection.sender.send(message).is_err() {
                                break;
                            }
                        }
                        Err(_) => break, // LSP disconnected
                    }
                }
            }
        }

        // Clean up
        drop(client_connection);
        drop(lsp_connection);
        
        client_io_threads.join()?;
        lsp_io_threads.join()?;

        let _ = lsp_process.wait();

        tracing::info!("Recording session completed");
        Ok(())
    }

    /// Run the proxy in replay mode
    pub fn run_replay_mode<P: AsRef<Path>>(&self, session_file: P) -> Result<()> {
        // Read and validate metadata
        let content = std::fs::read_to_string(&session_file)?;
        let mut lines = content.lines();
        
        let metadata_line = lines.next()
            .context("Empty session file")?;
        
        let metadata = SessionMetadata::from_json_line(metadata_line)?
            .context("Session file missing metadata")?;

        tracing::info!("Replaying session recorded with command: {:?}", metadata.lsp_command);
        tracing::info!("Current LSP command: {:?}", self.lsp_command);

        // Create replayer from file
        let replayer = MessageReplayer::from_file(&session_file)?;

        // Spawn LSP process
        let mut lsp_process = Command::new(&self.lsp_command[0])
            .args(&self.lsp_command[1..])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .with_context(|| format!("Failed to spawn LSP command: {:?}", self.lsp_command))?;

        let lsp_stdin = lsp_process.stdin.take().context("Failed to get LSP stdin")?;
        let lsp_stdout = lsp_process.stdout.take().context("Failed to get LSP stdout")?;

        tracing::info!("Starting LSP proxy in replay mode, session: {:?}", session_file.as_ref());

        // Create connection to LSP process
        let (lsp_connection, lsp_io_threads) = Connection::stdio_with(lsp_stdout, lsp_stdin);

        // Set up channel for replayed messages
        let (replay_tx, replay_rx) = crossbeam_channel::unbounded();

        // Replay messages in a separate thread
        let replayer_handle = std::thread::spawn(move || {
            replayer.replay_to_channel(&replay_tx)
        });

        // Forward replayed messages to LSP
        let mut message_count = 0;
        for message in replay_rx {
            message_count += 1;
            tracing::debug!("Replaying message {}: {:?}", message_count, message);

            if lsp_connection.sender.send(message).is_err() {
                tracing::error!("Failed to send message to LSP");
                break;
            }

            // Also log any responses from LSP
            if let Ok(response) = lsp_connection.receiver.try_recv() {
                tracing::debug!("LSP response: {:?}", response);
            }
        }

        tracing::info!("Finished replaying {} messages", message_count);

        // Wait for replayer thread
        if let Err(e) = replayer_handle.join().unwrap() {
            tracing::error!("Replay failed: {}", e);
        }

        // Give LSP some time to process final messages
        std::thread::sleep(std::time::Duration::from_millis(500));

        // Drain any remaining responses
        while let Ok(response) = lsp_connection.receiver.try_recv() {
            tracing::debug!("Final LSP response: {:?}", response);
        }

        tracing::info!("Replay completed. LSP process may still be running for inspection.");

        // Wait for LSP process to finish or be terminated
        let _ = lsp_process.wait();

        Ok(())
    }
}