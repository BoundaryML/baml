use std::borrow::Cow;

use baml_rpc::{runtime_api, NarrowingType};
use baml_types::{
    baml_value::TypeQuery, ir_type::TypeGeneric, type_meta, BamlValueWithMeta, Constraint, HasType,
    StreamingMode, TypeValue,
};

use super::{IRRpcState, IntoRpcEvent};

/// Convert a BamlValueWithMeta to RPC event without generating type references (uses Unknown instead).
/// This is more efficient for Native functions where type information isn't needed.
pub(super) fn to_rpc_event_without_types<
    'a,
    T: HasType<type_meta::NonStreaming> + std::fmt::Debug,
>(
    value: &'a BamlValueWithMeta<T>,
    lookup: &(impl IRRpcState + ?Sized),
) -> runtime_api::BamlValue<'a> {
    let content = match value {
        BamlValueWithMeta::String(s, _) => {
            baml_rpc::runtime_api::ValueContent::String(Cow::Borrowed(s))
        }
        BamlValueWithMeta::Int(v, _) => baml_rpc::runtime_api::ValueContent::Int(*v),
        BamlValueWithMeta::Float(v, _) => baml_rpc::runtime_api::ValueContent::Float(*v),
        BamlValueWithMeta::Bool(v, _) => baml_rpc::runtime_api::ValueContent::Boolean(*v),
        BamlValueWithMeta::Map(index_map, _) => baml_rpc::runtime_api::ValueContent::Map(
            index_map
                .iter()
                .map(|(k, v)| (k.clone(), to_rpc_event_without_types(v, lookup)))
                .collect(),
        ),
        BamlValueWithMeta::List(baml_value_with_metas, _) => {
            baml_rpc::runtime_api::ValueContent::List(
                baml_value_with_metas
                    .iter()
                    .map(|v| to_rpc_event_without_types(v, lookup))
                    .collect(),
            )
        }
        BamlValueWithMeta::Media(baml_media, _) => {
            baml_rpc::runtime_api::ValueContent::Media(baml_media.to_rpc_event(lookup))
        }
        BamlValueWithMeta::Enum(name, value, _) => baml_rpc::runtime_api::ValueContent::Enum {
            value: value.clone(),
        },
        BamlValueWithMeta::Class(_, index_map, _) => baml_rpc::runtime_api::ValueContent::Class {
            fields: index_map
                .iter()
                .map(|(k, v)| (k.clone(), to_rpc_event_without_types(v, lookup)))
                .collect(),
        },
        BamlValueWithMeta::Null(_) => baml_rpc::runtime_api::ValueContent::Null,
    };

    baml_rpc::runtime_api::BamlValue {
        metadata: runtime_api::ValueMetadata {
            type_index: runtime_api::TypeIndex::NotUnion,
            type_ref: baml_rpc::TypeReference::Unknown,
            check_results: None,
        },
        value: content,
    }
}

