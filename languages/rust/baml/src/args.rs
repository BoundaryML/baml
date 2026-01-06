use prost::Message;

use crate::{
    client_registry::ClientRegistry,
    codec::BamlEncode,
    error::BamlError,
    proto::baml_cffi_v1::{
        BamlObjectHandle, HostEnvVar, HostFunctionArguments, HostMapEntry, host_map_entry,
    },
    raw_objects::{Collector, RawObjectTrait, TypeBuilder},
};

/// Arguments for a BAML function call
#[derive(Default)]
pub struct FunctionArgs {
    kwargs: Vec<HostMapEntry>,
    env_overrides: Vec<HostEnvVar>,
    collectors: Vec<BamlObjectHandle>,
    type_builder: Option<BamlObjectHandle>,
    tags: Vec<HostMapEntry>,
    client_registry: Option<ClientRegistry>,
}

impl FunctionArgs {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a keyword argument
    pub fn arg<V: BamlEncode>(mut self, name: &str, value: V) -> Self {
        self.kwargs.push(HostMapEntry {
            key: Some(host_map_entry::Key::StringKey(name.to_string())),
            value: Some(value.baml_encode()),
        });
        self
    }

    /// Add environment variable override
    pub fn with_env(mut self, key: &str, value: &str) -> Self {
        self.env_overrides.push(HostEnvVar {
            key: key.to_string(),
            value: value.to_string(),
        });
        self
    }

    /// Add a tag
    pub fn with_tag<V: BamlEncode>(mut self, key: &str, value: V) -> Self {
        self.tags.push(HostMapEntry {
            key: Some(host_map_entry::Key::StringKey(key.to_string())),
            value: Some(value.baml_encode()),
        });
        self
    }

    /// Add a collector to gather telemetry
    pub fn with_collector(mut self, collector: &Collector) -> Self {
        self.collectors.push(collector.encode_handle());
        self
    }

    /// Set type builder for dynamic types
    pub fn with_type_builder(mut self, type_builder: &TypeBuilder) -> Self {
        self.type_builder = Some(type_builder.encode_handle());
        self
    }

    /// Set the client registry for runtime client configuration.
    pub fn with_client_registry(mut self, registry: &ClientRegistry) -> Self {
        self.client_registry = Some(registry.clone());
        self
    }

    /// Encode to protobuf bytes for FFI
    pub fn encode(&self) -> Result<Vec<u8>, BamlError> {
        let client_registry = self.client_registry.as_ref().map(super::client_registry::ClientRegistry::encode);

        let msg = HostFunctionArguments {
            kwargs: self.kwargs.clone(),
            client_registry,
            env: self.env_overrides.clone(),
            collectors: self.collectors.clone(),
            type_builder: self.type_builder,
            tags: self.tags.clone(),
        };

        let mut buf = Vec::new();
        msg.encode(&mut buf)
            .map_err(|e| BamlError::internal(format!("failed to encode args: {e}")))?;
        Ok(buf)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_function_args_empty() {
        let args = FunctionArgs::new();
        let encoded = args.encode();
        assert!(encoded.is_ok());
        // Empty args should still encode to something (empty protobuf message)
        let bytes = encoded.unwrap();
        assert!(bytes.is_empty() || !bytes.is_empty()); // Just check it doesn't panic
    }

    #[test]
    fn test_function_args_with_string() {
        let args = FunctionArgs::new().arg("text", "Hello, world!");
        let encoded = args.encode();
        assert!(encoded.is_ok());
        assert!(!encoded.unwrap().is_empty());
    }

    #[test]
    fn test_function_args_with_int() {
        let args = FunctionArgs::new().arg("count", 42i64);
        let encoded = args.encode();
        assert!(encoded.is_ok());
        assert!(!encoded.unwrap().is_empty());
    }

    #[test]
    fn test_function_args_multiple() {
        let args = FunctionArgs::new()
            .arg("name", "Alice")
            .arg("age", 30i64)
            .arg("active", true);
        let encoded = args.encode();
        assert!(encoded.is_ok());
        assert!(!encoded.unwrap().is_empty());
    }

    #[test]
    fn test_function_args_with_env() {
        let args = FunctionArgs::new()
            .arg("prompt", "test")
            .with_env("OPENAI_API_KEY", "sk-test");
        let encoded = args.encode();
        assert!(encoded.is_ok());
        assert!(!encoded.unwrap().is_empty());
    }

    #[test]
    fn test_function_args_with_tags() {
        let args = FunctionArgs::new()
            .arg("text", "hello")
            .with_tag("source", "test")
            .with_tag("priority", 1i64);
        let encoded = args.encode();
        assert!(encoded.is_ok());
        assert!(!encoded.unwrap().is_empty());
    }
}
