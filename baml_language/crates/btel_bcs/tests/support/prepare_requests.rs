use btel_bcs::wire::PrepareUploadsRequest;
use wiremock::Request;

/// The prepares the server received, by file sequence. Delivery prepares
/// files concurrently, so they arrive in any order; the server applies a
/// recording's files in sequence order however they arrive.
pub(crate) fn prepare_requests(requests: &[Request]) -> Vec<PrepareUploadsRequest> {
    let mut prepares: Vec<PrepareUploadsRequest> = requests
        .iter()
        .filter(|request| {
            request.method == "POST" && request.url.path().ends_with("uploads:prepare")
        })
        .map(|request| serde_json::from_slice(&request.body).unwrap())
        .collect();
    // Stable, so retries of one file keep the order they were sent in.
    prepares.sort_by_key(|request| request.recording.recording_file_sequence);
    prepares
}
