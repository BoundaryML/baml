//! Conversion from the compiler2 HIR/TIR to `ObjectPool`.
//!
//! Walks the HIR item trees for user-defined files, resolves types via TIR,
//! and populates a codegen-ready `ObjectPool` suitable for language-specific
//! code generators (e.g. `baml_codegen_python`).

use baml_codegen_types::{self as cg, Namespace, ObjectPool};
use baml_compiler2_ast::DeclarativeMeta;
use baml_compiler2_hir::{file_package, package::PackageId};
use baml_compiler2_tir::{
    lower_type_expr,
    ty::{PrimitiveType, Ty as TirTy},
};
use baml_db::Name;

use crate::ProjectDatabase;

/// Build a codegen `ObjectPool` from the compiler database.
///
/// Walks all user-package source files, extracts classes/enums/type aliases/
/// declarative functions, resolves their types, and converts to codegen types.
pub fn build_object_pool(db: &ProjectDatabase) -> ObjectPool {
    let mut pool = ObjectPool::new();

    let user_pkg_id = PackageId::new(db, Name::new("user"));
    let user_pkg_items = baml_compiler2_ppir::package_items(db, user_pkg_id);

    for source_file in db.get_source_files() {
        let pkg_info = file_package::file_package(db, source_file);
        if pkg_info.package.as_str() != "user" {
            continue;
        }

        let item_tree = baml_compiler2_ppir::file_item_tree(db, source_file);

        // Classes
        for class in item_tree.classes.values() {
            if !class.generic_params.is_empty() {
                continue;
            }
            let cg_name = cg::Name {
                name: class.name.clone(),
                namespace: namespace_for(class.name.as_str()),
            };
            let properties = class
                .fields
                .iter()
                .filter_map(|field| {
                    let ty = resolve_type_expr(
                        db,
                        field.type_expr.as_ref(),
                        user_pkg_items,
                        &pkg_info.namespace_path,
                        &[],
                    )?;
                    Some(cg::ClassProperty {
                        name: field.name.clone(),
                        docstring: None,
                        ty,
                    })
                })
                .collect();
            pool.insert(
                cg_name.clone(),
                cg::Object::Class(cg::Class {
                    name: cg_name,
                    docstring: None,
                    properties,
                }),
            );
        }

        // Enums
        for enum_def in item_tree.enums.values() {
            let cg_name = cg::Name {
                name: enum_def.name.clone(),
                namespace: namespace_for(enum_def.name.as_str()),
            };
            let variants = enum_def
                .variants
                .iter()
                .map(|v| cg::EnumVariant {
                    name: v.name.clone(),
                    docstring: None,
                    value: v.name.to_string(),
                })
                .collect();
            pool.insert(
                cg_name.clone(),
                cg::Object::Enum(cg::Enum {
                    name: cg_name,
                    docstring: None,
                    variants,
                }),
            );
        }

        // Type aliases
        for alias in item_tree.type_aliases.values() {
            if let Some(resolved) = resolve_type_expr(
                db,
                alias.type_expr.as_ref(),
                user_pkg_items,
                &pkg_info.namespace_path,
                &[],
            ) {
                let cg_name = cg::Name {
                    name: alias.name.clone(),
                    namespace: namespace_for(alias.name.as_str()),
                };
                pool.insert(
                    cg_name.clone(),
                    cg::Object::TypeAlias(cg::TypeAlias {
                        name: cg_name,
                        resolves_to: resolved,
                    }),
                );
            }
        }

        // Functions (only declarative LLM functions)
        for func in item_tree.functions.values() {
            if !matches!(&func.declarative_meta, Some(DeclarativeMeta::Llm(_))) {
                continue;
            }
            if !func.generic_params.is_empty() {
                continue;
            }

            let arguments: Vec<cg::FunctionArgument> = func
                .params
                .iter()
                .filter_map(|param| {
                    let ty = resolve_type_expr(
                        db,
                        param.type_expr.as_ref(),
                        user_pkg_items,
                        &pkg_info.namespace_path,
                        &[],
                    )?;
                    Some(cg::FunctionArgument {
                        name: param.name.clone(),
                        docstring: None,
                        ty,
                    })
                })
                .collect();

            let return_type = resolve_type_expr(
                db,
                func.return_type.as_ref(),
                user_pkg_items,
                &pkg_info.namespace_path,
                &[],
            )
            .unwrap_or(cg::Ty::Unit);

            let cg_name = cg::Name {
                name: func.name.clone(),
                namespace: Namespace::Types,
            };
            pool.insert(
                cg_name,
                cg::Object::Function(cg::Function {
                    name: func.name.clone(),
                    docstring: None,
                    arguments,
                    return_type,
                    stream_return_type: None, // TODO: streaming support
                    watchers: Vec::new(),
                }),
            );
        }
    }

    pool
}

