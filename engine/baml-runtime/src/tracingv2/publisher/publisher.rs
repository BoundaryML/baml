use baml_types::tracing::events::TraceEvent;
use futures::StreamExt;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::time::{self, Duration};

// Bring in our definitions for our event types and upload objects.
// (Again these are assumed to come from baml_types or your own modules.)
use baml_types::rpc::{TraceEventBatch, TraceEventUploadRequest};

/// Global publisher channel.
/// When the module is first used, we create an unbounded channel and then spawn the publisher task.
/// (We use `tokio::spawn` on native and `wasm_bindgen_futures::spawn_local` on wasm.)
pub static PUBLISHING_CHANNEL: once_cell::sync::Lazy<mpsc::UnboundedSender<Arc<TraceEvent>>> =
    once_cell::sync::Lazy::new(|| {
        let (tx, rx) = mpsc::unbounded_channel::<Arc<TraceEvent>>();
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
    rx: mpsc::UnboundedReceiver<Arc<TraceEvent>>,
}

impl TracePublisher {
    pub fn new(rx: mpsc::UnboundedReceiver<Arc<TraceEvent>>) -> Self {
        Self { rx }
    }

    /// Runs the publisher loop.
    ///
    /// The loop collects incoming events until either a batch reaches 1024 events
    /// or 5 seconds elapse. Then it calls `process_batch` on the collected events.
    pub async fn run(&mut self) {
        let mut buffer: Vec<Arc<TraceEvent>> = Vec::new();
        let mut tick_interval = time::interval(Duration::from_secs(3));

        loop {
            tokio::select! {
                // Receive a new event.
                Some(event) = self.rx.recv() => {
                    buffer.push(event);
                    if buffer.len() >= 1024 {
                        self.process_batch(std::mem::take(&mut buffer)).await;
                    }
                }
                // Every 5 seconds, process any events that have been buffered.
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
    async fn process_batch(&self, batch: Vec<Arc<TraceEvent>>) {
        // Assemble the upload request structure.
        let upload_request = TraceEventUploadRequest::V1 {
            project_id: "project123".to_string(),
            trace_event_batch: TraceEventBatch {
                events: batch.iter().map(|e| e.clone()).collect(),
            },
        };

        // Serialize to JSON.
        #[cfg(not(target_arch = "wasm32"))]
        if let Ok(json) = serde_json::to_string(&upload_request) {
            // Write the batch to a file asynchronously.
            use tokio::fs::OpenOptions;
            if let Ok(mut file) = OpenOptions::new()
                .create(true)
                .append(true)
                .open("/tmp/trace_events.json")
                .await
            {
                use tokio::io::AsyncWriteExt;
                if let Err(e) = file.write_all(format!("{}\n", json).as_bytes()).await {
                    log::error!("Failed to write to trace file: {}", e);
                }
            }
        }

        // Upload via HTTP with retry logic.
        let client = reqwest::Client::new();
        let mut retries = 3;
        while retries > 0 {
            match client
                .post("https://3vwc8vlts7.execute-api.us-east-1.amazonaws.com/v1/baml-traces")
                .json(&upload_request)
                .send()
                .await
            {
                Ok(response) => {
                    log::info!("Upload completed with status {}", response.status());
                    break;
                }
                Err(e) => {
                    log::error!("Upload failed: {}", e);
                    retries -= 1;
                    if retries > 0 {
                        time::sleep(Duration::from_secs(1)).await;
                    }
                }
            }
        }
    }
}
