use anyhow::{Context, Result};
use google_cloud_auth::credentials::{Builder, CacheableResource};
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE};
use serde_json::json;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize credentials using google-cloud-auth
    let credentials = Builder::default()
        .build()
        .context("Failed to load GCP credentials")?;

    // Get authentication headers
    let headers_result = credentials
        .headers(axum::http::Extensions::new())
        .await
        .context("Failed to get authentication headers")?;

    let http_headers = match headers_result {
        CacheableResource::New { data, .. } => data,
        CacheableResource::NotModified => {
            anyhow::bail!("Unexpected NotModified response from credentials");
        }
    };

    // Extract authorization token
    let auth_header = http_headers
        .get("Authorization")
        .and_then(|h| h.to_str().ok())
        .context("Missing Authorization header")?;

    println!("Successfully authenticated with GCP");

    // Prepare request headers
    let mut request_headers = HeaderMap::new();
    request_headers.insert(
        AUTHORIZATION,
        HeaderValue::from_str(auth_header).context("Invalid authorization header value")?,
    );
    request_headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));

    // Prepare request body
    let request_body = json!({
        "contents": [
            {
                "role": "user",
                "parts": [
                    {
                        "text": "Write a nice short story about Donkey kong and peanut butter. Keep it to 15 words or less."
                    }
                ]
            }
        ]
    });

    // Make request to Vertex AI
    let url = "https://us-central1-aiplatform.googleapis.com/v1/projects/sam-project-vertex-1/locations/us-central1/publishers/google/models/gemini-2.5-flash:generateContent";

    println!("Making request to Vertex AI...");

    let client = reqwest::Client::new();
    let response = client
        .post(url)
        .headers(request_headers)
        .json(&request_body)
        .send()
        .await
        .context("Failed to send request to Vertex AI")?;

    let status = response.status();
    let response_text = response
        .text()
        .await
        .context("Failed to read response body")?;

    println!("Response status: {}", status);
    println!("Response body: {}", response_text);

    if !status.is_success() {
        anyhow::bail!("Request failed with status {}: {}", status, response_text);
    }

    Ok(())
}
