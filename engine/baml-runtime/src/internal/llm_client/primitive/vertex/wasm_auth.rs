use std::{future::Future, pin::Pin, sync::Arc};

use anyhow::{Context, Result};
use internal_llm_client::vertex::ResolvedGcpAuthStrategy;
use serde::{Deserialize, Serialize};

use crate::{
    internal::wasm_jwt::encode_jwt,
    js_callback_provider::{get_js_callback_provider, GcpCredResult},
};

pub struct VertexAuth(Option<ServiceAccount>);

pub struct Token(String);

impl Token {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl VertexAuth {
    pub async fn get_or_create(auth_strategy: &ResolvedGcpAuthStrategy) -> Result<Arc<VertexAuth>> {
        // For WASM, just create new instances without caching
        let auth = Arc::new(Self::new(auth_strategy).await?);
        Ok(auth)
    }

    pub async fn new(auth_strategy: &ResolvedGcpAuthStrategy) -> Result<Self> {
        Ok(match auth_strategy {
            ResolvedGcpAuthStrategy::MaybeFilePath(str)
            | ResolvedGcpAuthStrategy::StringContainingJson(str) => {
                if str.starts_with("$") {
                    anyhow::bail!("Failed to resolve {}", str);
                }

                let debug_str = {
                    let s = serde_json::to_string(&serde_json::Value::String(str.clone()))
                        .expect("Serialization of string should always succeed");
                    if s.len() > 8 {
                        format!("{}...{}", &s[..4], &s[s.len() - 4..])
                    } else {
                        s
                    }
                };

                log::debug!("Attempting to auth using JsonString strategy");
                Self(Some(serde_json::from_str(str).context(format!("Failed to parse 'credentials' as GCP service account creds (are you using JSON format creds?); credentials={debug_str}"))?))
            }
            ResolvedGcpAuthStrategy::JsonObject(json) => {
                // NB: this should never happen in WASM, there's no way to pass a JSON object in
                log::debug!("Attempting to auth using JsonObject strategy");
                Self(Some(serde_json::from_value(
                    serde_json::to_value(json).context("Failed to parse service account credentials as GCP service account creds (issue during serialization)")?).context("Failed to parse service account credentials as GCP service account creds (are you using JSON format creds?)")?))
            }
            ResolvedGcpAuthStrategy::SystemDefault => Self(None),
        })
    }

    pub async fn token(&self, scopes: &[&str]) -> Result<Arc<Token>> {
        match &self.0 {
            Some(service_account) => {
                let token = service_account.get_oauth2_token().await.context(
                    "Failed to get OAuth2 token from provided service account credentials",
                )?;
                Ok(Arc::new(token))
            }
            None => {
                let cred_provider = get_js_callback_provider()?;
                let gcp_creds = cred_provider.gcp_req().await.context(
                    "Failed to load GCP creds token: try running `gcloud auth application-default login`",
                )?;
                Ok(Arc::new(Token(gcp_creds.access_token)))
            }
        }
    }

    pub async fn project_id(&self) -> Result<Arc<str>> {
        match &self.0 {
            Some(service_account) => Ok(service_account.project_id.clone().into()),
            None => {
                let cred_provider = get_js_callback_provider()?;
                let gcp_creds = cred_provider.gcp_req().await.context(
                    "Failed to load GCP creds project ID (load failed): try running `gcloud auth application-default login`",
                )?;
                Ok(gcp_creds.project_id.ok_or(anyhow::anyhow!(
                    "Failed to load GCP creds project ID (failed to resolve): try running `gcloud auth application-default login`",
                ))?.into())
            }
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

impl ServiceAccount {
    async fn get_oauth2_token(&self) -> Result<Token> {
        let claims = Claims::from_service_account(self);

        let jwt = encode_jwt(&serde_json::to_value(claims)?, &self.private_key)
            .await
            .map_err(|e| anyhow::anyhow!(format!("{e:?}")))?;

        // Make the token request
        let client = reqwest::Client::new();
        let params = [
            ("grant_type", "urn:ietf:params:oauth:grant-type:jwt-bearer"),
            ("assertion", &jwt),
        ];
        let res = client
            .post(&self.token_uri)
            .form(&params)
            .send()
            .await?
            .text()
            .await?;

        parse_token_response(&res).context(format!("OAuth2 access token request failed: {res}"))
    }
}
