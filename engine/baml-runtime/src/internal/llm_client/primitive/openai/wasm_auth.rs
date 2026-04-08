use anyhow::{Context, Result};
use internal_llm_client::openai::ResolvedAzureAuthStrategy;

use crate::js_callback_provider::get_js_callback_provider;

pub struct AzureAuth;

impl AzureAuth {
    /// Get or create an AzureAuth for the given auth strategy.
    ///
    /// In WASM, there is no credential caching — the JS callback bridge handles
    /// token refresh internally (the JS side may cache tokens as it sees fit).
    pub async fn get_or_create(auth_strategy: &ResolvedAzureAuthStrategy) -> Result<AzureAuth> {
        match auth_strategy {
            ResolvedAzureAuthStrategy::ApiKey => {
                anyhow::bail!(
                    "AzureAuth::get_or_create called for ApiKey strategy — this is a bug"
                );
            }
            ResolvedAzureAuthStrategy::EntraId { .. }
            | ResolvedAzureAuthStrategy::SystemDefault => Ok(AzureAuth),
        }
    }

    /// Acquire a bearer token for Azure OpenAI via the WASM JS callback bridge.
    pub async fn token(&self) -> Result<String> {
        let cred_provider = get_js_callback_provider().context(
            "Azure Entra ID WASM credential provider not initialized: \
             call init_js_callback_bridge() before using Azure Entra ID auth",
        )?;
        let creds = cred_provider.azure_req().await.context(
            "Failed to load Azure Entra ID token via WASM bridge: \
             ensure the loadAzureCreds callback returns a valid access token",
        )?;
        Ok(creds.access_token)
    }
}
