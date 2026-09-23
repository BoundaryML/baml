use btel_bcs::wire::PrepareUploadsRequest;
use wiremock::Request;

pub(crate) fn prepare_requests(requests: &[Request]) -> Vec<PrepareUploadsRequest> {
    requests
        .iter()
        .filter(|request| {
            request.method == "POST" && request.url.path().ends_with("uploads:prepare")
        })
        .map(|request| serde_json::from_slice(&request.body).unwrap())
        .collect()
}
