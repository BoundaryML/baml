// This is designed to build any type of client, not just primitives
use std::{collections::HashMap, str::FromStr, sync::Arc};

use anyhow::{Context, Result};
use baml_types::{BamlMap, BamlValue};
pub use internal_llm_client::ClientProvider;
use internal_llm_client::{ClientSpec, PropertyHandler, UnresolvedClientProperty};
use serde::{Deserialize, Deserializer, Serialize};

use crate::{internal::llm_client::llm_provider::LLMProvider, RuntimeContext};

#[derive(Clone)]
pub enum PrimitiveClient {
    OpenAI,
    Anthropic,
    Google,
    Vertex,
}

#[derive(Clone, Deserialize, Debug, PartialEq)]
pub struct ClientProperty {
    pub name: String,
    #[serde(deserialize_with = "deserialize_client_provider")]
    pub provider: ClientProvider,
    pub retry_policy: Option<String>,
    options: BamlMap<String, BamlValue>,
}

impl ClientProperty {
    pub fn new(
        name: String,
        provider: ClientProvider,
        retry_policy: Option<String>,
        options: BamlMap<String, BamlValue>,
    ) -> Self {
        Self {
            name,
            provider,
            retry_policy,
            options,
        }
    }

    pub fn from_shorthand(provider: &ClientProvider, model: &str) -> Self {
        Self {
            name: format!("{provider}/{model}"),
            provider: provider.clone(),
            retry_policy: None,
            options: vec![("model".to_string(), BamlValue::String(model.to_string()))]
                .into_iter()
                .collect(),
        }
    }

    pub fn unresolved_options(&self) -> Result<UnresolvedClientProperty<()>> {
        let property = PropertyHandler::new(
            self.options
                .iter()
                .map(|(k, v)| Ok((k.clone(), ((), v.to_resolvable()?))))
                .collect::<Result<_>>()?,
            (),
        );
        self.provider.parse_client_property(property).map_err(|e| {
            anyhow::anyhow!(
                "Failed to parse client options for {}:\n{}",
                self.name,
                e.into_iter()
                    .map(|e| e.message)
                    .collect::<Vec<_>>()
                    .join("\n")
            )
        })
    }
}

#[derive(Clone, Deserialize, Debug, PartialEq)]
pub struct ClientRegistry {
    #[serde(deserialize_with = "deserialize_clients")]
    clients: HashMap<String, ClientProperty>,
    primary: Option<String>,
}

impl Default for ClientRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ClientRegistry {
    pub fn new() -> Self {
        Self {
            clients: Default::default(),
            primary: None,
        }
    }

    pub fn add_client(&mut self, client: ClientProperty) {
        self.clients.insert(client.name.clone(), client);
    }

    pub fn set_primary(&mut self, primary: String) {
        self.primary = Some(primary);
    }

    pub fn is_empty(&self) -> bool {
        self.clients.is_empty() && self.primary.is_none()
    }

    pub fn to_clients(
        &self,
        ctx: &RuntimeContext,
    ) -> Result<(Option<String>, HashMap<String, Arc<LLMProvider>>)> {
        let mut clients = HashMap::new();
        for (name, client) in &self.clients {
            let provider = LLMProvider::try_from((client, ctx))
                .context(format!("Failed to parse client: {name}"))?;
            clients.insert(name.into(), Arc::new(provider));
        }
        // TODO: Also do validation here
        Ok((self.primary.clone(), clients))
    }
}

fn deserialize_clients<'de, D>(deserializer: D) -> Result<HashMap<String, ClientProperty>, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(Vec::deserialize(deserializer)?
        .into_iter()
        .map(|client: ClientProperty| (client.name.clone(), client))
        .collect())
}

fn deserialize_client_provider<'de, D>(deserializer: D) -> Result<ClientProvider, D::Error>
where
    D: Deserializer<'de>,
{
    let s: &str = Deserialize::deserialize(deserializer)?;
    ClientProvider::from_str(s).map_err(|e| serde::de::Error::custom(e.to_string()))
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use serde_json;

    use super::*;

    #[test]
    fn test_each_provider() {
        for provider in ClientProvider::allowed_providers() {
            let json = format!(
                r#"{{"name": "dummy_name", "provider": "{provider}", "retry_policy": null, "options": {{"model": "gpt-3"}}}}"#
            );
            let client: ClientProperty = serde_json::from_str(&json).unwrap();
            assert_eq!(client.provider, ClientProvider::from_str(provider).unwrap());
        }
    }

    #[test]
    fn test_deserialize_valid_client() {
        let json = r#"
            {
                "name": "dummy_name",
                "provider": "openai",
                "retry_policy": null,
                "options": {"model": "gpt-3"}
            }
        "#;
        let client: ClientProperty = serde_json::from_str(json).unwrap();
        assert_eq!(client.name, "dummy_name");
        assert_eq!(
            client.provider,
            ClientProvider::OpenAI(internal_llm_client::OpenAIClientProviderVariant::Base)
        );
        assert_eq!(client.retry_policy, None);
        assert_eq!(
            client.options.get("model"),
            Some(&BamlValue::String("gpt-3".to_string()))
        );
    }

    #[test]
    fn test_deserialize_invalid_provider() {
        let json = r#"
            {
                "name": "InvalidClient/gpt-3",
                "provider": "doesn't exist",
                "retry_policy": null,
                "options": {"model": "gpt-3"}
            }
        "#;
        let result: Result<ClientProperty, _> = serde_json::from_str(json);
        assert!(
            result.is_err(),
            "Deserialization should fail for an invalid provider"
        );
    }

    #[test]
    fn test_deserialize_client_registry() {
        let json = r#"
        {
            "clients": [
                {
                    "name": "dummy_name",
                    "provider": "openai",
                    "retry_policy": "always",
                    "options": {"model": "gpt-3"}
                }
            ]
        }
        "#;
        let registry: ClientRegistry = serde_json::from_str(json).unwrap();
        assert_eq!(registry.clients.len(), 1);
        let client = registry.clients.get("dummy_name").unwrap();
        assert_eq!(
            client.provider,
            ClientProvider::OpenAI(internal_llm_client::OpenAIClientProviderVariant::Base)
        );
        assert_eq!(registry.primary, None);
    }
}
