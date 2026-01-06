//! Prepared function - validated and ready for execution.

use ir_stub::{ClientSpec, PromptTemplate, TypeRef};

use crate::types::{BamlMap, BamlValue};

/// A value paired with its type information.
#[derive(Debug, Clone)]
pub struct TypedArg {
    pub value: BamlValue,
    pub type_ref: TypeRef,
}

/// A function that has been validated and prepared for execution.
///
/// This struct contains all the information needed to render prompts
/// and execute the function against an LLM provider.
#[derive(Debug, Clone)]
pub struct PreparedFunction {
    /// Function name.
    pub function_name: String,
    /// Validated and coerced arguments.
    pub args: BamlMap<String, BamlValue>,
    /// Type-annotated arguments (for constraint checking).
    /// Uses placeholder TypeRef until HIR/TIR integration.
    pub args_with_types: BamlMap<String, TypedArg>,
    /// Output type specification.
    pub output_type: TypeRef,
    /// Client specification from function definition.
    pub client_spec: ClientSpec,
    /// Prompt template (already resolved from function config).
    pub prompt_template: PromptTemplate,
}

impl PreparedFunction {
    /// Create a new prepared function with minimal configuration.
    /// Used for testing and stub implementations.
    pub fn new_stub(
        function_name: impl Into<String>,
        args: BamlMap<String, BamlValue>,
        output_type: TypeRef,
        client_spec: ClientSpec,
        prompt_template: PromptTemplate,
    ) -> Self {
        let args_with_types = args
            .iter()
            .map(|(k, v)| {
                (
                    k.clone(),
                    TypedArg {
                        value: v.clone(),
                        type_ref: TypeRef::new("unknown"),
                    },
                )
            })
            .collect();

        Self {
            function_name: function_name.into(),
            args,
            args_with_types,
            output_type,
            client_spec,
            prompt_template,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prepared_function_construction() {
        let mut args = BamlMap::new();
        args.insert("text".to_string(), BamlValue::from("Hello, world!"));

        let prepared = PreparedFunction::new_stub(
            "ExtractName",
            args,
            TypeRef::string(),
            ClientSpec::new("openai/gpt-4"),
            PromptTemplate::new("Extract the name from: {{ text }}"),
        );

        assert_eq!(prepared.function_name, "ExtractName");
        assert_eq!(prepared.args.len(), 1);
        assert_eq!(prepared.output_type.name, "string");
        assert_eq!(prepared.client_spec.client_name, "openai/gpt-4");
    }
}
