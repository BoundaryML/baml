use baml_types::rpc::{StudioTraceEventBatch, TraceEventUploadRequest};
use baml_types::tracing::events::TraceEvent;
use core::time::Duration;
use futures::StreamExt;
use std::sync::Arc;
use tokio::sync::mpsc;
#[cfg(not(target_family = "wasm"))]
use tokio::time::*;
#[cfg(target_family = "wasm")]
use wasmtimer::tokio::*;
pub enum PublisherMessage {
    Trace(Arc<TraceEvent>),
    Flush(tokio::sync::oneshot::Sender<()>),
}

/// Global publisher channel.
/// When the module is first used, we create an unbounded channel and then spawn the publisher task.
pub static PUBLISHING_CHANNEL: once_cell::sync::Lazy<mpsc::UnboundedSender<PublisherMessage>> =
    once_cell::sync::Lazy::new(|| {
        let (tx, rx) = mpsc::unbounded_channel::<PublisherMessage>();
        // Spawn the publisher task.
        #[cfg(not(target_arch = "wasm32"))]
        {
            tokio::spawn(async move {
                let mut publisher = TracePublisher::new(rx);
                publisher.run().await;
            });
        }
        #[cfg(target_arch = "wasm32")]
        {
            wasm_bindgen_futures::spawn_local(async move {
                let mut publisher = TracePublisher::new(rx);
                publisher.run().await;
            });
        }
        tx
    });

pub struct TracePublisher {
    rx: mpsc::UnboundedReceiver<PublisherMessage>,
}

impl TracePublisher {
    pub fn new(rx: mpsc::UnboundedReceiver<PublisherMessage>) -> Self {
        Self { rx }
    }

    /// Runs the publisher loop.
    ///
    /// The loop collects incoming events until a batch condition is reached, a timer expires,
    /// or a flush command is received.
    pub async fn run(&mut self) {
        let mut buffer: Vec<Arc<TraceEvent>> = Vec::new();
        let mut tick_interval = interval(Duration::from_secs(2));

        loop {
            tokio::select! {
                // Process any incoming command or event.
                Some(message) = self.rx.recv() => {
                    match message {
                        PublisherMessage::Trace(event) => {
                            buffer.push(event);
                            if buffer.len() >= 3 {
                                self.process_batch(std::mem::take(&mut buffer)).await;
                            }
                        },
                        PublisherMessage::Flush(flush_ack) => {
                            log::info!("Got a flush event");
                            // Flush the current buffer if it has any pending events.
                            if !buffer.is_empty() {
                                self.process_batch(std::mem::take(&mut buffer)).await;
                            }
                            // Signal flush completion.
                            let _ = flush_ack.send(());
                        },
                    }
                }
                // Periodic flush of pending events.
                _ = tick_interval.tick() => {
                    if !buffer.is_empty() {
                        self.process_batch(std::mem::take(&mut buffer)).await;
                    }
                }
            }
        }
    }

    /// Process a batch of events.
    ///
    /// In this example we:
    ///   1. Serialize the events into JSON.
    ///   2. Append the JSON to a file (using async file I/O on macOS).
    ///   3. Post the JSON to an HTTP API with up to 3 retries.
    async fn process_batch(&self, _batch: Vec<Arc<TraceEvent>>) {}
}

// Note, the library we are using doesnt seem to work well for flushing in Node
// but that's ok since noone uses our wasm build in node for logging.
// https://github.com/whizsid/wasmtimer-rs/issues/26
pub async fn flush() {
    log::info!("flushing");
    let (ack_tx, ack_rx) = tokio::sync::oneshot::channel();
    if let Err(e) = PUBLISHING_CHANNEL.send(PublisherMessage::Flush(ack_tx)) {
        log::error!("Failed to send flush request: {:?}", e);
        return;
    }

    // Set a timeout to avoid waiting indefinitely.
    let timeout_duration = Duration::from_secs(3);

    match timeout(timeout_duration, ack_rx).await {
        Ok(Ok(())) => log::info!("Flush acknowledged successfully."),
        Ok(Err(e)) => log::error!("Flush acknowledgement error: {:?}", e),
        Err(_) => log::error!("Flush timed out after {:?}", timeout_duration),
    }
}
