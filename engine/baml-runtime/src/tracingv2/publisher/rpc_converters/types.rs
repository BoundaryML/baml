use std::borrow::Cow;

use baml_rpc::{runtime_api, NarrowingType};
use baml_types::{BamlValueWithMeta, Constraint, HasFieldType};

use super::{IntoRpcEvent, TypeLookup};

impl<'a, T: HasFieldType> IntoRpcEvent<'a, runtime_api::BamlValue<'a>> for BamlValueWithMeta<T> {
    fn to_rpc_event(&'a self, lookup: &(impl TypeLookup + ?Sized)) -> runtime_api::BamlValue<'a> {
        let type_ref = self.field_type().to_rpc_event(lookup);
        let value = match self {
            BamlValueWithMeta::String(s, _) => {
                baml_rpc::runtime_api::ValueContent::String(Cow::Borrowed(s))
            }
            BamlValueWithMeta::Int(v, _) => baml_rpc::runtime_api::ValueContent::Int(*v),
            BamlValueWithMeta::Float(v, _) => baml_rpc::runtime_api::ValueContent::Float(*v),
            BamlValueWithMeta::Bool(v, _) => baml_rpc::runtime_api::ValueContent::Boolean(*v),
            BamlValueWithMeta::Map(index_map, _) => baml_rpc::runtime_api::ValueContent::Map(
                index_map
                    .iter()
                    .map(|(k, v)| (k.clone(), v.to_rpc_event(lookup)))
                    .collect(),
            ),
            BamlValueWithMeta::List(baml_value_with_metas, _) => {
                baml_rpc::runtime_api::ValueContent::List(
                    baml_value_with_metas
                        .iter()
                        .map(|v| v.to_rpc_event(lookup))
                        .collect(),
                )
            }
            BamlValueWithMeta::Media(baml_media, _) => {
                baml_rpc::runtime_api::ValueContent::Media(baml_media.to_rpc_event(lookup))
            }
            BamlValueWithMeta::Enum(name, value, _) => baml_rpc::runtime_api::ValueContent::Enum {
                value: value.clone(),
            },
            BamlValueWithMeta::Class(_, index_map, _) => {
                baml_rpc::runtime_api::ValueContent::Class {
                    fields: index_map
                        .iter()
                        .map(|(k, v)| (k.clone(), v.to_rpc_event(lookup)))
                        .collect(),
                }
            }
            BamlValueWithMeta::Null(_) => baml_rpc::runtime_api::ValueContent::Null,
        };

        baml_rpc::runtime_api::BamlValue { type_ref, value }
    }
}

