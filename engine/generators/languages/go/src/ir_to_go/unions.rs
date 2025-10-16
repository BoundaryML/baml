use baml_types::{
    ir_type::{TypeGeneric, TypeNonStreaming, TypeStreaming},
    ToUnionName,
};

use crate::{package::CurrentRenderPackage, r#type::TypeGo};

pub fn ir_union_to_go<'a>(
    union: &TypeNonStreaming,
    pkg: &'a CurrentRenderPackage,
) -> Option<crate::generated_types::UnionGo<'a>> {
    let go_type = crate::ir_to_go::type_to_go(union, pkg.lookup());
    if let TypeGo::Union { name, .. } = go_type {
        let TypeNonStreaming::Union(union_type_generic, _) = union else {
            panic!("ir_union_to_go expects a union. Got: {union}");
        };
        let variants = union_type_generic
            .iter_skip_null()
            .iter()
            .map(|t| {
                let go_type = crate::ir_to_go::type_to_go(t, pkg.lookup());
                crate::generated_types::VariantGo {
                    name: go_type.default_name_within_union(),
                    cffi_name: t.to_union_name(),
                    literal_repr: match t {
                        TypeGeneric::Literal(l, ..) => match l {
                            baml_types::LiteralValue::String(s) => Some(format!(
                                "\"{}\"",
                                s.replace("\\", "\\\\").replace("\"", "\\\"")
                            )),
                            baml_types::LiteralValue::Int(i) => Some(i.to_string()),
                            baml_types::LiteralValue::Bool(true) => Some("true".to_string()),
                            baml_types::LiteralValue::Bool(false) => Some("false".to_string()),
                        },
                        _ => None,
                    },
                    type_: go_type,
                }
            })
            .collect::<Vec<_>>();
        Some(crate::generated_types::UnionGo {
            name,
            cffi_name: union.to_union_name(),
            docstring: Some(format!("Generated from: {union}")),
            variants,
            pkg,
        })
    } else {
        None
    }
}

pub fn ir_union_to_go_stream<'a>(
    stream_union: &TypeStreaming,
    pkg: &'a CurrentRenderPackage,
) -> Option<crate::generated_types::UnionGo<'a>> {
    if matches!(
        stream_union.mode(&baml_types::StreamingMode::Streaming, pkg.lookup()),
        Ok(baml_types::StreamingMode::NonStreaming) | Err(_)
    ) {
        return None;
    }
    let go_type = crate::ir_to_go::stream_type_to_go(stream_union, pkg.lookup());
    if let TypeGo::Union { name, .. } = go_type {
        let TypeStreaming::Union(union_type_generic, _) = stream_union else {
            panic!("ir_union_to_go expects a union. Got: {stream_union}");
        };
        let variants = union_type_generic
            .iter_skip_null()
            .iter()
            .map(|t| {
                let go_type = crate::ir_to_go::stream_type_to_go(t, pkg.lookup());
                crate::generated_types::VariantGo {
                    name: go_type.default_name_within_union(),
                    cffi_name: t.to_union_name(),
                    literal_repr: match t {
                        TypeGeneric::Literal(l, ..) => match l {
                            baml_types::LiteralValue::String(s) => Some(format!(
                                "\"{}\"",
                                s.replace("\\", "\\\\").replace("\"", "\\\"")
                            )),
                            baml_types::LiteralValue::Int(i) => Some(i.to_string()),
                            baml_types::LiteralValue::Bool(true) => Some("true".to_string()),
                            baml_types::LiteralValue::Bool(false) => Some("false".to_string()),
                        },
                        _ => None,
                    },
                    type_: go_type,
                }
            })
            .collect::<Vec<_>>();
        Some(crate::generated_types::UnionGo {
            name,
            cffi_name: stream_union.to_union_name(),
            docstring: Some(format!("Generated from: {stream_union}")),
            variants,
            pkg,
        })
    } else {
        None
    }
}
