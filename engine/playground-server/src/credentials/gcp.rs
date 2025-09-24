use axum::http::Extensions;
use google_cloud_auth::credentials::CacheableResource;

use crate::api::{LoadGcpCredsRequest, LoadGcpCredsResponse};

pub async fn load_gcp_credentials(_request: LoadGcpCredsRequest) -> LoadGcpCredsResponse {
    let Ok(creds) = google_cloud_auth::credentials::Builder::default().build() else {
        return LoadGcpCredsResponse::Error {
            name: "CredentialLoadError".to_string(),
            message: "Failed to load GCP credentials".to_string(),
        };
    };
    let headers = creds.headers(Extensions::new()).await;
    match headers {
        // google-cloud-auth is never supposed to return NotModified
        // see https://github.com/googleapis/google-cloud-rust/issues/3361
        Ok(CacheableResource::NotModified) => LoadGcpCredsResponse::Error {
            name: "CredentialLoadError".to_string(),
            message: "unreachable!".to_string(),
        },
        Ok(CacheableResource::New { data: headers, .. }) => {
            let access_token = match headers
                .get("Authorization")
                .and_then(|h| h.to_str().ok())
                .map(|s| s.replace("Bearer ", ""))
            {
                Some(token) if !token.is_empty() => token,
                _ => {
                    return LoadGcpCredsResponse::Error {
                        name: "CredentialLoadError".to_string(),
                        message: "MissingAccessTokenError: missing or invalid Authorization header"
                            .to_string(),
                    }
                }
            };

            let project_id =
                match headers
                    .get("X-Goog-User-Project")
                    .and_then(|h| h.to_str().ok())
                    .map(|s| s.to_string())
                {
                    Some(project) if !project.is_empty() => project,
                    _ => return LoadGcpCredsResponse::Error {
                        name: "CredentialLoadError".to_string(),
                        message:
                            "MissingProjectIdError: missing or invalid X-Goog-User-Project header"
                                .to_string(),
                    },
                };

            LoadGcpCredsResponse::Ok {
                access_token,
                project_id,
            }
        }
        Err(e) => LoadGcpCredsResponse::Error {
            name: "CredentialLoadError".to_string(),
            message: format!("Have you run `gcloud auth application-default login`? Failed to load credentials: {:?}", e),
        },
    }
}
