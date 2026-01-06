//! Jinja runtime for BAML.
//!
//! This crate provides:
//! - `OutputFormatContent` - Schema information for parsing LLM responses
//! - Jinja template rendering helpers (stubbed for now)

mod output_format;

pub use output_format::{
    OutputFormatContent, OutputFormatBuilder,
    Class, ClassField, Enum, EnumVariant, Name,
};

use baml_types::BamlValue;

/// Render a template with the given context.
///
/// This is a stub implementation - real Jinja rendering will be added later.
pub fn render_template(
    template: &str,
    context: &indexmap::IndexMap<String, BamlValue>,
) -> Result<String, RenderError> {
    // Simple variable substitution (placeholder for real jinja rendering)
    let mut result = template.to_string();

    for (key, value) in context {
        let pattern = format!("{{{{ {} }}}}", key);
        let replacement = match value {
            BamlValue::String(s) => s.clone(),
            BamlValue::Int(i) => i.to_string(),
            BamlValue::Float(f) => f.to_string(),
            BamlValue::Bool(b) => b.to_string(),
            _ => serde_json::to_string(value).unwrap_or_default(),
        };
        result = result.replace(&pattern, &replacement);
    }

    Ok(result)
}

/// Error during template rendering.
#[derive(Debug, thiserror::Error)]
pub enum RenderError {
    #[error("Template error: {0}")]
    TemplateError(String),

    #[error("Variable not found: {0}")]
    VariableNotFound(String),

    #[error("Render error: {0}")]
    Other(String),
}

/// Evaluate a Jinja expression on a BamlValue.
///
/// This is a stub implementation - real expression evaluation will be added later.
pub fn evaluate_predicate(
    _value: &BamlValue,
    expression: &baml_types::JinjaExpression,
) -> Result<bool, RenderError> {
    // Stub: always return true for now
    // TODO: Integrate with minijinja for real expression evaluation
    let _ = expression;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_template_simple() {
        let mut ctx = indexmap::IndexMap::new();
        ctx.insert("name".to_string(), BamlValue::String("Alice".to_string()));

        let result = render_template("Hello, {{ name }}!", &ctx);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "Hello, Alice!");
    }
}
