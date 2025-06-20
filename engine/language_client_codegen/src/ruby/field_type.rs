use baml_types::{BamlMediaType, FieldType, LiteralValue, TypeValue, ir_type::UnionTypeViewGeneric};

use crate::field_type_attributes;

use super::ruby_language_features::ToRuby;

impl ToRuby for FieldType {
    fn to_ruby(&self) -> String {
        let base_repr = match self {
            FieldType::Class { name, .. } => format!("Baml::Types::{}", name.clone()),
            FieldType::Enum { name, .. } => format!("T.any(Baml::Types::{}, String)", name.clone()),
            // Sorbet does not support recursive type aliases.
            // https://sorbet.org/docs/type-aliases
            FieldType::RecursiveTypeAlias(_name, _) => "T.anything".to_string(),
            // TODO: Temporary solution until we figure out Ruby literals.
            FieldType::Literal(value, _) => value.literal_base_type().to_ruby(),
            // https://sorbet.org/docs/stdlib-generics
            FieldType::List(inner, _) => format!("T::Array[{}]", inner.to_ruby()),
            FieldType::Map(key, value, _) => format!(
                "T::Hash[{}, {}]",
                match key.as_ref() {
                    // For enums just default to strings.
                    FieldType::Enum { .. }
                    | FieldType::Literal(LiteralValue::String(_), _)
                    | FieldType::Union(_, _) => FieldType::string().to_ruby(),
                    _ => key.to_ruby(),
                },
                value.to_ruby()
            ),
            FieldType::Primitive(r#type, _) => String::from(match r#type {
                // https://sorbet.org/docs/class-types
                TypeValue::Bool => "T::Boolean",
                TypeValue::Float => "Float",
                TypeValue::Int => "Integer",
                TypeValue::String => "String",
                TypeValue::Null => "NilClass",
                // TODO: Create Baml::Types::Image
                TypeValue::Media(BamlMediaType::Image) => "Baml::Image",
                TypeValue::Media(BamlMediaType::Audio) => "Baml::Audio",
            }),
            FieldType::Union(inner, _) => {
                match inner.view() {
                    UnionTypeViewGeneric::Null => "NilClass".to_string(),
                    UnionTypeViewGeneric::Optional(field_type) => format!("T.nilable({})", field_type.to_ruby()),
                    UnionTypeViewGeneric::OneOf(field_types) => format!("T.any({})", field_types.iter().map(|t| t.to_ruby()).collect::<Vec<_>>().join(", ")),
                    UnionTypeViewGeneric::OneOfOptional(field_types) => format!("T.nilable(T.any({}))", field_types.iter().map(|t| t.to_ruby()).collect::<Vec<_>>().join(", ")),
                }
            }
            FieldType::Tuple(inner, _) => format!(
                // https://sorbet.org/docs/tuples
                "[{}]",
                inner
                    .iter()
                    .map(|t| t.to_ruby())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            FieldType::Arrow(_, _) => todo!("Arrow types should not be used in generated type definitions"),
        };
        let repr = match field_type_attributes(self) {
            Some(_) => {
                format!("Baml::Checked[{base_repr}]")
            }
            None => base_repr,
        };
        repr
    }
}
