use baml_types::{ir_type::TypeStreaming, FieldType, ToUnionName};

use crate::{package::CurrentRenderPackage, r#type::TypeGo};

pub fn ir_union_to_go<'a>(union: &FieldType, pkg: &'a CurrentRenderPackage) -> Option<crate::generated_types::UnionGo<'a>> {
    let go_type = crate::ir_to_go::type_to_go(union, pkg.lookup());
    if let TypeGo::Union { name, .. } = go_type {
        let FieldType::Union(union_type_generic, _) = union else {
            panic!("ir_union_to_go expects a union. Got: {}", union);
        };
        let variants = union_type_generic.iter_skip_null().iter().map(|t| {
            let go_type = crate::ir_to_go::type_to_go(t, pkg.lookup());
            crate::generated_types::VariantGo {
                name: go_type.default_name_within_union(),
                cffi_name: t.to_union_name(),
                type_: go_type,
            }
            })
            .collect::<Vec<_>>();
        Some(crate::generated_types::UnionGo {
            name,
            cffi_name: union.to_union_name(),
            docstring: Some(format!("Generated from: {}", union)),
            variants,
            pkg,
        })
    } else {
        None
    }
}

pub fn ir_union_to_go_stream<'a>(union: &FieldType, pkg: &'a CurrentRenderPackage) -> Option<crate::generated_types::UnionGo<'a>> {
    let stream_union = union.partialize(pkg.lookup());
    let go_type = crate::ir_to_go::stream_type_to_go(&stream_union, pkg.lookup());
    if let TypeGo::Union { name, .. } = go_type {
        let TypeStreaming::Union(union_type_generic, _) = stream_union else {
            panic!("ir_union_to_go expects a union. Got: {}", stream_union);
        };
        let variants = union_type_generic.iter_skip_null().iter().map(|t| {
            let go_type = crate::ir_to_go::stream_type_to_go(t, pkg.lookup());
            crate::generated_types::VariantGo {
                name: go_type.default_name_within_union(),
                cffi_name: t.to_union_name(),
                type_: go_type,
            }
            })
            .collect::<Vec<_>>();
        Some(crate::generated_types::UnionGo {
            name,
            // TODO: switch to stream_union.to_union_name()
            cffi_name: union.to_union_name(),
            docstring: Some(format!("Generated from: {}", union)),
            variants,
            pkg,
        })
    } else {
        None
    }
}
