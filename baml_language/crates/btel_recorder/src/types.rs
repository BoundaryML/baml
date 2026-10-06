//! Class and enum definitions for one recording. The runtime's resolver
//! hands over each declaration's definition once (or an unavailable
//! observation), so this keeps no per-recording state: it converts what
//! arrives. Readers merge repeats (`btel_reader::types`), so a definition
//! that arrives after an unavailable observation supersedes it.
use btel_types::{FieldType, TypeDeclaration, TypeDefinition};

use crate::proto::{self, type_definition::Resolution};

/// The message published for `definition`.
pub(crate) fn message(definition: TypeDefinition) -> proto::TypeDefinition {
    proto::TypeDefinition {
        type_tag: definition.tag.as_i64(),
        resolution: Some(match definition.declaration {
            Some(declaration) => Resolution::Declaration(convert(&declaration)),
            None => Resolution::Unavailable(proto::MetadataUnavailable {}),
        }),
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
