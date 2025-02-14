use anyhow::{Context, Result};
use internal_llm_client::vertex::ResolvedGcpAuthStrategy;
use serde::{Deserialize, Serialize};
use std::{future::Future, pin::Pin, sync::Arc};

use crate::internal::wasm_jwt::encode_jwt;

pub struct VertexAuth(ServiceAccount);

pub struct Token(String);

impl Token {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl VertexAuth {
    pub async fn new(auth_strategy: &ResolvedGcpAuthStrategy) -> Result<VertexAuth> {
        Ok(match auth_strategy {
            ResolvedGcpAuthStrategy::FilePath(path) => {
                anyhow::bail!("Failed to auth - cannot load credentials from files in WASM")
            }
            ResolvedGcpAuthStrategy::JsonString(json) => {
                log::debug!("Attempting to auth using JsonString strategy");
                Self(serde_json::from_str(&json).context("Failed to parse service account credentials as GCP service account creds (are you using JSON format creds?)")?)
            }
            ResolvedGcpAuthStrategy::JsonObject(json) => {
                log::debug!("Attempting to auth using JsonObject strategy");
                Self(serde_json::from_value(
                    serde_json::to_value(&json).context("Failed to parse service account credentials as GCP service account creds (issue during serialization)")?).context("Failed to parse service account credentials as GCP service account creds (are you using JSON format creds?)")?)
            }
            ResolvedGcpAuthStrategy::SystemDefault => {
                anyhow::bail!(
                    "Failed to auth - cannot load GCP application default credentials in WASM"
                )
            }
        })
    }

    pub async fn token(&self, scopes: &[&str]) -> Result<Arc<Token>> {
        let claims = Claims::from_service_account(&self.0);

        let jwt = encode_jwt(&serde_json::to_value(claims)?, &self.0.private_key)
            .await
            .map_err(|e| anyhow::anyhow!(format!("{e:?}")))?;

        // Make the token request
        let client = reqwest::Client::new();
        let params = [
            ("grant_type", "urn:ietf:params:oauth:grant-type:jwt-bearer"),
            ("assertion", &jwt),
        ];
        let res: serde_json::Value = client
            .post(&self.0.token_uri)
            .form(&params)
            .send()
            .await?
            .json()
            .await?;

        Ok(Arc::new(Token(
            res.as_object()
                .context("Token exchange did not return a JSON object")?
                .get("access_token")
                .context("Access token not found in response")?
                .as_str()
                .context("Access token is not a string")?
                .to_string(),
        )))
    }

    pub async fn project_id(&self) -> Result<String> {
        Ok(self.0.project_id.clone())
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct Claims {
    iss: String,
    scope: String,
    aud: String,
    exp: i64,
    iat: i64,
}

// This is currently hardcoded, but we could make it a property if we wanted
// https://developers.google.com/identity/protocols/oauth2/scopes
const DEFAULT_SCOPE: &str = "https://www.googleapis.com/auth/cloud-platform";

impl Claims {
    fn from_service_account(service_account: &ServiceAccount) -> Claims {
        let now = chrono::Utc::now();
        Claims {
            iss: service_account.client_email.clone(),
            scope: DEFAULT_SCOPE.to_string(),
            aud: service_account.token_uri.clone(),
            exp: (now + chrono::Duration::hours(60)).timestamp(),
            iat: now.timestamp(),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct ServiceAccount {
    pub token_uri: String,
    pub project_id: String,
    pub client_email: String,
    pub private_key: String,
}

async fn get_access_token(service_account: &ServiceAccount) -> Result<String> {
    // Create the JWT
    let claims = Claims::from_service_account(service_account);

    let jwt = encode_jwt(&serde_json::to_value(claims)?, &service_account.private_key)
        .await
        .map_err(|e| anyhow::anyhow!(format!("{e:?}")))?;

    // Make the token request
    let client = reqwest::Client::new();
    let params = [
        ("grant_type", "urn:ietf:params:oauth:grant-type:jwt-bearer"),
        ("assertion", &jwt),
    ];
    let res: serde_json::Value = client
        .post(&service_account.token_uri)
        .form(&params)
        .send()
        .await?
        .json()
        .await?;

    Ok(res
        .as_object()
        .context("Token exchange did not return a JSON object")?
        .get("access_token")
        .context("Access token not found in response")?
        .as_str()
        .context("Access token is not a string")?
        .to_string())
}
