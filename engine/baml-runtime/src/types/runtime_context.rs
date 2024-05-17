use anyhow::Result;
use internal_baml_core::ir::repr::Expression;
use serde::{self, Deserialize};
use serde_json::{self};
use std::collections::HashMap;

#[derive(Deserialize, Debug, Default, Clone)]
#[serde(deny_unknown_fields)]
pub struct RuntimeContext {
    #[serde(default = "HashMap::new")]
    pub env: HashMap<String, String>,
    #[serde(default = "HashMap::new")]
    pub tags: HashMap<String, serde_json::Value>,
}

impl RuntimeContext {
    #[cfg(feature = "no_wasm")]
    pub fn from_env() -> Self {
        Self {
            env: std::env::vars_os()
                .map(|(k, v)| {
                    (
                        k.to_string_lossy().to_string(),
                        v.to_string_lossy().to_string(),
                    )
                })
                .collect(),
            tags: HashMap::new(),
        }
    }

    pub fn merge<O: Into<RuntimeContext>>(mut self, other: Option<O>) -> Self {
        let Some(other) = other else {
            return self;
        };
        let other = other.into();
        self.env.extend(other.env.into_iter());
        self.tags.extend(other.tags.into_iter());
        self
    }

    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_env(mut self, key: String, value: String) -> Self {
        self.env.insert(key, value);
        self
    }

    pub fn with_env(mut self, env: HashMap<String, String>) -> Self {
        self.env = env;
        self
    }

    pub fn with_tags(mut self, tags: HashMap<String, serde_json::Value>) -> Self {
        self.tags = tags;
        self
    }

    pub fn resolve_expression<T: serde::de::DeserializeOwned>(
        &self,
        expr: &Expression,
    ) -> Result<T> {
        serde_json::from_value::<T>(super::expression_helper::to_value(self, expr)?).map_err(|e| {
            anyhow::anyhow!(
                "Failed to resolve expression {:?} with error: {:?}",
                expr,
                e
            )
        })
    }
}
