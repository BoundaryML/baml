use anyhow::{Context, Result};
use internal_llm_client::vertex::ResolvedGcpAuthStrategy;
use std::{future::Future, pin::Pin, sync::Arc};

use gcp_auth::{Error, Token, TokenProvider};

pub enum VertexAuth {
    CustomServiceAccount(gcp_auth::CustomServiceAccount),
    ConfigDefaultCredentials(gcp_auth::ConfigDefaultCredentials),
    MetadataServiceAccount(gcp_auth::MetadataServiceAccount),
    GCloudAuthorizedUser(gcp_auth::GCloudAuthorizedUser),
}

impl VertexAuth {
    pub async fn new(auth_strategy: &ResolvedGcpAuthStrategy) -> Result<VertexAuth> {
        match auth_strategy {
            ResolvedGcpAuthStrategy::FilePath(path) => {
                log::debug!("Attempting to auth using JsonFile strategy");
                let authz_user = gcp_auth::CustomServiceAccount::from_file(&path)?;
                Ok(VertexAuth::CustomServiceAccount(authz_user))
            }
            ResolvedGcpAuthStrategy::JsonString(s) => {
                log::debug!("Attempting to auth using JsonString strategy");
                let authz_user = gcp_auth::CustomServiceAccount::from_json(&s)?;
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
                match gcp_auth::ConfigDefaultCredentials::new().await {
                    Ok(authz_user) => {
                        log::debug!(
                            "Successful auth using GcloudApplicationDefaultCredentials strategy"
                        );
                        return Ok(VertexAuth::ConfigDefaultCredentials(authz_user));
                    }
                    Err(e) => {
                        log::error!(
                            "{:?}",
                            anyhow::Error::from(e).context(
                                "Failed to auth using GcloudApplicationDefaultCredentials strategy"
                            )
                        );
                    }
                }
                match gcp_auth::MetadataServiceAccount::new().await {
                    Ok(authz_user) => {
                        log::debug!("Successful auth using MetadataServiceAccount strategy");
                        return Ok(VertexAuth::MetadataServiceAccount(authz_user));
                    }
                    Err(e) => {
                        log::error!(
                            "{:?}",
                            anyhow::Error::from(e)
                                .context("Failed to auth using MetadataServiceAccount strategy")
                        );
                    }
                }
                match gcp_auth::GCloudAuthorizedUser::new().await {
                    Ok(authz_user) => {
                        log::debug!("Successful auth using GCloudAuthorizedUser strategy");
                        return Ok(VertexAuth::GCloudAuthorizedUser(authz_user));
                    }
                    Err(e) => {
                        log::error!(
                            "{:?}",
                            anyhow::Error::from(e)
                                .context("Failed to auth using GCloudAuthorizedUser strategy")
                        );
                    }
                }
                anyhow::bail!(
                    "Failed to auth - system_default strategy did not resolve successfully"
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
