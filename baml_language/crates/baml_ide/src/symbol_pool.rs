//! Conversion from the compiler2 HIR/TIR to `SymbolPool`.
//!
//! Walks each user-defined file via the `ppir` item-data firewall (enumeration
//! queries plus `*_data` lookups), resolves types via TIR, and populates a
//! codegen-ready `SymbolPool` suitable for language-specific code generators
//! such as `sdkgen_python_pydantic2`.

use std::collections::HashMap;

use baml_codegen_types::{self as cg, Origin, SymbolPool};
use baml_compiler2_ast::{self as ast, FunctionOrigin};
use baml_compiler2_hir::{compiler2_all_files, file_package, loc::FunctionLoc, package::PackageId};
use baml_db::{Name, ProjectDatabase};
use baml_type::{Freshness, ParamTy, QualifiedTypeName, Ty as TirTy, TyAttr};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Build a `cg::Name` from a `QualifiedTypeName`. Preserves `pkg`, the full
/// namespace path, and the bare name (including any `$stream` suffix).
fn name_from_qtn(qtn: &QualifiedTypeName) -> cg::Name {
    qtn.clone()
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
/// For example `"extract_resume$build_request"` → `Some(("extract_resume", "build_request"))`.
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
            let mut aliases = HashMap::new();
            for ns in pkg_items.namespaces.values() {
                for (name, def) in &ns.types {
                    if let baml_compiler2_hir::contributions::Definition::TypeAlias(loc) = def {
                        aliases.insert(
                            baml_compiler2_hir_ty::lower::qualify_def(db, *def, name),
                            baml_compiler2_hir_ty::lower::type_alias_value(db, *loc).to_plain(),
                        );
                    }
                }
            }
            let resolved = baml_type::ResolvedAliases::from_aliases(aliases);
            (resolved.aliases, resolved.recursive)
        });
        let alias_map: &HashMap<QualifiedTypeName, TirTy> = alias_map;
        let recursive_aliases: &std::collections::HashSet<QualifiedTypeName> = recursive_aliases;

        let source_file_path: String = source_file.path(db).to_string_lossy().into_owned();

        // Collect function locs that are not package-level functions so the
        // free-function walk below can skip them. Class methods, interface
        // default methods, and out-of-body implements methods are all functions,
        // but none of them are directly callable as `<pkg>.<ns>.<method>` from
        // generated SDKs.
        let mut non_free_function_locs: std::collections::HashSet<FunctionLoc> =
            std::collections::HashSet::new();
        for &class_loc in baml_compiler2_ppir::item_data::file_classes(db, source_file) {
            for &m in &baml_compiler2_ppir::item_data::class_data(db, class_loc).methods {
                non_free_function_locs.insert(m);
            }
        }
        for &iface_loc in baml_compiler2_ppir::item_data::file_interfaces(db, source_file) {
            // ALL interface methods: default (with a body) and required
            // (bodyless items under the unified method model) alike - a
            // required signature is an interface slot, not a callable.
            for &m in &baml_compiler2_ppir::item_data::interface_data(db, iface_loc).methods {
                non_free_function_locs.insert(m);
            }
        }
        for &impl_loc in baml_compiler2_ppir::item_data::file_free_impls(db, source_file) {
            for &m in &baml_compiler2_ppir::item_data::impl_block_data(db, impl_loc).methods {
                non_free_function_locs.insert(m);
            }
        }

        // Classes
        for &class_loc in baml_compiler2_ppir::item_data::file_classes(db, source_file) {
            let class = baml_compiler2_ppir::item_data::class_data(db, class_loc);
            let cg_name = cg::Name::new(pkg.clone(), ns_path.clone(), class.name.clone());
            let class_generic_params =
                baml_compiler2_hir_ty::lower::class_generic_frame(db, class_loc);
            let properties = class
                .fields
                .iter()
                .filter_map(|field| {
                    let ty = resolve_type_ref(
                        db,
                        &class.type_refs,
                        Some(field.type_ref),
                        source_file,
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
            for &method_loc in &class.methods {
                let method = baml_compiler2_ppir::item_data::function_data(db, method_loc);

                // Auto-derived methods (`to_json` / `from_json` synthesized
                // by `auto_derive_json`) are language-level plumbing, not
                // user-facing API. Skip them so client SDKs don't surface
                // them as static / instance methods on every class.
                if method.metadata.is_language_internal
                    || matches!(method.metadata.origin, FunctionOrigin::AutoDerive)
                {
                    continue;
                }

                // Interface-impl methods are not part of the generated surface.
                // Interfaces themselves are never emitted — host languages differ
                // too much on trait/protocol/interface semantics — so a method
                // that only exists to satisfy one has nothing to attach to.
                //
                // Emitting them is not merely redundant but unsound: the
                // generated name is the bare method name, so a type implementing
                // one interface at several instantiations (`Multiply<int>` and
                // `Multiply<bigint>` for `baml.time.Duration`) or two interfaces
                // sharing a method name (`Subtract<Duration>` and
                // `Subtract<Instant>` for `baml.time.Instant`) collides with
                // itself. The runtime path it invokes (`baml.time.Duration.mul`)
                // is ambiguous for the same reason.
                //
                // `class.methods` is flat: in-body `implements I { … }` and a
                // non-generic out-of-body `implement I for C` merged onto `C`
                // (`lower_cst`) both land here, so the interface target — not the
                // declaration site — is what identifies them.
                if baml_compiler2_ppir::item_data::method_interface_target(db, method_loc).is_some()
                {
                    continue;
                }

                if matches!(
                    baml_compiler2_ppir::function_body(db, method_loc).as_ref(),
                    baml_compiler2_hir::body::FunctionBody::Builtin(ast::BuiltinKind::Intrinsic)
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

                // Same elaboration as free functions (see the free-function
                // path). The method's own `generic_params` are its user +
                // synthetic effect params; the enclosing class's params join
                // only the lowering scope, not the method's declared generics.
                let sig = baml_compiler2_ppir::item_data::elaborated_function_data(db, method_loc);
                // Emitted generics are the method's *user* params only; effect
                // params (see the free-function path) join only the lowering
                // scope.
                let method_own_generics: Vec<Name> = sig.user_generic_params.clone();
                // Generics in scope inside the method body: the class's TypeVars
                // first, then the method's own user + synthetic effect params.
                let method_scope_generics =
                    baml_compiler2_hir_ty::lower::function_generic_frame(db, method_loc);
                // Empty bounds match the prior raw-AST lowering (see the
                // free-function path).
                let ctx = baml_compiler2_hir_ty::lower::lower_ctx_for_file(db, source_file)
                    .with_frame(method_scope_generics.clone())
                    .with_self_ty(Some(baml_compiler2_hir_ty::lower::class_self_ty(
                        db, class_loc,
                    )));
                let type_refs = &sig.type_refs;
                let lower = |id| {
                    let tir_ty = ctx.lower_type_ref(type_refs, id).to_plain();
                    convert_tir_to_codegen_ty(&tir_ty, alias_map, recursive_aliases)
                };
                let method_defaults =
                    baml_compiler2_ppir::function_parameter_defaults(db, method_loc);

                let arguments: Vec<cg::FunctionArgument> = sig
                    .params
                    .iter()
                    .enumerate()
                    .skip(usize::from(is_instance))
                    .map(|(index, param)| cg::FunctionArgument {
                        name: param.name.clone(),
                        docstring: None,
                        ty: lower(param.type_ref),
                        default: lower_codegen_default(
                            method_defaults.param_default(index),
                            &method_defaults.defaults,
                        ),
                    })
                    .collect();

                let return_type = sig.return_type.map_or(
                    cg::Ty::Void {
                        attr: TyAttr::default(),
                    },
                    lower,
                );

                let cg_method = cg::Function {
                    name: method.name.clone(),
                    generic_params: method_own_generics,
                    docstring: method.docstring.clone(),
                    arguments,
                    return_type,
                    throws: resolve_throws(db, method_loc, alias_map, recursive_aliases),
                    watchers: Vec::new(),
                    origin: Origin {
                        source_file_path: source_file_path.clone(),
                        span_start: u32::from(
                            baml_compiler2_ppir::item_data::function_source_map(db, method_loc)
                                .span
                                .start(),
                        ),
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
                    generic_params: class
                        .generic_params
                        .iter()
                        .map(|param| param.name.clone())
                        .collect(),
                    docstring: class.docstring.clone(),
                    properties,
                    static_methods: Vec::new(),
                    instance_methods: Vec::new(),
                    origin: Origin {
                        source_file_path: source_file_path.clone(),
                        span_start: u32::from(
                            baml_compiler2_ppir::item_data::class_source_map(db, class_loc)
                                .span
                                .start(),
                        ),
                    },
                }),
            );
        }

        // Enums
        for &enum_loc in baml_compiler2_ppir::item_data::file_enums(db, source_file) {
            let enum_def = baml_compiler2_ppir::item_data::enum_data(db, enum_loc);
            let cg_name = cg::Name::new(pkg.clone(), ns_path.clone(), enum_def.name.clone());
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
                        span_start: u32::from(
                            baml_compiler2_ppir::item_data::enum_source_map(db, enum_loc)
                                .span
                                .start(),
                        ),
                    },
                }),
            );
        }

        // Type aliases
        for &alias_loc in baml_compiler2_ppir::item_data::file_type_aliases(db, source_file) {
            let alias = baml_compiler2_ppir::item_data::type_alias_data(db, alias_loc);
            if let Some(resolved) = resolve_type_ref(
                db,
                &alias.type_refs,
                alias.value,
                source_file,
                &[],
                alias_map,
                recursive_aliases,
            ) {
                let cg_name = cg::Name::new(pkg.clone(), ns_path.clone(), alias.name.clone());

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
                            span_start: u32::from(
                                baml_compiler2_ppir::item_data::type_alias_source_map(
                                    db, alias_loc,
                                )
                                .span
                                .start(),
                            ),
                        },
                    }),
                );
            }
        }

        // Top-level functions — methods are skipped via `non_free_function_locs`
        // so they don't double-emit. Companion functions (names containing `$`)
        // flow through as their own pool entries; parent and companion alike are
        // inserted directly, keyed on the suffixed name.
        for &func_loc in baml_compiler2_ppir::item_data::file_functions(db, source_file) {
            if non_free_function_locs.contains(&func_loc) {
                continue;
            }
            let func = baml_compiler2_ppir::item_data::function_data(db, func_loc);

            // Internal-origin functions (e.g. `<Client>$new` synthesized for
            // primitive clients) are runtime plumbing, not user-callable —
            // skip them so they don't end up as Python factory bindings.
            if func.metadata.is_language_internal
                || matches!(func.metadata.origin, FunctionOrigin::Internal)
            {
                continue;
            }

            // The `$spec` companion returns `ai.FunctionSpec<Out>` — a BAML-side
            // recipe value for custom runners, not something a host language can
            // use (and not something every generator can even classify: the C#
            // generator hard-errors on it rather than skipping). The useful
            // companions — `$render_prompt`, `$parse`, `$stream` — stay.
            if func.name.as_str().ends_with("$spec") {
                continue;
            }

            if matches!(
                baml_compiler2_ppir::function_body(db, func_loc).as_ref(),
                baml_compiler2_hir::body::FunctionBody::Builtin(ast::BuiltinKind::Intrinsic)
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

            // Source the signature from the *elaborated* HIR data, not the raw
            // item tree: elaboration mints a synthetic effect param for every
            // callback parameter that omits a `throws` clause and rewrites that
            // parameter's `throws` to the fresh param. Both must reach the
            // codegen type — the inferred outer `throws` (from `callable_throws`)
            // references the effect param, so a raw lowering would leave it a
            // dangling, undeclared typevar and collapse the callback's own
            // `throws` to `Never`.
            let sig = baml_compiler2_ppir::item_data::elaborated_function_data(db, func_loc);
            // Effect params (minted for a callback whose `throws` is inferred)
            // join only the lowering *scope*, so the callback and the inferred
            // outer `throws` resolve to real typevars instead of dangling /
            // `Never`. They are inferred from the callback, not user-bound, so
            // they are NOT emitted as user-facing generics: an SDK that binds
            // generics by name (e.g. Python's `_types=`) must not see them — a
            // consumer that needs the throws type recovers it from the callback
            // parameter (the Rust SDK, via each callback's associated error
            // type).
            let scope_generics = baml_compiler2_hir_ty::lower::function_generic_frame(db, func_loc);
            let func_generic_params: Vec<Name> = sig.user_generic_params.clone();
            // Empty bounds match the prior raw-AST lowering: this path resolves
            // only in-scope typevars (incl. the effect params), not associated
            // projections that would need interface bounds.
            let ctx = baml_compiler2_hir_ty::lower::lower_ctx_for_file(db, source_file)
                .with_frame(scope_generics.clone());
            let type_refs = &sig.type_refs;
            let lower = |id| {
                let tir_ty = ctx.lower_type_ref(type_refs, id).to_plain();
                convert_tir_to_codegen_ty(&tir_ty, alias_map, recursive_aliases)
            };
            let func_defaults = baml_compiler2_ppir::function_parameter_defaults(db, func_loc);
            // The compiler injects a trailing `client: ai.Client? = null`
            // override onto every LLM function (and its companions). It is a
            // BAML-side concern typed as an INTERFACE, which no target
            // language can represent — leaving it in makes the whole function
            // "unsupported" and the generator drops it, so the SDK would
            // expose no LLM functions at all. Strip it here (the same rule
            // `param_schema` applies for the playground form): SDK callers get
            // the function's declared default client.
            let param_count = sig.params.len();
            // ...and on the `$stream` companion, where the same override is
            // typed `ai.stream.StreamingClient?` (companions carry no LLM
            // metadata of their own, hence the name check).
            let drop_injected_client = sig
                .params
                .last()
                .is_some_and(|p| p.name.as_str() == "client")
                && (baml_compiler2_ppir::item_data::function_llm_meta(db, func_loc).is_some()
                    || func.name.as_str().ends_with("$stream"));
            let visible_params = if drop_injected_client {
                param_count - 1
            } else {
                param_count
            };
            let arguments: Vec<cg::FunctionArgument> = sig
                .params
                .iter()
                .take(visible_params)
                .enumerate()
                .map(|(index, param)| cg::FunctionArgument {
                    name: param.name.clone(),
                    docstring: None,
                    ty: lower(param.type_ref),
                    default: lower_codegen_default(
                        func_defaults.param_default(index),
                        &func_defaults.defaults,
                    ),
                })
                .collect();

            let return_type = sig.return_type.map_or(
                cg::Ty::Void {
                    attr: TyAttr::default(),
                },
                lower,
            );

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
                    span_start: u32::from(
                        baml_compiler2_ppir::item_data::function_source_map(db, func_loc)
                            .span
                            .start(),
                    ),
                },
            };

            let cg_name = cg::Name::new(pkg.clone(), ns_path.clone(), func.name.clone());
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

