//! Per-call context - configuration consumed by each call.

use std::collections::HashMap;

use crate::types::BamlValue;

/// Per-call configuration, consumed by each call.
#[derive(Debug, Clone, Default)]
pub struct PerCallContext {
    /// Environment variables for this call.
    pub env_vars: HashMap<String, String>,
    /// Per-call tags.
    pub tags: HashMap<String, BamlValue>,
    /// Whether the call has been cancelled.
    cancelled: bool,
}

impl PerCallContext {
    /// Create a new per-call context.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a context with environment variables.
    pub fn with_env_vars(mut self, env_vars: HashMap<String, String>) -> Self {
        self.env_vars = env_vars;
        self
    }

    /// Add a tag to this call.
    pub fn with_tag(mut self, key: impl Into<String>, value: BamlValue) -> Self {
        self.tags.insert(key.into(), value);
        self
    }

    /// Check if the call has been cancelled.
    pub fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    /// Cancel the call.
    pub fn cancel(&mut self) {
        self.cancelled = true;
    }

    /// Get an environment variable.
    pub fn get_env(&self, key: &str) -> Option<&str> {
        self.env_vars.get(key).map(|s| s.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_env_vars() {
        let mut env = HashMap::new();
        env.insert("OPENAI_API_KEY".to_string(), "sk-test".to_string());

        let ctx = PerCallContext::new().with_env_vars(env);
        assert_eq!(ctx.get_env("OPENAI_API_KEY"), Some("sk-test"));
        assert_eq!(ctx.get_env("MISSING"), None);
    }

    #[test]
    fn test_cancellation() {
        let mut ctx = PerCallContext::new();
        assert!(!ctx.is_cancelled());

        ctx.cancel();
        assert!(ctx.is_cancelled());
    }
}
