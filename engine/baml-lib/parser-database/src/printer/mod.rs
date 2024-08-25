use internal_baml_diagnostics::DatamodelError;

mod print_enum_default;
mod print_type_default;

use crate::ParserDatabase;

/// Trait
pub trait WithSerializeableContent {
    /// Trait to render an object.
    fn serialize_data(&self, db: &'_ ParserDatabase) -> serde_json::Value;
}

/// Trait

/// Trait
pub trait WithSerialize: WithSerializeableContent {
    /// Trait to render an object.
    fn serialize(
        &self,
        db: &'_ ParserDatabase,
        span: &internal_baml_diagnostics::Span,
    ) -> Result<String, DatamodelError>;

    /// For generating ctx.output_format
    fn output_format(
        &self,
        db: &'_ ParserDatabase,
        span: &internal_baml_diagnostics::Span,
    ) -> Result<String, DatamodelError>;
}

/// print_type, print_enum implementation
pub fn serialize_with_printer(is_enum: bool, json: serde_json::Value) -> Result<String, String> {
    if is_enum {
        Ok(print_enum_default::print_enum(json))
    } else {
        Ok(print_type_default::print_entry(json))
    }
}
