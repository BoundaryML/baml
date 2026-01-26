mod ir_features;
mod publisher;

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use anyhow::Result;
pub(super) use publisher::AstSignatureWrapper;

cfg_if::cfg_if!(
    if #[cfg(target_arch = "wasm32")] {
        type DashMap<K, V> = std::sync::Arc<std::sync::Mutex<std::collections::HashMap<K, V>>>;
    } else {
        use dashmap::DashMap;
    }
);

use std::sync::Arc;

use internal_baml_core::{
    internal_baml_diagnostics::{Diagnostics, SourceFile},
    internal_baml_parser_database::ParserDatabase,
    ir::repr::IntermediateRepr,
    validate,
};

use crate::internal::llm_client::{llm_provider::LLMProvider, retry_policy::CallablePolicy};

// A cached client contains provider and other related stuff(env vars, etc)
// This exists because we want to avoid creating a new provider for every request
// Add more fields here which are cache-specific to avoid percolating them inside the provider
#[derive(Clone)]
pub struct CachedClient {
    pub provider: Arc<LLMProvider>,
    pub env_vars: HashMap<String, String>,
}

impl CachedClient {
    pub fn new(provider: Arc<LLMProvider>, env_vars: HashMap<String, String>) -> Self {
        Self { provider, env_vars }
    }

    pub fn has_env_vars_changed(&self, new_env_vars: &HashMap<String, String>) -> bool {
        // Check if any existing env vars have different values
        let values_changed = self
            .env_vars
            .iter()
            .any(|(k, v)| new_env_vars.get(k).is_some_and(|v2| v2 != v));

        if values_changed {
            return true;
        }

        // Check if BOUNDARY_PROXY_URL was added or removed (affects client configuration)
        let had_proxy = self.env_vars.contains_key("BOUNDARY_PROXY_URL");
        let has_proxy = new_env_vars.contains_key("BOUNDARY_PROXY_URL");

        had_proxy != has_proxy
    }
}

// InternalBamlRuntime has been merged into BamlRuntime in lib.rs