/// Resolve an optional type reference (from an item's firewall `type_refs`
/// arena) to a codegen `Ty`.
///
/// Returns `None` if `id` is `None` (the annotation was omitted).
fn resolve_type_ref(
    db: &ProjectDatabase,
    store: &baml_compiler2_hir::type_ref::TypeRefStore,
    id: Option<baml_compiler2_hir::type_ref::TypeRefId>,
    file: baml_base::SourceFile,
    generic_params: &[ParamTy],
    alias_map: &HashMap<QualifiedTypeName, TirTy>,
    recursive_aliases: &std::collections::HashSet<QualifiedTypeName>,
) -> Option<cg::Ty> {
    let id = id?;
    let ctx = baml_compiler2_hir_ty::lower::lower_ctx_for_file(db, file)
        .with_frame(generic_params.to_vec());
    let tir_ty = ctx.lower_type_ref(store, id).to_plain();
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
    match &baml_compiler2_hir_ty::callable::callable_throws(db, func_loc).0 {
        TirTy::Never { .. } | TirTy::Unknown { .. } => None,
        ty => Some(convert_tir_to_codegen_ty(ty, alias_map, recursive_aliases)),
    }
}

/// Convert a TIR type to the compiler-owned codegen family.
///
/// Public alias references remain nominal. The corresponding `TypeAlias`
/// symbol stores the separately resolved target, so alias chains survive all
/// the way to generators. Canonicalization is centralized on `CodegenTy` and
/// recursively normalizes every container.
fn convert_tir_to_codegen_ty(
    ty: &TirTy,
    _alias_map: &HashMap<QualifiedTypeName, TirTy>,
    _recursive_aliases: &std::collections::HashSet<QualifiedTypeName>,
) -> cg::Ty {
    convert_tir_leaf(ty).canonicalize()
}

