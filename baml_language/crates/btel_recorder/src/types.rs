//! Class and enum definitions for one recording. The runtime's resolver
//! supplies owned definitions; this publishes each declaration's definition
//! once per recording, across files. An unavailable observation is written
//! once and superseded by a later definition; a definition is never
//! overwritten.
use btel_types::{FieldType, TypeDeclaration, TypeDefinition};
use rustc_hash::FxHashSet;

use crate::proto::{self, type_definition::Resolution};

#[derive(Default)]
pub(crate) struct TypeDefinitions {
    published: FxHashSet<i64>,
    unavailable: FxHashSet<i64>,
}

impl TypeDefinitions {
    /// The message to publish for `definition`, if it says something new.
    pub(crate) fn resolve(&mut self, definition: TypeDefinition) -> Option<proto::TypeDefinition> {
        let tag = definition.tag.as_i64();
        if self.published.contains(&tag) {
            return None;
        }
        let resolution = match definition.declaration {
            Some(declaration) => {
                self.published.insert(tag);
                Resolution::Declaration(convert(&declaration))
            }
            None if self.unavailable.insert(tag) => {
                Resolution::Unavailable(proto::MetadataUnavailable {})
            }
            None => return None,
        };
        Some(proto::TypeDefinition {
            type_tag: tag,
            resolution: Some(resolution),
        })
    }
}

fn convert(d: &TypeDeclaration) -> proto::TypeDeclaration {
    proto::TypeDeclaration {
        is_enum: d.is_enum,
        name: borsh::to_vec(&d.name).expect("names serialize"),
        type_params: d.type_params,
        description: d.description.clone(),
        alias: d.alias.clone(),
        docstring: d.docstring.clone(),
        attributes: attributes(&d.attributes),
        stream_done: d.stream_done,
        fields: d
            .fields
            .iter()
            .map(|f| proto::TypeField {
                name: f.name.clone(),
                schema: schema(&f.schema),
                description: f.description.clone(),
                alias: f.alias.clone(),
                docstring: f.docstring.clone(),
                attributes: attributes(&f.attributes),
                skip: f.skip,
                stream_done: f.stream_done,
                must_exist: f.must_exist,
            })
            .collect(),
        variants: d
            .variants
            .iter()
            .map(|v| proto::TypeVariant {
                name: v.name.clone(),
                description: v.description.clone(),
                alias: v.alias.clone(),
                docstring: v.docstring.clone(),
                attributes: attributes(&v.attributes),
                skip: v.skip,
            })
            .collect(),
    }
}

fn schema(ty: &FieldType) -> Vec<u8> {
    borsh::to_vec(ty).expect("types serialize")
}

fn attributes(pairs: &[(String, String)]) -> Vec<proto::TypeAttribute> {
    pairs
        .iter()
        .map(|(key, value)| proto::TypeAttribute {
            key: key.clone(),
            value: value.clone(),
        })
        .collect()
}

/// Decode a published declaration back into the owned model (readers).
pub fn decode_declaration(d: &proto::TypeDeclaration) -> Option<TypeDeclaration> {
    use borsh::BorshDeserialize;
    let pairs = |attributes: &[proto::TypeAttribute]| {
        attributes
            .iter()
            .map(|a| (a.key.clone(), a.value.clone()))
            .collect()
    };
    Some(TypeDeclaration {
        is_enum: d.is_enum,
        name: baml_type::DeclarationName::try_from_slice(&d.name).ok()?,
        type_params: d.type_params,
        description: d.description.clone(),
        alias: d.alias.clone(),
        docstring: d.docstring.clone(),
        attributes: pairs(&d.attributes),
        stream_done: d.stream_done,
        fields: d
            .fields
            .iter()
            .map(|f| {
                Some(btel_types::TypeField {
                    name: f.name.clone(),
                    schema: FieldType::try_from_slice(&f.schema).ok()?,
                    description: f.description.clone(),
                    alias: f.alias.clone(),
                    docstring: f.docstring.clone(),
                    attributes: pairs(&f.attributes),
                    skip: f.skip,
                    stream_done: f.stream_done,
                    must_exist: f.must_exist,
                })
            })
            .collect::<Option<_>>()?,
        variants: d
            .variants
            .iter()
            .map(|v| btel_types::TypeVariant {
                name: v.name.clone(),
                description: v.description.clone(),
                alias: v.alias.clone(),
                docstring: v.docstring.clone(),
                attributes: pairs(&v.attributes),
                skip: v.skip,
            })
            .collect(),
    })
}
