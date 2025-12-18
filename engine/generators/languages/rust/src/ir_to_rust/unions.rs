use baml_types::{
    ir_type::{TypeGeneric, TypeNonStreaming, TypeStreaming},
    ToUnionName,
};

use crate::{package::CurrentRenderPackage, r#type::TypeRust};

pub fn ir_union_to_rust(
    union: &TypeNonStreaming,
    pkg: &CurrentRenderPackage,
) -> impl Iterator<Item = crate::generated_types::UnionRust> {
    let rust_type = crate::ir_to_rust::type_to_rust(union, pkg.lookup());
    let result: std::vec::IntoIter<crate::generated_types::UnionRust> = rust_type
        .flatten_unions()
        .into_iter()
        .filter_map(|rust_type| {
            if let TypeRust::Union { name, .. } = rust_type {
                let TypeNonStreaming::Union(union_type_generic, _) = union else {
                    panic!("ir_union_to_rust expects a union. Got: {union}");
                };
                let variants = union_type_generic
                    .iter_skip_null()
                    .iter()
                    .map(|t| {
                        let rust_type = crate::ir_to_rust::type_to_rust(t, pkg.lookup());
                        crate::generated_types::VariantRust {
                            name: rust_type.default_name_within_union(),
                            cffi_name: t.to_union_name(false),
                            literal_repr: match t {
                                TypeGeneric::Literal(l, ..) => match l {
                                    baml_types::LiteralValue::String(s) => Some(format!(
                                        "\"{}\"",
                                        s.replace("\\", "\\\\").replace("\"", "\\\"")
                                    )),
                                    baml_types::LiteralValue::Int(i) => Some(i.to_string()),
                                    baml_types::LiteralValue::Bool(true) => {
                                        Some("true".to_string())
                                    }
                                    baml_types::LiteralValue::Bool(false) => {
                                        Some("false".to_string())
                                    }
                                },
                                _ => None,
                            },
                            type_: rust_type,
                        }
                    })
                    .collect::<Vec<_>>();
                Some(crate::generated_types::UnionRust {
                    name: name.clone(),
                    cffi_name: union.to_union_name(false),
                    docstring: Some(format!("Generated from: {union}")),
                    variants,
                })
            } else {
                None
            }
        })
        .collect::<Vec<_>>()
        .into_iter();
    result
}

pub fn ir_union_to_rust_stream(
    stream_union: &TypeStreaming,
    pkg: &CurrentRenderPackage,
) -> impl Iterator<Item = crate::generated_types::UnionRust> {
    if matches!(
        stream_union.mode(&baml_types::StreamingMode::Streaming, pkg.lookup(), 1),
        Ok(baml_types::StreamingMode::NonStreaming) | Err(_)
    ) {
        return Vec::new().into_iter();
    }
    let rust_type = crate::ir_to_rust::stream_type_to_rust(stream_union, pkg.lookup());
    let result: Vec<crate::generated_types::UnionRust> = rust_type
        .flatten_unions()
        .into_iter()
        .filter_map(|rust_type| {
            if let TypeRust::Union { name, .. } = rust_type {
                let TypeStreaming::Union(union_type_generic, _) = stream_union else {
                    panic!("ir_union_to_rust expects a union. Got: {stream_union}");
                };
                let variants = union_type_generic
                    .iter_skip_null()
                    .iter()
                    .map(|t| {
                        let rust_type = crate::ir_to_rust::stream_type_to_rust(t, pkg.lookup());
                        crate::generated_types::VariantRust {
                            name: rust_type.default_name_within_union(),
                            cffi_name: t.to_union_name(false),
                            literal_repr: match t {
                                TypeGeneric::Literal(l, ..) => match l {
                                    baml_types::LiteralValue::String(s) => Some(format!(
                                        "\"{}\"",
                                        s.replace("\\", "\\\\").replace("\"", "\\\"")
                                    )),
                                    baml_types::LiteralValue::Int(i) => Some(i.to_string()),
                                    baml_types::LiteralValue::Bool(true) => {
                                        Some("true".to_string())
                                    }
                                    baml_types::LiteralValue::Bool(false) => {
                                        Some("false".to_string())
                                    }
                                },
                                _ => None,
                            },
                            type_: rust_type,
                        }
                    })
                    .collect::<Vec<_>>();
                Some(crate::generated_types::UnionRust {
                    name,
                    cffi_name: stream_union.to_union_name(false),
                    docstring: Some(format!("Generated from: {stream_union}")),
                    variants,
                })
            } else {
                None
            }
        })
        .collect::<Vec<_>>();

    result.into_iter()
}
