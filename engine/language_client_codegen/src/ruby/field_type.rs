
use baml_types::{BamlMediaType, FieldType, TypeValue};

use crate::field_type_attributes;

use super::ruby_language_features::ToRuby;

impl ToRuby for FieldType {
    fn to_ruby(&self) -> String {
        match self {
            FieldType::Class(name) => format!("Baml::Types::{}", name.clone()),
            FieldType::Enum(name) => format!("T.any(Baml::Types::{}, String)", name.clone()),
            // TODO: Temporary solution until we figure out Ruby literals.
            FieldType::Literal(value) => value.literal_base_type().to_ruby(),
            // https://sorbet.org/docs/stdlib-generics
            FieldType::List(inner) => format!("T::Array[{}]", inner.to_ruby()),
            FieldType::Map(key, value) => {
                format!("T::Hash[{}, {}]", key.to_ruby(), value.to_ruby())
            }
            FieldType::Primitive(r#type) => match r#type {
                // https://sorbet.org/docs/class-types
                TypeValue::Bool => "T::Boolean",
                TypeValue::Float => "Float",
                TypeValue::Int => "Integer",
                TypeValue::String => "String",
                TypeValue::Null => "NilClass",
                // TODO: Create Baml::Types::Image
                TypeValue::Media(BamlMediaType::Image) => "Baml::Image",
                TypeValue::Media(BamlMediaType::Audio) => "Baml::Audio",
            }
            .to_string(),
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
            FieldType::Constrained{base,..} => {
                match field_type_attributes(self) {
                    Some(_) => {
                        let base_type_ref = base.to_ruby();
                        format!("Baml::Checked[{base_type_ref}]")
                    }
                    None => {
                        base.to_ruby()
                    }
                }
            }
        }
    }
}