impl<'a, T: HasType<type_meta::NonStreaming> + std::fmt::Debug>
    IntoRpcEvent<'a, runtime_api::BamlValue<'a>> for BamlValueWithMeta<T>
{
    fn to_rpc_event(&'a self, lookup: &(impl IRRpcState + ?Sized)) -> runtime_api::BamlValue<'a> {
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

        baml_rpc::runtime_api::BamlValue {
            metadata: runtime_api::ValueMetadata {
                type_index: match &type_ref {
                    baml_rpc::TypeReferenceWithMetadata::Union { union_type, .. } => match value {
                        baml_rpc::runtime_api::ValueContent::Null => runtime_api::TypeIndex::Null,
                        _ => {
                            // Find which type in the union matches the actual value type
                            match union_type.types.iter().position(|union_variant_type| {
                                matches_value_with_rpc_type(self, union_variant_type, lookup)
                            }) {
                                Some(idx) => runtime_api::TypeIndex::Index(idx),
                                None => {
                                    baml_log::warn!(
                                        "Unexpected Error. Please report this error on https://github.com/boundaryml/baml/issues.\nCould not determine union variant index for value type: {} for value {}",
                                        type_ref, value
                                    );
                                    runtime_api::TypeIndex::NotFound
                                }
                            }
                        }
                    },
                    _ => runtime_api::TypeIndex::NotUnion,
                },
                type_ref,
                check_results: None,
            },
            value,
        }
    }
}

// Helper function to check if a BamlValueWithMeta matches a specific RPC type reference
fn matches_value_with_rpc_type<T: HasType<type_meta::NonStreaming>>(
    value: &BamlValueWithMeta<T>,
    rpc_type_ref: &baml_rpc::TypeReferenceWithMetadata<baml_rpc::TypeMetadata>,
    _lookup: &(impl IRRpcState + ?Sized),
) -> bool {
    use baml_rpc::TypeReferenceWithMetadata;
    match (value, rpc_type_ref) {
        (BamlValueWithMeta::String(_, _), TypeReferenceWithMetadata::String(_)) => true,
        (BamlValueWithMeta::Int(_, _), TypeReferenceWithMetadata::Int(_)) => true,
        (BamlValueWithMeta::Float(_, _), TypeReferenceWithMetadata::Float(_)) => true,
        (BamlValueWithMeta::Bool(_, _), TypeReferenceWithMetadata::Bool(_)) => true,
        (BamlValueWithMeta::Null(_), _) => true, // Null can match any type in a union
        (
            BamlValueWithMeta::Enum(enum_name, _, _),
            TypeReferenceWithMetadata::Enum { type_id, .. },
        ) => {
            // Compare the enum name with the type_id name
            enum_name == type_id.0.name()
        }
        (
            BamlValueWithMeta::Class(class_name, _, _),
            TypeReferenceWithMetadata::Class { type_id, .. },
        ) => {
            // Compare the class name with the type_id name
            class_name == type_id.0.name()
        }
        (
            BamlValueWithMeta::List(list_values, _),
            TypeReferenceWithMetadata::List(inner_type, _),
        ) => list_values
            .iter()
            .all(|value| matches_value_with_rpc_type(value, inner_type, _lookup)),
        (
            BamlValueWithMeta::Map(map_values, _),
            TypeReferenceWithMetadata::Map { key, value, .. },
        ) => map_values.iter().all(|(map_key, map_value)| {
            // TODO: Validate key type
            // matches_value_with_rpc_type(map_key, key, lookup)
            matches_value_with_rpc_type(map_value, value, _lookup)
        }),
        (BamlValueWithMeta::Media(media, _), TypeReferenceWithMetadata::Media(media_type, _)) => {
            matches!(
                (&media.media_type, media_type),
                (
                    baml_types::BamlMediaType::Image,
                    baml_rpc::MediaTypeDefinition::Image
                ) | (
                    baml_types::BamlMediaType::Audio,
                    baml_rpc::MediaTypeDefinition::Audio
                ) | (
                    baml_types::BamlMediaType::Pdf,
                    baml_rpc::MediaTypeDefinition::Pdf
                ) | (
                    baml_types::BamlMediaType::Video,
                    baml_rpc::MediaTypeDefinition::Video
                )
            )
        }
        // Also handle literal values
        (
            BamlValueWithMeta::String(s, _),
            TypeReferenceWithMetadata::Literal(baml_rpc::LiteralTypeDefinition::String(lit_s), _),
        ) => s == lit_s,
        (
            BamlValueWithMeta::Int(i, _),
            TypeReferenceWithMetadata::Literal(baml_rpc::LiteralTypeDefinition::Int(lit_i), _),
        ) => i == lit_i,
        (
            BamlValueWithMeta::Bool(b, _),
            TypeReferenceWithMetadata::Literal(baml_rpc::LiteralTypeDefinition::Bool(lit_b), _),
        ) => b == lit_b,
        _ => false,
    }
}

impl<'a> IntoRpcEvent<'a, baml_rpc::TypeReference> for baml_types::ir_type::TypeNonStreaming {
    fn to_rpc_event(&'a self, lookup: &(impl IRRpcState + ?Sized)) -> baml_rpc::TypeReference {
        use baml_rpc::{LiteralTypeDefinition, MediaTypeDefinition, TypeMetadata, TypeReference};
        let mut base_ref = match self {
            TypeGeneric::Primitive(type_value, _) => match type_value {
                baml_types::TypeValue::String => TypeReference::string(),
                baml_types::TypeValue::Int => TypeReference::int(),
                baml_types::TypeValue::Float => TypeReference::float(),
                baml_types::TypeValue::Bool => TypeReference::bool(),
                baml_types::TypeValue::Null => {
                    TypeReference::union(vec![TypeReference::string()], true)
                }
                baml_types::TypeValue::Media(baml_media_type) => {
                    TypeReference::media(match baml_media_type {
                        baml_types::BamlMediaType::Image => MediaTypeDefinition::Image,
                        baml_types::BamlMediaType::Audio => MediaTypeDefinition::Audio,
                        baml_types::BamlMediaType::Pdf => MediaTypeDefinition::Pdf,
                        baml_types::BamlMediaType::Video => MediaTypeDefinition::Video,
                    })
                }
            },
            TypeGeneric::Enum { name, .. } => lookup
                .type_lookup(name.as_str())
                .map(TypeReference::enum_type)
                .unwrap_or(TypeReference::Unknown),
            TypeGeneric::Literal(literal_value, _) => TypeReference::literal(match literal_value {
                baml_types::LiteralValue::String(s) => LiteralTypeDefinition::String(s.clone()),
                baml_types::LiteralValue::Int(i) => LiteralTypeDefinition::Int(*i),
                baml_types::LiteralValue::Bool(b) => LiteralTypeDefinition::Bool(*b),
            }),
            TypeGeneric::Class { name, .. } => lookup
                .type_lookup(name.as_str())
                .map(TypeReference::class)
                .unwrap_or(TypeReference::Unknown),
            TypeGeneric::List(field_type, _) => {
                TypeReference::list(field_type.to_rpc_event(lookup))
            }
            TypeGeneric::Map(field_type, field_type1, _) => TypeReference::map(
                field_type.to_rpc_event(lookup),
                field_type1.to_rpc_event(lookup),
            ),
            TypeGeneric::Union(field_types, _) => TypeReference::union(
                field_types
                    .iter_skip_null()
                    .into_iter()
                    .map(|t| t.to_rpc_event(lookup))
                    .collect(),
                field_types.is_optional(),
            ),
            TypeGeneric::Tuple(field_types, _) => {
                TypeReference::tuple(field_types.iter().map(|t| t.to_rpc_event(lookup)).collect())
            }
            TypeGeneric::RecursiveTypeAlias { name: alias, .. } => lookup
                .type_lookup(alias.as_str())
                .map(TypeReference::recursive_type_alias)
                .unwrap_or(TypeReference::Unknown),
            TypeGeneric::Arrow(..) => TypeReference::Unknown,
            TypeGeneric::Top(_) => panic!(
                "TypeGeneric::Top should have been resolved by the compiler before code generation. \
                 This indicates a bug in the type resolution phase."
            ),
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
    fn to_rpc_event(&'a self, lookup: &(impl IRRpcState + ?Sized)) -> baml_rpc::Expression {
        baml_rpc::Expression::Jinja(self.0.to_string())
    }
}

impl<'a> IntoRpcEvent<'a, baml_rpc::runtime_api::Media<'a>> for baml_types::BamlMedia {
    fn to_rpc_event(
        &'a self,
        lookup: &(impl IRRpcState + ?Sized),
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
        lookup: &(impl IRRpcState + ?Sized),
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
