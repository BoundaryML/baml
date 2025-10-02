use crate::types::{BamlMap, BamlValue};
use crate::BamlResult;
use std::collections::HashMap;

/// Context for BAML function calls
#[derive(Debug, Clone)]
pub struct BamlContext {
    /// Function arguments
    pub(crate) args: BamlMap<String, BamlValue>,
    /// Environment variables override
    pub(crate) env_vars: HashMap<String, String>,
    /// Client registry override
    pub(crate) client_registry: Option<crate::types::ClientRegistry>,
    /// Type builder override
    pub(crate) type_builder: Option<crate::types::TypeBuilder>,
    /// Collectors for usage tracking
    pub(crate) collectors: Vec<std::sync::Arc<crate::types::Collector>>,
    /// Tags for metadata
    pub(crate) tags: HashMap<String, String>,
}

impl BamlContext {
    /// Create a new empty context
    pub fn new() -> Self {
        Self {
            args: BamlMap::new(),
            env_vars: HashMap::new(),
            client_registry: None,
            type_builder: None,
            collectors: Vec::new(),
            tags: HashMap::new(),
        }
    }

    /// Set a function argument
    pub fn set_arg<K: Into<String>, V: crate::types::ToBamlValue>(
        mut self,
        key: K,
        value: V,
    ) -> BamlResult<Self> {
        let baml_value = value.to_baml_value()?;
        self.args.insert(key.into(), baml_value);
        Ok(self)
    }

    /// Set multiple arguments from an iterator
    pub fn set_args<I, K, V>(mut self, args: I) -> BamlResult<Self>
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<String>,
        V: crate::types::ToBamlValue,
    {
        for (key, value) in args {
            let baml_value = value.to_baml_value()?;
            self.args.insert(key.into(), baml_value);
        }
        Ok(self)
    }

    /// Set an environment variable override
    pub fn set_env_var<K: Into<String>, V: Into<String>>(mut self, key: K, value: V) -> Self {
        self.env_vars.insert(key.into(), value.into());
        self
    }

    /// Set multiple environment variable overrides
    pub fn set_env_vars<I, K, V>(mut self, env_vars: I) -> Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<String>,
        V: Into<String>,
    {
        for (key, value) in env_vars {
            self.env_vars.insert(key.into(), value.into());
        }
        self
    }

    /// Set client registry override
    pub fn with_client_registry(mut self, client_registry: crate::types::ClientRegistry) -> Self {
        self.client_registry = Some(client_registry);
        self
    }

    /// Set type builder override
    pub fn with_type_builder(mut self, type_builder: crate::types::TypeBuilder) -> Self {
        self.type_builder = Some(type_builder);
        self
    }

    /// Add a collector for usage tracking
    pub fn with_collector(mut self, collector: std::sync::Arc<crate::types::Collector>) -> Self {
        self.collectors.push(collector);
        self
    }

    /// Add multiple collectors
    pub fn with_collectors(
        mut self,
        collectors: Vec<std::sync::Arc<crate::types::Collector>>,
    ) -> Self {
        self.collectors.extend(collectors);
        self
    }

    /// Set a tag
    pub fn set_tag<K: Into<String>, V: Into<String>>(mut self, key: K, value: V) -> Self {
        self.tags.insert(key.into(), value.into());
        self
    }

    /// Set multiple tags
    pub fn set_tags<I, K, V>(mut self, tags: I) -> Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<String>,
        V: Into<String>,
    {
        for (key, value) in tags {
            self.tags.insert(key.into(), value.into());
        }
        self
    }

    /// Get the function arguments
    pub fn args(&self) -> &BamlMap<String, BamlValue> {
        &self.args
    }

    /// Get the environment variable overrides
    pub fn env_vars(&self) -> &HashMap<String, String> {
        &self.env_vars
    }

    /// Get the client registry override
    pub fn client_registry(&self) -> Option<&crate::types::ClientRegistry> {
        self.client_registry.as_ref()
    }

    /// Get the type builder override
    pub fn type_builder(&self) -> Option<&crate::types::TypeBuilder> {
        self.type_builder.as_ref()
    }

    /// Get the collectors
    pub fn collectors(&self) -> &[std::sync::Arc<crate::types::Collector>] {
        &self.collectors
    }

    /// Get the tags
    pub fn tags(&self) -> &HashMap<String, String> {
        &self.tags
    }
}

impl Default for BamlContext {
    fn default() -> Self {
        Self::new()
    }
}

/// Builder for creating BamlContext instances
#[derive(Debug, Clone, Default)]
pub struct BamlContextBuilder {
    context: BamlContext,
}

impl BamlContextBuilder {
    /// Create a new context builder
    pub fn new() -> Self {
        Self::default()
    }

    /// Set a function argument
    pub fn arg<K: Into<String>, V: crate::types::ToBamlValue>(
        mut self,
        key: K,
        value: V,
    ) -> BamlResult<Self> {
        self.context = self.context.set_arg(key, value)?;
        Ok(self)
    }

    /// Set an environment variable
    pub fn env_var<K: Into<String>, V: Into<String>>(mut self, key: K, value: V) -> Self {
        self.context = self.context.set_env_var(key, value);
        self
    }

    /// Set client registry
    pub fn client_registry(mut self, client_registry: crate::types::ClientRegistry) -> Self {
        self.context = self.context.with_client_registry(client_registry);
        self
    }

    /// Set type builder
    pub fn type_builder(mut self, type_builder: crate::types::TypeBuilder) -> Self {
        self.context = self.context.with_type_builder(type_builder);
        self
    }

    /// Add collector
    pub fn collector(mut self, collector: std::sync::Arc<crate::types::Collector>) -> Self {
        self.context = self.context.with_collector(collector);
        self
    }

    /// Set a tag
    pub fn tag<K: Into<String>, V: Into<String>>(mut self, key: K, value: V) -> Self {
        self.context = self.context.set_tag(key, value);
        self
    }

    /// Build the context
    pub fn build(self) -> BamlContext {
        self.context
    }
}
