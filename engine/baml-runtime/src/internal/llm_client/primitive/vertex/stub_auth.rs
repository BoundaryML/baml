//! Stub auth backend used when baml-runtime is built without the `vertex`
//! feature (e.g. vendored/minimal builds that want to drop the gcp_auth
//! dependency tree). Mirrors the API surface of `std_auth::VertexAuth` that
//! `VertexClient` uses; construction always fails with a clear error.

use std::sync::Arc;

use anyhow::Result;
use internal_llm_client::vertex::ResolvedGcpAuthStrategy;

pub enum VertexAuth {}

/// Mirrors `gcp_auth::Token` far enough for `VertexClient`.
pub struct Token;

impl Token {
    pub fn as_str(&self) -> &str {
        ""
    }
}

impl VertexAuth {
    pub async fn get_or_create(
        _auth_strategy: &ResolvedGcpAuthStrategy,
    ) -> Result<Arc<VertexAuth>> {
        anyhow::bail!(
            "The vertex-ai provider is unavailable: BAML was compiled without the `vertex` \
             feature. Rebuild baml-runtime with the `vertex` feature enabled to use Vertex AI."
        )
    }

    pub async fn token(&self, _scopes: &[&str]) -> Result<Arc<Token>> {
        match *self {}
    }

    pub async fn project_id(&self) -> Result<Arc<str>> {
        match *self {}
    }
}
