use std::collections::HashMap;

#[derive(Default)]
pub struct RuntimeContext {
    pub env: HashMap<String, String>,
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
}