/// Determine the namespace for an item based on its name.
/// Items with a `$stream` suffix belong to `StreamTypes`, others to `Types`.
fn namespace_for(name: &str) -> Namespace {
    if name.ends_with("$stream") {
        Namespace::StreamTypes
    } else {
        Namespace::Types
    }
}

/// Resolve an optional `SpannedTypeExpr` to a codegen `Ty`.
///
/// Returns `None` if the type expression is missing.
fn resolve_type_expr(
    db: &ProjectDatabase,
    spanned: Option<&baml_compiler2_ast::SpannedTypeExpr>,
    package_items: &baml_compiler2_hir::package::PackageItems<'_>,
    ns_context: &[Name],
    generic_params: &[Name],
) -> Option<cg::Ty> {
    let spanned = spanned?;
    let mut diagnostics = Vec::new();
    let tir_ty = lower_type_expr::lower_type_expr_in_ns(
        db,
        &spanned.expr,
        package_items,
        ns_context,
        generic_params,
        &mut diagnostics,
    );
    Some(convert_tir_to_codegen_ty(&tir_ty))
}

/// Convert a TIR `Ty` to a `baml_codegen_types::Ty`, simplifying as we go.
///
/// Simplification (analogous to `simplify_sap` but for codegen, without attrs):
/// - Optional → union with null
/// - Flatten nested unions
/// - Deduplicate variants (structural equality)
/// - Push null to end
/// - Unwrap singleton unions
fn convert_tir_to_codegen_ty(ty: &TirTy) -> cg::Ty {
    let converted = convert_tir_leaf(ty);
    // Simplify unions that were built during conversion.
    simplify_codegen_ty(converted)
}

/// Convert a single TIR type node without simplifying unions yet.
fn convert_tir_leaf(ty: &TirTy) -> cg::Ty {
    match ty {
        // Primitives
        TirTy::Primitive(PrimitiveType::Int, _) => cg::Ty::Int,
        TirTy::Primitive(PrimitiveType::Float, _) => cg::Ty::Float,
        TirTy::Primitive(PrimitiveType::String, _) => cg::Ty::String,
        TirTy::Primitive(PrimitiveType::Bool, _) => cg::Ty::Bool,
        TirTy::Primitive(PrimitiveType::Null, _) => cg::Ty::Null,
        TirTy::Primitive(PrimitiveType::Image, _) => cg::Ty::Media(baml_db::MediaKind::Image),
        TirTy::Primitive(PrimitiveType::Audio, _) => cg::Ty::Media(baml_db::MediaKind::Audio),
        TirTy::Primitive(PrimitiveType::Video, _) => cg::Ty::Media(baml_db::MediaKind::Video),
        TirTy::Primitive(PrimitiveType::Pdf, _) => cg::Ty::Media(baml_db::MediaKind::Pdf),

        // Named types
        TirTy::Class(qtn, _) => cg::Ty::Class(cg::Name {
            name: qtn.name().clone(),
            namespace: namespace_for(qtn.name().as_str()),
        }),
        TirTy::Enum(qtn, _) => cg::Ty::Enum(cg::Name {
            name: qtn.name().clone(),
            namespace: namespace_for(qtn.name().as_str()),
        }),
        TirTy::EnumVariant(qtn, _variant, _) => cg::Ty::Enum(cg::Name {
            name: qtn.name().clone(),
            namespace: namespace_for(qtn.name().as_str()),
        }),
        // Type aliases: keep as the alias name (the ObjectPool has the alias entry)
        TirTy::TypeAlias(qtn, _) => cg::Ty::Class(cg::Name {
            name: qtn.name().clone(),
            namespace: namespace_for(qtn.name().as_str()),
        }),

        // Containers — recurse via convert_tir_to_codegen_ty so children are simplified.
        TirTy::List(inner, _) | TirTy::EvolvingList(inner, _) => {
            cg::Ty::List(Box::new(convert_tir_to_codegen_ty(inner)))
        }
        TirTy::Map(k, v, _) | TirTy::EvolvingMap(k, v, _) => cg::Ty::Map {
            key: Box::new(convert_tir_to_codegen_ty(k)),
            value: Box::new(convert_tir_to_codegen_ty(v)),
        },
        // Unions and optionals: convert children, then let simplify_codegen_ty handle them.
        TirTy::Union(members, _) => {
            cg::Ty::Union(members.iter().map(convert_tir_to_codegen_ty).collect())
        }
        TirTy::Optional(inner, _) => {
            // Desugar Optional<T> into Union(T, Null) so simplification can
            // flatten/dedup with any nulls already present.
            cg::Ty::Union(vec![convert_tir_to_codegen_ty(inner), cg::Ty::Null])
        }
        TirTy::Literal(lit, _freshness, _) => cg::Ty::Literal(lit.clone()),

        // Bottom / sentinel / error recovery
        TirTy::Void { .. }
        | TirTy::Never { .. }
        | TirTy::Unknown { .. }
        | TirTy::Error { .. }
        | TirTy::TypeVar(..)
        | TirTy::BuiltinUnknown { .. }
        | TirTy::RustType { .. }
        | TirTy::Type { .. } => cg::Ty::Unit,

        // Function types don't map to codegen types
        TirTy::Function { .. } => cg::Ty::Unit,
    }
}

