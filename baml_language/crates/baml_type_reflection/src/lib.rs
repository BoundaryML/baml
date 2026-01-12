//! Dynamic BAML context - optional schema extensions.

/// Optional schema extensions (for @@dynamic types, client overrides).
/// STUB: Full implementation deferred.
#[derive(Debug, Clone, Default)]
pub struct DynamicBamlContext {
    /// Dynamic type definitions (stub).
    pub type_builder: Option<TypeBuilderStub>,
    /// Client overrides (stub).
    pub client_registry: Option<ClientRegistryStub>,
}

impl DynamicBamlContext {
    /// Create a new empty dynamic context.
    pub fn new() -> Self {
        Self::default()
    }

    /// Check if this context has any dynamic configuration.
    pub fn is_empty(&self) -> bool {
        self.type_builder.is_none() && self.client_registry.is_none()
    }
}

/// Placeholder for TypeBuilder - will integrate with real implementation later.
#[derive(Debug, Clone)]
pub struct TypeBuilderStub {
    // TODO: Define interface
}

/// Placeholder for ClientRegistry - will integrate with real implementation later.
#[derive(Debug, Clone)]
pub struct ClientRegistryStub {
    // TODO: Define interface
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_context() {
        let ctx = DynamicBamlContext::new();
        assert!(ctx.is_empty());
    }
}
