use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use bytes::Bytes;
use http_body_util::Full;
use hyper::header::CONTENT_TYPE;
use hyper::{Method, Request};
use serde::Serialize;
use tokio::sync::RwLock;
use tracing::{debug, instrument, Level};

use crate::types::{AuthorizedUserRefreshToken, HttpClient, Token};
use crate::{Error, TokenProvider};

/// A token provider that uses the default user credentials
///
/// Reads credentials from `.config/gcloud/application_default_credentials.json` on Linux and MacOS
/// or from `%APPDATA%/gcloud/application_default_credentials.json` on Windows.
/// See [GCloud Application Default Credentials](https://cloud.google.com/docs/authentication/application-default-credentials#personal)
/// for details.
#[derive(Debug)]
pub struct ConfigDefaultCredentials {
    client: HttpClient,
    token: RwLock<Arc<Token>>,
    credentials: AuthorizedUserRefreshToken,
}

impl ConfigDefaultCredentials {
    /// Check for user credentials in the default location and try to deserialize them
    pub async fn new() -> Result<Self, Error> {
        let client = HttpClient::new()?;
        Self::with_client(&client).await
    }

    pub(crate) async fn with_client(client: &HttpClient) -> Result<Self, Error> {
        debug!("try to load credentials from configuration");
        let mut config_path = config_dir()?;
        config_path.push(USER_CREDENTIALS_PATH);
        debug!(config = config_path.to_str(), "reading configuration file");

        let credentials = AuthorizedUserRefreshToken::from_file(&config_path)?;
        debug!(project = ?credentials.quota_project_id, client = credentials.client_id, "found user credentials");

        Ok(Self {
            client: client.clone(),
            token: RwLock::new(Self::fetch_token(&credentials, client).await?),
            credentials,
        })
    }

    #[instrument(level = Level::DEBUG, skip(cred, client))]
    async fn fetch_token(
        cred: &AuthorizedUserRefreshToken,
        client: &HttpClient,
    ) -> Result<Arc<Token>, Error> {
        client
            .token(
                &|| {
                    Request::builder()
                        .method(Method::POST)
                        .uri(DEFAULT_TOKEN_GCP_URI)
                        .header(CONTENT_TYPE, "application/json")
                        .body(Full::from(Bytes::from(
                            serde_json::to_vec(&RefreshRequest {
                                client_id: &cred.client_id,
                                client_secret: &cred.client_secret,
                                grant_type: "refresh_token",
                                refresh_token: &cred.refresh_token,
                            })
                            .unwrap(),
                        )))
                        .unwrap()
                },
                "ConfigDefaultCredentials",
            )
            .await
    }
}

#[async_trait]
impl TokenProvider for ConfigDefaultCredentials {
    async fn token(&self, _scopes: &[&str]) -> Result<Arc<Token>, Error> {
        let token = self.token.read().await.clone();
        if !token.has_expired() {
            return Ok(token);
        }

        let mut locked = self.token.write().await;
        let token = Self::fetch_token(&self.credentials, &self.client).await?;
        *locked = token.clone();
        Ok(token)
    }

    async fn project_id(&self) -> Result<Arc<str>, Error> {
        self.credentials
            .quota_project_id
            .clone()
            .ok_or(Error::Str("no project ID in user credentials"))
    }
}

#[derive(Serialize, Debug)]
struct RefreshRequest<'a> {
    client_id: &'a str,
    client_secret: &'a str,
    grant_type: &'a str,
    refresh_token: &'a str,
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn config_dir() -> Result<PathBuf, Error> {
    let mut home = home::home_dir().ok_or(Error::Str("home directory not found"))?;
    home.push(CONFIG_DIR);
    Ok(home)
}

#[cfg(target_os = "windows")]
fn config_dir() -> Result<PathBuf, Error> {
    let app_data = std::env::var(ENV_APPDATA)
        .map_err(|_| Error::Str("APPDATA environment variable not found"))?;
    let config_path = PathBuf::from(app_data);
    match config_path.exists() {
        true => Ok(config_path),
        false => Err(Error::Str("APPDATA directory not found")),
    }
}

const DEFAULT_TOKEN_GCP_URI: &str = "https://accounts.google.com/o/oauth2/token";
const USER_CREDENTIALS_PATH: &str = "gcloud/application_default_credentials.json";

#[cfg(any(target_os = "linux", target_os = "macos"))]
const CONFIG_DIR: &str = ".config";

#[cfg(target_os = "windows")]
const ENV_APPDATA: &str = "APPDATA";