impl<'a> IntoRpcEvent<'a, baml_rpc::TypeReference> for baml_types::FieldType {
    fn to_rpc_event(&'a self, lookup: &(impl TypeLookup + ?Sized)) -> baml_rpc::TypeReference {
        let simplified = self.simplify();
        use baml_rpc::{LiteralTypeDefinition, MediaTypeDefinition, TypeMetadata, TypeReference};
        let mut base_ref = match simplified {
            baml_types::FieldType::Primitive(type_value, _) => match type_value {
                baml_types::TypeValue::String => TypeReference::string(),
                baml_types::TypeValue::Int => TypeReference::int(),
                baml_types::TypeValue::Float => TypeReference::float(),
                baml_types::TypeValue::Bool => TypeReference::bool(),
                baml_types::TypeValue::Null => TypeReference::null(),
                baml_types::TypeValue::Media(baml_media_type) => {
                    TypeReference::media(match baml_media_type {
                        baml_types::BamlMediaType::Image => MediaTypeDefinition::Image,
                        baml_types::BamlMediaType::Audio => MediaTypeDefinition::Audio,
                    })
                }
            },
            baml_types::FieldType::Enum { name, .. } => lookup
                .type_lookup(name.as_str())
                .map(TypeReference::enum_type)
                .unwrap_or(TypeReference::Unknown),
            baml_types::FieldType::Literal(literal_value, _) => {
                TypeReference::literal(match literal_value {
                    baml_types::LiteralValue::String(s) => LiteralTypeDefinition::String(s),
                    baml_types::LiteralValue::Int(i) => LiteralTypeDefinition::Int(i),
                    baml_types::LiteralValue::Bool(b) => LiteralTypeDefinition::Bool(b),
                })
            }
            baml_types::FieldType::Class { name, .. } => lookup
                .type_lookup(name.as_str())
                .map(TypeReference::class)
                .unwrap_or(TypeReference::Unknown),
            baml_types::FieldType::List(field_type, _) => {
                TypeReference::list(field_type.to_rpc_event(lookup))
            }
            baml_types::FieldType::Map(field_type, field_type1, _) => TypeReference::map(
                field_type.to_rpc_event(lookup),
                field_type1.to_rpc_event(lookup),
            ),
            baml_types::FieldType::Union(field_types, _) => TypeReference::union(
                field_types
                    .iter_include_null()
                    .into_iter()
                    .map(|t| t.to_rpc_event(lookup))
                    .collect(),
            ),
            baml_types::FieldType::Tuple(field_types, _) => {
                TypeReference::tuple(field_types.iter().map(|t| t.to_rpc_event(lookup)).collect())
            }
            baml_types::FieldType::RecursiveTypeAlias { name: alias, .. } => lookup
                .type_lookup(alias.as_str())
                .map(TypeReference::recursive_type_alias)
                .unwrap_or(TypeReference::Unknown),
            baml_types::FieldType::Arrow(..) => TypeReference::Unknown,
        };
        if !self.meta().constraints.is_empty() {
            let constraints = self.meta().constraints.clone();
            let (asserts, checks) = constraints
                .into_iter()
                .partition::<Vec<_>, _>(|c| c.level == baml_types::ConstraintLevel::Assert);

            let narrowed_asserts = asserts
                .into_iter()
                .map(|c| NarrowingType {
                    name: c.label.clone(),
                    expressions: c.expression.to_rpc_event(lookup),
                })
                .collect();

            let narrowed_checks = checks
                .into_iter()
                .map(|c| NarrowingType {
                    name: c.label.expect("checks must be named").clone(),
                    expressions: c.expression.to_rpc_event(lookup),
                })
                .collect();

            let new_meta = TypeMetadata::new(narrowed_checks, narrowed_asserts);
            if let Some(metadata) = base_ref.metadata_mut() {
                metadata.merge(new_meta);
            }
            base_ref
        } else {
            base_ref
        }
    }
}

impl<'a> IntoRpcEvent<'a, baml_rpc::Expression> for baml_types::JinjaExpression {
    fn to_rpc_event(&'a self, lookup: &(impl TypeLookup + ?Sized)) -> baml_rpc::Expression {
        baml_rpc::Expression::Jinja(self.0.to_string())
    }
}

impl<'a> IntoRpcEvent<'a, baml_rpc::runtime_api::Media<'a>> for baml_types::BamlMedia {
    fn to_rpc_event(
        &'a self,
        lookup: &(impl TypeLookup + ?Sized),
    ) -> baml_rpc::runtime_api::Media<'a> {
        baml_rpc::runtime_api::Media {
            mime_type: self.mime_type.clone(),
            value: self.content.to_rpc_event(lookup),
        }
    }
}

impl<'a> IntoRpcEvent<'a, baml_rpc::runtime_api::MediaValue<'a>> for baml_types::BamlMediaContent {
    fn to_rpc_event(
        &'a self,
        lookup: &(impl TypeLookup + ?Sized),
    ) -> baml_rpc::runtime_api::MediaValue<'a> {
        match self {
            baml_types::BamlMediaContent::Url(url) => {
                baml_rpc::runtime_api::MediaValue::Url(Cow::Borrowed(url.url.as_str()))
            }
            baml_types::BamlMediaContent::Base64(base64) => {
                baml_rpc::runtime_api::MediaValue::Base64(Cow::Borrowed(base64.base64.as_str()))
            }
            baml_types::BamlMediaContent::File(file_path) => {
                baml_rpc::runtime_api::MediaValue::FilePath(Cow::Owned(
                    file_path.relpath.display().to_string(),
                ))
            }
        }
    }
}