// ---------------------------------------------------------------------------
// Codegen type simplification
// ---------------------------------------------------------------------------

/// Simplify a codegen type: flatten unions, dedup, null-to-end, unwrap singletons.
fn simplify_codegen_ty(ty: cg::Ty) -> cg::Ty {
    match ty {
        cg::Ty::Union(variants) => simplify_union(variants),
        cg::Ty::Optional(inner) => {
            // Shouldn't normally appear (we desugar above), but handle defensively.
            simplify_union(vec![*inner, cg::Ty::Null])
        }
        // Recurse into containers.
        cg::Ty::List(inner) => cg::Ty::List(Box::new(simplify_codegen_ty(*inner))),
        cg::Ty::Map { key, value } => cg::Ty::Map {
            key: Box::new(simplify_codegen_ty(*key)),
            value: Box::new(simplify_codegen_ty(*value)),
        },
        // Leaf types pass through unchanged.
        other => other,
    }
}

fn simplify_union(variants: Vec<cg::Ty>) -> cg::Ty {
    // 1. Flatten nested unions.
    let variants = flatten_union(variants);

    // 2. Deduplicate (structural equality).
    let variants = dedup_variants(variants);

    // 3. Push null to end.
    let variants = null_to_end(variants);

    // 4. Unwrap singleton.
    if variants.len() == 1 {
        variants.into_iter().next().unwrap()
    } else {
        cg::Ty::Union(variants)
    }
}

/// Flatten nested unions into a single level.
fn flatten_union(variants: Vec<cg::Ty>) -> Vec<cg::Ty> {
    let mut out = Vec::new();
    for v in variants {
        match v {
            cg::Ty::Union(inner) => out.extend(flatten_union(inner)),
            other => out.push(other),
        }
    }
    out
}

/// Remove structurally duplicate variants.
fn dedup_variants(variants: Vec<cg::Ty>) -> Vec<cg::Ty> {
    let mut result: Vec<cg::Ty> = Vec::new();
    for candidate in variants {
        if !result.contains(&candidate) {
            result.push(candidate);
        }
    }
    result
}

/// Push `Null` variants to the end.
fn null_to_end(variants: Vec<cg::Ty>) -> Vec<cg::Ty> {
    let mut non_null = Vec::new();
    let mut nulls = Vec::new();
    for v in variants {
        if matches!(v, cg::Ty::Null) {
            nulls.push(v);
        } else {
            non_null.push(v);
        }
    }
    non_null.extend(nulls);
    non_null
}
