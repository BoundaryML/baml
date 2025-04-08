use anyhow::{Context, Result};
use baml_types::rpc::TraceEventUploadRequest;

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let trace_data_path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("data/trace_events.jsonl");
    log::info!("trace_data_path: {}", trace_data_path.display());

    let events =
        std::fs::read_to_string(trace_data_path).context("Failed to read trace events file")?;

    let client = reqwest::Client::new();

    for event_str in events.lines() {
        if event_str.is_empty() {
            continue;
        }

        let upload_request: TraceEventUploadRequest =
            serde_json::from_str(event_str).context("Failed to parse trace upload request")?;

        let response = client
            .post("https://3vwc8vlts7.execute-api.us-east-1.amazonaws.com/v1/baml-trace")
            .json(&upload_request)
            .send()
            .await
            .context("Failed to upload file");

        match response {
            Ok(response) => {
                log::info!("Upload completed with status {}", response.status());
            }
            Err(e) => {
                log::warn!("Failed to upload file: {}", e);
            }
        }
    }

    Ok(())
}
