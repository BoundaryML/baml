//! Conversion from the compiler2 HIR/TIR to `SymbolPool`.
//!
//! Walks the HIR item trees for user-defined files, resolves types via TIR,
//! and populates a codegen-ready `SymbolPool` suitable for language-specific
//! code generators (e.g. `baml_codegen_python`).

use std::collections::HashMap;

use baml_codegen_types::{self as cg, Origin, SymbolPool};
use baml_compiler2_ast::DeclarativeMeta;
use baml_compiler2_hir::{
    compiler2_all_files, file_package, ids::FunctionMarker, ids::LocalItemId, package::PackageId,
};
use baml_compiler2_tir::{
    lower_type_expr,
    normalize::find_recursive_aliases,
    ty::{PrimitiveType, QualifiedTypeName, Ty as TirTy},
};
use baml_db::Name;

use crate::ProjectDatabase;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Build a `cg::Name` from a `QualifiedTypeName`. Preserves `pkg`, the full
/// namespace path, and the bare name (including any `$stream` suffix).
fn name_from_qtn(qtn: &QualifiedTypeName) -> cg::Name {
    cg::Name {
        pkg: qtn.package().clone(),
        namespace_path: qtn.namespace().clone(),
        name: qtn.name().clone(),
    }
}

/// If `name` contains a `$`, return `(parent_part, suffix_after_dollar)`.
/// For example `"ExtractResume$build_request"` → `Some(("ExtractResume", "build_request"))`.
/// If there's no `$`, returns `None`.
fn split_companion(name: &str) -> Option<(&str, &str)> {
    let pos = name.find('$')?;
    Some((&name[..pos], &name[pos + 1..]))
}

// ---------------------------------------------------------------------------
// Build SymbolPool
// ---------------------------------------------------------------------------

