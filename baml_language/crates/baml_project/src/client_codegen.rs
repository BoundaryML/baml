//! Conversion from the compiler2 HIR/TIR to `SymbolPool`.
//!
//! Walks the HIR item trees for user-defined files, resolves types via TIR,
//! and populates a codegen-ready `SymbolPool` suitable for language-specific
//! code generators (e.g. `sdkgen_python_pydantic2`).

use std::collections::HashMap;

use baml_codegen_types::{self as cg, Origin, SymbolPool};
use baml_compiler2_ast::{self as ast, FunctionOrigin};
use baml_compiler2_hir::{
    compiler2_all_files, file_package,
    ids::{FunctionMarker, LocalItemId},
    loc::FunctionLoc,
    package::PackageId,
};
use baml_compiler2_tir::{
    lower_type_expr,
    normalize::find_recursive_aliases,
    ty::{FunctionParamMode, MediaKind, QualifiedTypeName, Ty as TirTy},
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

fn lower_codegen_default(
    default_ref: Option<&baml_compiler2_hir::item_tree::DefaultExprRef>,
    defaults: &ast::FunctionDefaults,
) -> Option<cg::FunctionArgumentDefault> {
    let default_ref = default_ref?;
    let expr = defaults.expr(default_ref.expr);
    match expr {
        ast::Expr::Null => Some(cg::FunctionArgumentDefault::Null),
        ast::Expr::Literal(lit) => Some(cg::FunctionArgumentDefault::Literal(
            cg::DefaultLiteral::Scalar(lit.clone()),
        )),
        ast::Expr::Array { elements } if elements.is_empty() => Some(
            cg::FunctionArgumentDefault::Literal(cg::DefaultLiteral::EmptyList),
        ),
        ast::Expr::Map { entries } if entries.is_empty() => Some(
            cg::FunctionArgumentDefault::Literal(cg::DefaultLiteral::EmptyMap),
        ),
        // Only empty array/map defaults become structured
        // cg::FunctionArgumentDefault::Literal values
        // (cg::DefaultLiteral::EmptyList / cg::DefaultLiteral::EmptyMap).
        // Non-empty array/map AST literals intentionally fall through here and
        // are emitted via defaults.exprs.display_expr(DEFAULT_REF_EXPR), because
        // downstream backends treat non-empty
        // literal defaults as opaque source expressions.
        _ => Some(cg::FunctionArgumentDefault::Expression {
            source: Some(defaults.exprs.display_expr(default_ref.expr.expr())),
        }),
    }
}

/// If `name` contains a `$`, return `(parent_part, suffix_after_dollar)`.
/// For example `"ExtractResume$build_request"` → `Some(("ExtractResume", "build_request"))`.
/// If there's no `$`, returns `None`.
#[cfg(test)]
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
    enum MethodKind {
        Static,
        Instance,
    }

    struct PendingMethod {
        /// Pool key of the owning class.
        parent_key: cg::Name,
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

        // Collect function IDs that are not package-level functions so the
        // free-function walk below can skip them. Class methods, interface
        // default methods, and out-of-body implements methods all live in
        // `item_tree.functions`, but none of them are directly callable as
        // `<pkg>.<ns>.<method>` from generated SDKs.
        let mut non_free_function_ids: std::collections::HashSet<LocalItemId<FunctionMarker>> =
            std::collections::HashSet::new();
        for class in item_tree.classes.values() {
            for m in &class.methods {
                non_free_function_ids.insert(*m);
            }
        }
        for interface in item_tree.interfaces.values() {
            for m in &interface.default_methods {
                non_free_function_ids.insert(*m);
            }
        }
        for impl_id in &item_tree.free_impls {
            let Some(block) = item_tree.impls.get(impl_id) else {
                continue;
            };
            for m in &block.methods {
                non_free_function_ids.insert(*m);
            }
        }

        // Classes
        for class in item_tree.classes.values() {
            let cg_name = cg::Name {
                pkg: pkg.clone(),
                namespace_path: ns_path.clone(),
                name: class.name.clone(),
            };
            let class_generic_params: Vec<Name> = class.generic_params.clone();
            let properties = class
                .fields
                .iter()
                .filter_map(|field| {
                    let ty = resolve_type_expr(
                        db,
                        field.type_expr.as_ref(),
                        pkg_items,
                        &pkg_info.namespace_path,
                        &class_generic_params,
                        alias_map,
                        recursive_aliases,
                    )?;
                    Some(cg::ClassProperty {
                        name: field.name.clone(),
                        docstring: field.docstring.clone(),
                        ty,
                    })
                })
                .collect();

            // Methods — lower each into a `cg::Function`. Static vs.
            // instance is dispatched structurally on whether the first
            // parameter is named `self`. Companion methods (e.g.
            // `$build_request`) are independent `Function` entries that
            // sit alongside their parent method in the same vec; their
            // shared span keeps them adjacent after sorting.
            for method_id in &class.methods {
                let Some(method) = item_tree.functions.get(method_id) else {
                    continue;
                };

                // Auto-derived methods (`to_json` / `from_json` synthesized
                // by `auto_derive_json`) are language-level plumbing, not
                // user-facing API. Skip them so client SDKs don't surface
                // them as static / instance methods on every class.
                if matches!(method.origin, FunctionOrigin::AutoDerive) {
                    continue;
                }

                if matches!(
                    method.body.as_ref(),
                    Some(ast::FunctionBodyDef::Builtin(ast::BuiltinKind::Intrinsic))
                ) {
                    continue;
                }

                let is_instance = method
                    .params
                    .first()
                    .is_some_and(|p| p.name.as_str() == "self");
                let kind = if is_instance {
                    MethodKind::Instance
                } else {
                    MethodKind::Static
                };

                // Combined generics in scope inside the method body: the
                // class's TypeVars plus the method's own. Order matches
                // declaration: class-level first, method-level second.
                let mut method_scope_generics: Vec<Name> = class_generic_params.clone();
                method_scope_generics.extend(method.generic_params.iter().cloned());
                let method_loc = FunctionLoc::new(db, source_file, *method_id);
                let method_defaults =
                    baml_compiler2_ppir::function_parameter_defaults(db, method_loc);

                let arguments: Vec<cg::FunctionArgument> = method
                    .params
                    .iter()
                    .enumerate()
                    .skip(usize::from(is_instance))
                    .filter_map(|(index, param)| {
                        let ty = resolve_type_expr(
                            db,
                            param.type_expr.as_ref(),
                            pkg_items,
                            &pkg_info.namespace_path,
                            &method_scope_generics,
                            alias_map,
                            recursive_aliases,
                        )?;
                        Some(cg::FunctionArgument {
                            name: param.name.clone(),
                            docstring: None,
                            ty,
                            default: lower_codegen_default(
                                method_defaults.param_default(index),
                                &method_defaults.defaults,
                            ),
                        })
                    })
                    .collect();

                let return_type = resolve_type_expr(
                    db,
                    method.return_type.as_ref(),
                    pkg_items,
                    &pkg_info.namespace_path,
                    &method_scope_generics,
                    alias_map,
                    recursive_aliases,
                )
                .unwrap_or(cg::Ty::Unit);

                let cg_method = cg::Function {
                    name: method.name.clone(),
                    generic_params: method.generic_params.clone(),
                    docstring: method.docstring.clone(),
                    arguments,
                    return_type,
                    throws: resolve_throws(db, method_loc, alias_map, recursive_aliases),
                    watchers: Vec::new(),
                    origin: Origin {
                        source_file_path: source_file_path.clone(),
                        span_start: u32::from(method.span.start()),
                    },
                };

                pending_methods.push(PendingMethod {
                    parent_key: cg_name.clone(),
                    kind,
                    func: cg_method,
                });
            }

            pool.insert(
                cg_name.clone(),
                cg::Symbol::Class(cg::Class {
                    name: cg_name,
                    generic_params: class_generic_params,
                    docstring: class.docstring.clone(),
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
                    docstring: v.docstring.clone(),
                    value: v.name.to_string(),
                })
                .collect();
            pool.insert(
                cg_name.clone(),
                cg::Symbol::Enum(cg::Enum {
                    name: cg_name,
                    docstring: enum_def.docstring.clone(),
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
        // don't double-emit. Companion functions (names containing `$`) flow
        // through as their own pool entries; parent and companion alike are
        // inserted directly, keyed on the suffixed name.
        for (id, func) in &item_tree.functions {
            if non_free_function_ids.contains(id) {
                continue;
            }

            // Internal-origin functions (e.g. `<Client>$new` synthesized for
            // primitive clients) are runtime plumbing, not user-callable —
            // skip them so they don't end up as Python factory bindings.
            if matches!(func.origin, FunctionOrigin::Internal) {
                continue;
            }

            if matches!(
                func.body.as_ref(),
                Some(ast::FunctionBodyDef::Builtin(ast::BuiltinKind::Intrinsic))
            ) {
                continue;
            }

            // Companion functions arrive as their own pool entries (names
            // containing `$`); they share the parent's span so
            // `group_and_sort` keeps them contiguous. No further
            // parent-vs-companion gating needed: companion validity is
            // encoded by the suffix; non-LLM parents (pure-expression
            // bodies, etc.) are valid too. `FunctionOrigin::Internal` is
            // already filtered above.

            let func_generic_params: Vec<Name> = func.generic_params.clone();
            let func_loc = FunctionLoc::new(db, source_file, *id);
            let func_defaults = baml_compiler2_ppir::function_parameter_defaults(db, func_loc);
            let arguments: Vec<cg::FunctionArgument> = func
                .params
                .iter()
                .enumerate()
                .filter_map(|(index, param)| {
                    let ty = resolve_type_expr(
                        db,
                        param.type_expr.as_ref(),
                        pkg_items,
                        &pkg_info.namespace_path,
                        &func_generic_params,
                        alias_map,
                        recursive_aliases,
                    )?;
                    Some(cg::FunctionArgument {
                        name: param.name.clone(),
                        docstring: None,
                        ty,
                        default: lower_codegen_default(
                            func_defaults.param_default(index),
                            &func_defaults.defaults,
                        ),
                    })
                })
                .collect();

            let return_type = resolve_type_expr(
                db,
                func.return_type.as_ref(),
                pkg_items,
                &pkg_info.namespace_path,
                &func_generic_params,
                alias_map,
                recursive_aliases,
            )
            .unwrap_or(cg::Ty::Unit);

            let cg_func = cg::Function {
                name: func.name.clone(),
                generic_params: func_generic_params,
                docstring: func.docstring.clone(),
                arguments,
                return_type,
                throws: resolve_throws(db, func_loc, alias_map, recursive_aliases),
                watchers: Vec::new(),
                origin: Origin {
                    source_file_path: source_file_path.clone(),
                    span_start: u32::from(func.span.start()),
                },
            };

            let cg_name = cg::Name {
                pkg: pkg.clone(),
                namespace_path: ns_path.clone(),
                name: func.name.clone(),
            };
            pool.insert(cg_name, cg::Symbol::Function(cg_func));
        }
    }

    // Methods land on the owning class's `static_methods` /
    // `instance_methods` vec. Companion methods sit alongside their parents
    // in the same vec; span-based ordering at fan-out time keeps them
    // contiguous.
    for pm in pending_methods {
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

/// Resolve an optional `TypeExpr` to a codegen `Ty`.
///
/// Returns `None` if the type expression is missing.
fn resolve_type_expr(
    db: &ProjectDatabase,
    spanned: Option<&baml_compiler2_ast::TypeExpr>,
    package_items: &baml_compiler2_hir::package::PackageItems<'_>,
    ns_context: &[Name],
    generic_params: &[Name],
    alias_map: &HashMap<QualifiedTypeName, TirTy>,
    recursive_aliases: &std::collections::HashSet<QualifiedTypeName>,
) -> Option<cg::Ty> {
    let spanned = spanned?;
    let mut diagnostics = Vec::new();
    let tir_ty = lower_type_expr::lower_type_expr(
        spanned,
        &lower_type_expr::ScopeCtx {
            db,
            package_items,
            ns_context,
            generic_params,
            bounds: &lower_type_expr::TypeVarBoundsMap::default(),
            self_ty: None,
        },
        &mut diagnostics,
    );
    Some(convert_tir_to_codegen_ty(
        &tir_ty,
        alias_map,
        recursive_aliases,
    ))
}

/// Resolve a function's **inferred** throws contract to a codegen `Ty`.
///
/// Sources from `callable_throws` (not the syntactic `throws` clause): a
/// declared clause wins over inference inside that query, and a function that
/// throws *without* a written clause still surfaces its inferred escaping
/// throws. `None` when the function throws nothing (`Ty::Never`) or the
/// contract can't be resolved (`Ty::Unknown`). Used for the `Raises:`
/// docstring block (32d).
fn resolve_throws<'db>(
    db: &'db ProjectDatabase,
    func_loc: FunctionLoc<'db>,
    alias_map: &HashMap<QualifiedTypeName, TirTy>,
    recursive_aliases: &std::collections::HashSet<QualifiedTypeName>,
) -> Option<cg::Ty> {
    match baml_compiler2_tir::callable::callable_throws(db, func_loc) {
        TirTy::Never { .. } | TirTy::Unknown { .. } => None,
        ty => Some(convert_tir_to_codegen_ty(ty, alias_map, recursive_aliases)),
    }
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
        TirTy::Int { .. } => cg::Ty::Int,
        TirTy::Bigint { .. } => cg::Ty::Bigint,
        TirTy::Float { .. } => cg::Ty::Float,
        TirTy::String { .. } => cg::Ty::String,
        TirTy::Bool { .. } => cg::Ty::Bool,
        TirTy::Null { .. } => cg::Ty::Null,
        TirTy::Uint8Array { .. } => cg::Ty::Uint8Array,
        TirTy::Media(MediaKind::Image, _) => cg::Ty::Media(baml_db::MediaKind::Image),
        TirTy::Media(MediaKind::Audio, _) => cg::Ty::Media(baml_db::MediaKind::Audio),
        TirTy::Media(MediaKind::Video, _) => cg::Ty::Media(baml_db::MediaKind::Video),
        TirTy::Media(MediaKind::Pdf, _) => cg::Ty::Media(baml_db::MediaKind::Pdf),
        // `MediaKind::Generic` is never produced by TIR; handled defensively.
        TirTy::Media(MediaKind::Generic, _) => cg::Ty::BuiltinUnknown,

        // Named class types — preserve full QualifiedTypeName via name_from_qtn.
        TirTy::Class(qtn, type_args, _) => cg::Ty::Class(
            name_from_qtn(qtn),
            type_args
                .iter()
                .map(|t| convert_tir_to_codegen_ty(t, alias_map, recursive_aliases))
                .collect(),
        ),
        // Interfaces are BAML-side contracts, not serializable host SDK
        // models. Until codegen grows a structural interface representation,
        // surface interface-typed boundary positions as opaque values instead
        // of emitting references to types the SDK does not define.
        TirTy::Interface(_, _, _, _) => cg::Ty::BuiltinUnknown,
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
        TirTy::Map {
            key: k, value: v, ..
        }
        | TirTy::EvolvingMap(k, v, _) => cg::Ty::Map {
            key: Box::new(convert_tir_to_codegen_ty(k, alias_map, recursive_aliases)),
            value: Box::new(convert_tir_to_codegen_ty(v, alias_map, recursive_aliases)),
        },
        // Unions: convert children, then let simplify_codegen_ty handle them.
        // Nullable types (`T | null`) are just unions whose members include null.
        TirTy::Union(members, _) => cg::Ty::Union(
            members
                .iter()
                .map(|m| convert_tir_to_codegen_ty(m, alias_map, recursive_aliases))
                .collect(),
        ),
        TirTy::Literal(lit, _freshness, _) => cg::Ty::Literal(lit.clone()),

        // BEP-030: BAML's `unknown` top type → BuiltinUnknown.
        TirTy::BuiltinUnknown { .. } => cg::Ty::BuiltinUnknown,
        TirTy::AssociatedTypeProjection { .. } => cg::Ty::BuiltinUnknown,

        // BEP-030: Function types → Callable.
        TirTy::Function { params, ret, .. } => cg::Ty::Callable {
            params: params
                .iter()
                .map(|param| cg::CallableParam {
                    name: param.name.clone(),
                    ty: convert_tir_to_codegen_ty(&param.ty, alias_map, recursive_aliases),
                    mode: match param.mode {
                        FunctionParamMode::Required => cg::CodegenFunctionParamMode::Required,
                        FunctionParamMode::Optional => cg::CodegenFunctionParamMode::Optional,
                    },
                })
                .collect(),
            ret: Box::new(convert_tir_to_codegen_ty(ret, alias_map, recursive_aliases)),
        },

        // Type variable — codegen-side `Ty::TypeVar` mirrors TIR.
        TirTy::TypeVar(name, _) => cg::Ty::TypeVar(name.clone()),

        // `$rust_type` — opaque Rust-managed state. Surfaces as
        // `BamlPyHandle` in Python codegen; other languages will pick
        // their own opaque-handle mapping.
        TirTy::RustType { .. } => cg::Ty::RustType,

        // Bottom / sentinel / error recovery — map to Unit. An inference hole
        // (`_`) should have been filled before codegen; map defensively to Unit.
        TirTy::Void { .. }
        | TirTy::Never { .. }
        | TirTy::Unknown { .. }
        | TirTy::Error { .. }
        | TirTy::Infer { .. }
        | TirTy::Type { .. } => cg::Ty::Unit,

        // BEP-034: surface a `Future<T, E>` as the codegen-side `Unit`
        // for v1 — codegen for the host-side `Future` shape is a
        // follow-up. The error path is acceptable since BAML code that
        // returns futures must `await` them before crossing the host
        // boundary in v1.
        TirTy::Future(_, _, _) => cg::Ty::Unit,
        // `Resource`/`PromptAst` are never produced by TIR; the
        // arms exist only so the match stays exhaustive over the shared
        // `baml_type::Ty`.
        TirTy::Resource { .. } | TirTy::PromptAst { .. } => cg::Ty::BuiltinUnknown,
    }
}

// ---------------------------------------------------------------------------
// Codegen type simplification
// ---------------------------------------------------------------------------

/// Simplify a codegen type: flatten unions, dedup, null-to-end, unwrap singletons.
fn simplify_codegen_ty(ty: cg::Ty) -> cg::Ty {
    match ty {
        cg::Ty::Union(variants) => simplify_union(variants),
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

    #[test]
    fn codegen_type_simplification_flattens_and_deduplicates_nullability() {
        let simplified = simplify_codegen_ty(cg::Ty::Union(vec![
            cg::Ty::Null,
            cg::Ty::Union(vec![cg::Ty::String, cg::Ty::Null]),
            cg::Ty::Null,
        ]));
        assert_eq!(
            simplified,
            cg::Ty::Union(vec![cg::Ty::String, cg::Ty::Null])
        );

        let nested_container = simplify_codegen_ty(cg::Ty::List(Box::new(cg::Ty::Union(vec![
            cg::Ty::Null,
            cg::Ty::Int,
            cg::Ty::Null,
        ]))));
        assert_eq!(
            nested_container,
            cg::Ty::List(Box::new(cg::Ty::Union(vec![cg::Ty::Int, cg::Ty::Null])))
        );
    }

    // ── Integration tests using ProjectDatabase ─────────────────────────────

    /// Verifies that companions land in the pool as their own
    /// `Symbol::Function` entries keyed on the suffixed name. A BAML
    /// declarative function causes `$build_request`, `$render_prompt`,
    /// and `$parse` companion functions to be synthesized by the
    /// companion expander.
    #[test]
    fn test_companions_inserted_as_independent_pool_entries() {
        let root = Path::new("/tmp/bep030_companions");
        let mut db = ProjectDatabase::new();
        db.set_project_root(root);
        db.add_or_update_file(
            root.join("main.baml").as_path(),
            "class Resume { name string }\nfunction ExtractResume(resume: string) -> Resume {\n    client \"openai/gpt-4o\"\n    prompt #\"Extract resume from {{resume}}\"#\n}\n",
        );

        let pool = build_symbol_pool(&db);

        // The parent and each companion must be present as their own
        // `Symbol::Function` entry, keyed on the suffixed name.
        for expected in [
            "ExtractResume",
            "ExtractResume$build_request",
            "ExtractResume$render_prompt",
            "ExtractResume$parse",
        ] {
            let key = pool
                .keys()
                .find(|k| k.name.as_str() == expected)
                .unwrap_or_else(|| panic!("{expected} must be in the pool"));
            assert!(
                matches!(pool.get(key), Some(cg::Symbol::Function(_))),
                "{expected} must be a Function symbol",
            );
        }
    }

    #[test]
    fn test_llm_functions_and_companions_get_default_client_argument() {
        let root = Path::new("/tmp/llm_default_client_arg");
        let mut db = ProjectDatabase::new();
        db.set_project_root(root);
        db.add_or_update_file(
            root.join("main.baml").as_path(),
            r##"
client<llm> GPT4 {
  provider "openai"
  options {
    model "gpt-4o"
    api_key "test"
  }
}

class Resume { name string }

function ExtractResume(resume: string) -> Resume {
  client GPT4
  prompt #"Extract resume from {{resume}}"#
}
"##,
        );

        let pool = build_symbol_pool(&db);

        for (bare, expected_prefix) in [
            ("ExtractResume", &["resume"][..]),
            ("ExtractResume$render_prompt", &["resume"][..]),
            ("ExtractResume$build_request", &["resume"][..]),
            ("ExtractResume$build_request_stream", &["resume"][..]),
            ("ExtractResume$stream", &["resume"][..]),
            ("ExtractResume$parse", &["json"][..]),
            ("ExtractResume$parse_stream", &["sse"][..]),
        ] {
            let key = cg::Name {
                pkg: Name::new("user"),
                namespace_path: vec![],
                name: Name::new(bare),
            };
            let Some(cg::Symbol::Function(func)) = pool.get(&key) else {
                panic!("missing function {bare}");
            };

            let arg_names: Vec<&str> = func.arguments.iter().map(|a| a.name.as_str()).collect();
            let mut expected_names = expected_prefix.to_vec();
            expected_names.push("client");
            assert_eq!(arg_names, expected_names, "arguments for {bare}");

            let client_arg = func.arguments.last().expect("client argument");
            assert_eq!(client_arg.name.as_str(), "client");
            assert_eq!(
                client_arg.default,
                Some(cg::FunctionArgumentDefault::Expression {
                    source: Some("GPT4".to_string()),
                }),
                "client default for {bare}",
            );
            match &client_arg.ty {
                cg::Ty::Class(name, args) => {
                    assert_eq!(name.to_string(), "baml.llm.Client");
                    assert!(args.is_empty());
                }
                other => panic!("client type for {bare} should be baml.llm.Client, got {other:?}"),
            }
        }
    }

    #[test]
    fn test_llm_function_user_client_param_is_compiler_error() {
        let root = Path::new("/tmp/llm_reserved_client_arg");
        let mut db = ProjectDatabase::new();
        db.set_project_root(root);
        db.add_or_update_file(
            root.join("main.baml").as_path(),
            r##"
client<llm> GPT4 {
  provider "openai"
  options {
    model "gpt-4o"
    api_key "test"
  }
}

function Extract(client: string, text: string) -> string {
  client GPT4
  prompt #"{{ text }}"#
}
"##,
        );

        let diagnostics = crate::collect_compiler2_diagnostics(&db);
        assert!(
            diagnostics.iter().any(|diag| {
                diag.message
                    .contains("cannot declare a parameter named `client`")
                    && diag
                        .message
                        .contains("reserved for the compiler-injected LLM client override")
            }),
            "expected reserved `client` compiler error, got: {diagnostics:#?}"
        );
    }

    /// Smoke test: `///` on classes / fields / enums / variants must reach the
    /// `SymbolPool`. Pre-existing failure mode here was that the symbol pool
    /// was built but every `docstring` was `None` despite the AST carrying
    /// it — hence the codegen produced bodies with no `"""…"""`.
    #[test]
    fn test_doc_comments_reach_symbol_pool() {
        let root = Path::new("/tmp/docstrings_repro");
        let mut db = ProjectDatabase::new();
        db.set_project_root(root);
        db.add_or_update_file(
            root.join("main.baml").as_path(),
            "/// A document with a title.\nclass Doc {\n  /// Title shown in lists.\n  title string\n}\n\n/// Sentiment labels.\nenum Sentiment {\n  /// Smiling face.\n  HAPPY\n  SAD\n}\n",
        );

        let pool = build_symbol_pool(&db);

        let doc_key = pool
            .keys()
            .find(|k| k.name.as_str() == "Doc")
            .expect("Doc class missing from pool");
        let cg::Symbol::Class(doc) = &pool[doc_key] else {
            panic!("Doc must be a Class");
        };
        assert_eq!(
            doc.docstring.as_deref(),
            Some("A document with a title."),
            "class /// must reach pool",
        );
        let title = doc
            .properties
            .iter()
            .find(|p| p.name.as_str() == "title")
            .expect("title field missing");
        assert_eq!(
            title.docstring.as_deref(),
            Some("Title shown in lists."),
            "field /// must reach pool",
        );

        let enum_key = pool
            .keys()
            .find(|k| k.name.as_str() == "Sentiment")
            .expect("Sentiment enum missing");
        let cg::Symbol::Enum(en) = &pool[enum_key] else {
            panic!("Sentiment must be an Enum");
        };
        assert_eq!(
            en.docstring.as_deref(),
            Some("Sentiment labels."),
            "enum /// must reach pool",
        );
        let happy = en
            .variants
            .iter()
            .find(|v| v.name.as_str() == "HAPPY")
            .expect("HAPPY variant missing");
        assert_eq!(
            happy.docstring.as_deref(),
            Some("Smiling face."),
            "variant /// must reach pool",
        );
    }

    /// 32d: the inferred throws contract (`callable_throws`) must reach the
    /// `SymbolPool` as `Function.throws`. Covers both a declared union clause
    /// and a no-clause-but-throwing body (the inferred-contract case — the
    /// whole reason for sourcing from `callable_throws`, not the syntactic
    /// clause).
    #[test]
    fn test_throws_reaches_symbol_pool() {
        fn walk(ty: &cg::Ty, out: &mut Vec<String>) {
            match ty {
                cg::Ty::Class(n, _) | cg::Ty::Enum(n) | cg::Ty::TypeAlias(n) => {
                    out.push(n.name.as_str().to_string());
                }
                cg::Ty::Union(ms) => ms.iter().for_each(|m| walk(m, out)),
                _ => {}
            }
        }

        let root = Path::new("/tmp/throws_repro");
        let mut db = ProjectDatabase::new();
        db.set_project_root(root);
        db.add_or_update_file(
            root.join("main.baml").as_path(),
            concat!(
                "class E1 { message string }\n",
                "class E2 { code int }\n\n",
                "function F() -> int throws E1 | E2 {\n",
                "  throw E1 { message: \"x\" }\n",
                "}\n\n",
                "function G() -> int {\n",
                "  throw E1 { message: \"y\" }\n",
                "}\n",
            ),
        );

        let pool = build_symbol_pool(&db);

        let throws_names = |fn_name: &str| -> Vec<String> {
            let key = pool
                .keys()
                .find(|k| k.name.as_str() == fn_name)
                .unwrap_or_else(|| panic!("{fn_name} missing from pool"));
            let cg::Symbol::Function(f) = &pool[key] else {
                panic!("{fn_name} must be a Function");
            };
            let mut out = Vec::new();
            if let Some(t) = &f.throws {
                walk(t, &mut out);
            }
            out
        };

        // Declared union throws → both names, in declaration order.
        assert_eq!(
            throws_names("F"),
            vec!["E1".to_string(), "E2".to_string()],
            "declared union throws must reach pool in order",
        );
        // No `throws` clause but a throwing body → the inferred contract still
        // surfaces E1.
        assert_eq!(
            throws_names("G"),
            vec!["E1".to_string()],
            "inferred throws (no clause) must reach pool",
        );
    }

    #[test]
    fn test_function_defaults_populate_codegen_metadata() {
        fn alloc_default(
            defaults: &mut ast::FunctionDefaults,
            function: LocalItemId<FunctionMarker>,
            expr: ast::Expr,
        ) -> baml_compiler2_hir::item_tree::DefaultExprRef {
            let expr = ast::DefaultExprId::new(defaults.exprs.exprs.alloc(expr));
            baml_compiler2_hir::item_tree::DefaultExprRef { function, expr }
        }

        let function = LocalItemId::<FunctionMarker>::new(1, 0);
        let mut defaults = ast::FunctionDefaults::empty();

        let literal_int = alloc_default(
            &mut defaults,
            function,
            ast::Expr::Literal(ast::Literal::Int(10)),
        );
        let callee = defaults
            .exprs
            .exprs
            .alloc(ast::Expr::Path(vec![Name::new("default_filter")]));
        let expression = alloc_default(
            &mut defaults,
            function,
            ast::Expr::Call {
                callee,
                args: Vec::new(),
                type_args: Vec::new(),
            },
        );
        let empty_list = alloc_default(
            &mut defaults,
            function,
            ast::Expr::Array {
                elements: Vec::new(),
            },
        );
        let nullable_null = alloc_default(&mut defaults, function, ast::Expr::Null);

        assert_eq!(lower_codegen_default(None, &defaults), None);
        assert_eq!(
            lower_codegen_default(Some(&literal_int), &defaults),
            Some(cg::FunctionArgumentDefault::Literal(
                cg::DefaultLiteral::Scalar(ast::Literal::Int(10))
            ))
        );
        assert_eq!(
            lower_codegen_default(Some(&expression), &defaults),
            Some(cg::FunctionArgumentDefault::Expression {
                source: Some("default_filter()".to_string())
            })
        );
        assert_eq!(
            lower_codegen_default(Some(&empty_list), &defaults),
            Some(cg::FunctionArgumentDefault::Literal(
                cg::DefaultLiteral::EmptyList
            ))
        );
        assert_eq!(
            lower_codegen_default(Some(&nullable_null), &defaults),
            Some(cg::FunctionArgumentDefault::Null)
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

    /// Pure-expression functions (no `llm` declarative meta) must reach the
    /// pool as `Symbol::Function` entries so Python codegen emits a factory
    /// binding for them. Regression for 18b §2 / 18c2: a non-LLM,
    /// non-internal parent function used to be filtered out, leaving the
    /// only emission path through synthetic class-field workarounds.
    #[test]
    fn test_pure_expression_function_reaches_pool() {
        let root = Path::new("/tmp/18c2_pure_expression_function");
        let mut db = ProjectDatabase::new();
        db.set_project_root(root);
        db.add_or_update_file(
            root.join("main.baml").as_path(),
            "function ReturnInt() -> int { 42 }\n",
        );

        let pool = build_symbol_pool(&db);

        let key = pool
            .keys()
            .find(|k| k.name.as_str() == "ReturnInt")
            .expect("ReturnInt must be in the pool");
        assert!(
            matches!(pool.get(key), Some(cg::Symbol::Function(_))),
            "ReturnInt must be a Function symbol",
        );
    }

    #[test]
    fn repeated_null_union_reaches_codegen_as_one_nullable_type() {
        let root = Path::new("/tmp/repeated_null_union_codegen_type");
        let mut db = ProjectDatabase::new();
        db.set_project_root(root);
        db.add_or_update_file(
            root.join("main.baml").as_path(),
            "function repeat(value: null | string | null) -> null | string | null { value }\n",
        );

        let diagnostics = crate::collect_compiler2_diagnostics(&db);
        assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:#?}");

        let pool = build_symbol_pool(&db);
        let function = pool
            .values()
            .find_map(|symbol| match symbol {
                cg::Symbol::Function(function) if function.name.as_str() == "repeat" => {
                    Some(function)
                }
                _ => None,
            })
            .expect("repeat function missing from codegen pool");
        let nullable_string = cg::Ty::Union(vec![cg::Ty::String, cg::Ty::Null]);
        assert_eq!(function.arguments[0].ty, nullable_string);
        assert_eq!(function.return_type, nullable_string);
    }

    #[test]
    fn test_interface_and_implements_methods_do_not_reach_free_function_pool() {
        let root = Path::new("/tmp/interface_methods_not_free_codegen_functions");
        let mut db = ProjectDatabase::new();
        db.set_project_root(root);
        db.add_or_update_file(
            root.join("main.baml").as_path(),
            r#"
interface Callable {
  function run(self) -> int throws never { 1 }
}

class Box {
  implements Callable {}
}

implements Callable for int {
  function run(self) -> int throws never { self }
}

function top() -> int { 0 }
"#,
        );

        let diagnostics = crate::collect_compiler2_diagnostics(&db);
        assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:#?}");

        let pool = build_symbol_pool(&db);

        assert!(
            !pool
                .keys()
                .any(|k| k.name.as_str() == "run" && k.namespace_path.is_empty()),
            "interface/default-impl methods must not become free SDK functions"
        );
        let key = pool
            .keys()
            .find(|k| k.name.as_str() == "top")
            .expect("top must be in the pool");
        assert!(
            matches!(pool.get(key), Some(cg::Symbol::Function(_))),
            "ordinary free functions should still reach the pool"
        );
    }

    #[test]
    fn test_interface_typed_sdk_boundaries_are_opaque() {
        let root = Path::new("/tmp/interface_typed_sdk_boundaries_are_opaque");
        let mut db = ProjectDatabase::new();
        db.set_project_root(root);
        db.add_or_update_file(
            root.join("main.baml").as_path(),
            r#"
interface Marker {}

class Box {
  implements Marker {}
}

function passthrough(x: Marker) -> Marker { x }
"#,
        );

        let diagnostics = crate::collect_compiler2_diagnostics(&db);
        assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:#?}");

        let pool = build_symbol_pool(&db);
        let key = pool
            .keys()
            .find(|k| k.name.as_str() == "passthrough")
            .expect("passthrough must be in the pool");
        let Some(cg::Symbol::Function(function)) = pool.get(key) else {
            panic!("passthrough must be a function");
        };

        assert_eq!(
            function.arguments[0].ty,
            cg::Ty::BuiltinUnknown,
            "interface parameters should be opaque at the SDK boundary"
        );
        assert_eq!(
            function.return_type,
            cg::Ty::BuiltinUnknown,
            "interface returns should be opaque at the SDK boundary"
        );
    }
}
