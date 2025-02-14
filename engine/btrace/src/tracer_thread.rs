use anyhow::Context;
use baml_types::rpc::TraceEventUploadRequest;
use baml_types::tracing::TraceEventBatch;
use std::io::Write;
use std::{fs::OpenOptions, pin::pin};

use super::TraceEvent;
use tokio_stream::{wrappers::UnboundedReceiverStream, StreamExt};
pub struct TracerThread {
    rx: UnboundedReceiverStream<TraceEvent>,
}

impl TracerThread {
    pub fn new(rx: tokio::sync::mpsc::UnboundedReceiver<TraceEvent>) -> Self {
        Self {
            rx: UnboundedReceiverStream::new(rx),
        }
    }

    pub fn run(rx: tokio::sync::mpsc::UnboundedReceiver<TraceEvent>) {
        std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(
                Self {
                    rx: UnboundedReceiverStream::new(rx),
                }
                .run_impl(),
            );
        });
    }

    pub async fn run_impl(self) {
        let mut stream = pin!(self
            .rx
            .chunks_timeout(1024, std::time::Duration::from_secs(5)));

        while let Some(events) = stream.next().await {
            let upload_request = TraceEventUploadRequest::V1 {
                project_id: "project123".to_string(),
                trace_event_batch: TraceEventBatch { events },
            };

            // Serialize the upload_request to JSON
            if let Ok(json) = serde_json::to_string(&upload_request) {
                // Open the file in append mode
                if let Ok(mut file) = OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open("/tmp/trace_events.json")
                {
                    // Write the JSON string to the file
                    writeln!(file, "{}\n", json).expect("Failed to write to file");
                }
            }

            loop {
                // TODO: this impl is wrong, every time a batch of trace events is ready,
                // we should enqueue it for send, instead of blocking on send before processing the next batch
                let client = reqwest::Client::new();
                let response = client
                    .post("https://3vwc8vlts7.execute-api.us-east-1.amazonaws.com/v1/baml-traces")
                    .json(&upload_request)
                    .send()
                    .await
                    .context("Failed to upload file");

                let Ok(response) = response else {
                    continue;
                };

                log::info!("Upload completed with status {}", response.status());

                // TODO: do not bail out early
                break;
            }
        }

        log::debug!("Trace upload complete");
    }
}