fn convert_tir_leaf(ty: &TirTy) -> cg::Ty {
    // Each recursive invocation reads the attribute from its own source node,
    // so nested SAP/streaming annotations survive the codegen boundary.
    let attr = || ty.attr().clone();
    let convert = |ty: &TirTy| convert_tir_leaf(ty);
    match ty {
        TirTy::Int { .. } => cg::Ty::Int { attr: attr() },
        TirTy::Bigint { .. } => cg::Ty::Bigint { attr: attr() },
        TirTy::Float { .. } => cg::Ty::Float { attr: attr() },
        TirTy::String { .. } => cg::Ty::String { attr: attr() },
        TirTy::Bool { .. } => cg::Ty::Bool { attr: attr() },
        TirTy::Null { .. } => cg::Ty::Null { attr: attr() },
        TirTy::Uint8Array { .. } => cg::Ty::Uint8Array { attr: attr() },
        TirTy::Media(kind, _) => cg::Ty::Media(*kind, attr()),
        TirTy::Literal(literal, _, _) => {
            cg::Ty::Literal(literal.clone(), Freshness::Regular, attr())
        }
        TirTy::Class(qtn, type_args, _) => cg::Ty::Class(
            name_from_qtn(qtn),
            type_args.iter().map(convert).collect(),
            attr(),
        ),
        TirTy::Interface(qtn, generics, associated_types, _) => cg::Ty::Interface(
            name_from_qtn(qtn),
            generics.iter().map(convert).collect(),
            associated_types
                .iter()
                .map(|(name, ty)| (name.clone(), convert(ty)))
                .collect(),
            attr(),
        ),
        TirTy::Enum(qtn, _) => cg::Ty::Enum(name_from_qtn(qtn), attr()),
        TirTy::EnumVariant(qtn, variant, _) => {
            cg::Ty::EnumVariant(name_from_qtn(qtn), variant.clone(), attr())
        }
        TirTy::TypeAlias(qtn, _) => cg::Ty::TypeAlias(name_from_qtn(qtn), attr()),
        TirTy::List(inner, _) | TirTy::EvolvingList(inner, _) => {
            cg::Ty::List(Box::new(convert(inner)), attr())
        }
        TirTy::Map {
            key: k, value: v, ..
        }
        | TirTy::EvolvingMap(k, v, _) => cg::Ty::Map {
            key: Box::new(convert(k)),
            value: Box::new(convert(v)),
            attr: attr(),
        },
        TirTy::Union(members, _) => cg::Ty::Union(members.iter().map(convert).collect(), attr()),
        TirTy::Function {
            params,
            ret,
            throws,
            ..
        } => cg::Ty::Function {
            params: params
                .iter()
                .map(|param| cg::CallableParam {
                    name: param.name.clone(),
                    ty: convert(&param.ty),
                    mode: param.mode,
                })
                .collect(),
            ret: Box::new(convert(ret)),
            throws: Box::new(convert(throws)),
            attr: attr(),
        },
        TirTy::Future(value, error, _) => {
            cg::Ty::Future(Box::new(convert(value)), Box::new(convert(error)), attr())
        }
        TirTy::RustType { .. } => cg::Ty::RustType { attr: attr() },
        TirTy::Type { .. } => cg::Ty::Type { attr: attr() },
        TirTy::Resource { .. } => cg::Ty::Resource { attr: attr() },
        TirTy::PromptAst { .. } => cg::Ty::PromptAst { attr: attr() },
        TirTy::Void { .. } => cg::Ty::Void { attr: attr() },
        TirTy::TypeVar(name, _) => cg::Ty::TypeVar(name.clone(), attr()),
        TirTy::BuiltinUnknown { .. } => cg::Ty::BuiltinUnknown { attr: attr() },
        TirTy::Never { .. } => cg::Ty::Never { attr: attr() },

        // These are compiler recovery/inference states, not public API types.
        // Diagnostics have already been emitted; retain the historical opaque
        // fallback so code generation remains total in error-tolerant flows.
        TirTy::AssociatedTypeProjection { .. } | TirTy::Unknown { .. } | TirTy::Error { .. } => {
            cg::Ty::BuiltinUnknown { attr: attr() }
        }
        TirTy::Infer { .. } => cg::Ty::Void { attr: attr() },
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use std::path::Path;

    use baml_compiler2_hir::ids::{FunctionMarker, LocalItemId};
    use baml_type::TyAttrValue;

    use super::*;
    use crate::test_support::TestDbExt;

    fn codegen_attr() -> TyAttr {
        TyAttr::default()
    }

    fn codegen_alias(name: cg::Name) -> cg::Ty {
        cg::Ty::TypeAlias(name, codegen_attr())
    }

    fn codegen_list(inner: cg::Ty) -> cg::Ty {
        cg::Ty::List(Box::new(inner), codegen_attr())
    }

    fn codegen_union(members: Vec<cg::Ty>) -> cg::Ty {
        cg::Ty::Union(members, codegen_attr())
    }

    #[test]
    fn tir_attributes_survive_recursive_codegen_lowering() {
        let outer_attr = TyAttr {
            sap_pending_never: TyAttrValue::Set,
            ..TyAttr::default()
        };
        let inner_attr = TyAttr {
            sap_parse_without_null: TyAttrValue::Set,
            ..TyAttr::default()
        };
        let tir = TirTy::List(
            Box::new(TirTy::String {
                attr: inner_attr.clone(),
            }),
            outer_attr.clone(),
        );

        assert_eq!(
            convert_tir_to_codegen_ty(&tir, &HashMap::new(), &std::collections::HashSet::new(),),
            cg::Ty::List(Box::new(cg::Ty::String { attr: inner_attr }), outer_attr,)
        );
    }

    // ── Unit tests for pure helpers ─────────────────────────────────────────

    #[test]
    fn test_split_companion_with_dollar() {
        assert_eq!(
            split_companion("extract_resume$build_request"),
            Some(("extract_resume", "build_request"))
        );
        assert_eq!(split_companion("Foo$parse"), Some(("Foo", "parse")));
        assert_eq!(
            split_companion("extract_resume$render_prompt"),
            Some(("extract_resume", "render_prompt"))
        );
    }

    #[test]
    fn test_split_companion_no_dollar() {
        assert_eq!(split_companion("extract_resume"), None);
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
        assert_eq!(cg_name.package().as_str(), "user");
        assert_eq!(
            cg_name.namespace(),
            &vec![Name::new("foo")],
            "namespace_path mismatch: {:?}",
            cg_name.namespace(),
        );
        assert_eq!(cg_name.name().as_str(), "Sentiment");
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

    /// Verifies that companions land in the pool as their own
    /// `Symbol::Function` entries keyed on the suffixed name. A BAML
    /// declarative function causes `$build_request`, `$render_prompt`,
    /// and `$parse` companion functions to be synthesized by the
    /// companion expander.
    #[test]
    fn test_companions_inserted_as_independent_pool_entries() {
        let root = Path::new("/tmp/bep030_companions");
        let mut db = ProjectDatabase::new();
        db.workspace(root);
        db.file(
            root.join("main.baml").as_path(),
            "class Resume { name string }\nfunction extract_resume(resume: string) -> Resume {\n    client: \"openai/gpt-4o\"\n    prompt: `extract resume from ${resume} ${ctx.output_format}`\n}\n",
        );

        let pool = build_symbol_pool(&db);

        // The parent and each host-facing companion must be present as their
        // own `Symbol::Function` entry, keyed on the suffixed name. `$spec` is
        // deliberately absent — it returns a BAML-side `ai.FunctionSpec`.
        for expected in [
            "extract_resume",
            "extract_resume$render_prompt",
            "extract_resume$parse",
        ] {
            let key = pool
                .keys()
                .find(|k| k.name().as_str() == expected)
                .unwrap_or_else(|| panic!("{expected} must be in the pool"));
            assert!(
                matches!(pool.get(key), Some(cg::Symbol::Function(_))),
                "{expected} must be a Function symbol",
            );
        }
    }

    #[test]
    fn test_llm_functions_hide_the_injected_client_argument() {
        let root = Path::new("/tmp/llm_default_client_arg");
        let mut db = ProjectDatabase::new();
        db.workspace(root);
        db.file(
            root.join("main.baml").as_path(),
            r##"
class Resume { name string }

function extract_resume(resume: string) -> Resume {
  client: "openai/gpt-4o"
  prompt: `extract resume from ${resume} ${ctx.output_format}`
}
"##,
        );

        let pool = build_symbol_pool(&db);

        // The compiler injects a trailing `client: ai.Client? = null` override
        // onto the LLM function. `ai.Client` is an interface, which no target
        // language can represent — leaving it in the pool made every generator
        // classify the function as unsupported and drop it entirely. The pool
        // must expose the user's own parameters only.
        for (bare, expected) in [
            ("extract_resume", &["resume"][..]),
            ("extract_resume$render_prompt", &["resume"][..]),
            ("extract_resume$parse", &["json"][..]),
        ] {
            let key = cg::Name::new(Name::new("user"), vec![], Name::new(bare));
            let Some(cg::Symbol::Function(func)) = pool.get(&key) else {
                panic!("missing function {bare}");
            };
            let arg_names: Vec<&str> = func.arguments.iter().map(|a| a.name.as_str()).collect();
            assert_eq!(arg_names, expected, "arguments for {bare}");
        }
    }

    #[test]
    fn test_llm_function_user_client_param_is_compiler_error() {
        let root = Path::new("/tmp/llm_reserved_client_arg");
        let mut db = ProjectDatabase::new();
        db.workspace(root);
        db.file(
            root.join("main.baml").as_path(),
            r##"
client<llm> GPT4 {
  provider "openai"
  options {
    model "gpt-4o"
    api_key "test"
  }
}

function extract(client: string, text: string) -> string {
  client: GPT4
  prompt: `${text}`
}
"##,
        );

        let diagnostics = baml_db::collect_compiler2_diagnostics(&db);
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
        db.workspace(root);
        db.file(
            root.join("main.baml").as_path(),
            "/// A document with a title.\nclass Doc {\n  /// Title shown in lists.\n  title string\n}\n\n/// Sentiment labels.\nenum Sentiment {\n  /// Smiling face.\n  HAPPY\n  SAD\n}\n",
        );

        let pool = build_symbol_pool(&db);

        let doc_key = pool
            .keys()
            .find(|k| k.name().as_str() == "Doc")
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
            .find(|k| k.name().as_str() == "Sentiment")
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
                cg::Ty::Class(n, _, _) | cg::Ty::Enum(n, _) | cg::Ty::TypeAlias(n, _) => {
                    out.push(n.name().as_str().to_string());
                }
                cg::Ty::Union(ms, _) => ms.iter().for_each(|m| walk(m, out)),
                _ => {}
            }
        }

        let root = Path::new("/tmp/throws_repro");
        let mut db = ProjectDatabase::new();
        db.workspace(root);
        db.file(
            root.join("main.baml").as_path(),
            concat!(
                "class E1 { message string }\n",
                "class E2 { code int }\n\n",
                "function f() -> int throws E1 | E2 {\n",
                "  throw E1 { message: \"x\" }\n",
                "}\n\n",
                "function g() -> int {\n",
                "  throw E1 { message: \"y\" }\n",
                "}\n",
            ),
        );

        let pool = build_symbol_pool(&db);

        let throws_names = |fn_name: &str| -> Vec<String> {
            let key = pool
                .keys()
                .find(|k| k.name().as_str() == fn_name)
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
            throws_names("f"),
            vec!["E1".to_string(), "E2".to_string()],
            "declared union throws must reach pool in order",
        );
        // No `throws` clause but a throwing body → the inferred contract still
        // surfaces E1.
        assert_eq!(
            throws_names("g"),
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
        db.workspace(root);
        db.file(
            root.join("main.baml").as_path(),
            "class Counter {\n  count int\n  function bump(self, by: int) -> int { self.count + by }\n  function zero() -> int { 0 }\n}\n",
        );

        let pool = build_symbol_pool(&db);

        let key = pool
            .keys()
            .find(|k| k.name().as_str() == "Counter")
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

    /// `Self` must be resolved by the compiler-owned type lowering before any
    /// host generator sees the signature. This pins bare and nested positions
    /// for both instance and static class methods.
    #[test]
    fn test_class_method_self_lowers_to_owning_codegen_class() {
        let root = Path::new("/tmp/12c_method_self");
        let mut db = ProjectDatabase::new();
        db.workspace(root);
        db.file(
            root.join("main.baml").as_path(),
            r#"
class Mirror {
  value string
  function clone(self, other: Self?) -> map<string, Self> { {"value": self} }
  function pick(self, value: Self | int) -> Self | string { self }
  function wrap(value: Self[]) -> Self { value[0] }
}

class GenericMirror<T> {
  value T
  function nested(self, other: Self?) -> map<string, Self[]> { {"value": [self]} }
  function identity(value: Self) -> Self { value }
}
"#,
        );

        let pool = build_symbol_pool(&db);
        let (owner, class) = pool
            .iter()
            .find_map(|(name, symbol)| match symbol {
                cg::Symbol::Class(class) if name.name().as_str() == "Mirror" => Some((name, class)),
                _ => None,
            })
            .expect("Mirror class missing from pool");

        let clone = class
            .instance_methods
            .iter()
            .find(|method| method.name.as_str() == "clone")
            .expect("clone instance method missing");
        assert!(matches!(
            &clone.arguments[0].ty,
            cg::Ty::Union(members, _)
                if members.iter().any(|ty| matches!(ty, cg::Ty::Class(name, args, _) if name == owner && args.is_empty()))
                    && members.iter().any(|ty| matches!(ty, cg::Ty::Null { .. }))
        ));
        assert!(matches!(
            &clone.return_type,
            cg::Ty::Map { key, value, .. }
                if matches!(key.as_ref(), cg::Ty::String { .. })
                    && matches!(value.as_ref(), cg::Ty::Class(name, args, _) if name == owner && args.is_empty())
        ));

        let pick = class
            .instance_methods
            .iter()
            .find(|method| method.name.as_str() == "pick")
            .expect("pick instance method missing");
        assert!(matches!(
            &pick.arguments[0].ty,
            cg::Ty::Union(members, _)
                if members.iter().any(|ty| matches!(ty, cg::Ty::Class(name, args, _) if name == owner && args.is_empty()))
                    && members.iter().any(|ty| matches!(ty, cg::Ty::Int { .. }))
        ));
        assert!(matches!(
            &pick.return_type,
            cg::Ty::Union(members, _)
                if members.iter().any(|ty| matches!(ty, cg::Ty::Class(name, args, _) if name == owner && args.is_empty()))
                    && members.iter().any(|ty| matches!(ty, cg::Ty::String { .. }))
        ));

        let wrap = class
            .static_methods
            .iter()
            .find(|method| method.name.as_str() == "wrap")
            .expect("wrap static method missing");
        assert!(matches!(
            &wrap.arguments[0].ty,
            cg::Ty::List(inner, _)
                if matches!(inner.as_ref(), cg::Ty::Class(name, args, _) if name == owner && args.is_empty())
        ));
        assert!(matches!(
            &wrap.return_type,
            cg::Ty::Class(name, args, _) if name == owner && args.is_empty()
        ));

        let (generic_owner, generic_class) = pool
            .iter()
            .find_map(|(name, symbol)| match symbol {
                cg::Symbol::Class(class) if name.name().as_str() == "GenericMirror" => {
                    Some((name, class))
                }
                _ => None,
            })
            .expect("GenericMirror class missing from pool");
        let nested = generic_class
            .instance_methods
            .iter()
            .find(|method| method.name.as_str() == "nested")
            .expect("nested generic instance method missing");
        let is_instantiated_self = |ty: &cg::Ty| {
            matches!(
                ty,
                cg::Ty::Class(name, args, _)
                    if name == generic_owner
                        && matches!(args.as_slice(), [cg::Ty::TypeVar(name, _)] if name.as_str() == "T")
            )
        };
        assert!(matches!(
            &nested.arguments[0].ty,
            cg::Ty::Union(members, _)
                if members.iter().any(&is_instantiated_self)
                    && members.iter().any(|ty| matches!(ty, cg::Ty::Null { .. }))
        ));
        assert!(matches!(
            &nested.return_type,
            cg::Ty::Map { key, value, .. }
                if matches!(key.as_ref(), cg::Ty::String { .. })
                    && matches!(value.as_ref(), cg::Ty::List(inner, _) if is_instantiated_self(inner))
        ));

        let identity = generic_class
            .static_methods
            .iter()
            .find(|method| method.name.as_str() == "identity")
            .expect("identity generic static method missing");
        assert!(is_instantiated_self(&identity.arguments[0].ty));
        assert!(is_instantiated_self(&identity.return_type));
    }

    /// Verifies that a class in a namespaced folder gets `namespace_path` populated.
    /// A file in the `ns_foo` subdirectory should produce a class with
    /// `namespace_path == ["foo"]` and `pkg == "user"`.
    #[test]
    fn test_namespaced_class_has_path() {
        let root = Path::new("/tmp/bep030_ns_path");
        let mut db = ProjectDatabase::new();
        db.workspace(root);
        db.file(
            root.join("ns_foo").join("sentiment.baml").as_path(),
            "class Sentiment { label string }\n",
        );

        let pool = build_symbol_pool(&db);

        let sentiment_key = pool.keys().find(|k| k.name().as_str() == "Sentiment");
        assert!(sentiment_key.is_some(), "Sentiment must be in the pool");

        let key = sentiment_key.unwrap();
        assert_eq!(key.package().as_str(), "user", "pkg mismatch: {key:?}");
        assert_eq!(
            key.namespace(),
            &vec![Name::new("foo")],
            "namespace_path mismatch: {:?}",
            key.namespace(),
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
        db.workspace(root);
        db.file(
            root.join("main.baml").as_path(),
            "function return_int() -> int { 42 }\n",
        );

        let pool = build_symbol_pool(&db);

        let key = pool
            .keys()
            .find(|k| k.name().as_str() == "return_int")
            .expect("return_int must be in the pool");
        assert!(
            matches!(pool.get(key), Some(cg::Symbol::Function(_))),
            "return_int must be a Function symbol",
        );
    }

    #[test]
    fn test_interface_and_implements_methods_do_not_reach_free_function_pool() {
        let root = Path::new("/tmp/interface_methods_not_free_codegen_functions");
        let mut db = ProjectDatabase::new();
        db.workspace(root);
        db.file(
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

        let diagnostics = baml_db::collect_compiler2_diagnostics(&db);
        assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:#?}");

        let pool = build_symbol_pool(&db);

        assert!(
            !pool
                .keys()
                .any(|k| k.name().as_str() == "run" && k.namespace().is_empty()),
            "interface/default-impl methods must not become free SDK functions"
        );
        let key = pool
            .keys()
            .find(|k| k.name().as_str() == "top")
            .expect("top must be in the pool");
        assert!(
            matches!(pool.get(key), Some(cg::Symbol::Function(_))),
            "ordinary free functions should still reach the pool"
        );
    }

    #[test]
    fn test_interface_typed_sdk_boundaries_preserve_compiler_type() {
        let root = Path::new("/tmp/interface_typed_sdk_boundaries_are_opaque");
        let mut db = ProjectDatabase::new();
        db.workspace(root);
        db.file(
            root.join("main.baml").as_path(),
            r#"
interface Marker {}

class Box {
  implements Marker {}
}

function passthrough(x: Marker) -> Marker { x }
"#,
        );

        let diagnostics = baml_db::collect_compiler2_diagnostics(&db);
        assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:#?}");

        let pool = build_symbol_pool(&db);
        let key = pool
            .keys()
            .find(|k| k.name().as_str() == "passthrough")
            .expect("passthrough must be in the pool");
        let Some(cg::Symbol::Function(function)) = pool.get(key) else {
            panic!("passthrough must be a function");
        };

        for ty in [&function.arguments[0].ty, &function.return_type] {
            assert!(
                matches!(ty, cg::Ty::Interface(name, generics, associated, _)
                    if name.bare_name() == "Marker"
                        && generics.is_empty()
                        && associated.is_empty()),
                "interface identity should survive in shared codegen IR: {ty:?}"
            );
        }
    }

    #[test]
    fn aliases_keep_identity_chains_and_canonical_targets_in_codegen() {
        let root = Path::new("/tmp/codegen_alias_identity");
        let mut db = ProjectDatabase::new();
        db.workspace(root);
        db.file(
            root.join("main.baml").as_path(),
            r#"
type Text = string
type TextChain = Text
type MaybeText = null | TextChain | null
type Rec = int | Rec[]

class Holder {
  direct Text
  chain TextChain
  maybe MaybeText
  nested MaybeText[]
  mapped map<string, TextChain[]>
}

function normalize(value: null | string | null) -> null | string | null { value }
"#,
        );

        let diagnostics = baml_db::collect_compiler2_diagnostics(&db);
        assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:#?}");

        let pool = build_symbol_pool(&db);
        let name = |name: &str| cg::Name::new(Name::new("user"), vec![], Name::new(name));
        let text = name("Text");
        let text_chain = name("TextChain");
        let maybe_text = name("MaybeText");
        let rec = name("Rec");

        let cg::Symbol::TypeAlias(text_decl) = &pool[&text] else {
            panic!("Text must be an alias")
        };
        assert_eq!(
            text_decl.resolves_to,
            cg::Ty::String {
                attr: codegen_attr()
            }
        );

        let cg::Symbol::TypeAlias(chain_decl) = &pool[&text_chain] else {
            panic!("TextChain must be an alias")
        };
        assert_eq!(chain_decl.resolves_to, codegen_alias(text.clone()));

        let cg::Symbol::TypeAlias(maybe_decl) = &pool[&maybe_text] else {
            panic!("MaybeText must be an alias")
        };
        assert_eq!(
            maybe_decl.resolves_to,
            codegen_union(vec![
                codegen_alias(text_chain.clone()),
                cg::Ty::Null {
                    attr: codegen_attr()
                }
            ])
        );

        let cg::Symbol::TypeAlias(rec_decl) = &pool[&rec] else {
            panic!("Rec must be an alias")
        };
        assert!(rec_decl.recursive);
        assert_eq!(
            rec_decl.resolves_to,
            codegen_union(vec![
                cg::Ty::Int {
                    attr: codegen_attr()
                },
                codegen_list(codegen_alias(rec.clone())),
            ])
        );

        let cg::Symbol::Class(holder) = &pool[&name("Holder")] else {
            panic!("Holder must be a class")
        };
        let property = |property_name: &str| {
            &holder
                .properties
                .iter()
                .find(|property| property.name.as_str() == property_name)
                .unwrap_or_else(|| panic!("missing Holder.{property_name}"))
                .ty
        };
        assert_eq!(property("direct"), &codegen_alias(text));
        assert_eq!(property("chain"), &codegen_alias(text_chain.clone()));
        assert_eq!(property("maybe"), &codegen_alias(maybe_text.clone()));
        assert_eq!(property("nested"), &codegen_list(codegen_alias(maybe_text)));
        assert_eq!(
            property("mapped"),
            &cg::Ty::Map {
                key: Box::new(cg::Ty::String {
                    attr: codegen_attr()
                }),
                value: Box::new(codegen_list(codegen_alias(text_chain))),
                attr: codegen_attr(),
            }
        );

        let cg::Symbol::Function(normalize) = &pool[&name("normalize")] else {
            panic!("normalize must be a function")
        };
        let nullable_string = codegen_union(vec![
            cg::Ty::String {
                attr: codegen_attr(),
            },
            cg::Ty::Null {
                attr: codegen_attr(),
            },
        ]);
        assert_eq!(normalize.arguments[0].ty, nullable_string);
        assert_eq!(normalize.return_type, nullable_string);
        cg::validate_symbol_pool_map_keys(&pool).expect("canonical alias map keys must validate");
    }

    #[test]
    fn aliased_map_keys_are_checked_through_resolved_targets() {
        use baml_compiler_diagnostics::diagnostic::DiagnosticId;

        let legal_root = Path::new("/tmp/codegen_legal_alias_map_key");
        let mut legal_db = ProjectDatabase::new();
        legal_db.workspace(legal_root);
        legal_db.file(
            legal_root.join("main.baml").as_path(),
            "type Key = \"first\" | \"second\"\ntype KeyChain = Key\nclass Lookup { values map<KeyChain, int> }\n",
        );
        let diagnostics = baml_db::collect_compiler2_diagnostics(&legal_db);
        assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:#?}");
        let legal_pool = build_symbol_pool(&legal_db);
        let lookup_name = cg::Name::new(Name::new("user"), vec![], Name::new("Lookup"));
        let key_chain = cg::Name::new(Name::new("user"), vec![], Name::new("KeyChain"));
        let cg::Symbol::Class(lookup) = &legal_pool[&lookup_name] else {
            panic!("Lookup must be a class")
        };
        assert_eq!(
            lookup.properties[0].ty,
            cg::Ty::Map {
                key: Box::new(codegen_alias(key_chain)),
                value: Box::new(cg::Ty::Int {
                    attr: codegen_attr()
                }),
                attr: codegen_attr(),
            }
        );
        cg::validate_symbol_pool_map_keys(&legal_pool)
            .expect("string-denoting alias key must validate");

        let illegal_root = Path::new("/tmp/codegen_illegal_alias_map_key");
        let mut illegal_db = ProjectDatabase::new();
        illegal_db.workspace(illegal_root);
        illegal_db.file(
            illegal_root.join("main.baml").as_path(),
            "type BadKey = int\nclass Lookup { values map<BadKey, string> }\n",
        );
        let diagnostics = baml_db::collect_compiler2_diagnostics(&illegal_db);
        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| diagnostic.id == DiagnosticId::InvalidMapKeyType),
            "the compiler must reject an int-denoting alias map key: {diagnostics:#?}"
        );
        let illegal_pool = build_symbol_pool(&illegal_db);
        assert!(matches!(
            cg::validate_symbol_pool_map_keys(&illegal_pool),
            Err(cg::CodegenTypeError::InvalidMapKey(key))
                if matches!(key.as_ref(), cg::Ty::TypeAlias(..))
        ));
    }

    #[test]
    fn same_alias_name_in_different_namespaces_stays_qualified() {
        let root = Path::new("/tmp/codegen_namespaced_alias_identity");
        let mut db = ProjectDatabase::new();
        db.workspace(root);
        db.file(
            root.join("ns_left/types.baml").as_path(),
            "type Shared = string\nclass Left { value Shared }\n",
        );
        db.file(
            root.join("ns_right/types.baml").as_path(),
            "type Shared = int\nclass Right { value Shared }\n",
        );

        let diagnostics = baml_db::collect_compiler2_diagnostics(&db);
        assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:#?}");
        let pool = build_symbol_pool(&db);
        let qualified = |namespace: &str, name: &str| {
            cg::Name::new(
                Name::new("user"),
                vec![Name::new(namespace)],
                Name::new(name),
            )
        };
        let left_alias = qualified("left", "Shared");
        let right_alias = qualified("right", "Shared");
        assert_ne!(left_alias, right_alias);
        assert!(matches!(
            &pool[&left_alias],
            cg::Symbol::TypeAlias(alias)
                if alias.resolves_to == cg::Ty::String { attr: codegen_attr() }
        ));
        assert!(matches!(
            &pool[&right_alias],
            cg::Symbol::TypeAlias(alias)
                if alias.resolves_to == cg::Ty::Int { attr: codegen_attr() }
        ));

        for (class_name, alias_name) in [
            (qualified("left", "Left"), left_alias),
            (qualified("right", "Right"), right_alias),
        ] {
            let cg::Symbol::Class(class) = &pool[&class_name] else {
                panic!("{class_name} must be a class")
            };
            assert_eq!(class.properties[0].ty, codegen_alias(alias_name));
        }
    }
}
