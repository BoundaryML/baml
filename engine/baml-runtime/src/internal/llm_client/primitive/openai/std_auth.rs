use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use anyhow::{Context, Result};
use azure_core::credentials::{Secret, TokenCredential};
use azure_identity::{
    AzureCliCredential, AzureCliCredentialOptions, ClientSecretCredential,
    ClientSecretCredentialOptions, ManagedIdentityCredential,
};
use internal_llm_client::openai::ResolvedAzureAuthStrategy;
use once_cell::sync::Lazy;

/// The token scope for Azure OpenAI (public cloud).
const AZURE_OPENAI_TOKEN_SCOPE: &str = "https://cognitiveservices.azure.com/.default";

/// Global cache for Azure credential objects.
/// Caches the credential object (not the token) — `azure_identity` handles token refresh
/// internally. This avoids re-creating credentials (and re-reading env vars / spawning CLI
/// processes) on every request.
static AZURE_AUTH_CACHE: Lazy<RwLock<HashMap<String, Arc<dyn TokenCredential>>>> =
    Lazy::new(|| RwLock::new(HashMap::new()));

pub struct AzureAuth {
    credential: Arc<dyn TokenCredential>,
}

impl AzureAuth {
    fn cache_key(auth_strategy: &ResolvedAzureAuthStrategy) -> String {
        match auth_strategy {
            ResolvedAzureAuthStrategy::ApiKey => "api_key".to_string(),
            ResolvedAzureAuthStrategy::EntraId {
                tenant_id,
                client_id,
                ..
            } => format!("{tenant_id}:{client_id}"),
            ResolvedAzureAuthStrategy::SystemDefault => "system_default".to_string(),
        }
    }

    /// Get or create a cached `AzureAuth` for the given auth strategy.
    ///
    /// Returns an error for `ApiKey` strategy — callers should not invoke this for API key auth.
    pub async fn get_or_create(
        auth_strategy: &ResolvedAzureAuthStrategy,
    ) -> Result<Arc<AzureAuth>> {
        let cache_key = Self::cache_key(auth_strategy);

        // Try cache first
        if let Ok(cache) = AZURE_AUTH_CACHE.read() {
            if let Some(cached_cred) = cache.get(&cache_key) {
                return Ok(Arc::new(AzureAuth {
                    credential: cached_cred.clone(),
                }));
            }
        }

        // Create new credential
        let credential: Arc<dyn TokenCredential> = match auth_strategy {
            ResolvedAzureAuthStrategy::ApiKey => {
                anyhow::bail!("AzureAuth::get_or_create called for ApiKey strategy — this is a bug");
            }
            ResolvedAzureAuthStrategy::EntraId {
                tenant_id,
                client_id,
                client_secret,
            } => {
                if let Some(secret) = client_secret {
                    log::debug!("Azure Entra ID: using ClientSecretCredential");
                    ClientSecretCredential::new(
                        tenant_id,
                        client_id.clone(),
                        Secret::new(secret.clone()),
                        None,
                    )
                    .context("Failed to create Azure ClientSecretCredential")?
                } else {
                    // No client_secret provided — use managed identity
                    // (The tenant_id/client_id presence triggered EntraId mode, but without a
                    // secret we fall back to ManagedIdentityCredential which uses IMDS/App Service)
                    log::debug!(
                        "Azure Entra ID: no client_secret provided, using ManagedIdentityCredential"
                    );
                    ManagedIdentityCredential::new(None)
                        .context("Failed to create Azure ManagedIdentityCredential")?
                }
            }
            ResolvedAzureAuthStrategy::SystemDefault => {
                // Try ManagedIdentityCredential first (works on Azure VMs, App Service, AKS),
                // then fall back to AzureCliCredential (works locally after `az login`).
                // Note: DefaultAzureCredential was removed in azure_identity 0.28.0, so we
                // build our own two-step chain here.
                let mut errors: Vec<String> = Vec::new();

                match ManagedIdentityCredential::new(None) {
                    Ok(cred) => {
                        // Eagerly probe the credential by requesting a token.
                        // On a machine without managed identity (dev laptop), IMDS will time out.
                        // We accept the latency here (once per process lifetime due to caching).
                        match cred
                            .get_token(&[AZURE_OPENAI_TOKEN_SCOPE], None)
                            .await
                        {
                            Ok(_) => {
                                log::debug!(
                                    "Azure SystemDefault: ManagedIdentityCredential succeeded"
                                );
                                cred
                            }
                            Err(e) => {
                                errors.push(format!("ManagedIdentityCredential: {e}"));
                                log::debug!(
                                    "Azure SystemDefault: ManagedIdentityCredential failed, trying AzureCliCredential"
                                );
                                match AzureCliCredential::new(None)
                                    .context("Failed to create AzureCliCredential")?
                                    .get_token(&[AZURE_OPENAI_TOKEN_SCOPE], None)
                                    .await
                                {
                                    Ok(_) => {
                                        log::debug!(
                                            "Azure SystemDefault: AzureCliCredential succeeded"
                                        );
                                        AzureCliCredential::new(None)
                                            .context("Failed to create AzureCliCredential")?
                                    }
                                    Err(e) => {
                                        errors.push(format!("AzureCliCredential: {e}"));
                                        anyhow::bail!(
                                            "Azure SystemDefault credential chain exhausted. Tried:\n{}",
                                            errors.join("\n")
                                        );
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => {
                        errors.push(format!("ManagedIdentityCredential (init): {e}"));
                        log::debug!(
                            "Azure SystemDefault: ManagedIdentityCredential init failed, trying AzureCliCredential"
                        );
                        AzureCliCredential::new(None)
                            .context("Failed to create AzureCliCredential")?
                    }
                }
            }
        };

        // Cache the credential object
        if let Ok(mut cache) = AZURE_AUTH_CACHE.write() {
            cache.insert(cache_key, credential.clone());
        }

        Ok(Arc::new(AzureAuth { credential }))
    }

    /// Acquire a bearer token for Azure OpenAI.
    pub async fn token(&self) -> Result<String> {
        let access_token = self
            .credential
            .get_token(&[AZURE_OPENAI_TOKEN_SCOPE], None)
            .await
            .context("Failed to acquire Azure Entra ID token")?;
        Ok(access_token.token.secret().to_string())
    }
}
