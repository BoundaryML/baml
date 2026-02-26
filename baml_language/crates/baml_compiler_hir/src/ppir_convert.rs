//! Conversion from PPIR output types to HIR types.
//!
//! These functions convert `ppir::PpirTypeRef`, `ppir::Class`, and
//! `ppir::TypeAlias` into their HIR equivalents. Called from
//! `file_item_tree` when merging PPIR-generated `stream_*` items.

use crate::{Attribute, Class, Field, Path, TypeAlias, TypeRef};

/// Convert a `ppir::PpirTypeRef` to an `hir::TypeRef`.
pub(crate) fn convert_ppir_type_ref(ppir_ty: &baml_compiler_ppir::PpirTypeRef) -> TypeRef {
    use baml_compiler_ppir::PpirTypeRef as PT;
    match ppir_ty {
        PT::Named(name) => TypeRef::Path(Path::single(name.clone())),
        PT::Int => TypeRef::Int,
        PT::Float => TypeRef::Float,
        PT::String => TypeRef::String,
        PT::Bool => TypeRef::Bool,
        PT::Null => TypeRef::Null,
        PT::Never => TypeRef::named(baml_base::Name::new("never")),
        PT::Optional(inner) => TypeRef::Optional(Box::new(convert_ppir_type_ref(inner))),
        PT::List(inner) => TypeRef::List(Box::new(convert_ppir_type_ref(inner))),
        PT::Map { key, value } => TypeRef::Map {
            key: Box::new(convert_ppir_type_ref(key)),
            value: Box::new(convert_ppir_type_ref(value)),
        },
        PT::Union(variants) => TypeRef::Union(variants.iter().map(convert_ppir_type_ref).collect()),
        PT::StringLiteral(s) => TypeRef::StringLiteral(s.clone()),
        PT::IntLiteral(i) => TypeRef::IntLiteral(*i),
        PT::BoolLiteral(b) => TypeRef::BoolLiteral(*b),
        PT::Media(kind) => TypeRef::Media(*kind),
        PT::Unknown => TypeRef::Error,
    }
}

/// Convert a `ppir::Class` (generated stream_* class) to an `hir::Class`.
pub(crate) fn convert_ppir_class(ppir_class: &baml_compiler_ppir::Class) -> Class {
    let fields = ppir_class
        .fields
        .iter()
        .map(|pf| Field {
            name: pf.name.clone(),
            type_ref: convert_ppir_type_ref(&pf.type_ref),
            alias: match &pf.alias {
                Some(a) => Attribute::Explicit(a.clone()),
                None => Attribute::Unset,
            },
            description: match &pf.description {
                Some(d) => Attribute::Explicit(d.clone()),
                None => Attribute::Unset,
            },
            skip: if pf.skip {
                Attribute::Explicit(())
            } else {
                Attribute::Unset
            },
            starts_as: pf.starts_as.clone(),
        })
        .collect();

    Class {
        name: ppir_class.name.clone(),
        fields,
        is_dynamic: if ppir_class.is_dynamic {
            Attribute::Explicit(())
        } else {
            Attribute::Unset
        },
        alias: Attribute::Unset,
        description: Attribute::Unset,
    }
}

/// Convert a `ppir::TypeAlias` (generated stream_* alias) to an `hir::TypeAlias`.
pub(crate) fn convert_ppir_type_alias(ppir_alias: &baml_compiler_ppir::TypeAlias) -> TypeAlias {
    TypeAlias {
        name: ppir_alias.name.clone(),
        type_ref: convert_ppir_type_ref(&ppir_alias.type_ref),
    }
}
