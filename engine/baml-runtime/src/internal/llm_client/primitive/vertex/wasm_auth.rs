use anyhow::{Context, Result};
use internal_llm_client::vertex::ResolvedGcpAuthStrategy;
use serde::{Deserialize, Serialize};
use std::{future::Future, pin::Pin, sync::Arc};

use crate::{
    internal::wasm_jwt::encode_jwt,
    js_callback_provider::{get_remote_cred_provider, GcpCredResult},
};

// pub struct VertexAuth(ServiceAccount);

pub struct VertexAuth {}

pub struct Token(String);

impl Token {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl VertexAuth {
    // pub async fn new(auth_strategy: &ResolvedGcpAuthStrategy) -> Result<VertexAuth> {
    //     Ok(match auth_strategy {
    //         ResolvedGcpAuthStrategy::FilePath(path) => {
    //             anyhow::bail!(
    //                 "Failed to auth - cannot load credentials from a file in WASM (path='{}...', path.len={})",
    //                 path.chars().take(5).collect::<String>(),
    //                 path.len()
    //             )
    //         }
    //         ResolvedGcpAuthStrategy::JsonString(json) => {
    //             log::debug!("Attempting to auth using JsonString strategy");
    //             Self(serde_json::from_str(&json).context("Failed to parse service account credentials as GCP service account creds (are you using JSON format creds?)")?)
    //         }
    //         ResolvedGcpAuthStrategy::JsonObject(json) => {
    //             // NB: this should never happen in WASM, there's no way to pass a JSON object in
    //             log::debug!("Attempting to auth using JsonObject strategy");
    //             Self(serde_json::from_value(
    //                 serde_json::to_value(&json).context("Failed to parse service account credentials as GCP service account creds (issue during serialization)")?).context("Failed to parse service account credentials as GCP service account creds (are you using JSON format creds?)")?)
    //         }
    //         ResolvedGcpAuthStrategy::SystemDefault => {
    //             anyhow::bail!(
    //                 "Failed to auth - failed to load default credentials in WASM (please set env.GOOGLE_APPLICATION_CREDENTIALS, see https://docs.boundaryml.com/ref/llm-client-providers/google-vertex#using-a-vertex-ai-client-in-the-playground)"
    //             )
    //         }
    //     })
    // }

    pub async fn new(_auth_strategy: &ResolvedGcpAuthStrategy) -> Result<VertexAuth> {
        Ok(VertexAuth {})
    }

    pub async fn token(&self, scopes: &[&str]) -> Result<Arc<Token>> {
        let cred_provider = get_remote_cred_provider()?;
        let gcp_creds = cred_provider.gcp_req().await?;
        match gcp_creds {
            GcpCredResult::Ok { access_token, .. } => Ok(Arc::new(Token(access_token))),
            GcpCredResult::Err { name, message } => Err(anyhow::Error::msg(format!(
                "Error occurred while fetching gcp creds: {name}: {message}"
            ))
            .context(
                "Failed to load GCP creds: try running `gcloud auth application-default login`",
            )),
        }
    }

    pub async fn project_id(&self) -> Result<String> {
        // Ok(self.0.project_id.clone())
        let cred_provider = get_remote_cred_provider()?;
        let gcp_creds = cred_provider.gcp_req().await?;
        match gcp_creds {
            GcpCredResult::Ok { project_id, .. } => Ok(project_id),
            GcpCredResult::Err { name, message } => Err(anyhow::Error::msg(format!(
                "Error occurred while fetching project_id: {name}: {message}"
            ))
            .context(
                "Failed to load GCP creds: try running `gcloud auth application-default login`",
            )),
        }
    }
}

fn parse_token_response(response: &str) -> Result<Token> {
    let res: serde_json::Value =
        serde_json::from_str(response).context("Failed to parse token response as JSON")?;

    Ok(Token(
        res.as_object()
            .context("Token exchange did not return a JSON object")?
            .get("access_token")
            .context("Access token not found in response")?
            .as_str()
            .context("Access token is not a string")?
            .to_string(),
    ))
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
            exp: (now + chrono::Duration::hours(1)).timestamp(),
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
