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

    pub fn resolve_expression(&self, expr: &Expression) -> Result<serde_json::Value> {
        super::expression_helper::to_value(self, expr)
    }
}
