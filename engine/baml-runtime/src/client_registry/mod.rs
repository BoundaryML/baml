// This is designed to build any type of client, not just primitives
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::sync::Arc;

use baml_types::{BamlMap, BamlValue};
use serde::Serialize;

use crate::{internal::llm_client::llm_provider::LLMProvider, RuntimeContext};

#[derive(Clone)]
pub enum PrimitiveClient {
    OpenAI,
    Anthropic,
    Google,
    Vertex,
}

#[derive(Serialize, Clone)]
pub struct ClientProperty {
    pub name: String,
    pub provider: String,
    pub retry_policy: Option<String>,
    pub options: BamlMap<String, BamlValue>,
}

#[derive(Clone)]
pub struct ClientRegistry {
    clients: HashMap<String, ClientProperty>,
    primary: Option<String>,
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

    pub fn to_clients(
        &self,
        ctx: &RuntimeContext,
    ) -> Result<(Option<String>, HashMap<String, Arc<LLMProvider>>)> {
        let mut clients = HashMap::new();
        for (name, client) in &self.clients {
            let provider = LLMProvider::try_from((client, ctx))
                .context(format!("Failed to parse client: {}", name))?;
            clients.insert(name.into(), Arc::new(provider));
        }
        // TODO: Also do validation here
        Ok((self.primary.clone(), clients))
    }
}
