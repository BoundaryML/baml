//! `ClientRegistry` for runtime client configuration.
//!
//! This module provides a way to configure LLM clients at runtime,
//! either by overriding the primary client or by creating custom clients.

use std::collections::HashMap;

use serde_json::Value;

use crate::{
    codec::BamlEncode,
    proto::baml_cffi_v1::{host_map_entry, HostClientProperty, HostClientRegistry, HostMapEntry},
};

/// A client property for runtime client configuration.
#[derive(Debug, Clone)]
struct ClientProperty {
    name: String,
    provider: String,
    retry_policy: Option<String>,
    options: HashMap<String, Value>,
}

/// A registry for configuring LLM clients at runtime.
///
/// Use `ClientRegistry::new()` to create an empty registry, then:
/// - Call `add_llm_client()` to add custom clients
/// - Call `set_primary_client()` to set which client to use
///
/// # Example
/// ```ignore
/// let mut registry = ClientRegistry::new();
/// registry.add_llm_client("MyClient", "openai", [
///     ("model".to_string(), json!("gpt-4")),
///     ("api_key".to_string(), json!("sk-...")),
/// ].into_iter().collect());
/// registry.set_primary_client("MyClient");
/// ```
#[derive(Debug, Clone, Default)]
pub struct ClientRegistry {
    primary: Option<String>,
    clients: Vec<ClientProperty>,
}

impl ClientRegistry {
    /// Create a new empty client registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add an LLM client to the registry.
    ///
    /// # Arguments
    /// * `name` - The name to reference this client by
    /// * `provider` - The provider type (e.g., "openai", "anthropic")
    /// * `options` - Provider-specific options as JSON values
    pub fn add_llm_client(
        &mut self,
        name: impl Into<String>,
        provider: impl Into<String>,
        options: HashMap<String, Value>,
    ) {
        self.clients.push(ClientProperty {
            name: name.into(),
            provider: provider.into(),
            retry_policy: None,
            options,
        });
    }

    /// Set the primary client to use for function calls.
    pub fn set_primary_client(&mut self, name: impl Into<String>) {
        self.primary = Some(name.into());
    }

    /// Check if this registry is empty (no clients and no primary set).
    pub fn is_empty(&self) -> bool {
        self.primary.is_none() && self.clients.is_empty()
    }

    /// Encode this registry to the protobuf format for FFI.
    pub(crate) fn encode(&self) -> HostClientRegistry {
        let clients = self
            .clients
            .iter()
            .map(|c| {
                let options = c
                    .options
                    .iter()
                    .map(|(k, v)| HostMapEntry {
                        key: Some(host_map_entry::Key::StringKey(k.clone())),
                        value: Some(v.baml_encode()), // Uses BamlEncode for serde_json::Value
                    })
                    .collect();

                HostClientProperty {
                    name: c.name.clone(),
                    provider: c.provider.clone(),
                    retry_policy: c.retry_policy.clone(),
                    options,
                }
            })
            .collect();

        HostClientRegistry {
            primary: self.primary.clone(),
            clients,
        }
    }
}
