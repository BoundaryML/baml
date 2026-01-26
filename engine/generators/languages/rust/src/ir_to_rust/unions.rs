use std::collections::HashSet;

use baml_types::{
    ir_type::{TypeGeneric, TypeNonStreaming, TypeStreaming},
    ToUnionName,
};

use crate::{package::CurrentRenderPackage, r#type::TypeRust, RecursiveCycles};

pub fn ir_union_to_rust(
    union: &TypeNonStreaming,
    pkg: &CurrentRenderPackage,
    cycles: &RecursiveCycles,
) -> impl Iterator<Item = crate::generated_types::UnionRust> {
    // For unions, we need to check if any class in the union is part of a cycle
    // If so, we pass that cycle to the variant type conversion
    let containing_cycle = find_cycle_for_union(union, cycles);

    let rust_type = crate::ir_to_rust::type_to_rust(union, pkg.lookup(), containing_cycle);
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
                        let rust_type =
                            crate::ir_to_rust::type_to_rust(t, pkg.lookup(), containing_cycle);
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

/// Find if any class referenced in this union is part of a recursive cycle.
/// If so, return that cycle so we can apply Box<T> to those class references.
fn find_cycle_for_union<'a>(
    union: &TypeNonStreaming,
    cycles: &'a RecursiveCycles,
) -> Option<&'a HashSet<String>> {
    if let TypeNonStreaming::Union(union_type_generic, _) = union {
        for t in union_type_generic.iter_skip_null().iter() {
            if let TypeNonStreaming::Class { name, .. } = t {
                if let Some(cycle) = cycles.iter().find(|c| c.contains(name)) {
                    return Some(cycle);
                }
            }
        }
    }
    None
}

/// Find if any class referenced in this streaming union is part of a recursive cycle.
fn find_cycle_for_stream_union<'a>(
    stream_union: &TypeStreaming,
    cycles: &'a RecursiveCycles,
) -> Option<&'a HashSet<String>> {
    if let TypeStreaming::Union(union_type_generic, _) = stream_union {
        for t in union_type_generic.iter_skip_null().iter() {
            if let TypeStreaming::Class { name, .. } = t {
                if let Some(cycle) = cycles.iter().find(|c| c.contains(name)) {
                    return Some(cycle);
                }
            }
        }
    }
    None
}

pub fn ir_union_to_rust_stream(
    stream_union: &TypeStreaming,
    pkg: &CurrentRenderPackage,
    cycles: &RecursiveCycles,
) -> impl Iterator<Item = crate::generated_types::UnionRust> {
    if matches!(
        stream_union.mode(&baml_types::StreamingMode::Streaming, pkg.lookup(), 1),
        Ok(baml_types::StreamingMode::NonStreaming) | Err(_)
    ) {
        return Vec::new().into_iter();
    }

    let containing_cycle = find_cycle_for_stream_union(stream_union, cycles);

    let rust_type =
        crate::ir_to_rust::stream_type_to_rust(stream_union, pkg.lookup(), containing_cycle);
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
                        let rust_type = crate::ir_to_rust::stream_type_to_rust(
                            t,
                            pkg.lookup(),
                            containing_cycle,
                        );
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
