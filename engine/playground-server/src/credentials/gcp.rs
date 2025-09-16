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
        Ok(CacheableResource::NotModified) => LoadGcpCredsResponse::Error {
            name: "CredentialLoadError".to_string(),
            message: "TODO implement cache handling".to_string(),
        },
        Ok(CacheableResource::New { data: headers, .. }) => LoadGcpCredsResponse::Ok {
            access_token: headers
                .get("Authorization")
                .unwrap()
                .to_str()
                .unwrap()
                .to_string()
                .replace("Bearer ", ""),
            project_id: headers
                .get("X-Goog-User-Project")
                .unwrap()
                .to_str()
                .unwrap()
                .to_string(),
        },
        Err(e) => LoadGcpCredsResponse::Error {
            name: "CredentialLoadError".to_string(),
            message: e.to_string(),
        },
    }
}
