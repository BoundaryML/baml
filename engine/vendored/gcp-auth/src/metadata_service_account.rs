use std::str;
use std::sync::Arc;

use async_trait::async_trait;
use bytes::Bytes;
use http_body_util::Full;
use hyper::{Method, Request};
use tokio::sync::RwLock;
use tracing::{debug, instrument, Level};

use crate::types::{HttpClient, Token};
use crate::{Error, TokenProvider};

/// A token provider that queries the GCP instance metadata server for access tokens
///
/// See https://cloud.google.com/compute/docs/metadata/predefined-metadata-keys for details.
#[derive(Debug)]
pub struct MetadataServiceAccount {
    client: HttpClient,
    project_id: Arc<str>,
    token: RwLock<Arc<Token>>,
}

impl MetadataServiceAccount {
    /// Check that the GCP instance metadata server is available and try to fetch a token
    pub async fn new() -> Result<Self, Error> {
        let client = HttpClient::new()?;
        Self::with_client(&client).await
    }

    pub(crate) async fn with_client(client: &HttpClient) -> Result<Self, Error> {
        debug!("try to fetch token from GCP instance metadata server");
        let token = RwLock::new(Self::fetch_token(client).await?);

        debug!("getting project ID from GCP instance metadata server");
        let req = metadata_request(DEFAULT_PROJECT_ID_GCP_URI);
        let body = client.request(req, "MetadataServiceAccount").await?;
        let project_id = match str::from_utf8(&body) {
            Ok(s) if !s.is_empty() => Arc::from(s),
            Ok(_) => {
                return Err(Error::Str(
                    "empty project ID from GCP instance metadata server",
                ))
            }
            Err(_) => {
                return Err(Error::Str(
                    "received invalid UTF-8 project ID from GCP instance metadata server",
                ))
            }
        };

        Ok(Self {
            client: client.clone(),
            project_id,
            token,
        })
    }

    #[instrument(level = Level::DEBUG, skip(client))]
    async fn fetch_token(client: &HttpClient) -> Result<Arc<Token>, Error> {
        client
            .token(
                &|| metadata_request(DEFAULT_TOKEN_GCP_URI),
                "MetadataServiceAccount",
            )
            .await
    }
}

#[async_trait]
impl TokenProvider for MetadataServiceAccount {
    async fn token(&self, _scopes: &[&str]) -> Result<Arc<Token>, Error> {
        let token = self.token.read().await.clone();
        if !token.has_expired() {
            return Ok(token);
        }

        let mut locked = self.token.write().await;
        let token = Self::fetch_token(&self.client).await?;
        *locked = token.clone();
        Ok(token)
    }

    async fn project_id(&self) -> Result<Arc<str>, Error> {
        Ok(self.project_id.clone())
    }
}

fn metadata_request(uri: &str) -> Request<Full<Bytes>> {
    Request::builder()
        .method(Method::GET)
        .uri(uri)
        .header("Metadata-Flavor", "Google")
        .body(Full::from(Bytes::new()))
        .unwrap()
}

// https://cloud.google.com/compute/docs/metadata/predefined-metadata-keys
const DEFAULT_PROJECT_ID_GCP_URI: &str =
    "http://metadata.google.internal/computeMetadata/v1/project/project-id";
const DEFAULT_TOKEN_GCP_URI: &str =
    "http://metadata.google.internal/computeMetadata/v1/instance/service-accounts/default/token";
