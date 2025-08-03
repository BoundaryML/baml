use std::{
    collections::HashMap,
    future::Future,
    pin::Pin,
    sync::{Arc, RwLock},
};

use anyhow::{Context, Result};
use gcp_auth::{Error, Token, TokenProvider};
use internal_llm_client::vertex::ResolvedGcpAuthStrategy;
use once_cell::sync::Lazy;

// Global cache for auth instances
static AUTH_CACHE: Lazy<RwLock<HashMap<String, Arc<VertexAuth>>>> =
    Lazy::new(|| RwLock::new(HashMap::new()));

pub enum VertexAuth {
    CustomServiceAccount(gcp_auth::CustomServiceAccount),
    ConfigDefaultCredentials(gcp_auth::ConfigDefaultCredentials),
    MetadataServiceAccount(gcp_auth::MetadataServiceAccount),
    GCloudAuthorizedUser(gcp_auth::GCloudAuthorizedUser),
}

impl VertexAuth {
    fn cache_key(auth_strategy: &ResolvedGcpAuthStrategy) -> String {
        match auth_strategy {
            ResolvedGcpAuthStrategy::MaybeFilePath(path) => format!("file:{path}"),
            ResolvedGcpAuthStrategy::StringContainingJson(json) => {
                // Hash the JSON to avoid storing sensitive data in cache key
                use std::{
                    collections::hash_map::DefaultHasher,
                    hash::{Hash, Hasher},
                };
                let mut hasher = DefaultHasher::new();
                json.hash(&mut hasher);
                format!("json_hash:{}", hasher.finish())
            }
            ResolvedGcpAuthStrategy::JsonObject(obj) => {
                // Hash the object to avoid storing sensitive data in cache key
                use std::{
                    collections::hash_map::DefaultHasher,
                    hash::{Hash, Hasher},
                };
                let mut hasher = DefaultHasher::new();
                serde_json::to_string(obj)
                    .unwrap_or_default()
                    .hash(&mut hasher);
                format!("obj_hash:{}", hasher.finish())
            }
            ResolvedGcpAuthStrategy::SystemDefault => "system_default".to_string(),
        }
    }

    pub async fn get_or_create(auth_strategy: &ResolvedGcpAuthStrategy) -> Result<Arc<VertexAuth>> {
        let cache_key = Self::cache_key(auth_strategy);

        // Try to get from cache first
        if let Ok(cache) = AUTH_CACHE.read() {
            if let Some(cached) = cache.get(&cache_key) {
                return Ok(cached.clone());
            }
        }

        // Create new auth instance
        let auth = Arc::new(Self::new(auth_strategy).await?);

        // Cache it
        if let Ok(mut cache) = AUTH_CACHE.write() {
            cache.insert(cache_key, auth.clone());
        }

        Ok(auth)
    }

