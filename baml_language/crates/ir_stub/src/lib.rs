//! Placeholder IR types - will be replaced by HIR/TIR from salsa::Db
//!
//! This crate provides stub types for schema-ast, IR, and TypedIR dependencies.
//! These are used as placeholders while the runtime is being developed, and will
//! be replaced with actual types from the BAML language infrastructure.

use serde::{Deserialize, Serialize};

/// Placeholder for type reference.
/// Will be replaced by actual HIR/TIR type reference.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TypeRef {
    /// Type name for display/debugging
    pub name: String,
}

impl TypeRef {
    pub fn new(name: impl Into<String>) -> Self {
        Self { name: name.into() }
    }

    pub fn string() -> Self {
        Self::new("string")
    }

    pub fn int() -> Self {
        Self::new("int")
    }

    pub fn float() -> Self {
        Self::new("float")
    }

    pub fn bool() -> Self {
        Self::new("bool")
    }
}

/// Placeholder for function definition.
/// Will be replaced by actual HIR/TIR function definition.
#[derive(Debug, Clone)]
pub struct FunctionDef {
    pub name: String,
    pub params: Vec<ParamDef>,
    pub output_type: TypeRef,
    pub client_spec: ClientSpec,
    pub prompt_template: PromptTemplate,
}

/// Function parameter definition.
#[derive(Debug, Clone)]
pub struct ParamDef {
    pub name: String,
    pub param_type: TypeRef,
}

/// Placeholder for client specification.
/// Will be replaced by actual client configuration from HIR/TIR.
#[derive(Debug, Clone)]
pub struct ClientSpec {
    pub client_name: String,
    // TODO: retry policy, fallback strategy, etc.
}

impl ClientSpec {
    pub fn new(client_name: impl Into<String>) -> Self {
        Self {
            client_name: client_name.into(),
        }
    }
}

/// Placeholder for prompt template.
/// Will be replaced by actual prompt template from HIR/TIR.
#[derive(Debug, Clone)]
pub struct PromptTemplate {
    pub template: String,
    // TODO: template macros, etc.
}

impl PromptTemplate {
    pub fn new(template: impl Into<String>) -> Self {
        Self {
            template: template.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_type_ref_construction() {
        let type_ref = TypeRef::new("MyType");
        assert_eq!(type_ref.name, "MyType");
    }

    #[test]
    fn test_type_ref_primitives() {
        assert_eq!(TypeRef::string().name, "string");
        assert_eq!(TypeRef::int().name, "int");
        assert_eq!(TypeRef::float().name, "float");
        assert_eq!(TypeRef::bool().name, "bool");
    }

    #[test]
    fn test_client_spec_construction() {
        let spec = ClientSpec::new("openai/gpt-4");
        assert_eq!(spec.client_name, "openai/gpt-4");
    }
}
