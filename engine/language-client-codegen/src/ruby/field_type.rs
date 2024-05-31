use internal_baml_core::ir::{FieldType, TypeValue};

use super::ruby_language_features::ToRuby;

impl ToRuby for FieldType {
    fn to_ruby(&self) -> String {
        match self {
            FieldType::Class(name) => format!("Baml::Types::{}", name.clone()),
            FieldType::Enum(name) => format!("Baml::Types::{}", name.clone()),
            // https://sorbet.org/docs/stdlib-generics
            FieldType::List(inner) => format!("T::Array[{}]", inner.to_ruby()),
            FieldType::Map(key, value) => {
                format!("T::Hash[{}, {}]", key.to_ruby(), value.to_ruby())
            }
            FieldType::Primitive(r#type) => match r#type {
                // https://sorbet.org/docs/class-types
                TypeValue::Bool => "T::Boolean".to_string(),
                TypeValue::Float => "Float".to_string(),
                TypeValue::Int => "Integer".to_string(),
                TypeValue::String => "String".to_string(),
                TypeValue::Null => "NilClass".to_string(),
                // TODO: Create Baml::Types::Image
                TypeValue::Image => "Baml::Types::Image".to_string(),
            },
            FieldType::Union(inner) => format!(
                // https://sorbet.org/docs/union-types
                "T.any({})",
                inner
                    .iter()
                    .map(|t| t.to_ruby())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            FieldType::Tuple(inner) => format!(
                // https://sorbet.org/docs/tuples
                "[{}]",
                inner
                    .iter()
                    .map(|t| t.to_ruby())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            FieldType::Optional(inner) => format!("T.nilable({})", inner.to_ruby()),
        }
    }
}