    pub async fn new(auth_strategy: &ResolvedGcpAuthStrategy) -> Result<VertexAuth> {
        match auth_strategy {
            ResolvedGcpAuthStrategy::MaybeFilePath(path) => {
                log::debug!("Attempting to auth using JsonFile strategy");
                let authz_user =
                    gcp_auth::CustomServiceAccount::from_file(path).context(format!(
                        "Failed to parse credentials as JSON file: {}",
                        serde_json::to_string(&path)
                            .expect("Serialization of string should always succeed")
                    ))?;
                Ok(VertexAuth::CustomServiceAccount(authz_user))
            }
            ResolvedGcpAuthStrategy::StringContainingJson(s) => {
                log::debug!("Attempting to auth using JsonString strategy");
                let authz_user = gcp_auth::CustomServiceAccount::from_json(s).context(format!(
                    "Failed to parse credentials as JSON string: {}",
                    {
                        let s = serde_json::to_string(&s)
                            .expect("Serialization of string should always succeed");
                        if s.len() > 8 {
                            format!("{}...{}", &s[..4], &s[s.len() - 4..])
                        } else {
                            s
                        }
                    }
                ))?;
                Ok(VertexAuth::CustomServiceAccount(authz_user))
            }
            ResolvedGcpAuthStrategy::JsonObject(o) => {
                log::debug!("Attempting to auth using JsonObject strategy");
                let authz_user = gcp_auth::CustomServiceAccount::from_json(
                    &serde_json::to_string(&o).context(
                        "Failed to serialize service account credentials using JsonObject strategy",
                    )?,
                )?;
                Ok(VertexAuth::CustomServiceAccount(authz_user))
            }
            ResolvedGcpAuthStrategy::SystemDefault => {
                log::debug!("Attempting to auth using SystemDefault strategy (local mods)");
                let mut errors = Vec::new();

                match gcp_auth::ConfigDefaultCredentials::new().await {
                    Ok(authz_user) => {
                        log::debug!(
                            "Successful auth using GcloudApplicationDefaultCredentials strategy"
                        );
                        return Ok(VertexAuth::ConfigDefaultCredentials(authz_user));
                    }
                    Err(e) => {
                        errors.push(anyhow::Error::from(e).context(
                            "Failed to auth using GcloudApplicationDefaultCredentials strategy",
                        ));
                    }
                }
                match gcp_auth::MetadataServiceAccount::new().await {
                    Ok(authz_user) => {
                        log::debug!("Successful auth using MetadataServiceAccount strategy");
                        return Ok(VertexAuth::MetadataServiceAccount(authz_user));
                    }
                    Err(e) => {
                        errors.push(
                            anyhow::Error::from(e)
                                .context("Failed to auth using MetadataServiceAccount strategy"),
                        );
                    }
                }
                match gcp_auth::GCloudAuthorizedUser::new().await {
                    Ok(authz_user) => {
                        log::debug!("Successful auth using GCloudAuthorizedUser strategy");
                        return Ok(VertexAuth::GCloudAuthorizedUser(authz_user));
                    }
                    Err(e) => {
                        errors.push(
                            anyhow::Error::from(e)
                                .context("Failed to auth using GCloudAuthorizedUser strategy"),
                        );
                    }
                }

                // Log all collected errors if no strategy succeeded
                for err in &errors {
                    log::error!("{err:?}");
                }
                anyhow::bail!(
                    "Failed to auth - system_default strategy did not resolve successfully. Errors encountered: {:?}",
                    errors
                )
            }
        }
    }

    async fn token_impl(&self, scopes: &[&str]) -> Result<Arc<Token>, Error> {
        match self {
            VertexAuth::CustomServiceAccount(authz_user) => authz_user.token(scopes).await,
            VertexAuth::ConfigDefaultCredentials(authz_user) => authz_user.token(scopes).await,
            VertexAuth::MetadataServiceAccount(authz_user) => authz_user.token(scopes).await,
            VertexAuth::GCloudAuthorizedUser(authz_user) => authz_user.token(scopes).await,
        }
    }

    async fn project_id_impl(&self) -> Result<Arc<str>, Error> {
        match self {
            VertexAuth::CustomServiceAccount(authz_user) => {
                TokenProvider::project_id(authz_user).await
            }
            VertexAuth::ConfigDefaultCredentials(authz_user) => authz_user.project_id().await,
            VertexAuth::MetadataServiceAccount(authz_user) => authz_user.project_id().await,
            VertexAuth::GCloudAuthorizedUser(authz_user) => authz_user.project_id().await,
        }
    }
}

impl TokenProvider for VertexAuth {
    fn token<'life0, 'life1, 'life2, 'async_trait>(
        &'life0 self,
        scopes: &'life1 [&'life2 str],
    ) -> Pin<Box<dyn Future<Output = Result<Arc<Token>, Error>> + Send + 'async_trait>>
    where
        'life0: 'async_trait,
        'life1: 'async_trait,
        'life2: 'async_trait,
    {
        Box::pin(self.token_impl(scopes))
    }

    fn project_id<'life0, 'async_trait>(
        &'life0 self,
    ) -> Pin<Box<dyn Future<Output = Result<Arc<str>, Error>> + Send + 'async_trait>>
    where
        'life0: 'async_trait,
    {
        Box::pin(self.project_id_impl())
    }
}