/// Build a codegen `SymbolPool` from the compiler database.
///
/// Walks every file visible to compiler2 (user files + stdlib stubs),
/// extracts classes/enums/type aliases/functions/methods, resolves their
/// types, and converts to codegen types.
pub fn build_symbol_pool(db: &ProjectDatabase) -> SymbolPool {
    struct PendingFunction {
        pkg: Name,
        ns_path: Vec<Name>,
        bare_name: Name,
        companion_suffix: Option<String>,
        func: cg::Function,
    }

    struct ParentEntry {
        pkg: Name,
        ns_path: Vec<Name>,
        bare_name: Name,
        func: cg::Function,
    }

    enum MethodKind {
        Static,
        Instance,
    }

    struct PendingMethod {
        /// Pool key of the owning class.
        parent_key: cg::Name,
        bare_name: Name,
        companion_suffix: Option<String>,
        kind: MethodKind,
        func: cg::Function,
    }

    let mut pool = SymbolPool::new();

    // Per-package alias and recursive-alias maps. A type alias only resolves
    // within its own package, so we cache one map per package the first time
    // we see a file from that package.
    let mut alias_caches: HashMap<
        Name,
        (
            HashMap<QualifiedTypeName, TirTy>,
            std::collections::HashSet<QualifiedTypeName>,
        ),
    > = HashMap::new();

    let mut pending_functions: Vec<PendingFunction> = Vec::new();
    let mut pending_methods: Vec<PendingMethod> = Vec::new();

    for source_file in compiler2_all_files(db) {
        let pkg_info = file_package::file_package(db, source_file);
        let pkg: Name = pkg_info.package.clone();
        let ns_path: Vec<Name> = pkg_info.namespace_path.clone();

        let pkg_id = PackageId::new(db, pkg.clone());
        let pkg_items = baml_compiler2_ppir::package_items(db, pkg_id);

        let (alias_map, recursive_aliases) = alias_caches.entry(pkg.clone()).or_insert_with(|| {
            let aliases = baml_compiler2_tir::inference::collect_type_aliases(db, pkg_items);
            let recursive = find_recursive_aliases(&aliases);
            (aliases, recursive)
        });
        let alias_map: &HashMap<QualifiedTypeName, TirTy> = alias_map;
        let recursive_aliases: &std::collections::HashSet<QualifiedTypeName> = recursive_aliases;

        let item_tree = baml_compiler2_ppir::file_item_tree(db, source_file);
        let source_file_path: String = source_file.path(db).to_string_lossy().into_owned();

        // Collect the set of function IDs that are class methods for this
        // file so the free-function walk below can skip them. Methods live in
        // `item_tree.functions` alongside top-level functions; without this
        // filter, a user-declared LLM method would incorrectly land in the
        // free-function pool.
        let mut method_ids: std::collections::HashSet<LocalItemId<FunctionMarker>> =
            std::collections::HashSet::new();
        for class in item_tree.classes.values() {
            for m in &class.methods {
                method_ids.insert(*m);
            }
        }

        // Classes
        for class in item_tree.classes.values() {
            if !class.generic_params.is_empty() {
                continue;
            }
            let cg_name = cg::Name {
                pkg: pkg.clone(),
                namespace_path: ns_path.clone(),
                name: class.name.clone(),
            };
            let properties = class
                .fields
                .iter()
                .filter_map(|field| {
                    let ty = resolve_type_expr(
                        db,
                        field.type_expr.as_ref(),
                        pkg_items,
                        &pkg_info.namespace_path,
                        &[],
                        alias_map,
                        recursive_aliases,
                    )?;
                    Some(cg::ClassProperty {
                        name: field.name.clone(),
                        docstring: None,
                        ty,
                    })
                })
                .collect();

            // Methods — lower each into a `cg::Function` and queue up for
            // the post-walk attach pass. Static vs. instance is dispatched
            // structurally on whether the first parameter is named `self`.
            // Companions of methods (e.g. `$build_request`) flow through the
            // same pending list and reattach via the second-pass logic.
            for method_id in &class.methods {
                let Some(method) = item_tree.functions.get(method_id) else {
                    continue;
                };
                if !method.generic_params.is_empty() {
                    continue;
                }

                let method_name_str = method.name.as_str();
                let (bare_name, companion_suffix) = match split_companion(method_name_str) {
                    Some((parent, suffix)) => (Name::new(parent), Some(suffix.to_string())),
                    None => (method.name.clone(), None),
                };

                let is_instance = method
                    .params
                    .first()
                    .is_some_and(|p| p.name.as_str() == "self");
                let kind = if is_instance {
                    MethodKind::Instance
                } else {
                    MethodKind::Static
                };

                let arguments: Vec<cg::FunctionArgument> = method
                    .params
                    .iter()
                    .skip(usize::from(is_instance))
                    .filter_map(|param| {
                        let ty = resolve_type_expr(
                            db,
                            param.type_expr.as_ref(),
                            pkg_items,
                            &pkg_info.namespace_path,
                            &[],
                            alias_map,
                            recursive_aliases,
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
                    method.return_type.as_ref(),
                    pkg_items,
                    &pkg_info.namespace_path,
                    &[],
                    alias_map,
                    recursive_aliases,
                )
                .unwrap_or(cg::Ty::Unit);

                let cg_method = cg::Function {
                    name: method.name.clone(),
                    docstring: None,
                    arguments,
                    return_type,
                    stream_return_type: None,
                    watchers: Vec::new(),
                    companions: Vec::new(),
                    origin: Origin {
                        source_file_path: source_file_path.clone(),
                        span_start: u32::from(method.span.start()),
                    },
                };

                pending_methods.push(PendingMethod {
                    parent_key: cg_name.clone(),
                    bare_name,
                    companion_suffix,
                    kind,
                    func: cg_method,
                });
            }

            pool.insert(
                cg_name.clone(),
                cg::Symbol::Class(cg::Class {
                    name: cg_name,
                    docstring: None,
                    properties,
                    static_methods: Vec::new(),
                    instance_methods: Vec::new(),
                    origin: Origin {
                        source_file_path: source_file_path.clone(),
                        span_start: u32::from(class.span.start()),
                    },
                }),
            );
        }

        // Enums
        for enum_def in item_tree.enums.values() {
            let cg_name = cg::Name {
                pkg: pkg.clone(),
                namespace_path: ns_path.clone(),
                name: enum_def.name.clone(),
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
                cg::Symbol::Enum(cg::Enum {
                    name: cg_name,
                    docstring: None,
                    variants,
                    origin: Origin {
                        source_file_path: source_file_path.clone(),
                        span_start: u32::from(enum_def.span.start()),
                    },
                }),
            );
        }

        // Type aliases
        for alias in item_tree.type_aliases.values() {
            if let Some(resolved) = resolve_type_expr(
                db,
                alias.type_expr.as_ref(),
                pkg_items,
                &pkg_info.namespace_path,
                &[],
                alias_map,
                recursive_aliases,
            ) {
                let cg_name = cg::Name {
                    pkg: pkg.clone(),
                    namespace_path: ns_path.clone(),
                    name: alias.name.clone(),
                };

                let qtn = QualifiedTypeName::new(
                    pkg_info.package.clone(),
                    pkg_info.namespace_path.clone(),
                    alias.name.clone(),
                );
                let is_recursive = recursive_aliases.contains(&qtn);

                pool.insert(
                    cg_name.clone(),
                    cg::Symbol::TypeAlias(cg::TypeAlias {
                        name: cg_name,
                        resolves_to: resolved,
                        recursive: is_recursive,
                        origin: Origin {
                            source_file_path: source_file_path.clone(),
                            span_start: u32::from(alias.span.start()),
                        },
                    }),
                );
            }
        }

        // Top-level functions — methods are skipped via `method_ids` so they
        // don't double-emit.
        for (id, func) in &item_tree.functions {
            if method_ids.contains(id) {
                continue;
            }
            if !func.generic_params.is_empty() {
                continue;
            }

            let func_name_str = func.name.as_str();

            let (bare_name, companion_suffix) = match split_companion(func_name_str) {
                Some((parent, suffix)) => (Name::new(parent), Some(suffix.to_string())),
                None => (func.name.clone(), None),
            };

            // For parent functions, require declarative LLM meta.
            // Companion functions inherit validity from their parent.
            if companion_suffix.is_none()
                && !matches!(&func.declarative_meta, Some(DeclarativeMeta::Llm(_)))
            {
                continue;
            }

            let arguments: Vec<cg::FunctionArgument> = func
                .params
                .iter()
                .filter_map(|param| {
                    let ty = resolve_type_expr(
                        db,
                        param.type_expr.as_ref(),
                        pkg_items,
                        &pkg_info.namespace_path,
                        &[],
                        alias_map,
                        recursive_aliases,
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
                pkg_items,
                &pkg_info.namespace_path,
                &[],
                alias_map,
                recursive_aliases,
            )
            .unwrap_or(cg::Ty::Unit);

            let cg_func = cg::Function {
                name: func.name.clone(),
                docstring: None,
                arguments,
                return_type,
                stream_return_type: None, // TODO: streaming support
                watchers: Vec::new(),
                companions: Vec::new(),
                origin: Origin {
                    source_file_path: source_file_path.clone(),
                    span_start: u32::from(func.span.start()),
                },
            };

            pending_functions.push(PendingFunction {
                pkg: pkg.clone(),
                ns_path: ns_path.clone(),
                bare_name,
                companion_suffix,
                func: cg_func,
            });
        }
    }

    // Second pass: build a map of parent functions, then attach companions.
    let mut parents: Vec<ParentEntry> = Vec::new();
    let mut companions: Vec<PendingFunction> = Vec::new();

    for pf in pending_functions {
        if pf.companion_suffix.is_none() {
            parents.push(ParentEntry {
                pkg: pf.pkg,
                ns_path: pf.ns_path,
                bare_name: pf.bare_name,
                func: pf.func,
            });
        } else {
            companions.push(pf);
        }
    }

    // Attach companions to parents.
    for companion in companions {
        let suffix = companion.companion_suffix.as_deref().unwrap();
        if let Some(parent_entry) = parents.iter_mut().find(|p| {
            p.pkg == companion.pkg
                && p.ns_path == companion.ns_path
                && p.bare_name == companion.bare_name
        }) {
            parent_entry
                .func
                .companions
                .push((suffix.to_string(), companion.func));
        }
        // If no parent found, the companion is silently dropped (shouldn't happen in valid code).
    }

    // Insert parent functions (with companions attached) into the pool.
    for parent in parents {
        let cg_name = cg::Name {
            pkg: parent.pkg,
            namespace_path: parent.ns_path,
            name: parent.bare_name,
        };
        pool.insert(cg_name, cg::Symbol::Function(parent.func));
    }

    // Method attach pass — analogous to the function companion pass above.
    // Group parents and companions, splice companions into their parent
    // method's `companions` vec, then push the assembled methods into the
    // owning class's `static_methods` / `instance_methods` vec.
    let mut method_parents: Vec<PendingMethod> = Vec::new();
    let mut method_companions: Vec<PendingMethod> = Vec::new();
    for pm in pending_methods {
        if pm.companion_suffix.is_none() {
            method_parents.push(pm);
        } else {
            method_companions.push(pm);
        }
    }
    for companion in method_companions {
        let suffix = companion.companion_suffix.as_deref().unwrap();
        if let Some(parent) = method_parents
            .iter_mut()
            .find(|p| p.parent_key == companion.parent_key && p.bare_name == companion.bare_name)
        {
            parent
                .func
                .companions
                .push((suffix.to_string(), companion.func));
        }
    }
    for pm in method_parents {
        if let Some(cg::Symbol::Class(class)) = pool.get_mut(&pm.parent_key) {
            match pm.kind {
                MethodKind::Static => class.static_methods.push(pm.func),
                MethodKind::Instance => class.instance_methods.push(pm.func),
            }
        }
    }

    pool
}

// ---------------------------------------------------------------------------
// Type resolution
// ---------------------------------------------------------------------------

/// Resolve an optional `SpannedTypeExpr` to a codegen `Ty`.
///
/// Returns `None` if the type expression is missing.
fn resolve_type_expr(
    db: &ProjectDatabase,
    spanned: Option<&baml_compiler2_ast::SpannedTypeExpr>,
    package_items: &baml_compiler2_hir::package::PackageItems<'_>,
    ns_context: &[Name],
    generic_params: &[Name],
    alias_map: &HashMap<QualifiedTypeName, TirTy>,
    recursive_aliases: &std::collections::HashSet<QualifiedTypeName>,
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
    Some(convert_tir_to_codegen_ty(
        &tir_ty,
        alias_map,
        recursive_aliases,
    ))
}

/// Convert a TIR `Ty` to a `baml_codegen_types::Ty`, simplifying as we go.
///
/// Simplification (analogous to `simplify_sap` but for codegen, without attrs):
/// - Optional → union with null
/// - Flatten nested unions
/// - Deduplicate variants (structural equality)
/// - Push null to end
/// - Unwrap singleton unions
fn convert_tir_to_codegen_ty(
    ty: &TirTy,
    alias_map: &HashMap<QualifiedTypeName, TirTy>,
    recursive_aliases: &std::collections::HashSet<QualifiedTypeName>,
) -> cg::Ty {
    let converted = convert_tir_leaf(ty, alias_map, recursive_aliases);
    // Simplify unions that were built during conversion.
    simplify_codegen_ty(converted)
}

/// Convert a single TIR type node without simplifying unions yet.
fn convert_tir_leaf(
    ty: &TirTy,
    alias_map: &HashMap<QualifiedTypeName, TirTy>,
    recursive_aliases: &std::collections::HashSet<QualifiedTypeName>,
) -> cg::Ty {
    match ty {
        // Primitives
        TirTy::Primitive(PrimitiveType::Int, _) => cg::Ty::Int,
        TirTy::Primitive(PrimitiveType::Float, _) => cg::Ty::Float,
        TirTy::Primitive(PrimitiveType::String, _) => cg::Ty::String,
        TirTy::Primitive(PrimitiveType::Bool, _) => cg::Ty::Bool,
        TirTy::Primitive(PrimitiveType::Null, _) => cg::Ty::Null,
        TirTy::Primitive(PrimitiveType::Uint8Array, _) => cg::Ty::Uint8Array,
        TirTy::Primitive(PrimitiveType::Image, _) => cg::Ty::Media(baml_db::MediaKind::Image),
        TirTy::Primitive(PrimitiveType::Audio, _) => cg::Ty::Media(baml_db::MediaKind::Audio),
        TirTy::Primitive(PrimitiveType::Video, _) => cg::Ty::Media(baml_db::MediaKind::Video),
        TirTy::Primitive(PrimitiveType::Pdf, _) => cg::Ty::Media(baml_db::MediaKind::Pdf),

        // Named types — preserve full QualifiedTypeName via name_from_qtn.
        TirTy::Class(qtn, _, _) => cg::Ty::Class(name_from_qtn(qtn)),
        TirTy::Enum(qtn, _) => cg::Ty::Enum(name_from_qtn(qtn)),
        TirTy::EnumVariant(qtn, _variant, _) => cg::Ty::Enum(name_from_qtn(qtn)),

        // Type aliases: if recursive, keep as TypeAlias (opaque); otherwise inline.
        TirTy::TypeAlias(qtn, _) => {
            if recursive_aliases.contains(qtn) {
                cg::Ty::TypeAlias(name_from_qtn(qtn))
            } else if let Some(target) = alias_map.get(qtn) {
                // Inline non-recursive aliases.
                convert_tir_to_codegen_ty(target, alias_map, recursive_aliases)
            } else {
                // Unknown alias (e.g. from another package) — keep opaque as TypeAlias.
                cg::Ty::TypeAlias(name_from_qtn(qtn))
            }
        }

        // Containers — recurse via convert_tir_to_codegen_ty so children are simplified.
        TirTy::List(inner, _) | TirTy::EvolvingList(inner, _) => cg::Ty::List(Box::new(
            convert_tir_to_codegen_ty(inner, alias_map, recursive_aliases),
        )),
        TirTy::Map(k, v, _) | TirTy::EvolvingMap(k, v, _) => cg::Ty::Map {
            key: Box::new(convert_tir_to_codegen_ty(k, alias_map, recursive_aliases)),
            value: Box::new(convert_tir_to_codegen_ty(v, alias_map, recursive_aliases)),
        },
        // Unions and optionals: convert children, then let simplify_codegen_ty handle them.
        TirTy::Union(members, _) => cg::Ty::Union(
            members
                .iter()
                .map(|m| convert_tir_to_codegen_ty(m, alias_map, recursive_aliases))
                .collect(),
        ),
        TirTy::Optional(inner, _) => {
            // Desugar Optional<T> into Union(T, Null) so simplification can
            // flatten/dedup with any nulls already present.
            cg::Ty::Union(vec![
                convert_tir_to_codegen_ty(inner, alias_map, recursive_aliases),
                cg::Ty::Null,
            ])
        }
        TirTy::Literal(lit, _freshness, _) => cg::Ty::Literal(lit.clone()),

        // BEP-030: BAML's `unknown` top type → BuiltinUnknown.
        TirTy::BuiltinUnknown { .. } => cg::Ty::BuiltinUnknown,

        // BEP-030: Function types → Callable.
        TirTy::Function { params, ret, .. } => cg::Ty::Callable {
            params: params
                .iter()
                .map(|(_, p)| convert_tir_to_codegen_ty(p, alias_map, recursive_aliases))
                .collect(),
            ret: Box::new(convert_tir_to_codegen_ty(ret, alias_map, recursive_aliases)),
        },

        // Bottom / sentinel / error recovery — map to Unit.
        TirTy::Void { .. }
        | TirTy::Never { .. }
        | TirTy::Unknown { .. }
        | TirTy::Error { .. }
        | TirTy::TypeVar(..)
        | TirTy::RustType { .. }
        | TirTy::Type { .. } => cg::Ty::Unit,
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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::ProjectDatabase;

    // ── Unit tests for pure helpers ─────────────────────────────────────────

    #[test]
    fn test_split_companion_with_dollar() {
        assert_eq!(
            split_companion("ExtractResume$build_request"),
            Some(("ExtractResume", "build_request"))
        );
        assert_eq!(split_companion("Foo$parse"), Some(("Foo", "parse")));
        assert_eq!(
            split_companion("ExtractResume$render_prompt"),
            Some(("ExtractResume", "render_prompt"))
        );
    }

    #[test]
    fn test_split_companion_no_dollar() {
        assert_eq!(split_companion("ExtractResume"), None);
        assert_eq!(split_companion("Foo"), None);
        assert_eq!(split_companion(""), None);
    }

    #[test]
    fn test_split_companion_dollar_stream_suffix() {
        // $stream is also handled as a companion split.
        assert_eq!(split_companion("Resume$stream"), Some(("Resume", "stream")));
    }

    #[test]
    fn test_name_from_qtn_preserves_full_path() {
        let qtn = QualifiedTypeName::new(
            Name::new("user"),
            vec![Name::new("foo")],
            Name::new("Sentiment"),
        );
        let cg_name = name_from_qtn(&qtn);
        assert_eq!(cg_name.pkg.as_str(), "user");
        assert_eq!(
            cg_name.namespace_path,
            vec![Name::new("foo")],
            "namespace_path mismatch: {:?}",
            cg_name.namespace_path,
        );
        assert_eq!(cg_name.name.as_str(), "Sentiment");
        assert!(!cg_name.is_stream());
    }

    #[test]
    fn test_name_from_qtn_stream_suffix() {
        let qtn = QualifiedTypeName::new(Name::new("user"), vec![], Name::new("Resume$stream"));
        let cg_name = name_from_qtn(&qtn);
        assert!(cg_name.is_stream());
        assert_eq!(cg_name.bare_name(), "Resume");
    }

    // ── Integration tests using ProjectDatabase ─────────────────────────────

    /// Verifies that companions are attached to their parent function.
    /// A BAML declarative function causes `$build_request`, `$render_prompt`, and `$parse`
    /// companion functions to be synthesized by the companion expander.
    /// `build_symbol_pool` should collect them as `companions` on the parent function.
    #[test]
    fn test_companions_attached_to_parent() {
        let root = Path::new("/tmp/bep030_companions");
        let mut db = ProjectDatabase::new();
        db.set_project_root(root);
        db.add_or_update_file(
            root.join("main.baml").as_path(),
            "class Resume { name string }\nfunction ExtractResume(resume: string) -> Resume {\n    client \"openai/gpt-4o\"\n    prompt #\"Extract resume from {{resume}}\"#\n}\n",
        );

        let pool = build_symbol_pool(&db);

        // Find ExtractResume in the pool.
        let extract_key = pool.keys().find(|k| k.name.as_str() == "ExtractResume");
        assert!(extract_key.is_some(), "ExtractResume must be in the pool");

        let maybe_func = extract_key.and_then(|k| pool.get(k)).and_then(|obj| {
            if let cg::Symbol::Function(f) = obj {
                Some(f)
            } else {
                None
            }
        });
        assert!(
            maybe_func.is_some(),
            "ExtractResume must be a Function object"
        );

        let func = maybe_func.unwrap();
        let companion_names: Vec<&str> = func.companions.iter().map(|(s, _)| s.as_str()).collect();

        // Should have build_request, render_prompt, parse companions.
        assert!(
            companion_names.contains(&"build_request"),
            "build_request companion expected; got: {companion_names:?}"
        );
        assert!(
            companion_names.contains(&"render_prompt"),
            "render_prompt companion expected; got: {companion_names:?}"
        );
        assert!(
            companion_names.contains(&"parse"),
            "parse companion expected; got: {companion_names:?}"
        );
    }

    /// Verifies that user-package classes with static and instance methods
    /// land on the owning class as `static_methods` / `instance_methods`,
    /// the receiver is dropped from instance-method `arguments`, and free
    /// functions on the same file route through the standard pool entry.
    #[test]
    fn test_user_class_methods_attach_to_class() {
        let root = Path::new("/tmp/12b_user_methods");
        let mut db = ProjectDatabase::new();
        db.set_project_root(root);
        db.add_or_update_file(
            root.join("main.baml").as_path(),
            "class Counter {\n  count int\n  function bump(self, by: int) -> int { self.count + by }\n  function zero() -> int { 0 }\n}\n",
        );

        let pool = build_symbol_pool(&db);

        let key = pool
            .keys()
            .find(|k| k.name.as_str() == "Counter")
            .expect("Counter class missing from pool");
        let Some(cg::Symbol::Class(class)) = pool.get(key) else {
            panic!("Counter must be a Class");
        };

        let static_names: Vec<&str> = class
            .static_methods
            .iter()
            .map(|m| m.name.as_str())
            .collect();
        let instance_names: Vec<&str> = class
            .instance_methods
            .iter()
            .map(|m| m.name.as_str())
            .collect();
        assert_eq!(static_names, vec!["zero"], "static methods mismatch");
        assert_eq!(instance_names, vec!["bump"], "instance methods mismatch");

        // Instance method's `arguments` excludes the `self` receiver — it's
        // a Python convention prepended at render time.
        let bump = &class.instance_methods[0];
        let bump_args: Vec<&str> = bump.arguments.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(bump_args, vec!["by"], "self should not be in arguments");
    }

    /// Verifies that a class in a namespaced folder gets `namespace_path` populated.
    /// A file in the `ns_foo` subdirectory should produce a class with
    /// `namespace_path == ["foo"]` and `pkg == "user"`.
    #[test]
    fn test_namespaced_class_has_path() {
        let root = Path::new("/tmp/bep030_ns_path");
        let mut db = ProjectDatabase::new();
        db.set_project_root(root);
        db.add_or_update_file(
            root.join("ns_foo").join("sentiment.baml").as_path(),
            "class Sentiment { label string }\n",
        );

        let pool = build_symbol_pool(&db);

        let sentiment_key = pool.keys().find(|k| k.name.as_str() == "Sentiment");
        assert!(sentiment_key.is_some(), "Sentiment must be in the pool");

        let key = sentiment_key.unwrap();
        assert_eq!(key.pkg.as_str(), "user", "pkg mismatch: {:?}", key.pkg);
        assert_eq!(
            key.namespace_path,
            vec![Name::new("foo")],
            "namespace_path mismatch: {:?}",
            key.namespace_path,
        );
        assert!(!key.is_stream(), "Sentiment must not be marked as stream");
    }
}
