//! `check_file` — aggregate parse + HIR + TIR diagnostics for a single file.
//!
//! This is NOT a Salsa query — it is a regular function that calls cached
//! Salsa queries beneath it and aggregates their results into a
//! `Vec<Diagnostic>` ready for the LSP layer to convert into LSP types.
//!
//! ## Pipeline
//!
//! 1. **Parse errors** — via `baml_compiler_parser::parse_errors`. Always fast
//!    because parsing is Salsa-cached per file.
//! 2. **HIR2 diagnostics** — stored in `file_semantic_index(...).extra`. These
//!    cover duplicate field/variant/binding names found during scope tree
//!    construction.
//! 3. **TIR2 scope diagnostics** — via `render_scope_diagnostics(db, scope_id)`
//!    for each scope. These cover type mismatches, unresolved names, etc. in
//!    expression-body functions. Calls `infer_scope_types` (Salsa-cached per
//!    scope) internally.
//! 4. **TIR2 structural diagnostics** — type errors in class field annotations
//!    and type alias bodies, via `resolve_class_fields` and `resolve_type_alias`
//!    (both Salsa-cached per item).

use std::{collections::HashSet, fmt::Write as _};

use baml_base::{FileId, Name, SourceFile, Span};
use baml_compiler_diagnostics::{Diagnostic, DiagnosticId, DiagnosticPhase, ToDiagnostic};
use baml_compiler2_hir::{body::FunctionBody, file_semantic_index, scope::ScopeKind};
use baml_compiler2_tir::{
    infer_context::{DiagnosticLocation, TirTypeError},
    inference::render_scope_diagnostics,
    ty::{QualifiedTypeName, Ty, TyAttr},
};
use indexmap::IndexMap;
use text_size::{TextRange, TextSize};

use crate::Db;

/// Collect all compiler2 diagnostics for a file (parse + HIR2 + TIR2).
///
/// Returns a flat `Vec<Diagnostic>` in source order (parse first, then HIR,
/// then TIR). The LSP layer converts these to `lsp_types::Diagnostic` values.
///
/// This is a regular function, not a Salsa query. Caching happens at the
/// underlying query layers (parsing, HIR indexing, type inference).
pub fn check_file(db: &dyn Db, file: SourceFile) -> Vec<Diagnostic> {
    let file_id = file.file_id(db);
    let mut diagnostics: Vec<Diagnostic> = Vec::new();

    // ── 1. Parse errors ───────────────────────────────────────────────────────
    //
    // `parse_errors` is Salsa-cached per file. Calling it here is cheap after
    // the first call for a given file revision.
    let parse_errors = baml_compiler_parser::parse_errors(db, file);
    for err in &parse_errors {
        diagnostics.push(err.to_diagnostic());
    }

    // ── 2. HIR2 diagnostics ───────────────────────────────────────────────────
    //
    // `file_semantic_index` is Salsa-tracked with `no_eq` (re-runs on every
    // file change). HIR2 diagnostics live in the optional `extra` box — we only
    // pay for iteration when there are diagnostics.
    let index = file_semantic_index(db, file);
    if let Some(extra) = &index.extra {
        // 2a. Lowering diagnostics (CST → AST structural errors)
        for ld in &extra.lowering_diagnostics {
            diagnostics.push(ld.to_diagnostic(file_id));
        }
        // 2b. HIR2 semantic diagnostics (duplicate definitions, etc.)
        for hir_diag in &extra.diagnostics {
            diagnostics.push(hir_diag.to_diagnostic(file_id));
        }
    }

    // ── 3. TIR2 scope diagnostics ─────────────────────────────────────────────
    //
    // `render_scope_diagnostics` calls `infer_scope_types(db, scope_id)` (Salsa-
    // cached per scope) and resolves the arena IDs in each diagnostic to source
    // `TextRange` values via the function body's `AstSourceMap`.
    for scope_id in &index.scope_ids {
        let rendered = render_scope_diagnostics(db, *scope_id);
        for r in rendered {
            diagnostics.push(tir_rendered_to_diagnostic_for_file(db, file, r));
        }
    }

    // ── 4. TIR2 structural diagnostics ───────────────────────────────────────
    //
    // Type errors in class field annotations and type alias bodies. These are
    // produced by `resolve_class_fields` and `resolve_type_alias` (both Salsa-
    // cached per item), which already store `TextRange` in their diagnostics —
    // no source map lookup needed here.
    for (_name, contrib) in &index.symbol_contributions.types {
        use baml_compiler2_hir::contributions::Definition;
        match contrib.definition {
            Definition::Class(class_loc) => {
                let resolved = baml_compiler2_tir::inference::resolve_class_fields(db, class_loc);
                for (error, span) in &resolved.diagnostics {
                    diagnostics.push(
                        Diagnostic::error(
                            tir_type_error_to_diagnostic_id(error),
                            source_aware_tir_type_error_message(db, file, error),
                        )
                        .with_primary_span(Span {
                            file_id,
                            range: *span,
                        })
                        .with_phase(DiagnosticPhase::Type),
                    );
                }
            }
            Definition::TypeAlias(alias_loc) => {
                let resolved = baml_compiler2_tir::inference::resolve_type_alias(db, alias_loc);
                for (error, span) in &resolved.diagnostics {
                    diagnostics.push(
                        Diagnostic::error(
                            tir_type_error_to_diagnostic_id(error),
                            source_aware_tir_type_error_message(db, file, error),
                        )
                        .with_primary_span(Span {
                            file_id,
                            range: *span,
                        })
                        .with_phase(DiagnosticPhase::Type),
                    );
                }
            }
            _ => {}
        }
    }

    let item_tree = baml_compiler2_hir::file_item_tree(db, file);
    let pkg_info = baml_compiler2_hir::file_package::file_package(db, file);
    let pkg_id = baml_compiler2_hir::package::PackageId::new(db, pkg_info.package.clone());
    let res_ctx = baml_compiler2_tir::package_interface::package_resolution_context(db, pkg_id);
    let pkg_items = &res_ctx.own_items;
    let aliases = collect_type_aliases_for_resolution_context(db, res_ctx);
    let ast_items = {
        let tree = baml_compiler_parser::syntax_tree(db, file);
        let (items, _, _) = baml_compiler2_ast::lower_file(&tree);
        items
    };
    diagnostics.extend(validate_associated_type_bindings_in_items(
        db,
        file_id,
        &ast_items,
        pkg_items,
        &pkg_info.namespace_path,
        &aliases,
    ));

    // ── 5. Jinja prompt/template diagnostics ────────────────────────────────
    //
    // Declarative LLM prompts and template_strings are MiniJinja templates, not
    // regular expression bodies. Validate them with the shared MiniJinja AST
    // type checker so prompt diagnostics match runtime template semantics.
    let source_text = file.text(db);
    diagnostics.extend(check_jinja_templates(
        db,
        file_id,
        &item_tree,
        pkg_items,
        &pkg_info.namespace_path,
        source_text,
    ));

    // ── 6. Function signature diagnostics ────────────────────────────────────
    //
    // Build a method → enclosing class list so we can merge class generic params.
    let mut method_to_class = Vec::new();
    for (class_id, class_data) in &item_tree.classes {
        for &method_id in &class_data.methods {
            method_to_class.push((method_id, *class_id));
        }
    }
    // Out-of-body `implement Interface for Type` methods: their `Self` resolves
    // to the `for` target and the block's generic params are in scope. Bodied
    // (`$rust_function`/builtin) impl methods skip the scope-inference path, so
    // without this their signatures would leave `Self` unresolved here.
    let mut method_to_impl = Vec::new();
    for imp in &item_tree.implements_for {
        for &method_id in &imp.methods {
            method_to_impl.push((method_id, imp));
        }
    }

    for (local_id, func_data) in &item_tree.functions {
        let func_loc = baml_compiler2_hir::loc::FunctionLoc::new(db, file, *local_id);
        let body = baml_compiler2_hir::body::function_body(db, func_loc);

        // Expression-body functions already have their signatures checked
        // during scope inference (step 3). Only check non-expr bodies here.
        if matches!(body.as_ref(), FunctionBody::Expr(_)) {
            continue;
        }

        let sig = baml_compiler2_hir::signature::function_signature(db, func_loc);
        let mut type_errors = Vec::new();
        let mut param_types = Vec::new();

        // Compute the effective generic params: method params + enclosing class params.
        let mut generic_params = func_data.generic_params.clone();
        let enclosing_class_id = method_to_class
            .iter()
            .find(|(mid, _)| mid == local_id)
            .map(|(_, class_id)| *class_id);
        if let Some(class_id) = enclosing_class_id {
            let class_data = &item_tree[class_id];
            // Prepend class generic params (class params come first, method params after)
            let mut merged = class_data.generic_params.clone();
            merged.extend(generic_params);
            generic_params = merged;
        }
        // BEP-044: inside an out-of-body `implement Interface for Type` block,
        // `Self` is the `for` target and the block's generic params are in scope.
        let enclosing_impl = method_to_impl
            .iter()
            .find(|(mid, _)| mid == local_id)
            .map(|(_, imp)| *imp);
        if let Some(imp) = enclosing_impl {
            let mut merged = imp.generic_params.clone();
            merged.extend(generic_params);
            generic_params = merged;
        }
        // Pre-resolve `Self` to the enclosing impl's `for` target before lowering
        // signature types, mirroring the body path in `tir::inference`.
        let self_replacement = enclosing_impl.map(|imp| imp.for_target.expr.clone());
        let lower_sig_te = |te: &baml_compiler2_ast::TypeExpr,
                            generic_params: &[Name],
                            diags: &mut Vec<baml_compiler2_tir::infer_context::TirTypeError>|
         -> Ty {
            let resolved = match &self_replacement {
                Some(replacement) => {
                    baml_compiler2_tir::lower_type_expr::substitute_self_in(te, replacement)
                }
                None => te.clone(),
            };
            baml_compiler2_tir::lower_type_expr::lower_type_expr_in_ns(
                db,
                &resolved,
                pkg_items,
                &pkg_info.namespace_path,
                generic_params,
                diags,
            )
        };

        // Check return type — use the span from the item tree's SpannedTypeExpr.
        if let Some(ret_te) = &sig.return_type {
            lower_sig_te(ret_te, &generic_params, &mut type_errors);
            if !type_errors.is_empty() {
                if let Some(ret_spanned) = &func_data.return_type {
                    for error in type_errors.drain(..) {
                        diagnostics.push(
                            Diagnostic::error(
                                tir_type_error_to_diagnostic_id(&error),
                                error.to_string(),
                            )
                            .with_primary_span(Span {
                                file_id,
                                range: ret_spanned.span,
                            })
                            .with_phase(DiagnosticPhase::Type),
                        );
                    }
                }
            }
        }

        // Check parameter types — use the type_expr span, not the whole param span.
        for (i, param) in sig.params.iter().enumerate() {
            type_errors.clear();
            let param_ty = if param.name.as_str() == "self"
                && matches!(param.ty, baml_compiler2_ast::TypeExpr::Unknown { .. })
            {
                // `self`'s type is the enclosing receiver: the class for an in-body
                // method, or the impl's `for` target for an out-of-body
                // `implement I for C` method (mirroring the body path in
                // `tir::inference`). Falling back to `Unknown` would otherwise
                // leave `self` untyped in the latter case.
                if let Some(class_id) = enclosing_class_id.as_ref() {
                    let class_data = &item_tree[*class_id];
                    pkg_items
                        .lookup_type(&pkg_info.namespace_path, &class_data.name)
                        .map(|def| {
                            // Carry the class's generic params as TypeVars so `self`
                            // is `Class<T..>`, not bare `Class` — mirrors the body
                            // path in `tir::inference`. A bare `self` leaks an
                            // unparameterized receiver into generic-class method
                            // bodies (e.g. the auto-derived `to_json`'s
                            // `to_string<Self>(self)`).
                            let class_args: Vec<Ty> = class_data
                                .generic_params
                                .iter()
                                .map(|p| Ty::TypeVar(p.clone(), TyAttr::default()))
                                .collect();
                            Ty::Class(
                                baml_compiler2_tir::lower_type_expr::qualify_def(
                                    db,
                                    def,
                                    &class_data.name,
                                ),
                                class_args,
                                TyAttr::default(),
                            )
                        })
                        .unwrap_or(Ty::Unknown {
                            attr: TyAttr::default(),
                        })
                } else if let Some(imp) = enclosing_impl {
                    baml_compiler2_tir::lower_type_expr::lower_type_expr_in_ns(
                        db,
                        &imp.for_target.expr,
                        pkg_items,
                        &pkg_info.namespace_path,
                        &generic_params,
                        &mut type_errors,
                    )
                } else {
                    Ty::Unknown {
                        attr: TyAttr::default(),
                    }
                }
            } else {
                lower_sig_te(&param.ty, &generic_params, &mut type_errors)
            };
            if !type_errors.is_empty() {
                if let Some(param) = func_data.params.get(i) {
                    if let Some(type_spanned) = &param.type_expr {
                        for error in type_errors.drain(..) {
                            diagnostics.push(
                                Diagnostic::error(
                                    tir_type_error_to_diagnostic_id(&error),
                                    error.to_string(),
                                )
                                .with_primary_span(Span {
                                    file_id,
                                    range: type_spanned.span,
                                })
                                .with_phase(DiagnosticPhase::Type),
                            );
                        }
                    }
                }
            }
            param_types.push((param.name.clone(), param_ty));
        }

        if let Some(scope_id) = function_scope_id(index, func_data) {
            let context = baml_compiler2_tir::infer_context::InferContext::new(db, scope_id);
            let mut builder = baml_compiler2_tir::builder::TypeInferenceBuilder::new(
                context,
                res_ctx,
                pkg_id,
                scope_id,
                aliases.clone(),
            );
            builder.set_generic_params(generic_params);
            for (name, ty) in &param_types {
                builder.add_local(name.clone(), ty.clone());
                builder.param_types.push((name.clone(), ty.clone()));
            }
            let parameter_defaults =
                baml_compiler2_hir::signature::function_parameter_defaults(db, func_loc);
            builder.check_function_parameter_defaults(
                &func_data.params,
                &parameter_defaults,
                &param_types,
            );

            let (
                _expressions,
                _pattern_types,
                _resolutions,
                _catch_residual_throws,
                _exhaustive_matches,
                type_check_diagnostics,
                _path_root_types,
                _path_segment_types,
                _path_member_resolutions,
                _param_types,
                _call_plans,
                _call_type_instantiations,
                _function_coercions,
                _call_throws,
                _template_body_params,
                _default_parameter_inference,
            ) = builder.finish();
            for tir_diag in type_check_diagnostics.diagnostics {
                if !is_function_default_signature_diagnostic(&tir_diag) {
                    continue;
                }
                diagnostics.push(tir_rendered_to_diagnostic_for_file(
                    db,
                    file,
                    tir_diag.render(db, file, None),
                ));
            }
        }
    }

    // ── 7. Interface validation (BEP-044) ────────────────────────────────────
    //
    // Structural / semantic checks for `interface I { ... }` declarations and
    // `implements I { ... }` blocks. Runs over the AST and the package items
    // so cross-file interface references work.
    diagnostics.extend(check_interfaces(
        db,
        file,
        file_id,
        &ast_items,
        pkg_items,
        &pkg_info.namespace_path,
        &aliases,
    ));

    // Deduplicate: multiple steps can produce the same diagnostic (e.g. scope
    // inference + signature validation for the same unresolved return type).
    diagnostics.dedup_by(|a, b| {
        a.code() == b.code() && a.message == b.message && a.primary_span() == b.primary_span()
    });

    diagnostics
}

/// Validate `interface` and `implements` blocks for a single file.
///
/// Diagnostics emitted here cover:
/// - Cycle in interface `extends`.
/// - `implements I {}` references an unknown interface.
/// - Duplicate `implements I` blocks on the same class.
/// - Missing implementations of required interface methods.
/// - Method bodies in `implements I {}` that name something I doesn't declare.
/// - Field type mismatches between class and interface.
/// - Two interfaces requiring the same field with conflicting types.
fn check_interfaces<'db>(
    db: &'db dyn Db,
    file: SourceFile,
    file_id: FileId,
    items: &[baml_compiler2_ast::Item],
    pkg_items: &baml_compiler2_hir::package::PackageItems<'db>,
    namespace_path: &[Name],
    aliases: &std::collections::HashMap<QualifiedTypeName, Ty>,
) -> Vec<Diagnostic> {
    use baml_compiler2_hir::diagnostic::Hir2Diagnostic;

    let mut diagnostics = Vec::new();

    // The package these impls live in — the orphan rule's notion of "local".
    let current_package = baml_compiler2_hir::file_package::file_package(db, file).package;

    // Detect direct + transitive cycles in interface `extends`.
    for item in items {
        if let baml_compiler2_ast::Item::Interface(iface) = item
            && let Some(chain) = interface_has_cycle(db, iface, pkg_items, namespace_path)
        {
            diagnostics.push(
                Hir2Diagnostic::InterfaceExtendsCycle {
                    chain,
                    span: iface.name_span,
                }
                .to_diagnostic(file_id),
            );
        }
    }

    // Detect field conflicts in interface `extends` chains.
    for item in items {
        if let baml_compiler2_ast::Item::Interface(iface) = item
            && !iface.requires.is_empty()
            && interface_has_cycle(db, iface, pkg_items, namespace_path).is_none()
        {
            validate_interface_extends_fields(
                &InterfaceValidationCtx {
                    db,
                    pkg_items,
                    namespace_path,
                    aliases,
                },
                file,
                file_id,
                iface,
                &mut diagnostics,
            );
        }
    }

    // E0133: an interface can only `requires` other interfaces. A non-interface
    // target (class, enum, or unknown) is rejected at the `requires` clause
    // itself — not deferred to an implementor's `implements` site.
    for item in items {
        if let baml_compiler2_ast::Item::Interface(iface) = item
            && interface_has_cycle(db, iface, pkg_items, namespace_path).is_none()
        {
            for parent_te in &iface.requires {
                if resolve_interface_path(db, &parent_te.expr, pkg_items, namespace_path).is_none()
                {
                    // BEP-044 wf3 #19: echo the user-written name verbatim
                    // (e.g. `root.a.Ghost`) rather than stripping to the bare
                    // leaf — matches how `implements` renders its target.
                    let target_name = format!("{}", parent_te.expr);
                    let interface_name = Name::new(
                        interface_qtn_for_file(db, file, &iface.name).render_user_facing(),
                    );
                    // Distinguish "name exists but isn't an interface" (E0133)
                    // from "name doesn't exist at all" (E0112), mirroring the
                    // `implements` path. Without this, a `requires` on an
                    // unknown name wrongly claims the name "is not an interface".
                    let diag = if is_non_interface_type(&parent_te.expr, pkg_items, namespace_path)
                    {
                        Hir2Diagnostic::InterfaceRequiresNonInterface {
                            interface_name,
                            target_name,
                            span: parent_te.span,
                        }
                    } else {
                        Hir2Diagnostic::UnknownRequiredInterface {
                            interface_name,
                            target_name,
                            span: parent_te.span,
                        }
                    };
                    diagnostics.push(diag.to_diagnostic(file_id));
                }
            }
        }
    }

    // BEP-044 wf3 #G17 [decision O1]: `Self` is only valid in method-signature
    // positions (params / return / throws). In an interface FIELD type it is
    // rejected here with one clear diagnostic — historically it produced a
    // contradictory cascade (E0116 demanding the class field be `Self?`, then
    // E0002 `unresolved type: Self` when the user wrote `Self?`). Recursive
    // fields should use the interface's own name instead.
    for item in items {
        if let baml_compiler2_ast::Item::Interface(iface) = item {
            for field in &iface.fields {
                if let Some(te) = &field.type_expr
                    && type_expr_contains_self(&te.expr)
                {
                    diagnostics.push(
                        Diagnostic::error(
                            DiagnosticId::SelfInInterfaceField,
                            format!(
                                "`Self` is not allowed in an interface field type; field `{}` of \
                                 interface `{}` must use a concrete type (use the interface's own \
                                 name for recursion)",
                                field.name, iface.name
                            ),
                        )
                        .with_primary_span(Span {
                            file_id,
                            range: te.span,
                        })
                        .with_phase(DiagnosticPhase::Type),
                    );
                }
            }
        }
    }

    for item in items {
        if let baml_compiler2_ast::Item::Interface(iface) = item {
            validate_associated_type_default_bounds(
                db,
                file_id,
                iface,
                pkg_items,
                namespace_path,
                aliases,
                &mut diagnostics,
            );
        }
    }

    for item in items {
        if let baml_compiler2_ast::Item::Class(class) = item {
            validate_class_implements(
                db,
                file_id,
                class,
                pkg_items,
                namespace_path,
                aliases,
                &mut diagnostics,
            );
        }
    }

    for item in items {
        if let baml_compiler2_ast::Item::ImplementsFor(imp) = item {
            validate_implements_for(
                db,
                file_id,
                &current_package,
                imp,
                items,
                pkg_items,
                namespace_path,
                aliases,
                &mut diagnostics,
            );
        }
    }

    // Interface coherence (overlap, E0132) is a per-package property over the
    // whole dependency closure, not a per-file one. Compute it once for the
    // package and surface the violations whose offending impl lives in this
    // file (its conflicting partner may be in another file or a dependency).
    let pkg_id = baml_compiler2_hir::package::PackageId::new(db, current_package.clone());
    for violation in baml_compiler2_tir::interfaces::package_coherence_diagnostics(db, pkg_id) {
        // Anchor the error on whichever conflicting impl lives in *this* file, pointing
        // at its partner. A cross-file pair is reported once per file (each anchored on
        // its own impl), so neither offending file is left unmarked — checking only the
        // `primary` side left the file holding the `secondary` impl looking clean.
        let (primary, secondary) = if violation.primary.file_id == file_id {
            (violation.primary, violation.secondary)
        } else if violation.secondary.file_id == file_id {
            (violation.secondary, violation.primary)
        } else {
            continue;
        };
        // A definite overlap and an undecidable one are both rejected (overlap
        // can't be ruled out, and the resolver assumes ≤1 impl), but the message
        // distinguishes them so the user knows whether it's a proven conflict or a
        // type too complex to analyze.
        let message: &str = if violation.indeterminate {
            "these interface implementations are too complex to prove disjoint; \
             simplify the types involved so coherence can be decided"
        } else {
            "overlapping interface implementations for the same receiver/interface"
        };
        diagnostics.push(
            Diagnostic::error(DiagnosticId::OverlappingImplements, message)
                .with_primary_span(primary)
                .with_secondary(secondary, "conflicting implementation is here")
                .with_phase(DiagnosticPhase::Type),
        );
    }

    // E0145 + impl-resolution lowering errors: `impl_data` owns these (the
    // single compiler-side source); check.rs only surfaces them, mapping each
    // diagnostic's span-free origin to a precise source range.
    {
        use baml_compiler2_tir::interfaces::ImplDiagnosticLocation;
        let item_tree = baml_compiler2_hir::file_item_tree(db, file);
        for impl_id in item_tree.impls.keys() {
            let impl_loc = baml_compiler2_hir::loc::ImplLoc::new(db, file, *impl_id);
            let Ok(data) = baml_compiler2_tir::interfaces::impl_data(db, impl_loc).as_ref() else {
                continue;
            };
            if data.diagnostics.is_empty() {
                continue;
            }
            let Some(sm) =
                baml_compiler2_tir::interfaces::impl_data_source_map(db, impl_loc).as_ref()
            else {
                continue;
            };
            for (error, loc) in &data.diagnostics {
                let span = match loc {
                    ImplDiagnosticLocation::InterfaceTarget => sm.interface_target_span,
                    ImplDiagnosticLocation::ForTarget => sm.for_target_span.unwrap_or(sm.impl_span),
                    ImplDiagnosticLocation::Bound => sm.impl_span,
                };
                diagnostics.push(
                    Diagnostic::error(tir_type_error_to_diagnostic_id(error), error.to_string())
                        .with_primary_span(span)
                        .with_phase(DiagnosticPhase::Type),
                );
            }
        }
    }

    for item in items {
        if let baml_compiler2_ast::Item::Interface(iface) = item {
            for method in &iface.default_methods {
                if let Some(ret) = &method.return_type
                    && type_expr_contains_self(&ret.expr)
                {
                    diagnostics.push(
                        Diagnostic::error(
                            DiagnosticId::TypeMismatch,
                            format!(
                                "default method `{}` on interface `{}` cannot return `Self`",
                                method.name, iface.name
                            ),
                        )
                        .with_primary(
                            Span {
                                file_id,
                                range: ret.span,
                            },
                            "`Self` return type is not allowed on interface default methods",
                        )
                        .with_phase(DiagnosticPhase::Hir),
                    );
                }
            }
        }
    }

    diagnostics
}

/// Resolve a `TypeExpr::Path` to an interface, by name, walking the package.
///
/// Returns `None` if the path doesn't name an interface (including: name
/// doesn't exist, or resolves to a class/enum/etc.).
#[derive(Debug, Clone)]
struct ResolvedInterfaceData<'db> {
    loc: baml_compiler2_hir::loc::InterfaceLoc<'db>,
    iface: baml_compiler2_hir::item_tree::Interface,
    qtn: QualifiedTypeName,
}

impl ResolvedInterfaceData<'_> {
    fn display_name(&self) -> Name {
        Name::new(self.qtn.render_user_facing())
    }
}

fn resolve_interface_path<'db>(
    db: &'db dyn Db,
    target: &baml_compiler2_ast::TypeExpr,
    pkg_items: &baml_compiler2_hir::package::PackageItems<'db>,
    namespace_path: &[Name],
) -> Option<ResolvedInterfaceData<'db>> {
    let resolved = baml_compiler2_tir::interfaces::resolve_path_to_interface_identity(
        db,
        target,
        pkg_items,
        namespace_path,
    )?;
    let item_tree = baml_compiler2_hir::file_item_tree(db, resolved.loc.file(db));
    let iface = item_tree.interfaces.get(&resolved.loc.id(db))?.clone();
    Some(ResolvedInterfaceData {
        loc: resolved.loc,
        iface,
        qtn: resolved.qtn,
    })
}

fn path_lookup_namespace<'a>(head: &'a [Name], namespace_path: &'a [Name]) -> &'a [Name] {
    if head.is_empty() {
        namespace_path
    } else if head
        .first()
        .is_some_and(|segment| segment.as_str() == "root")
    {
        &head[1..]
    } else {
        head
    }
}

/// Returns the cycle path `A -> B -> … -> A` (interface simple names) when
/// `iface`'s transitive `requires` closure loops back to itself, or `None` when
/// there is no cycle. The full path (rather than a bool) lets E0118 report the
/// actionable chain instead of a single node (BEP-044 wf3 #G12).
fn interface_has_cycle<'db>(
    db: &'db dyn Db,
    iface: &baml_compiler2_ast::InterfaceDef,
    pkg_items: &baml_compiler2_hir::package::PackageItems<'db>,
    namespace_path: &[Name],
) -> Option<Vec<Name>> {
    use baml_compiler2_ast::TypeExpr;

    let self_probe = TypeExpr::Path {
        segments: vec![iface.name.clone()],
        generic_args: Vec::new(),
        associated_type_bindings: Vec::new(),
        attrs: Vec::new(),
    };
    let self_loc =
        resolve_interface_path(db, &self_probe, pkg_items, namespace_path).map(|r| r.loc);
    // Frontier item: (segments to probe, name-chain leading here from `iface`).
    let leaf = |segments: &[Name]| segments.last().cloned().unwrap_or_default();
    let mut frontier: Vec<(Vec<Name>, Vec<Name>)> = iface
        .requires
        .iter()
        .filter_map(|p| match &p.expr {
            TypeExpr::Path { segments, .. } if !segments.is_empty() => {
                Some((segments.clone(), vec![iface.name.clone(), leaf(segments)]))
            }
            _ => None,
        })
        .collect();
    let mut visited = HashSet::new();
    while let Some((path, chain)) = frontier.pop() {
        if !visited.insert(path.clone()) {
            continue;
        }
        let probe = TypeExpr::Path {
            segments: path,
            generic_args: Vec::new(),
            associated_type_bindings: Vec::new(),
            attrs: Vec::new(),
        };
        if let Some(parent) = resolve_interface_path(db, &probe, pkg_items, namespace_path) {
            if self_loc.as_ref().is_some_and(|loc| *loc == parent.loc) {
                return Some(chain);
            }
            for parent in &parent.iface.requires {
                if let TypeExpr::Path { segments, .. } = &parent.expr
                    && !segments.is_empty()
                {
                    let mut next_chain = chain.clone();
                    next_chain.push(leaf(segments));
                    frontier.push((segments.clone(), next_chain));
                }
            }
        }
    }
    None
}

/// A method signature in canonical string form, used by the interface
/// validator to compare class method overrides against the interface's
/// declared signature.
#[derive(Debug, Clone, PartialEq, Eq)]
struct MethodSignature {
    /// Generic type parameters local to this method, in declaration order.
    generic_params: Vec<Name>,
    /// Rendered generic bounds parallel to `generic_params`.
    generic_param_bounds: Vec<Option<String>>,
    /// `(name, type)` pairs in declaration order. `self` is excluded — its
    /// type is the implementing class and so trivially matches.
    params: Vec<(Name, String)>,
    /// Rendered return type, or `"<unspecified>"` when missing.
    return_type: String,
    /// Rendered declared throws type. `None` means the signature did not
    /// declare `throws`; interface implementations must preserve that spelling.
    throws: Option<String>,
    /// Substituted `TypeExpr`s parallel to `params`/`return_type`/`throws`, kept
    /// so signature matching can compare *semantically* (union-order- and
    /// alias-insensitive) rather than by rendered string (BEP-044 wf3 G8). A
    /// `None` param entry means the param had no declared type. Excluded from
    /// `PartialEq` — equality stays string-based for any incidental use; the
    /// semantic check lives in [`MethodSignature::matches`].
    param_types: Vec<Option<baml_compiler2_ast::TypeExpr>>,
    return_te: Option<baml_compiler2_ast::TypeExpr>,
    throws_te: Option<baml_compiler2_ast::TypeExpr>,
}

#[derive(Clone, Copy)]
struct SignatureMatchContext<'a, 'db> {
    db: &'a dyn Db,
    expected_pkg_items: &'a baml_compiler2_hir::package::PackageItems<'db>,
    expected_namespace_path: &'a [Name],
    actual_pkg_items: &'a baml_compiler2_hir::package::PackageItems<'db>,
    actual_namespace_path: &'a [Name],
    aliases: &'a std::collections::HashMap<QualifiedTypeName, Ty>,
    ignore_param_names: bool,
    /// Generic params of the enclosing class / `implements` target. Method
    /// signatures in an impl may reference them (e.g. `type Item = T` on a
    /// generic implementor), so semantic lowering must treat them as type
    /// vars on both sides — otherwise lowering fails and the comparison
    /// degrades to brittle string equality.
    outer_generic_params: &'a [Name],
}

#[derive(Clone, Copy)]
struct InterfaceRequiresQuery<'a> {
    db: &'a dyn Db,
    sub_qtn: &'a QualifiedTypeName,
    sub_args: &'a [Ty],
    sub_assoc: &'a [(Name, Ty)],
    sup_qtn: &'a QualifiedTypeName,
    sup_args: &'a [Ty],
    sup_assoc: &'a [(Name, Ty)],
    aliases: &'a std::collections::HashMap<QualifiedTypeName, Ty>,
}

impl MethodSignature {
    /// Build a signature after substituting generic and associated-type
    /// references in the param/return/throws types using `subst`.
    fn from_params_and_return_with_subst(
        generic_params: &[Name],
        generic_param_bounds: &[Option<baml_compiler2_ast::TypeExpr>],
        params: &[baml_compiler2_ast::Param],
        return_type: Option<&baml_compiler2_ast::SpannedTypeExpr>,
        throws: Option<&baml_compiler2_ast::SpannedTypeExpr>,
        subst: &std::collections::HashMap<Name, baml_compiler2_ast::TypeExpr>,
    ) -> Self {
        let scoped_subst = if generic_params.is_empty() {
            subst.clone()
        } else {
            let mut scoped = subst.clone();
            for param in generic_params {
                scoped.remove(param);
            }
            scoped
        };
        // Capture the original AST inputs before the string-rendering shadows
        // `params`/`return_type`/`throws` below.
        let orig_params = params;
        let orig_return = return_type;
        let orig_throws = throws;
        let generic_param_bounds = generic_param_bounds
            .iter()
            .map(|bound| {
                bound
                    .as_ref()
                    .map(|bound| substitute_type_vars(bound, &scoped_subst).to_string())
            })
            .collect();
        let params = params
            .iter()
            .filter(|p| p.name.as_str() != "self")
            .map(|p| {
                let ty_str = p
                    .type_expr
                    .as_ref()
                    .map(|te| substitute_type_vars(&te.expr, &scoped_subst).to_string())
                    .unwrap_or_else(|| "<unknown>".to_string());
                (p.name.clone(), ty_str)
            })
            .collect();
        let return_type = return_type
            .map(|te| substitute_type_vars(&te.expr, &scoped_subst).to_string())
            .unwrap_or_else(|| "<unspecified>".to_string());
        let throws = throws.map(|te| substitute_type_vars(&te.expr, &scoped_subst).to_string());
        // Keep the substituted TypeExprs for semantic (not string) matching.
        let param_types = orig_params
            .iter()
            .filter(|p| p.name.as_str() != "self")
            .map(|p| {
                p.type_expr
                    .as_ref()
                    .map(|te| substitute_type_vars(&te.expr, &scoped_subst))
            })
            .collect();
        let return_te = orig_return.map(|te| substitute_type_vars(&te.expr, &scoped_subst));
        let throws_te = orig_throws.map(|te| substitute_type_vars(&te.expr, &scoped_subst));
        Self {
            generic_params: generic_params.to_vec(),
            generic_param_bounds,
            params,
            return_type,
            throws,
            param_types,
            return_te,
            throws_te,
        }
    }

    fn render(&self) -> String {
        let generic_params: Vec<String> = self
            .generic_params
            .iter()
            .enumerate()
            .map(|(idx, param)| {
                if let Some(Some(bound)) = self.generic_param_bounds.get(idx) {
                    format!("{param} extends {bound}")
                } else {
                    param.to_string()
                }
            })
            .collect();
        let ps: Vec<String> = self
            .params
            .iter()
            .map(|(n, t)| format!("{n}: {t}"))
            .collect();
        let generics = if generic_params.is_empty() {
            String::new()
        } else {
            format!("<{}>", generic_params.join(", "))
        };
        let mut rendered = format!("{generics}({}) -> {}", ps.join(", "), self.return_type);
        if let Some(throws) = &self.throws {
            write!(rendered, " throws {throws}").expect("writing to String cannot fail");
        }
        rendered
    }

    /// Semantic signature match for interface impls (BEP-044 wf3 G8). Compares
    /// each component (param, return, throws) by lowering both `TypeExpr`s in
    /// `namespace_path` and testing `is_same_normalized_type`, so union member
    /// order (`A | B` vs `B | A`) and type aliases (`Err` vs `IoError`) no
    /// longer cause false mismatches. Falls back to string equality per
    /// component when a type can't be lowered (via `type_exprs_compatible`).
    /// Generic params/bounds and `throws` presence stay exact (invariant).
    fn matches(&self, other: &Self, ctx: SignatureMatchContext<'_, '_>) -> bool {
        if self.generic_params != other.generic_params
            || self.generic_param_bounds != other.generic_param_bounds
            || self.params.len() != other.params.len()
        {
            return false;
        }
        let mut gp_all: Vec<Name> = self.generic_params.clone();
        for p in ctx.outer_generic_params {
            if !gp_all.contains(p) {
                gp_all.push(p.clone());
            }
        }
        let gp: &[Name] = &gp_all;
        let cmp = |a: &baml_compiler2_ast::TypeExpr, b: &baml_compiler2_ast::TypeExpr| {
            type_exprs_compatible(
                ctx.db,
                ctx.expected_pkg_items,
                ctx.expected_namespace_path,
                gp,
                a,
                ctx.actual_pkg_items,
                ctx.actual_namespace_path,
                gp,
                b,
                ctx.aliases,
            )
        };
        for (i, ((an, at), (bn, bt))) in self.params.iter().zip(&other.params).enumerate() {
            if !ctx.ignore_param_names && an != bn {
                return false;
            }
            match (
                self.param_types.get(i).and_then(|t| t.as_ref()),
                other.param_types.get(i).and_then(|t| t.as_ref()),
            ) {
                (Some(a), Some(b)) => {
                    if !cmp(a, b) {
                        return false;
                    }
                }
                _ => {
                    if at != bt {
                        return false;
                    }
                }
            }
        }
        match (&self.return_te, &other.return_te) {
            (Some(a), Some(b)) => {
                if !cmp(a, b) {
                    return false;
                }
            }
            (None, None) => {}
            _ => {
                if self.return_type != other.return_type {
                    return false;
                }
            }
        }
        // `throws` presence must match exactly (omitting it stays E0120); when
        // both declare it, `throws` is covariant — the impl (`other`) may
        // narrow the interface's (`self`) declared throws.
        match (&self.throws_te, &other.throws_te) {
            (Some(iface_throws), Some(impl_throws)) => {
                throws_covariant_compatible(ctx, gp, iface_throws, impl_throws)
            }
            (None, None) => true,
            _ => false,
        }
    }
}

/// Substitute generic parameter references in a `TypeExpr`. A single-segment
/// `Path` whose segment matches a key in `subst` is replaced with the
/// corresponding `TypeExpr`. Containers (`List`, `Optional`, `Union`, etc.)
/// recurse so nested usages like `T[]` and `T?` substitute too.
fn substitute_type_vars(
    ty: &baml_compiler2_ast::TypeExpr,
    subst: &std::collections::HashMap<Name, baml_compiler2_ast::TypeExpr>,
) -> baml_compiler2_ast::TypeExpr {
    use baml_compiler2_ast::TypeExpr;
    if subst.is_empty() {
        return ty.clone();
    }
    match ty {
        TypeExpr::Path {
            segments,
            generic_args,
            associated_type_bindings,
            attrs,
        } => {
            if segments.len() == 2
                && segments[0].as_str() == "Self"
                && generic_args.is_empty()
                && associated_type_bindings.is_empty()
                && let Some(replacement) = subst.get(&segments[1])
            {
                return replacement.clone();
            }
            if segments.len() == 1
                && generic_args.is_empty()
                && associated_type_bindings.is_empty()
                && let Some(replacement) = subst.get(&segments[0])
            {
                return replacement.clone();
            }
            TypeExpr::Path {
                segments: segments.clone(),
                generic_args: generic_args
                    .iter()
                    .map(|a| substitute_type_vars(a, subst))
                    .collect(),
                associated_type_bindings: associated_type_bindings
                    .iter()
                    .map(|binding| baml_compiler2_ast::AssociatedTypeBinding {
                        name: binding.name.clone(),
                        ty: Box::new(substitute_type_vars(&binding.ty, subst)),
                    })
                    .collect(),
                attrs: attrs.clone(),
            }
        }
        TypeExpr::AssociatedTypeProjection {
            base,
            interface,
            member,
            attrs,
        } => TypeExpr::AssociatedTypeProjection {
            base: Box::new(substitute_type_vars(base, subst)),
            interface: interface
                .as_ref()
                .map(|interface| Box::new(substitute_type_vars(interface, subst))),
            member: member.clone(),
            attrs: attrs.clone(),
        },
        TypeExpr::List { inner, attrs } => TypeExpr::List {
            inner: Box::new(substitute_type_vars(inner, subst)),
            attrs: attrs.clone(),
        },
        TypeExpr::Optional { inner, attrs } => TypeExpr::Optional {
            inner: Box::new(substitute_type_vars(inner, subst)),
            attrs: attrs.clone(),
        },
        TypeExpr::Union { variants, attrs } => TypeExpr::Union {
            variants: variants
                .iter()
                .map(|m| substitute_type_vars(m, subst))
                .collect(),
            attrs: attrs.clone(),
        },
        TypeExpr::Map { key, value, attrs } => TypeExpr::Map {
            key: Box::new(substitute_type_vars(key, subst)),
            value: Box::new(substitute_type_vars(value, subst)),
            attrs: attrs.clone(),
        },
        TypeExpr::Function {
            params,
            ret,
            throws,
            attrs,
        } => TypeExpr::Function {
            params: params
                .iter()
                .map(|param| baml_compiler2_ast::FunctionTypeParam {
                    name: param.name.clone(),
                    optional: param.optional,
                    ty: substitute_type_vars(&param.ty, subst),
                })
                .collect(),
            ret: Box::new(substitute_type_vars(ret, subst)),
            throws: throws
                .as_ref()
                .map(|throws| Box::new(substitute_type_vars(throws, subst))),
            attrs: attrs.clone(),
        },
        _ => ty.clone(),
    }
}

fn subst_without_names(
    subst: &std::collections::HashMap<Name, baml_compiler2_ast::TypeExpr>,
    names: &[Name],
) -> std::collections::HashMap<Name, baml_compiler2_ast::TypeExpr> {
    if names.is_empty() {
        return subst.clone();
    }
    let mut scoped = subst.clone();
    for name in names {
        scoped.remove(name);
    }
    scoped
}

fn associated_type_subst_from_bindings(
    iface: &baml_compiler2_hir::item_tree::Interface,
    base_subst: &std::collections::HashMap<Name, baml_compiler2_ast::TypeExpr>,
    bindings: &[baml_compiler2_ast::AssociatedTypeBindingDef],
) -> std::collections::HashMap<Name, baml_compiler2_ast::TypeExpr> {
    let mut subst = base_subst.clone();
    for assoc in &iface.associated_types {
        if let Some(binding) = bindings.iter().find(|binding| binding.name == assoc.name)
            && let Some(type_expr) = &binding.type_expr
        {
            subst.insert(
                assoc.name.clone(),
                substitute_type_vars(&type_expr.expr, &subst),
            );
            continue;
        }
        if !subst.contains_key(&assoc.name)
            && let Some(default) = &assoc.default
        {
            let default_expr = substitute_type_vars(&default.expr, &subst);
            subst.insert(assoc.name.clone(), default_expr);
        }
    }
    subst
}

fn associated_type_subst_from_type_args(
    iface: &baml_compiler2_hir::item_tree::Interface,
    base_subst: &std::collections::HashMap<Name, baml_compiler2_ast::TypeExpr>,
    associated_type_bindings: &[baml_compiler2_ast::AssociatedTypeBinding],
) -> std::collections::HashMap<Name, baml_compiler2_ast::TypeExpr> {
    let mut subst = base_subst.clone();
    for assoc in &iface.associated_types {
        if let Some(binding) = associated_type_bindings
            .iter()
            .find(|binding| binding.name == assoc.name)
        {
            subst.insert(
                assoc.name.clone(),
                substitute_type_vars(&binding.ty, &subst),
            );
            continue;
        }
        if !subst.contains_key(&assoc.name)
            && let Some(default) = &assoc.default
        {
            let default_expr = substitute_type_vars(&default.expr, &subst);
            subst.insert(assoc.name.clone(), default_expr);
        }
    }
    subst
}

fn augment_subst_with_class_required_parent_associated_types(
    db: &dyn Db,
    class: &baml_compiler2_ast::ClassDef,
    iface: &baml_compiler2_hir::item_tree::Interface,
    pkg_items: &baml_compiler2_hir::package::PackageItems<'_>,
    namespace_path: &[Name],
    subst: &mut std::collections::HashMap<Name, baml_compiler2_ast::TypeExpr>,
) {
    let mut candidates: IndexMap<Name, Vec<baml_compiler2_ast::TypeExpr>> = IndexMap::new();

    for parent_te in &iface.requires {
        let Some(required_parent) =
            resolve_interface_path(db, &parent_te.expr, pkg_items, namespace_path)
        else {
            continue;
        };
        let Some(class_parent_block) = class.implements.iter().find(|block| {
            resolve_interface_path(db, &block.target.expr, pkg_items, namespace_path)
                .is_some_and(|implemented| implemented.qtn == required_parent.qtn)
        }) else {
            continue;
        };
        let parent_args: &[baml_compiler2_ast::TypeExpr] = match &class_parent_block.target.expr {
            baml_compiler2_ast::TypeExpr::Path { generic_args, .. } => generic_args.as_slice(),
            _ => &[][..],
        };
        let parent_generic_subst: std::collections::HashMap<Name, baml_compiler2_ast::TypeExpr> =
            required_parent
                .iface
                .generic_params
                .iter()
                .zip(parent_args.iter())
                .map(|(param, arg)| (param.clone(), substitute_type_vars(arg, subst)))
                .collect();
        let parent_subst = associated_type_subst_from_bindings(
            &required_parent.iface,
            &parent_generic_subst,
            &class_parent_block.associated_type_bindings,
        );
        for assoc in &required_parent.iface.associated_types {
            if let Some(ty) = parent_subst.get(&assoc.name) {
                candidates
                    .entry(assoc.name.clone())
                    .or_default()
                    .push(ty.clone());
            }
        }
    }

    for (name, values) in candidates {
        if values.len() == 1 && !subst.contains_key(&name) {
            if let Some(value) = values.into_iter().next() {
                subst.insert(name, value);
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn validate_associated_type_binding_defs(
    db: &dyn Db,
    file_id: FileId,
    iface_file_id: FileId,
    impl_span: TextRange,
    iface: &baml_compiler2_hir::item_tree::Interface,
    iface_display_name: &Name,
    iface_pkg_items: &baml_compiler2_hir::package::PackageItems<'_>,
    binding_pkg_items: &baml_compiler2_hir::package::PackageItems<'_>,
    iface_namespace_path: &[Name],
    binding_namespace_path: &[Name],
    generic_params: &[Name],
    generic_bounds: &GenericBoundExprMap,
    generic_subst: &std::collections::HashMap<Name, baml_compiler2_ast::TypeExpr>,
    bindings: &[baml_compiler2_ast::AssociatedTypeBindingDef],
    aliases: &std::collections::HashMap<QualifiedTypeName, Ty>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let associated: IndexMap<Name, &baml_compiler2_ast::AssociatedTypeDef> = iface
        .associated_types
        .iter()
        .map(|assoc| (assoc.name.clone(), assoc))
        .collect();
    let mut seen: IndexMap<Name, Vec<TextRange>> = IndexMap::new();
    for binding in bindings {
        seen.entry(binding.name.clone())
            .or_default()
            .push(binding.name_span);
        if !associated.contains_key(&binding.name) {
            diagnostics.push(
                Diagnostic::error(
                    DiagnosticId::UnknownType,
                    format!(
                        "unknown associated type `{}` for interface `{}`",
                        binding.name, iface_display_name
                    ),
                )
                .with_primary_span(Span {
                    file_id,
                    range: binding.name_span,
                })
                .with_phase(DiagnosticPhase::Type),
            );
        }
    }
    for (name, sites) in seen.iter().filter(|(_, sites)| sites.len() > 1) {
        let mut diag = Diagnostic::error(
            DiagnosticId::DuplicateField,
            format!("Duplicate associated type binding `{name}`"),
        )
        .with_phase(DiagnosticPhase::Type);
        if let Some(first) = sites.first().copied() {
            diag = diag.with_secondary(
                Span {
                    file_id,
                    range: first,
                },
                "first binding is here",
            );
        }
        for site in sites.iter().skip(1).copied() {
            diag = diag.with_primary(
                Span {
                    file_id,
                    range: site,
                },
                "duplicate binding",
            );
        }
        diagnostics.push(diag);
    }
    for assoc in &iface.associated_types {
        if assoc.default.is_none() && !seen.contains_key(&assoc.name) {
            diagnostics.push(
                Diagnostic::error(
                    DiagnosticId::TypeMismatch,
                    format!(
                        "missing associated type binding `{}` for interface `{}`",
                        assoc.name, iface_display_name
                    ),
                )
                .with_primary_span(Span {
                    file_id,
                    range: impl_span,
                })
                .with_related(
                    Span {
                        file_id: iface_file_id,
                        range: assoc.name_span,
                    },
                    "associated type declared here",
                )
                .with_phase(DiagnosticPhase::Type),
            );
        }
    }

    let associated_subst = associated_type_subst_from_bindings(iface, generic_subst, bindings);
    for binding in bindings {
        let Some(assoc) = associated.get(&binding.name).copied() else {
            continue;
        };
        let (Some(bound), Some(binding_ty_expr)) = (&assoc.bound, &binding.type_expr) else {
            continue;
        };
        let mut binding_diags = Vec::new();
        let binding_ty = baml_compiler2_tir::lower_type_expr::lower_type_expr_in_ns(
            db,
            &binding_ty_expr.expr,
            binding_pkg_items,
            binding_namespace_path,
            generic_params,
            &mut binding_diags,
        );
        let mut bound_diags = Vec::new();
        let bound_expr = substitute_type_vars(&bound.expr, &associated_subst);
        let bound_ty = baml_compiler2_tir::lower_type_expr::lower_type_expr_in_ns(
            db,
            &bound_expr,
            iface_pkg_items,
            iface_namespace_path,
            generic_params,
            &mut bound_diags,
        );
        if binding_diags.is_empty()
            && bound_diags.is_empty()
            && !ty_nominal_subtype_with_generic_bounds(
                db,
                &binding_ty,
                &bound_ty,
                binding_pkg_items,
                binding_namespace_path,
                generic_params,
                generic_bounds,
                aliases,
            )
        {
            diagnostics.push(
                Diagnostic::error(
                    DiagnosticId::TypeMismatch,
                    format!(
                        "associated type binding `{}` does not satisfy bound `{}`",
                        binding.name,
                        bound_ty.render_user_facing()
                    ),
                )
                .with_primary_span(Span {
                    file_id,
                    range: binding_ty_expr.span,
                })
                .with_phase(DiagnosticPhase::Type),
            );
        }
    }
}

fn validate_no_associated_type_bindings_on_implements_target(
    file_id: FileId,
    target_span: TextRange,
    target_bindings: &[baml_compiler2_ast::AssociatedTypeBinding],
    diagnostics: &mut Vec<Diagnostic>,
) {
    if target_bindings.is_empty() {
        return;
    }
    diagnostics.push(
        Diagnostic::error(
            DiagnosticId::TypeMismatch,
            "associated type bindings are not allowed in `implements` targets; bind them inside \
             the implements block with `type Name = ...`",
        )
        .with_primary_span(Span {
            file_id,
            range: target_span,
        })
        .with_phase(DiagnosticPhase::Type),
    );
}

fn validate_associated_type_default_bounds(
    db: &dyn Db,
    file_id: FileId,
    iface: &baml_compiler2_ast::InterfaceDef,
    pkg_items: &baml_compiler2_hir::package::PackageItems<'_>,
    namespace_path: &[Name],
    aliases: &std::collections::HashMap<QualifiedTypeName, Ty>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let mut associated_subst: std::collections::HashMap<Name, baml_compiler2_ast::TypeExpr> =
        std::collections::HashMap::new();
    let generic_bounds = generic_bound_expr_map(&iface.generic_params, &iface.generic_param_bounds);
    for assoc in &iface.associated_types {
        let (Some(bound), Some(default)) = (&assoc.bound, &assoc.default) else {
            if let Some(default) = &assoc.default {
                associated_subst.insert(
                    assoc.name.clone(),
                    substitute_type_vars(&default.expr, &associated_subst),
                );
            }
            continue;
        };
        let mut bound_diags = Vec::new();
        let bound_expr = substitute_type_vars(&bound.expr, &associated_subst);
        let bound_ty = baml_compiler2_tir::lower_type_expr::lower_type_expr_in_ns(
            db,
            &bound_expr,
            pkg_items,
            namespace_path,
            &iface.generic_params,
            &mut bound_diags,
        );
        let mut default_diags = Vec::new();
        let default_expr = substitute_type_vars(&default.expr, &associated_subst);
        let default_ty = baml_compiler2_tir::lower_type_expr::lower_type_expr_in_ns(
            db,
            &default_expr,
            pkg_items,
            namespace_path,
            &iface.generic_params,
            &mut default_diags,
        );
        if bound_diags.is_empty()
            && default_diags.is_empty()
            && !ty_nominal_subtype_with_generic_bounds(
                db,
                &default_ty,
                &bound_ty,
                pkg_items,
                namespace_path,
                &iface.generic_params,
                &generic_bounds,
                aliases,
            )
        {
            diagnostics.push(
                Diagnostic::error(
                    DiagnosticId::TypeMismatch,
                    format!(
                        "associated type default `{}` does not satisfy bound `{}`",
                        assoc.name,
                        bound_ty.render_user_facing()
                    ),
                )
                .with_primary_span(Span {
                    file_id,
                    range: default.span,
                })
                .with_phase(DiagnosticPhase::Type),
            );
        }
        associated_subst.insert(assoc.name.clone(), default_expr);
    }
}

#[allow(clippy::too_many_arguments)]
fn ty_nominal_subtype_with_generic_bounds(
    db: &dyn Db,
    sub: &Ty,
    sup: &Ty,
    pkg_items: &baml_compiler2_hir::package::PackageItems<'_>,
    namespace_path: &[Name],
    generic_params: &[Name],
    generic_bounds: &GenericBoundExprMap,
    aliases: &std::collections::HashMap<QualifiedTypeName, Ty>,
) -> bool {
    match sub {
        Ty::Unknown { .. } | Ty::Error { .. } => true,
        Ty::TypeVar(name, _) => {
            baml_compiler2_tir::normalize::is_same_normalized_type(sub, sup, aliases)
                || typevar_bound_nominal_subtype(
                    db,
                    name,
                    sup,
                    pkg_items,
                    namespace_path,
                    generic_params,
                    generic_bounds,
                    aliases,
                    &mut HashSet::new(),
                )
        }
        Ty::Union(members, _) => {
            !members.is_empty()
                && members.iter().all(|member| {
                    ty_nominal_subtype_with_generic_bounds(
                        db,
                        member,
                        sup,
                        pkg_items,
                        namespace_path,
                        generic_params,
                        generic_bounds,
                        aliases,
                    )
                })
        }
        _ => ty_nominal_subtype(db, sub, sup, aliases),
    }
}

fn validate_associated_type_bindings_in_items(
    db: &dyn Db,
    file_id: FileId,
    items: &[baml_compiler2_ast::Item],
    pkg_items: &baml_compiler2_hir::package::PackageItems<'_>,
    namespace_path: &[Name],
    aliases: &std::collections::HashMap<QualifiedTypeName, Ty>,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    for item in items {
        match item {
            baml_compiler2_ast::Item::Function(function) => {
                let empty_bounds = GenericBoundExprMap::new();
                validate_associated_type_bindings_in_function(
                    db,
                    file_id,
                    function,
                    &[],
                    &empty_bounds,
                    pkg_items,
                    namespace_path,
                    aliases,
                    &mut diagnostics,
                );
            }
            baml_compiler2_ast::Item::Class(class) => {
                let outer_generics = class.generic_params.clone();
                let outer_bounds =
                    generic_bound_expr_map(&class.generic_params, &class.generic_param_bounds);
                for field in &class.fields {
                    if let Some(te) = &field.type_expr {
                        validate_associated_type_bindings_in_type_expr(
                            db,
                            file_id,
                            &te.expr,
                            te.span,
                            pkg_items,
                            namespace_path,
                            &outer_generics,
                            &outer_bounds,
                            aliases,
                            &mut diagnostics,
                        );
                    }
                }
                for method in &class.methods {
                    validate_associated_type_bindings_in_function(
                        db,
                        file_id,
                        method,
                        &outer_generics,
                        &outer_bounds,
                        pkg_items,
                        namespace_path,
                        aliases,
                        &mut diagnostics,
                    );
                }
                for block in &class.implements {
                    validate_associated_type_bindings_in_type_expr(
                        db,
                        file_id,
                        &block.target.expr,
                        block.target.span,
                        pkg_items,
                        namespace_path,
                        &outer_generics,
                        &outer_bounds,
                        aliases,
                        &mut diagnostics,
                    );
                    for binding in &block.associated_type_bindings {
                        if let Some(te) = &binding.type_expr {
                            validate_associated_type_bindings_in_type_expr(
                                db,
                                file_id,
                                &te.expr,
                                te.span,
                                pkg_items,
                                namespace_path,
                                &outer_generics,
                                &outer_bounds,
                                aliases,
                                &mut diagnostics,
                            );
                        }
                    }
                    for method in &block.methods {
                        validate_associated_type_bindings_in_function(
                            db,
                            file_id,
                            method,
                            &outer_generics,
                            &outer_bounds,
                            pkg_items,
                            namespace_path,
                            aliases,
                            &mut diagnostics,
                        );
                    }
                }
            }
            baml_compiler2_ast::Item::Interface(iface) => {
                validate_associated_type_declaration_names(file_id, iface, &mut diagnostics);
                let iface_bounds =
                    generic_bound_expr_map(&iface.generic_params, &iface.generic_param_bounds);
                for bound in iface.generic_param_bounds.iter().flatten() {
                    validate_associated_type_bindings_in_type_expr(
                        db,
                        file_id,
                        bound,
                        iface.span,
                        pkg_items,
                        namespace_path,
                        &iface.generic_params,
                        &iface_bounds,
                        aliases,
                        &mut diagnostics,
                    );
                }
                for parent in &iface.requires {
                    validate_associated_type_bindings_in_type_expr(
                        db,
                        file_id,
                        &parent.expr,
                        parent.span,
                        pkg_items,
                        namespace_path,
                        &iface.generic_params,
                        &iface_bounds,
                        aliases,
                        &mut diagnostics,
                    );
                }
                for field in &iface.fields {
                    if let Some(te) = &field.type_expr {
                        validate_associated_type_bindings_in_type_expr(
                            db,
                            file_id,
                            &te.expr,
                            te.span,
                            pkg_items,
                            namespace_path,
                            &iface.generic_params,
                            &iface_bounds,
                            aliases,
                            &mut diagnostics,
                        );
                    }
                }
                for assoc in &iface.associated_types {
                    if let Some(bound) = &assoc.bound {
                        validate_associated_type_bindings_in_type_expr(
                            db,
                            file_id,
                            &bound.expr,
                            bound.span,
                            pkg_items,
                            namespace_path,
                            &iface.generic_params,
                            &iface_bounds,
                            aliases,
                            &mut diagnostics,
                        );
                    }
                    if let Some(default) = &assoc.default {
                        validate_associated_type_bindings_in_type_expr(
                            db,
                            file_id,
                            &default.expr,
                            default.span,
                            pkg_items,
                            namespace_path,
                            &iface.generic_params,
                            &iface_bounds,
                            aliases,
                            &mut diagnostics,
                        );
                    }
                }
                for method in &iface.required_methods {
                    validate_associated_type_bindings_in_method_sig(
                        db,
                        file_id,
                        method,
                        &iface.generic_params,
                        &iface_bounds,
                        pkg_items,
                        namespace_path,
                        aliases,
                        &mut diagnostics,
                    );
                }
                for method in &iface.default_methods {
                    validate_associated_type_bindings_in_function(
                        db,
                        file_id,
                        method,
                        &iface.generic_params,
                        &iface_bounds,
                        pkg_items,
                        namespace_path,
                        aliases,
                        &mut diagnostics,
                    );
                }
            }
            baml_compiler2_ast::Item::TypeAlias(alias) => {
                if let Some(te) = &alias.type_expr {
                    let empty_bounds = GenericBoundExprMap::new();
                    validate_associated_type_bindings_in_type_expr(
                        db,
                        file_id,
                        &te.expr,
                        te.span,
                        pkg_items,
                        namespace_path,
                        &[],
                        &empty_bounds,
                        aliases,
                        &mut diagnostics,
                    );
                }
            }
            baml_compiler2_ast::Item::ImplementsFor(imp) => {
                let impl_generics: Vec<Name> = imp
                    .generic_params
                    .iter()
                    .map(|(name, _)| name.clone())
                    .collect();
                // Single-bound view (first `&`-bound only) — unchanged behavior;
                // the full bound set lives on the new HIR `ImplBlock`.
                let impl_bound_exprs: Vec<Option<baml_compiler2_ast::TypeExpr>> = imp
                    .generic_params
                    .iter()
                    .map(|(_, bounds)| bounds.first().cloned())
                    .collect();
                let impl_bounds = generic_bound_expr_map(&impl_generics, &impl_bound_exprs);
                for (_, bounds) in &imp.generic_params {
                    if let Some(bound) = bounds.first() {
                        validate_associated_type_bindings_in_type_expr(
                            db,
                            file_id,
                            bound,
                            imp.span,
                            pkg_items,
                            namespace_path,
                            &impl_generics,
                            &impl_bounds,
                            aliases,
                            &mut diagnostics,
                        );
                    }
                }
                validate_associated_type_bindings_in_type_expr(
                    db,
                    file_id,
                    &imp.interface_target.expr,
                    imp.interface_target.span,
                    pkg_items,
                    namespace_path,
                    &impl_generics,
                    &impl_bounds,
                    aliases,
                    &mut diagnostics,
                );
                validate_associated_type_bindings_in_type_expr(
                    db,
                    file_id,
                    &imp.for_target.expr,
                    imp.for_target.span,
                    pkg_items,
                    namespace_path,
                    &impl_generics,
                    &impl_bounds,
                    aliases,
                    &mut diagnostics,
                );
                for binding in &imp.associated_type_bindings {
                    if let Some(te) = &binding.type_expr {
                        validate_associated_type_bindings_in_type_expr(
                            db,
                            file_id,
                            &te.expr,
                            te.span,
                            pkg_items,
                            namespace_path,
                            &impl_generics,
                            &impl_bounds,
                            aliases,
                            &mut diagnostics,
                        );
                    }
                }
                for method in &imp.methods {
                    validate_associated_type_bindings_in_function(
                        db,
                        file_id,
                        method,
                        &impl_generics,
                        &impl_bounds,
                        pkg_items,
                        namespace_path,
                        aliases,
                        &mut diagnostics,
                    );
                }
            }
            _ => {}
        }
    }

    diagnostics
}

fn validate_associated_type_declaration_names(
    file_id: FileId,
    iface: &baml_compiler2_ast::InterfaceDef,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for assoc in &iface.associated_types {
        if iface
            .generic_params
            .iter()
            .any(|param| param == &assoc.name)
        {
            diagnostics.push(
                Diagnostic::error(
                    DiagnosticId::DuplicateField,
                    format!(
                        "associated type `{}` collides with generic parameter `{}`",
                        assoc.name, assoc.name
                    ),
                )
                .with_primary_span(Span {
                    file_id,
                    range: assoc.span,
                })
                .with_phase(DiagnosticPhase::Type),
            );
        }
    }
}

type GenericBoundExprMap = std::collections::HashMap<Name, baml_compiler2_ast::TypeExpr>;

fn generic_bound_expr_map(
    params: &[Name],
    bounds: &[Option<baml_compiler2_ast::TypeExpr>],
) -> GenericBoundExprMap {
    params
        .iter()
        .zip(bounds.iter())
        .filter_map(|(name, bound)| bound.as_ref().map(|bound| (name.clone(), bound.clone())))
        .collect()
}

fn extend_generic_bound_expr_map(
    outer: &GenericBoundExprMap,
    params: &[Name],
    bounds: &[Option<baml_compiler2_ast::TypeExpr>],
) -> GenericBoundExprMap {
    let mut merged = outer.clone();
    merged.extend(generic_bound_expr_map(params, bounds));
    merged
}

#[allow(clippy::too_many_arguments)]
fn validate_associated_type_bindings_in_function(
    db: &dyn Db,
    file_id: FileId,
    function: &baml_compiler2_ast::FunctionDef,
    outer_generic_params: &[Name],
    outer_generic_bounds: &GenericBoundExprMap,
    pkg_items: &baml_compiler2_hir::package::PackageItems<'_>,
    namespace_path: &[Name],
    aliases: &std::collections::HashMap<QualifiedTypeName, Ty>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let mut generic_params = outer_generic_params.to_vec();
    generic_params.extend(function.generic_params.iter().cloned());
    let generic_bounds = extend_generic_bound_expr_map(
        outer_generic_bounds,
        &function.generic_params,
        &function.generic_param_bounds,
    );
    for bound in function.generic_param_bounds.iter().flatten() {
        validate_associated_type_bindings_in_type_expr(
            db,
            file_id,
            bound,
            function.span,
            pkg_items,
            namespace_path,
            &generic_params,
            &generic_bounds,
            aliases,
            diagnostics,
        );
    }
    for param in &function.params {
        if let Some(te) = &param.type_expr {
            validate_ambiguous_typevar_associated_projection_in_type_expr(
                db,
                file_id,
                &te.expr,
                te.span,
                pkg_items,
                namespace_path,
                &generic_bounds,
                diagnostics,
            );
            validate_associated_type_bindings_in_type_expr(
                db,
                file_id,
                &te.expr,
                te.span,
                pkg_items,
                namespace_path,
                &generic_params,
                &generic_bounds,
                aliases,
                diagnostics,
            );
        }
    }
    if let Some(ret) = &function.return_type {
        validate_ambiguous_typevar_associated_projection_in_type_expr(
            db,
            file_id,
            &ret.expr,
            ret.span,
            pkg_items,
            namespace_path,
            &generic_bounds,
            diagnostics,
        );
        validate_associated_type_bindings_in_type_expr(
            db,
            file_id,
            &ret.expr,
            ret.span,
            pkg_items,
            namespace_path,
            &generic_params,
            &generic_bounds,
            aliases,
            diagnostics,
        );
    }
    if let Some(throws) = &function.throws {
        validate_ambiguous_typevar_associated_projection_in_type_expr(
            db,
            file_id,
            &throws.expr,
            throws.span,
            pkg_items,
            namespace_path,
            &generic_bounds,
            diagnostics,
        );
        validate_associated_type_bindings_in_type_expr(
            db,
            file_id,
            &throws.expr,
            throws.span,
            pkg_items,
            namespace_path,
            &generic_params,
            &generic_bounds,
            aliases,
            diagnostics,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn validate_associated_type_bindings_in_method_sig(
    db: &dyn Db,
    file_id: FileId,
    method: &baml_compiler2_ast::MethodSigDef,
    outer_generic_params: &[Name],
    outer_generic_bounds: &GenericBoundExprMap,
    pkg_items: &baml_compiler2_hir::package::PackageItems<'_>,
    namespace_path: &[Name],
    aliases: &std::collections::HashMap<QualifiedTypeName, Ty>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let mut generic_params = outer_generic_params.to_vec();
    generic_params.extend(method.generic_params.iter().cloned());
    let generic_bounds = extend_generic_bound_expr_map(
        outer_generic_bounds,
        &method.generic_params,
        &method.generic_param_bounds,
    );
    for bound in method.generic_param_bounds.iter().flatten() {
        validate_associated_type_bindings_in_type_expr(
            db,
            file_id,
            bound,
            method.span,
            pkg_items,
            namespace_path,
            &generic_params,
            &generic_bounds,
            aliases,
            diagnostics,
        );
    }
    for param in &method.params {
        if let Some(te) = &param.type_expr {
            validate_ambiguous_typevar_associated_projection_in_type_expr(
                db,
                file_id,
                &te.expr,
                te.span,
                pkg_items,
                namespace_path,
                &generic_bounds,
                diagnostics,
            );
            validate_associated_type_bindings_in_type_expr(
                db,
                file_id,
                &te.expr,
                te.span,
                pkg_items,
                namespace_path,
                &generic_params,
                &generic_bounds,
                aliases,
                diagnostics,
            );
        }
    }
    if let Some(ret) = &method.return_type {
        validate_ambiguous_typevar_associated_projection_in_type_expr(
            db,
            file_id,
            &ret.expr,
            ret.span,
            pkg_items,
            namespace_path,
            &generic_bounds,
            diagnostics,
        );
        validate_associated_type_bindings_in_type_expr(
            db,
            file_id,
            &ret.expr,
            ret.span,
            pkg_items,
            namespace_path,
            &generic_params,
            &generic_bounds,
            aliases,
            diagnostics,
        );
    }
    if let Some(throws) = &method.throws {
        validate_ambiguous_typevar_associated_projection_in_type_expr(
            db,
            file_id,
            &throws.expr,
            throws.span,
            pkg_items,
            namespace_path,
            &generic_bounds,
            diagnostics,
        );
        validate_associated_type_bindings_in_type_expr(
            db,
            file_id,
            &throws.expr,
            throws.span,
            pkg_items,
            namespace_path,
            &generic_params,
            &generic_bounds,
            aliases,
            diagnostics,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn validate_associated_type_bindings_in_type_expr(
    db: &dyn Db,
    file_id: FileId,
    expr: &baml_compiler2_ast::TypeExpr,
    span: TextRange,
    pkg_items: &baml_compiler2_hir::package::PackageItems<'_>,
    namespace_path: &[Name],
    generic_params: &[Name],
    generic_bounds: &GenericBoundExprMap,
    aliases: &std::collections::HashMap<QualifiedTypeName, Ty>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    use baml_compiler2_ast::TypeExpr;

    match expr {
        TypeExpr::Path {
            segments,
            generic_args,
            associated_type_bindings,
            ..
        } => {
            for arg in generic_args {
                validate_associated_type_bindings_in_type_expr(
                    db,
                    file_id,
                    arg,
                    span,
                    pkg_items,
                    namespace_path,
                    generic_params,
                    generic_bounds,
                    aliases,
                    diagnostics,
                );
            }
            for binding in associated_type_bindings {
                validate_associated_type_bindings_in_type_expr(
                    db,
                    file_id,
                    &binding.ty,
                    span,
                    pkg_items,
                    namespace_path,
                    generic_params,
                    generic_bounds,
                    aliases,
                    diagnostics,
                );
            }
            validate_associated_type_bindings_on_interface_type(
                db,
                file_id,
                expr,
                span,
                pkg_items,
                namespace_path,
                generic_params,
                generic_bounds,
                aliases,
                diagnostics,
            );
            if segments.len() >= 2 && generic_args.is_empty() && associated_type_bindings.is_empty()
            {
                validate_unqualified_associated_type_projection(
                    db,
                    file_id,
                    expr,
                    segments,
                    span,
                    pkg_items,
                    namespace_path,
                    generic_params,
                    generic_bounds,
                    aliases,
                    diagnostics,
                );
            }
        }
        TypeExpr::AssociatedTypeProjection {
            base, interface, ..
        } => {
            validate_associated_type_bindings_in_type_expr(
                db,
                file_id,
                base,
                span,
                pkg_items,
                namespace_path,
                generic_params,
                generic_bounds,
                aliases,
                diagnostics,
            );
            if let Some(interface) = interface {
                validate_associated_type_bindings_in_type_expr(
                    db,
                    file_id,
                    interface,
                    span,
                    pkg_items,
                    namespace_path,
                    generic_params,
                    generic_bounds,
                    aliases,
                    diagnostics,
                );
            }
            // The non-interface qualifier and the unknown-associated-member checks
            // are now in the TIR lowering (`lower_explicit_projection_qualifier`),
            // emitted on every compile path. This still validates the
            // unknown-interface and base-implements-interface cases.
            validate_qualified_associated_type_projection(
                db,
                file_id,
                expr,
                span,
                pkg_items,
                namespace_path,
                generic_params,
                generic_bounds,
                aliases,
                diagnostics,
            );
        }
        TypeExpr::Optional { inner, .. } | TypeExpr::List { inner, .. } => {
            validate_associated_type_bindings_in_type_expr(
                db,
                file_id,
                inner,
                span,
                pkg_items,
                namespace_path,
                generic_params,
                generic_bounds,
                aliases,
                diagnostics,
            );
        }
        TypeExpr::Map { key, value, .. } => {
            validate_associated_type_bindings_in_type_expr(
                db,
                file_id,
                key,
                span,
                pkg_items,
                namespace_path,
                generic_params,
                generic_bounds,
                aliases,
                diagnostics,
            );
            validate_associated_type_bindings_in_type_expr(
                db,
                file_id,
                value,
                span,
                pkg_items,
                namespace_path,
                generic_params,
                generic_bounds,
                aliases,
                diagnostics,
            );
        }
        TypeExpr::Union { variants, .. } => {
            for variant in variants {
                validate_associated_type_bindings_in_type_expr(
                    db,
                    file_id,
                    variant,
                    span,
                    pkg_items,
                    namespace_path,
                    generic_params,
                    generic_bounds,
                    aliases,
                    diagnostics,
                );
            }
        }
        TypeExpr::Function {
            params,
            ret,
            throws,
            ..
        } => {
            // A function type carries no generics of its own; recurse with the
            // enclosing scope.
            for param in params {
                validate_associated_type_bindings_in_type_expr(
                    db,
                    file_id,
                    &param.ty,
                    span,
                    pkg_items,
                    namespace_path,
                    generic_params,
                    generic_bounds,
                    aliases,
                    diagnostics,
                );
            }
            validate_associated_type_bindings_in_type_expr(
                db,
                file_id,
                ret,
                span,
                pkg_items,
                namespace_path,
                generic_params,
                generic_bounds,
                aliases,
                diagnostics,
            );
            if let Some(throws) = throws {
                validate_associated_type_bindings_in_type_expr(
                    db,
                    file_id,
                    throws,
                    span,
                    pkg_items,
                    namespace_path,
                    generic_params,
                    generic_bounds,
                    aliases,
                    diagnostics,
                );
            }
        }
        _ => {}
    }
}

#[allow(clippy::too_many_arguments)]
fn validate_unqualified_associated_type_projection(
    db: &dyn Db,
    file_id: FileId,
    expr: &baml_compiler2_ast::TypeExpr,
    segments: &[Name],
    span: TextRange,
    pkg_items: &baml_compiler2_hir::package::PackageItems<'_>,
    namespace_path: &[Name],
    generic_params: &[Name],
    _generic_bounds: &GenericBoundExprMap,
    aliases: &std::collections::HashMap<QualifiedTypeName, Ty>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some(member) = segments.last() else {
        return;
    };
    let base_expr = baml_compiler2_ast::TypeExpr::Path {
        segments: segments[..segments.len() - 1].to_vec(),
        generic_args: Vec::new(),
        associated_type_bindings: Vec::new(),
        attrs: Vec::new(),
    };
    let mut base_diags = Vec::new();
    let base_ty = baml_compiler2_tir::lower_type_expr::lower_type_expr_in_ns(
        db,
        &base_expr,
        pkg_items,
        namespace_path,
        generic_params,
        &mut base_diags,
    );
    if !base_diags.is_empty() {
        return;
    }
    let expanded_base_ty = expand_alias_chain(base_ty, aliases);
    let message = match expanded_base_ty {
        Ty::Class(class_qtn, _, _) => {
            let matches = matching_associated_type_projection_interfaces(db, &class_qtn, member)
                .unwrap_or_default();
            if matches.len() == 1 {
                return;
            }
            if matches.is_empty() {
                format!(
                    "unknown associated type `{member}` for class `{}`",
                    class_qtn.render_user_facing()
                )
            } else {
                let base = base_expr.to_string();
                let alternatives = matches
                    .iter()
                    .map(|interface| format!("({base} as {interface}).{member}"))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!(
                    "ambiguous associated type projection `{expr}`; disambiguate with one of: {alternatives}"
                )
            }
        }
        Ty::Interface(iface_qtn, _, _, _) => {
            let sources =
                associated_type_projection_sources_for_interface_qtn(db, &iface_qtn, member);
            if sources.len() == 1 {
                return;
            }
            if sources.is_empty() {
                format!(
                    "unknown associated type `{member}` for interface `{}`",
                    iface_qtn.render_user_facing()
                )
            } else {
                let base = base_expr.to_string();
                let alternatives = sources
                    .iter()
                    .map(|interface| format!("({base} as {interface}).{member}"))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!(
                    "ambiguous associated type projection `{expr}`; disambiguate with one of: {alternatives}"
                )
            }
        }
        _ => return,
    };

    diagnostics.push(
        Diagnostic::error(DiagnosticId::TypeMismatch, message)
            .with_primary_span(Span {
                file_id,
                range: span,
            })
            .with_phase(DiagnosticPhase::Type),
    );
}

#[allow(clippy::too_many_arguments)]
fn validate_qualified_associated_type_projection(
    db: &dyn Db,
    file_id: FileId,
    expr: &baml_compiler2_ast::TypeExpr,
    span: TextRange,
    pkg_items: &baml_compiler2_hir::package::PackageItems<'_>,
    namespace_path: &[Name],
    generic_params: &[Name],
    generic_bounds: &GenericBoundExprMap,
    aliases: &std::collections::HashMap<QualifiedTypeName, Ty>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let baml_compiler2_ast::TypeExpr::AssociatedTypeProjection {
        base,
        interface: Some(interface),
        ..
    } = expr
    else {
        return;
    };

    // The qualifier must name an interface. The non-interface case ("must use an
    // interface") and the unknown-associated-member case are now diagnosed by the
    // TIR lowering (`lower_explicit_projection_qualifier`), emitted on every
    // compile path; here we only report a qualifier path that doesn't resolve to
    // any interface at all.
    if resolve_interface_path(db, interface, pkg_items, namespace_path).is_none() {
        if !is_non_interface_type(interface, pkg_items, namespace_path) {
            diagnostics.push(
                Diagnostic::error(
                    DiagnosticId::TypeMismatch,
                    format!("unknown interface `{interface}` in associated type projection"),
                )
                .with_primary_span(Span {
                    file_id,
                    range: span,
                })
                .with_phase(DiagnosticPhase::Type),
            );
        }
        return;
    }

    let mut base_diags = Vec::new();
    let base_ty = baml_compiler2_tir::lower_type_expr::lower_type_expr_in_ns(
        db,
        base,
        pkg_items,
        namespace_path,
        generic_params,
        &mut base_diags,
    );
    if !base_diags.is_empty() {
        return;
    }

    let mut interface_diags = Vec::new();
    let interface_ty = baml_compiler2_tir::lower_type_expr::lower_type_expr_in_ns(
        db,
        interface,
        pkg_items,
        namespace_path,
        generic_params,
        &mut interface_diags,
    );
    let Ty::Interface(_, _, _, _) = &interface_ty else {
        return;
    };
    if !interface_diags.is_empty() {
        return;
    }

    let base_ty = expand_alias_chain(base_ty, aliases);
    let base_implements_interface = match &base_ty {
        Ty::Unknown { .. } | Ty::Error { .. } => return,
        Ty::TypeVar(name, _) => typevar_bound_nominal_subtype(
            db,
            name,
            &interface_ty,
            pkg_items,
            namespace_path,
            generic_params,
            generic_bounds,
            aliases,
            &mut HashSet::new(),
        ),
        _ => ty_nominal_subtype(db, &base_ty, &interface_ty, aliases),
    };

    if !base_implements_interface {
        diagnostics.push(
            Diagnostic::error(
                DiagnosticId::TypeMismatch,
                format!(
                    "type `{}` does not implement interface `{}`",
                    base_ty.render_user_facing(),
                    interface_ty.render_user_facing()
                ),
            )
            .with_primary_span(Span {
                file_id,
                range: span,
            })
            .with_phase(DiagnosticPhase::Type),
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn typevar_bound_nominal_subtype(
    db: &dyn Db,
    name: &Name,
    sup: &Ty,
    pkg_items: &baml_compiler2_hir::package::PackageItems<'_>,
    namespace_path: &[Name],
    generic_params: &[Name],
    generic_bounds: &GenericBoundExprMap,
    aliases: &std::collections::HashMap<QualifiedTypeName, Ty>,
    visited: &mut HashSet<Name>,
) -> bool {
    if !visited.insert(name.clone()) {
        return false;
    }

    let Some(bound) = generic_bounds.get(name) else {
        return false;
    };
    let mut bound_diags = Vec::new();
    let bound_ty = baml_compiler2_tir::lower_type_expr::lower_type_expr_in_ns(
        db,
        bound,
        pkg_items,
        namespace_path,
        generic_params,
        &mut bound_diags,
    );
    if !bound_diags.is_empty() {
        return false;
    }

    let bound_ty = expand_alias_chain(bound_ty, aliases);
    if ty_nominal_subtype(db, &bound_ty, sup, aliases) {
        return true;
    }

    match bound_ty {
        Ty::TypeVar(next, _) => typevar_bound_nominal_subtype(
            db,
            &next,
            sup,
            pkg_items,
            namespace_path,
            generic_params,
            generic_bounds,
            aliases,
            visited,
        ),
        _ => false,
    }
}

fn expand_alias_chain(ty: Ty, aliases: &std::collections::HashMap<QualifiedTypeName, Ty>) -> Ty {
    let mut current = ty;
    let mut seen = HashSet::new();
    loop {
        let Ty::TypeAlias(qtn, _) = &current else {
            return current;
        };
        if !seen.insert(qtn.clone()) {
            return current;
        }
        let Some(next) = aliases.get(qtn).cloned() else {
            return current;
        };
        current = next;
    }
}

fn matching_associated_type_projection_interfaces(
    db: &dyn Db,
    class_qtn: &QualifiedTypeName,
    member: &Name,
) -> Option<Vec<String>> {
    use baml_compiler2_hir::contributions::Definition;

    let pkg_id = baml_compiler2_hir::package::PackageId::new(db, class_qtn.package().clone());
    let pkg_items = baml_compiler2_hir::package::package_items(db, pkg_id);
    let Definition::Class(class_loc) =
        pkg_items.lookup_type(class_qtn.namespace(), class_qtn.name())?
    else {
        return None;
    };
    let item_tree = baml_compiler2_hir::file_item_tree(db, class_loc.file(db));
    let class_data = item_tree.classes.get(&class_loc.id(db))?;
    let class_pkg = baml_compiler2_hir::file_package::file_package(db, class_loc.file(db));
    let class_ns = class_pkg.namespace_path;
    let mut matches = Vec::new();

    for impl_target in &class_data.implements {
        let Some(iface) =
            resolve_interface_path(db, &impl_target.target.expr, pkg_items, &class_ns)
        else {
            continue;
        };
        if iface
            .iface
            .associated_types
            .iter()
            .any(|assoc| assoc.name == *member)
        {
            matches.push(impl_target.target.expr.to_string());
        }
    }

    Some(matches)
}

#[allow(clippy::too_many_arguments)]
fn validate_ambiguous_typevar_associated_projection_in_type_expr(
    db: &dyn Db,
    file_id: FileId,
    expr: &baml_compiler2_ast::TypeExpr,
    span: TextRange,
    pkg_items: &baml_compiler2_hir::package::PackageItems<'_>,
    namespace_path: &[Name],
    generic_bounds: &std::collections::HashMap<Name, baml_compiler2_ast::TypeExpr>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    use baml_compiler2_ast::TypeExpr;

    match expr {
        TypeExpr::Path {
            segments,
            generic_args,
            associated_type_bindings,
            ..
        } => {
            if segments.len() == 2 && generic_args.is_empty() && associated_type_bindings.is_empty()
            {
                let base = &segments[0];
                let member = &segments[1];
                if let Some(bound) = generic_bounds.get(base) {
                    let sources = associated_type_projection_sources_for_interface_bound(
                        db,
                        bound,
                        member,
                        pkg_items,
                        namespace_path,
                    );
                    if sources.is_empty() {
                        diagnostics.push(
                            Diagnostic::error(
                                DiagnosticId::UnknownType,
                                format!("unknown associated type `{member}` for bound `{bound}`"),
                            )
                            .with_primary_span(Span {
                                file_id,
                                range: span,
                            })
                            .with_phase(DiagnosticPhase::Type),
                        );
                    } else if sources.len() >= 2 {
                        let alternatives = sources
                            .iter()
                            .map(|interface| format!("({base} as {interface}).{member}"))
                            .collect::<Vec<_>>()
                            .join(", ");
                        diagnostics.push(
                            Diagnostic::error(
                                DiagnosticId::TypeMismatch,
                                format!(
                                    "ambiguous associated type projection `{base}.{member}`; disambiguate with one of: {alternatives}"
                                ),
                            )
                            .with_primary_span(Span {
                                file_id,
                                range: span,
                            })
                            .with_phase(DiagnosticPhase::Type),
                        );
                    }
                }
            }
            for arg in generic_args {
                validate_ambiguous_typevar_associated_projection_in_type_expr(
                    db,
                    file_id,
                    arg,
                    span,
                    pkg_items,
                    namespace_path,
                    generic_bounds,
                    diagnostics,
                );
            }
            for binding in associated_type_bindings {
                validate_ambiguous_typevar_associated_projection_in_type_expr(
                    db,
                    file_id,
                    &binding.ty,
                    span,
                    pkg_items,
                    namespace_path,
                    generic_bounds,
                    diagnostics,
                );
            }
        }
        TypeExpr::AssociatedTypeProjection {
            base, interface, ..
        } => {
            validate_ambiguous_typevar_associated_projection_in_type_expr(
                db,
                file_id,
                base,
                span,
                pkg_items,
                namespace_path,
                generic_bounds,
                diagnostics,
            );
            if let Some(interface) = interface {
                validate_ambiguous_typevar_associated_projection_in_type_expr(
                    db,
                    file_id,
                    interface,
                    span,
                    pkg_items,
                    namespace_path,
                    generic_bounds,
                    diagnostics,
                );
            }
        }
        TypeExpr::Optional { inner, .. } | TypeExpr::List { inner, .. } => {
            validate_ambiguous_typevar_associated_projection_in_type_expr(
                db,
                file_id,
                inner,
                span,
                pkg_items,
                namespace_path,
                generic_bounds,
                diagnostics,
            );
        }
        TypeExpr::Map { key, value, .. } => {
            validate_ambiguous_typevar_associated_projection_in_type_expr(
                db,
                file_id,
                key,
                span,
                pkg_items,
                namespace_path,
                generic_bounds,
                diagnostics,
            );
            validate_ambiguous_typevar_associated_projection_in_type_expr(
                db,
                file_id,
                value,
                span,
                pkg_items,
                namespace_path,
                generic_bounds,
                diagnostics,
            );
        }
        TypeExpr::Union { variants, .. } => {
            for variant in variants {
                validate_ambiguous_typevar_associated_projection_in_type_expr(
                    db,
                    file_id,
                    variant,
                    span,
                    pkg_items,
                    namespace_path,
                    generic_bounds,
                    diagnostics,
                );
            }
        }
        TypeExpr::Function {
            params,
            ret,
            throws,
            ..
        } => {
            for param in params {
                validate_ambiguous_typevar_associated_projection_in_type_expr(
                    db,
                    file_id,
                    &param.ty,
                    span,
                    pkg_items,
                    namespace_path,
                    generic_bounds,
                    diagnostics,
                );
            }
            validate_ambiguous_typevar_associated_projection_in_type_expr(
                db,
                file_id,
                ret,
                span,
                pkg_items,
                namespace_path,
                generic_bounds,
                diagnostics,
            );
            if let Some(throws) = throws {
                validate_ambiguous_typevar_associated_projection_in_type_expr(
                    db,
                    file_id,
                    throws,
                    span,
                    pkg_items,
                    namespace_path,
                    generic_bounds,
                    diagnostics,
                );
            }
        }
        _ => {}
    }
}

fn associated_type_projection_sources_for_interface_bound(
    db: &dyn Db,
    bound: &baml_compiler2_ast::TypeExpr,
    member: &Name,
    pkg_items: &baml_compiler2_hir::package::PackageItems<'_>,
    namespace_path: &[Name],
) -> Vec<String> {
    let Some(root) = resolve_interface_path(db, bound, pkg_items, namespace_path) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut visited = HashSet::new();
    let mut stack = vec![root];

    while let Some(current) = stack.pop() {
        if !visited.insert(current.qtn.clone()) {
            continue;
        }
        if current
            .iface
            .associated_types
            .iter()
            .any(|assoc| assoc.name == *member)
        {
            out.push(current.qtn.render_user_facing());
        }
        let current_file = current.loc.file(db);
        let current_pkg = baml_compiler2_hir::file_package::file_package(db, current_file);
        let current_pkg_id =
            baml_compiler2_hir::package::PackageId::new(db, current_pkg.package.clone());
        let current_pkg_items = baml_compiler2_hir::package::package_items(db, current_pkg_id);
        for parent in &current.iface.requires {
            if let Some(parent) = resolve_interface_path(
                db,
                &parent.expr,
                current_pkg_items,
                &current_pkg.namespace_path,
            ) {
                stack.push(parent);
            }
        }
    }

    out
}

fn associated_type_projection_sources_for_interface_qtn(
    db: &dyn Db,
    iface_qtn: &QualifiedTypeName,
    member: &Name,
) -> Vec<String> {
    let pkg_id = baml_compiler2_hir::package::PackageId::new(db, iface_qtn.package().clone());
    let pkg_items = baml_compiler2_hir::package::package_items(db, pkg_id);
    let Some(baml_compiler2_hir::contributions::Definition::Interface(root_loc)) =
        pkg_items.lookup_type(iface_qtn.namespace(), iface_qtn.name())
    else {
        return Vec::new();
    };
    let root_tree = baml_compiler2_hir::file_item_tree(db, root_loc.file(db));
    let Some(root_iface) = root_tree.interfaces.get(&root_loc.id(db)) else {
        return Vec::new();
    };
    let root = ResolvedInterfaceData {
        loc: root_loc,
        qtn: iface_qtn.clone(),
        iface: root_iface.clone(),
    };
    let mut out = Vec::new();
    let mut visited = HashSet::new();
    let mut stack = vec![root];

    while let Some(current) = stack.pop() {
        if !visited.insert(current.qtn.clone()) {
            continue;
        }
        if current
            .iface
            .associated_types
            .iter()
            .any(|assoc| assoc.name == *member)
        {
            out.push(current.qtn.render_user_facing());
        }
        let current_file = current.loc.file(db);
        let current_pkg = baml_compiler2_hir::file_package::file_package(db, current_file);
        let current_pkg_id =
            baml_compiler2_hir::package::PackageId::new(db, current_pkg.package.clone());
        let current_pkg_items = baml_compiler2_hir::package::package_items(db, current_pkg_id);
        for parent in &current.iface.requires {
            if let Some(parent) = resolve_interface_path(
                db,
                &parent.expr,
                current_pkg_items,
                &current_pkg.namespace_path,
            ) {
                stack.push(parent);
            }
        }
    }

    out
}

#[allow(clippy::too_many_arguments)]
fn validate_associated_type_bindings_on_interface_type(
    db: &dyn Db,
    file_id: FileId,
    expr: &baml_compiler2_ast::TypeExpr,
    span: TextRange,
    pkg_items: &baml_compiler2_hir::package::PackageItems<'_>,
    namespace_path: &[Name],
    generic_params: &[Name],
    generic_bounds: &GenericBoundExprMap,
    aliases: &std::collections::HashMap<QualifiedTypeName, Ty>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let baml_compiler2_ast::TypeExpr::Path {
        generic_args,
        associated_type_bindings,
        ..
    } = expr
    else {
        return;
    };
    if associated_type_bindings.is_empty() {
        return;
    }
    let Some(resolved_iface) = resolve_interface_path(db, expr, pkg_items, namespace_path) else {
        return;
    };
    let iface_display_name = resolved_iface.display_name();
    let iface_file = resolved_iface.loc.file(db);
    let iface_pkg_info = baml_compiler2_hir::file_package::file_package(db, iface_file);
    let iface_pkg_id =
        baml_compiler2_hir::package::PackageId::new(db, iface_pkg_info.package.clone());
    let iface_pkg_items = baml_compiler2_hir::package::package_items(db, iface_pkg_id);
    let iface_namespace_path = iface_pkg_info.namespace_path;
    let associated: IndexMap<Name, &baml_compiler2_ast::AssociatedTypeDef> = resolved_iface
        .iface
        .associated_types
        .iter()
        .map(|assoc| (assoc.name.clone(), assoc))
        .collect();
    let mut seen: IndexMap<Name, usize> = IndexMap::new();

    for binding in associated_type_bindings {
        *seen.entry(binding.name.clone()).or_default() += 1;
        if !associated.contains_key(&binding.name) {
            diagnostics.push(
                Diagnostic::error(
                    DiagnosticId::UnknownType,
                    format!(
                        "unknown associated type `{}` for interface `{}`",
                        binding.name, iface_display_name
                    ),
                )
                .with_primary_span(Span {
                    file_id,
                    range: span,
                })
                .with_phase(DiagnosticPhase::Type),
            );
        }
    }

    for (name, _) in seen.iter().filter(|(_, count)| **count > 1) {
        diagnostics.push(
            Diagnostic::error(
                DiagnosticId::DuplicateField,
                format!("Duplicate associated type binding `{name}`"),
            )
            .with_primary(
                Span {
                    file_id,
                    range: span,
                },
                "duplicate binding",
            )
            .with_phase(DiagnosticPhase::Type),
        );
    }

    let generic_subst: std::collections::HashMap<Name, baml_compiler2_ast::TypeExpr> =
        resolved_iface
            .iface
            .generic_params
            .iter()
            .zip(generic_args.iter())
            .map(|(p, a)| (p.clone(), a.clone()))
            .collect();
    let associated_subst = associated_type_subst_from_type_args(
        &resolved_iface.iface,
        &generic_subst,
        associated_type_bindings,
    );

    for binding in associated_type_bindings {
        let Some(assoc) = associated.get(&binding.name).copied() else {
            continue;
        };
        let Some(bound) = &assoc.bound else {
            continue;
        };
        let mut binding_diags = Vec::new();
        let binding_ty = baml_compiler2_tir::lower_type_expr::lower_type_expr_in_ns(
            db,
            &binding.ty,
            pkg_items,
            namespace_path,
            generic_params,
            &mut binding_diags,
        );
        let mut bound_diags = Vec::new();
        let bound_expr = substitute_type_vars(&bound.expr, &associated_subst);
        let bound_ty = baml_compiler2_tir::lower_type_expr::lower_type_expr_in_ns(
            db,
            &bound_expr,
            iface_pkg_items,
            &iface_namespace_path,
            generic_params,
            &mut bound_diags,
        );
        if binding_diags.is_empty()
            && bound_diags.is_empty()
            && !ty_nominal_subtype_with_generic_bounds(
                db,
                &binding_ty,
                &bound_ty,
                pkg_items,
                namespace_path,
                generic_params,
                generic_bounds,
                aliases,
            )
        {
            diagnostics.push(
                Diagnostic::error(
                    DiagnosticId::TypeMismatch,
                    format!(
                        "associated type binding `{}` does not satisfy bound `{}`",
                        binding.name,
                        bound_ty.render_user_facing()
                    ),
                )
                .with_primary_span(Span {
                    file_id,
                    range: span,
                })
                .with_phase(DiagnosticPhase::Type),
            );
        }
    }
}

#[derive(Debug, Default)]
struct InterfaceMembers {
    /// (origin interface name, field name, field type)
    fields: Vec<(Name, Name, Option<baml_compiler2_ast::SpannedTypeExpr>)>,
    /// (origin interface name, required method name, signature)
    required_methods: Vec<(InterfaceMemberOrigin, Name, MethodSignature)>,
    /// (origin interface name, default method name, signature)
    default_methods: Vec<(InterfaceMemberOrigin, Name, MethodSignature)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct InterfaceMemberOrigin {
    name: Name,
    qualified_name: QualifiedTypeName,
    type_args: Vec<baml_compiler2_ast::TypeExpr>,
    lowered_type_args: Vec<Ty>,
}

impl InterfaceMemberOrigin {
    fn display_name(&self) -> Name {
        Name::new(self.qualified_name.render_user_facing())
    }
}

type InterfaceMemberStackEntry = (
    baml_compiler2_hir::item_tree::Interface,
    baml_base::SourceFile,
    std::collections::HashMap<Name, baml_compiler2_ast::TypeExpr>,
    Vec<baml_compiler2_ast::TypeExpr>,
    Vec<Name>,
);

fn interface_qtn_for_file(db: &dyn Db, file: SourceFile, name: &Name) -> QualifiedTypeName {
    let pkg_info = baml_compiler2_hir::file_package::file_package(db, file);
    QualifiedTypeName::new(
        pkg_info.package.clone(),
        pkg_info.namespace_path,
        name.clone(),
    )
}

fn lower_interface_origin_type_args(
    db: &dyn Db,
    file: SourceFile,
    type_args: &[baml_compiler2_ast::TypeExpr],
    generic_params: &[Name],
) -> Vec<Ty> {
    let pkg_info = baml_compiler2_hir::file_package::file_package(db, file);
    let pkg_id = baml_compiler2_hir::package::PackageId::new(db, pkg_info.package.clone());
    let pkg_items = baml_compiler2_hir::package::package_items(db, pkg_id);
    lower_interface_type_args_in_context(
        db,
        pkg_items,
        &pkg_info.namespace_path,
        type_args,
        generic_params,
    )
}

fn lower_interface_type_args_in_context(
    db: &dyn Db,
    pkg_items: &baml_compiler2_hir::package::PackageItems<'_>,
    namespace_path: &[Name],
    type_args: &[baml_compiler2_ast::TypeExpr],
    generic_params: &[Name],
) -> Vec<Ty> {
    let mut diags = Vec::new();
    type_args
        .iter()
        .map(|arg| {
            baml_compiler2_tir::lower_type_expr::lower_type_expr_in_ns(
                db,
                arg,
                pkg_items,
                namespace_path,
                generic_params,
                &mut diags,
            )
        })
        .collect()
}

/// Walk `extends` of `iface` (including `iface` itself) and gather all members
/// contributed up the chain. Methods are tagged with the interface they came
/// from so diagnostics can point at the right contract.
#[allow(dead_code)]
fn collect_interface_members<'db>(
    db: &'db dyn Db,
    iface: &baml_compiler2_hir::item_tree::Interface,
    iface_file: baml_base::SourceFile,
    pkg_items: &baml_compiler2_hir::package::PackageItems<'db>,
    namespace_path: &[Name],
) -> InterfaceMembers {
    collect_interface_members_with_subst(
        db,
        iface,
        iface_file,
        pkg_items,
        namespace_path,
        &std::collections::HashMap::new(),
        &[],
    )
}

/// Like [`collect_interface_members`] but applies a type-variable
/// substitution to every field, parameter, and return type. Used when an
/// `implements Container<int>` block needs the interface's `T`-typed
/// signatures rewritten to `int` before comparison.
fn collect_interface_members_with_subst<'db>(
    db: &'db dyn Db,
    iface: &baml_compiler2_hir::item_tree::Interface,
    iface_file: baml_base::SourceFile,
    _pkg_items: &baml_compiler2_hir::package::PackageItems<'db>,
    _namespace_path: &[Name],
    subst: &std::collections::HashMap<Name, baml_compiler2_ast::TypeExpr>,
    generic_params_in_scope: &[Name],
) -> InterfaceMembers {
    use baml_compiler2_ast::TypeExpr;

    let mut out = InterfaceMembers::default();
    let mut visited: HashSet<Vec<Name>> = HashSet::new();
    let root_type_args: Vec<baml_compiler2_ast::TypeExpr> = iface
        .generic_params
        .iter()
        .map(|param| {
            subst.get(param).cloned().unwrap_or_else(|| TypeExpr::Path {
                segments: vec![param.clone()],
                generic_args: Vec::new(),
                associated_type_bindings: Vec::new(),
                attrs: Vec::new(),
            })
        })
        .collect();
    let mut stack: Vec<InterfaceMemberStackEntry> =
        vec![(iface.clone(), iface_file, subst.clone(), root_type_args, {
            let mut params = iface.generic_params.clone();
            params.extend(generic_params_in_scope.iter().cloned());
            params
        })];
    visited.insert(vec![iface.name.clone()]);

    while let Some((
        current,
        current_file,
        current_subst,
        current_type_args,
        generic_params_in_scope,
    )) = stack.pop()
    {
        let origin = InterfaceMemberOrigin {
            name: current.name.clone(),
            qualified_name: interface_qtn_for_file(db, current_file, &current.name),
            lowered_type_args: lower_interface_origin_type_args(
                db,
                current_file,
                &current_type_args,
                &generic_params_in_scope,
            ),
            type_args: current_type_args.clone(),
        };
        for field in &current.fields {
            let substituted =
                field
                    .type_expr
                    .as_ref()
                    .map(|te| baml_compiler2_ast::SpannedTypeExpr {
                        expr: substitute_type_vars(&te.expr, &current_subst),
                        span: te.span,
                    });
            out.fields
                .push((origin.display_name(), field.name.clone(), substituted));
        }
        for sig in &current.required_methods {
            // Convert the HIR `FunctionParam` list (no AST defaults) into a
            // canonical signature for comparison.
            let ast_params: Vec<baml_compiler2_ast::Param> = sig
                .params
                .iter()
                .map(|p| baml_compiler2_ast::Param {
                    name: p.name.clone(),
                    type_expr: p.type_expr.clone(),
                    default: None,
                    span: p.span,
                    name_span: p.span,
                })
                .collect();
            let signature = MethodSignature::from_params_and_return_with_subst(
                &sig.generic_params,
                &sig.generic_param_bounds,
                &ast_params,
                sig.return_type.as_ref(),
                sig.throws.as_ref(),
                &current_subst,
            );
            out.required_methods
                .push((origin.clone(), sig.name.clone(), signature));
        }
        // Default-method ids point into the same file's item tree as the
        // interface itself — fetch each function's name + signature from
        // there.
        let cur_tree = baml_compiler2_hir::file_item_tree(db, current_file);
        for fid in &current.default_methods {
            if let Some(f) = cur_tree.functions.get(fid) {
                let ast_params: Vec<baml_compiler2_ast::Param> = f
                    .params
                    .iter()
                    .map(|p| baml_compiler2_ast::Param {
                        name: p.name.clone(),
                        type_expr: p.type_expr.clone(),
                        default: None,
                        span: p.span,
                        name_span: p.span,
                    })
                    .collect();
                let signature = MethodSignature::from_params_and_return_with_subst(
                    &f.generic_params,
                    &f.generic_param_bounds,
                    &ast_params,
                    f.return_type.as_ref(),
                    f.throws.as_ref(),
                    &current_subst,
                );
                out.default_methods
                    .push((origin.clone(), f.name.clone(), signature));
            }
        }

        for parent_te in &current.requires {
            let TypeExpr::Path { segments, .. } = &parent_te.expr else {
                continue;
            };
            if segments.is_empty() {
                continue;
            }
            if !visited.insert(segments.clone()) {
                continue;
            }
            let probe = TypeExpr::Path {
                segments: segments.clone(),
                generic_args: Vec::new(),
                associated_type_bindings: Vec::new(),
                attrs: Vec::new(),
            };
            let current_pkg_info = baml_compiler2_hir::file_package::file_package(db, current_file);
            let current_pkg_id =
                baml_compiler2_hir::package::PackageId::new(db, current_pkg_info.package.clone());
            let current_pkg_items = baml_compiler2_hir::package::package_items(db, current_pkg_id);
            if let Some(parent) = resolve_interface_path(
                db,
                &probe,
                current_pkg_items,
                &current_pkg_info.namespace_path,
            ) {
                let (parent_args, parent_assoc_bindings): (
                    &[baml_compiler2_ast::TypeExpr],
                    &[baml_compiler2_ast::AssociatedTypeBinding],
                ) = match &parent_te.expr {
                    TypeExpr::Path {
                        generic_args,
                        associated_type_bindings,
                        ..
                    } => (generic_args.as_slice(), associated_type_bindings.as_slice()),
                    _ => (&[][..], &[][..]),
                };
                let parent_args = parent_args
                    .iter()
                    .map(|arg| substitute_type_vars(arg, &current_subst))
                    .collect::<Vec<_>>();
                let parent_generic_subst: std::collections::HashMap<
                    Name,
                    baml_compiler2_ast::TypeExpr,
                > = parent
                    .iface
                    .generic_params
                    .iter()
                    .zip(parent_args.iter())
                    .map(|(param, arg)| (param.clone(), arg.clone()))
                    .collect();
                let parent_assoc_bindings = parent_assoc_bindings
                    .iter()
                    .map(|binding| baml_compiler2_ast::AssociatedTypeBinding {
                        name: binding.name.clone(),
                        ty: Box::new(substitute_type_vars(&binding.ty, &current_subst)),
                    })
                    .collect::<Vec<_>>();
                let parent_subst = associated_type_subst_from_type_args(
                    &parent.iface,
                    &parent_generic_subst,
                    &parent_assoc_bindings,
                );
                let mut parent_generic_params = generic_params_in_scope.clone();
                parent_generic_params.extend(parent.iface.generic_params.iter().cloned());
                stack.push((
                    parent.iface,
                    parent.loc.file(db),
                    parent_subst,
                    parent_args,
                    parent_generic_params,
                ));
            }
        }
    }

    out
}

/// Returns `true` if the type expression resolves to a type that exists
/// but is NOT an interface (e.g. a class or enum).
fn is_non_interface_type(
    target: &baml_compiler2_ast::TypeExpr,
    pkg_items: &baml_compiler2_hir::package::PackageItems<'_>,
    namespace_path: &[Name],
) -> bool {
    use baml_compiler2_ast::TypeExpr;
    use baml_compiler2_hir::contributions::Definition;

    let TypeExpr::Path { segments, .. } = target else {
        return false;
    };
    let Some((name, head)) = segments.split_last() else {
        return false;
    };
    let lookup_ns = path_lookup_namespace(head, namespace_path);
    // True for any name that resolves to a *non-interface* definition (class,
    // enum, type alias, …). We exclude the single interface kind rather than
    // enumerate every non-interface kind: TIR lowering emits
    // `NonInterfaceProjectionQualifier` for all of them, and an allow-list here
    // silently regressed type aliases (a duplicate "unknown interface", and the
    // `requires`/`implement` sites mislabeling an existing alias as "doesn't
    // exist"). An unresolved name is *not* a non-interface — it's "doesn't
    // exist" — so `None` stays `false`.
    match pkg_items.lookup_type(lookup_ns, name) {
        Some(Definition::Interface(_)) => false,
        Some(_) => true,
        None => false,
    }
}

fn rendered_type_args(args: &[baml_compiler2_ast::TypeExpr]) -> Vec<String> {
    args.iter().map(ToString::to_string).collect()
}

fn type_args_from_target_expr(
    target: &baml_compiler2_ast::TypeExpr,
) -> Vec<baml_compiler2_ast::TypeExpr> {
    match target {
        baml_compiler2_ast::TypeExpr::Path { generic_args, .. } => generic_args.clone(),
        _ => Vec::new(),
    }
}

fn interface_origin_matches_target_expr<'db>(
    db: &'db dyn Db,
    target: &baml_compiler2_ast::TypeExpr,
    pkg_items: &baml_compiler2_hir::package::PackageItems<'db>,
    namespace_path: &[Name],
    generic_params: &[Name],
    aliases: &std::collections::HashMap<QualifiedTypeName, Ty>,
    origin: &InterfaceMemberOrigin,
) -> bool {
    let Some(resolved) = resolve_interface_path(db, target, pkg_items, namespace_path) else {
        return false;
    };
    if resolved.qtn != origin.qualified_name {
        return false;
    }
    let target_type_args = type_args_from_target_expr(target);
    if target_type_args.len() != origin.type_args.len() {
        return false;
    }
    let target_lowered = lower_interface_type_args_in_context(
        db,
        pkg_items,
        namespace_path,
        &target_type_args,
        generic_params,
    );
    if target_lowered.len() == origin.lowered_type_args.len()
        && target_lowered
            .iter()
            .zip(origin.lowered_type_args.iter())
            .all(|(target_arg, origin_arg)| {
                baml_compiler2_tir::normalize::is_same_normalized_type(
                    target_arg, origin_arg, aliases,
                )
            })
    {
        return true;
    }

    rendered_type_args(&target_type_args) == rendered_type_args(&origin.type_args)
}

struct InterfaceValidationCtx<'db, 'a> {
    db: &'db dyn Db,
    pkg_items: &'a baml_compiler2_hir::package::PackageItems<'db>,
    namespace_path: &'a [Name],
    aliases: &'a std::collections::HashMap<QualifiedTypeName, Ty>,
}

fn interface_target_matches_required_parent(
    ctx: &InterfaceValidationCtx<'_, '_>,
    candidate_target: &baml_compiler2_ast::TypeExpr,
    candidate_namespace_path: &[Name],
    candidate_bindings: &[baml_compiler2_ast::AssociatedTypeBindingDef],
    required_parent: &baml_compiler2_ast::TypeExpr,
    required_parent_namespace_path: &[Name],
    generic_params: &[Name],
) -> bool {
    let Some(candidate) = resolve_interface_path(
        ctx.db,
        candidate_target,
        ctx.pkg_items,
        candidate_namespace_path,
    ) else {
        return false;
    };
    let Some(required) = resolve_interface_path(
        ctx.db,
        required_parent,
        ctx.pkg_items,
        required_parent_namespace_path,
    ) else {
        return false;
    };
    if candidate.qtn != required.qtn {
        return false;
    }

    let candidate_args = lower_path_generic_args(
        ctx.db,
        candidate_target,
        ctx.pkg_items,
        candidate_namespace_path,
        generic_params,
    );
    let required_args = lower_path_generic_args(
        ctx.db,
        required_parent,
        ctx.pkg_items,
        required_parent_namespace_path,
        generic_params,
    );
    if candidate_args.len() != required_args.len()
        || !candidate_args
            .iter()
            .zip(required_args.iter())
            .all(|(candidate_arg, required_arg)| {
                baml_compiler2_tir::normalize::is_same_normalized_type(
                    candidate_arg,
                    required_arg,
                    ctx.aliases,
                )
            })
    {
        return false;
    }

    let candidate_generic_subst: std::collections::HashMap<Name, baml_compiler2_ast::TypeExpr> =
        match candidate_target {
            baml_compiler2_ast::TypeExpr::Path { generic_args, .. } => candidate
                .iface
                .generic_params
                .iter()
                .zip(generic_args.iter())
                .map(|(param, arg)| (param.clone(), arg.clone()))
                .collect(),
            _ => std::collections::HashMap::new(),
        };
    let candidate_subst = associated_type_subst_from_bindings(
        &candidate.iface,
        &candidate_generic_subst,
        candidate_bindings,
    );

    let required_generic_subst: std::collections::HashMap<Name, baml_compiler2_ast::TypeExpr> =
        match required_parent {
            baml_compiler2_ast::TypeExpr::Path { generic_args, .. } => required
                .iface
                .generic_params
                .iter()
                .zip(generic_args.iter())
                .map(|(param, arg)| (param.clone(), arg.clone()))
                .collect(),
            _ => std::collections::HashMap::new(),
        };
    let required_assoc_bindings = match required_parent {
        baml_compiler2_ast::TypeExpr::Path {
            associated_type_bindings,
            ..
        } => associated_type_bindings.as_slice(),
        _ => &[][..],
    };
    let required_subst = associated_type_subst_from_type_args(
        &required.iface,
        &required_generic_subst,
        required_assoc_bindings,
    );

    required
        .iface
        .associated_types
        .iter()
        .filter_map(|assoc| required_subst.get(&assoc.name).map(|ty| (&assoc.name, ty)))
        .all(|(assoc_name, required_ty_expr)| {
            let Some(candidate_ty_expr) = candidate_subst.get(assoc_name) else {
                return false;
            };
            let mut candidate_diags = Vec::new();
            let candidate_ty = baml_compiler2_tir::lower_type_expr::lower_type_expr_in_ns(
                ctx.db,
                candidate_ty_expr,
                ctx.pkg_items,
                candidate_namespace_path,
                generic_params,
                &mut candidate_diags,
            );
            let mut required_diags = Vec::new();
            let required_ty = baml_compiler2_tir::lower_type_expr::lower_type_expr_in_ns(
                ctx.db,
                required_ty_expr,
                ctx.pkg_items,
                required_parent_namespace_path,
                generic_params,
                &mut required_diags,
            );
            candidate_diags.is_empty()
                && required_diags.is_empty()
                && baml_compiler2_tir::normalize::is_same_normalized_type(
                    &candidate_ty,
                    &required_ty,
                    ctx.aliases,
                )
        })
}

fn implements_for_targets_match(
    ctx: &InterfaceValidationCtx<'_, '_>,
    lhs: &baml_compiler2_ast::TypeExpr,
    lhs_generic_params: &[Name],
    rhs: &baml_compiler2_ast::TypeExpr,
    rhs_generic_params: &[Name],
) -> bool {
    let mut lhs_diags = Vec::new();
    let mut rhs_diags = Vec::new();
    let lhs_ty = baml_compiler2_tir::lower_type_expr::lower_type_expr_in_ns(
        ctx.db,
        lhs,
        ctx.pkg_items,
        ctx.namespace_path,
        lhs_generic_params,
        &mut lhs_diags,
    );
    let rhs_ty = baml_compiler2_tir::lower_type_expr::lower_type_expr_in_ns(
        ctx.db,
        rhs,
        ctx.pkg_items,
        ctx.namespace_path,
        rhs_generic_params,
        &mut rhs_diags,
    );
    lhs_diags.is_empty()
        && rhs_diags.is_empty()
        && (baml_compiler2_tir::normalize::is_same_normalized_type(&lhs_ty, &rhs_ty, ctx.aliases)
            || baml_compiler2_tir::interfaces::match_ty_pattern(
                &lhs_ty,
                &rhs_ty,
                lhs_generic_params,
                ctx.aliases,
            )
            .is_some()
            || baml_compiler2_tir::interfaces::match_ty_pattern(
                &rhs_ty,
                &lhs_ty,
                rhs_generic_params,
                ctx.aliases,
            )
            .is_some())
}

fn implements_for_target_matches_class(
    ctx: &InterfaceValidationCtx<'_, '_>,
    target: &baml_compiler2_ast::TypeExpr,
    target_generic_params: &[Name],
    class: &baml_compiler2_ast::ClassDef,
) -> bool {
    use baml_compiler2_hir::contributions::Definition;

    let Some(Definition::Class(class_loc)) =
        ctx.pkg_items.lookup_type(ctx.namespace_path, &class.name)
    else {
        return false;
    };
    let class_qtn = baml_compiler2_tir::lower_type_expr::qualify_def(
        ctx.db,
        Definition::Class(class_loc),
        &class.name,
    );
    let class_ty = Ty::Class(
        class_qtn,
        class
            .generic_params
            .iter()
            .map(|param| Ty::TypeVar(param.clone(), TyAttr::default()))
            .collect(),
        TyAttr::default(),
    );
    let mut target_diags = Vec::new();
    let target_ty = baml_compiler2_tir::lower_type_expr::lower_type_expr_in_ns(
        ctx.db,
        target,
        ctx.pkg_items,
        ctx.namespace_path,
        target_generic_params,
        &mut target_diags,
    );
    target_diags.is_empty()
        && (baml_compiler2_tir::normalize::is_same_normalized_type(
            &target_ty,
            &class_ty,
            ctx.aliases,
        ) || baml_compiler2_tir::interfaces::match_ty_pattern(
            &class_ty,
            &target_ty,
            &class.generic_params,
            ctx.aliases,
        )
        .is_some()
            || baml_compiler2_tir::interfaces::match_ty_pattern(
                &target_ty,
                &class_ty,
                target_generic_params,
                ctx.aliases,
            )
            .is_some())
}

fn item_implements_required_parent_for_target(
    ctx: &InterfaceValidationCtx<'_, '_>,
    item: &baml_compiler2_ast::Item,
    current: &baml_compiler2_ast::ImplementsForDef,
    current_generic_params: &[Name],
    required_parent: &baml_compiler2_ast::TypeExpr,
    required_parent_namespace_path: &[Name],
) -> bool {
    match item {
        baml_compiler2_ast::Item::ImplementsFor(candidate) => {
            if candidate.span == current.span {
                return false;
            }
            let candidate_generic_params: Vec<Name> = candidate
                .generic_params
                .iter()
                .map(|(name, _)| name.clone())
                .collect();
            implements_for_targets_match(
                ctx,
                &candidate.for_target.expr,
                &candidate_generic_params,
                &current.for_target.expr,
                current_generic_params,
            ) && interface_target_matches_required_parent(
                ctx,
                &candidate.interface_target.expr,
                ctx.namespace_path,
                &candidate.associated_type_bindings,
                required_parent,
                required_parent_namespace_path,
                &candidate_generic_params,
            )
        }
        baml_compiler2_ast::Item::Class(class) => {
            implements_for_target_matches_class(
                ctx,
                &current.for_target.expr,
                current_generic_params,
                class,
            ) && class.implements.iter().any(|candidate| {
                interface_target_matches_required_parent(
                    ctx,
                    &candidate.target.expr,
                    ctx.namespace_path,
                    &candidate.associated_type_bindings,
                    required_parent,
                    required_parent_namespace_path,
                    &class.generic_params,
                )
            })
        }
        _ => false,
    }
}

fn has_sibling_implements_for_origin(
    ctx: &InterfaceValidationCtx<'_, '_>,
    current: &baml_compiler2_ast::ImplementsForDef,
    all_items: &[baml_compiler2_ast::Item],
    origin: &InterfaceMemberOrigin,
) -> bool {
    all_items.iter().any(|item| {
        let baml_compiler2_ast::Item::ImplementsFor(candidate) = item else {
            return false;
        };
        if candidate.span == current.span {
            return false;
        }
        let candidate_generic_params: Vec<Name> = candidate
            .generic_params
            .iter()
            .map(|(name, _)| name.clone())
            .collect();
        let current_generic_params: Vec<Name> = current
            .generic_params
            .iter()
            .map(|(name, _)| name.clone())
            .collect();
        implements_for_targets_match(
            ctx,
            &candidate.for_target.expr,
            &candidate_generic_params,
            &current.for_target.expr,
            &current_generic_params,
        ) && interface_origin_matches_target_expr(
            ctx.db,
            &candidate.interface_target.expr,
            ctx.pkg_items,
            ctx.namespace_path,
            &candidate_generic_params,
            ctx.aliases,
            origin,
        )
    })
}

fn lower_path_generic_args<'db>(
    db: &'db dyn Db,
    expr: &baml_compiler2_ast::TypeExpr,
    pkg_items: &baml_compiler2_hir::package::PackageItems<'db>,
    namespace_path: &[Name],
    generic_params: &[Name],
) -> Vec<Ty> {
    let baml_compiler2_ast::TypeExpr::Path { generic_args, .. } = expr else {
        return Vec::new();
    };
    let mut diags = Vec::new();
    generic_args
        .iter()
        .map(|arg| {
            baml_compiler2_tir::lower_type_expr::lower_type_expr_in_ns(
                db,
                arg,
                pkg_items,
                namespace_path,
                generic_params,
                &mut diags,
            )
        })
        .collect()
}

fn validate_interface_extends_fields(
    ctx: &InterfaceValidationCtx<'_, '_>,
    file: SourceFile,
    file_id: FileId,
    iface: &baml_compiler2_ast::InterfaceDef,
    diagnostics: &mut Vec<Diagnostic>,
) {
    use baml_compiler2_hir::diagnostic::Hir2Diagnostic;

    let mut seen: IndexMap<Name, (Name, baml_compiler2_ast::TypeExpr, String)> = IndexMap::new();

    // Seed with the interface's own fields.
    for field in &iface.fields {
        if let Some(te) = &field.type_expr {
            seen.insert(
                field.name.clone(),
                (iface.name.clone(), te.expr.clone(), format!("{}", te.expr)),
            );
        }
    }

    // Walk each parent via resolve and collect its members.
    for parent_te in &iface.requires {
        let Some(parent) =
            resolve_interface_path(ctx.db, &parent_te.expr, ctx.pkg_items, ctx.namespace_path)
        else {
            continue;
        };
        let members = collect_interface_members_with_subst(
            ctx.db,
            &parent.iface,
            parent.loc.file(ctx.db),
            ctx.pkg_items,
            ctx.namespace_path,
            &std::collections::HashMap::new(),
            &iface.generic_params,
        );
        for (origin, field_name, field_te) in &members.fields {
            let Some(field_te) = field_te else { continue };
            let ty_str = format!("{}", field_te.expr);
            if let Some((existing_origin, existing_ty, existing_rendered)) = seen.get(field_name) {
                if !type_exprs_compatible(
                    ctx.db,
                    ctx.pkg_items,
                    ctx.namespace_path,
                    &iface.generic_params,
                    existing_ty,
                    ctx.pkg_items,
                    ctx.namespace_path,
                    &iface.generic_params,
                    &field_te.expr,
                    ctx.aliases,
                ) {
                    diagnostics.push(
                        Hir2Diagnostic::InterfaceExtendsFieldConflict {
                            interface_name: Name::new(
                                interface_qtn_for_file(ctx.db, file, &iface.name)
                                    .render_user_facing(),
                            ),
                            field_name: field_name.clone(),
                            first_interface: existing_origin.clone(),
                            first_type: existing_rendered.clone(),
                            second_interface: origin.clone(),
                            second_type: ty_str,
                            span: iface.name_span,
                        }
                        .to_diagnostic(file_id),
                    );
                }
            } else {
                seen.insert(
                    field_name.clone(),
                    (origin.clone(), field_te.expr.clone(), ty_str),
                );
            }
        }
    }
}

/// Fully expand every `Ty::TypeAlias` in `ty` — at the top level *and* at every
/// nesting depth — so callers reason about the concrete type rather than opaque
/// aliases. Used to build duplicate-`implements` dedup keys: `Container<List<A>>`
/// and `Container<List<int>>` (where `type A = int`) must produce the same key,
/// which requires expanding the alias nested inside the `List`, not just a
/// top-level alias.
///
/// Recursive aliases (`type A = List<A>`) survive lowering unexpanded; the
/// `seen` set guards against looping on those by leaving an already-visited
/// alias in place once re-encountered along a single chain.
fn expand_type_alias(ty: &Ty, aliases: &std::collections::HashMap<QualifiedTypeName, Ty>) -> Ty {
    expand_type_alias_rec(ty, aliases, &mut HashSet::new())
}

/// Recursive worker for [`expand_type_alias`]. `seen` tracks the aliases already
/// followed along the current chain so a self-referential alias terminates
/// instead of recursing forever.
fn expand_type_alias_rec(
    ty: &Ty,
    aliases: &std::collections::HashMap<QualifiedTypeName, Ty>,
    seen: &mut HashSet<QualifiedTypeName>,
) -> Ty {
    let recurse =
        |sub: &Ty, seen: &mut HashSet<QualifiedTypeName>| expand_type_alias_rec(sub, aliases, seen);
    match ty {
        // Resolve the alias to its definition, then keep expanding — the
        // definition may itself be (or contain) further aliases. A repeated
        // alias on this chain is a cycle: leave it unexpanded and stop.
        Ty::TypeAlias(qtn, _) => {
            if !seen.insert(qtn.clone()) {
                return ty.clone();
            }
            let expanded = match aliases.get(qtn) {
                Some(next) => recurse(next, seen),
                None => ty.clone(),
            };
            seen.remove(qtn);
            expanded
        }

        // Compound types: rebuild with every sub-type expanded.
        Ty::Class(name, args, attr) => Ty::Class(
            name.clone(),
            args.iter().map(|a| recurse(a, seen)).collect(),
            attr.clone(),
        ),
        Ty::Interface(name, args, assoc, attr) => Ty::Interface(
            name.clone(),
            args.iter().map(|a| recurse(a, seen)).collect(),
            assoc
                .iter()
                .map(|(n, t)| (n.clone(), recurse(t, seen)))
                .collect(),
            attr.clone(),
        ),
        Ty::List(inner, attr) => Ty::List(Box::new(recurse(inner, seen)), attr.clone()),
        Ty::Map { key, value, attr } => Ty::Map {
            key: Box::new(recurse(key, seen)),
            value: Box::new(recurse(value, seen)),
            attr: attr.clone(),
        },
        Ty::Union(members, attr) => Ty::Union(
            members.iter().map(|m| recurse(m, seen)).collect(),
            attr.clone(),
        ),
        Ty::Function {
            params,
            ret,
            throws,
            attr,
        } => Ty::Function {
            params: params
                .iter()
                .map(|p| baml_compiler2_tir::ty::FunctionParamTy {
                    name: p.name.clone(),
                    ty: recurse(&p.ty, seen),
                    mode: p.mode,
                })
                .collect(),
            ret: Box::new(recurse(ret, seen)),
            throws: Box::new(recurse(throws, seen)),
            attr: attr.clone(),
        },
        Ty::Future(value, error, attr) => Ty::Future(
            Box::new(recurse(value, seen)),
            Box::new(recurse(error, seen)),
            attr.clone(),
        ),
        Ty::WatchAccessor(inner, attr) => {
            Ty::WatchAccessor(Box::new(recurse(inner, seen)), attr.clone())
        }
        Ty::AssociatedTypeProjection {
            base,
            interface,
            member,
            attr,
        } => Ty::AssociatedTypeProjection {
            base: Box::new(recurse(base, seen)),
            interface: interface
                .as_ref()
                .map(|i| Box::new(i.map_tys(|t| recurse(t, seen)))),
            member: member.clone(),
            attr: attr.clone(),
        },
        Ty::EvolvingList(inner, attr) => {
            Ty::EvolvingList(Box::new(recurse(inner, seen)), attr.clone())
        }
        Ty::EvolvingMap(key, value, attr) => Ty::EvolvingMap(
            Box::new(recurse(key, seen)),
            Box::new(recurse(value, seen)),
            attr.clone(),
        ),

        // Leaf types: no nested `Ty`, so nothing to expand. Listed exhaustively
        // (no catch-all) so a future sub-type-carrying variant fails to compile
        // here rather than silently evading alias expansion.
        Ty::Int { .. }
        | Ty::Bigint { .. }
        | Ty::Float { .. }
        | Ty::String { .. }
        | Ty::Bool { .. }
        | Ty::Null { .. }
        | Ty::Uint8Array { .. }
        | Ty::Media(..)
        | Ty::Literal(..)
        | Ty::Enum(..)
        | Ty::EnumVariant(..)
        | Ty::RustType { .. }
        | Ty::Type { .. }
        | Ty::Resource { .. }
        | Ty::PromptAst { .. }
        | Ty::Void { .. }
        | Ty::TypeVar(..)
        | Ty::BuiltinUnknown { .. }
        | Ty::Never { .. }
        | Ty::Unknown { .. }
        | Ty::Error { .. } => ty.clone(),
    }
}

/// Outcome of the BEP-044 orphan-rule check for an out-of-body impl.
enum OrphanCheck<'a> {
    Ok,
    /// An uncovered type parameter appears before any local type in `[for, args]`.
    UncoveredParam(&'a Name),
    /// No type local to the impl's package appears anywhere in `[for, args]`.
    NoLocalType,
}

/// The orphan rule — Rust's RFC 2451 "covered" rule, per BEP-044. An out-of-body
/// `implement<P..> I<args..> for T` is allowed only if `I` is local to
/// `current_package`, or — scanning the input types `[T, args..]` left to right —
/// a type local to `current_package` appears before any *uncovered* type
/// parameter. This keeps interface coherence checkable per package (a foreign
/// interface can only be implemented when anchored on a local type, so two
/// packages can never author colliding impls).
///
/// BAML has no fundamental (transparent-wrapper) types, so a parameter is
/// "uncovered" exactly when it is an input's own root, and non-local
/// constructors are opaque (their type args don't participate). Associated
/// bindings are excluded (they're outputs). In-body impls are exempt — their
/// `for` type is the enclosing class, which is always local — so this is applied
/// only to out-of-body `implement … for …`.
fn orphan_check<'a>(
    current_package: &Name,
    iface_qtn: &QualifiedTypeName,
    for_ty: &'a Ty,
    iface_args: &'a [Ty],
) -> OrphanCheck<'a> {
    // Implementing your own interface is always allowed, for any type.
    if iface_qtn.package() == current_package {
        return OrphanCheck::Ok;
    }
    for input in std::iter::once(for_ty).chain(iface_args.iter()) {
        match input {
            Ty::Class(tn, ..) | Ty::Enum(tn, ..) if tn.package() == current_package => {
                return OrphanCheck::Ok;
            }
            Ty::TypeVar(name, _) => return OrphanCheck::UncoveredParam(name),
            _ => {}
        }
    }
    OrphanCheck::NoLocalType
}

#[expect(clippy::too_many_arguments)]
fn validate_implements_for<'db>(
    db: &'db dyn Db,
    file_id: FileId,
    current_package: &Name,
    imp: &baml_compiler2_ast::ImplementsForDef,
    all_items: &[baml_compiler2_ast::Item],
    pkg_items: &baml_compiler2_hir::package::PackageItems<'db>,
    namespace_path: &[Name],
    aliases: &std::collections::HashMap<QualifiedTypeName, Ty>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    use baml_compiler2_hir::diagnostic::Hir2Diagnostic;

    let target_name = Name::new(format!("{}", imp.for_target.expr));
    let generic_param_names: Vec<Name> =
        imp.generic_params.iter().map(|(n, _)| n.clone()).collect();
    // Single-bound view (first `&`-bound only) — unchanged behavior.
    let generic_param_bounds: Vec<Option<baml_compiler2_ast::TypeExpr>> = imp
        .generic_params
        .iter()
        .map(|(_, bounds)| bounds.first().cloned())
        .collect();
    let generic_bounds = generic_bound_expr_map(&generic_param_names, &generic_param_bounds);
    let ctx = InterfaceValidationCtx {
        db,
        pkg_items,
        namespace_path,
        aliases,
    };
    // Lower the for-target purely to drive the concreteness/orphan logic below.
    // Lowering diagnostics (unresolved types, etc.) are emitted by `impl_data`
    // (the single source for impl type-expr diagnostics) and surfaced once at the
    // impl span — so they are collected into a throwaway sink here, not re-emitted.
    // An unresolvable target can't be orphan-checked, so bail.
    let mut target_type_errors = Vec::new();
    let target_ty = baml_compiler2_tir::lower_type_expr::lower_type_expr_in_ns(
        db,
        &imp.for_target.expr,
        pkg_items,
        namespace_path,
        &generic_param_names,
        &mut target_type_errors,
    );
    if !target_type_errors.is_empty() {
        return;
    }

    // BEP-044: only a *concrete* type may implement an interface (see
    // `Ty::is_valid_impl_subject` for the full classification — e.g. a
    // union/optional/interface has no single implementor, and a literal or enum
    // variant is a singleton subtype whose values dispatch through their base).
    //
    // Aliases are expanded first so the gate sees through them — otherwise
    // `type U = int | string; implements I for U {}` would slip past as an
    // opaque `Ty::TypeAlias`.
    let resolved_target = expand_type_alias(&target_ty, aliases);
    if !resolved_target.is_valid_impl_subject() {
        diagnostics.push(
            Diagnostic::error(
                DiagnosticId::ImplTargetNotConcrete,
                format!(
                    "cannot implement interface `{}` for `{}`: only concrete types may \
                     implement interfaces",
                    imp.interface_target.expr, imp.for_target.expr
                ),
            )
            .with_primary_span(Span {
                file_id,
                range: imp.for_target.span,
            })
            .with_phase(DiagnosticPhase::Type),
        );
        return;
    }

    // BEP-044 wf3 #G10: every declared generic parameter must be determined by
    // the implementor (`for`) type. A param that appears only in the interface
    // arguments (`implements<T> Tagged<T> for Holder`) is unconstrained, hence
    // unsound — the runtime can't recover `T`. Reject it.
    {
        let mut bound_in_target: HashSet<Name> = HashSet::new();
        collect_type_var_names(
            &imp.for_target.expr,
            &generic_param_names,
            &mut bound_in_target,
        );
        for (name, _) in &imp.generic_params {
            if !bound_in_target.contains(name) {
                diagnostics.push(
                    Diagnostic::error(
                        DiagnosticId::UnconstrainedImplTypeParam,
                        format!(
                            "type parameter `{name}` is not constrained by the implementor type \
                             `{}`; it appears only in the interface arguments",
                            imp.for_target.expr
                        ),
                    )
                    .with_primary_span(Span {
                        file_id,
                        range: imp.interface_target.span,
                    })
                    .with_phase(DiagnosticPhase::Type),
                );
            }
        }
    }

    let Some(resolved_iface) =
        resolve_interface_path(db, &imp.interface_target.expr, pkg_items, namespace_path)
    else {
        let is_non_interface =
            is_non_interface_type(&imp.interface_target.expr, pkg_items, namespace_path);
        if is_non_interface {
            diagnostics.push(
                Hir2Diagnostic::NotAnInterface {
                    class_name: target_name,
                    target_name: format!("{}", imp.interface_target.expr),
                    span: imp.interface_target.span,
                }
                .to_diagnostic(file_id),
            );
        } else {
            diagnostics.push(
                Hir2Diagnostic::UnknownInterface {
                    class_name: target_name,
                    target_name: format!("{}", imp.interface_target.expr),
                    span: imp.interface_target.span,
                }
                .to_diagnostic(file_id),
            );
        }
        return;
    };

    let iface_display_name = resolved_iface.display_name();
    let iface_qtn = resolved_iface.qtn.clone();

    // BEP-044 orphan rule (RFC 2451 covered rule): a foreign interface may only be
    // implemented out-of-body when anchored on a type local to this package,
    // before any uncovered type parameter — see `orphan_check`.
    {
        let mut iface_lower_errs = Vec::new();
        let iface_ty = baml_compiler2_tir::lower_type_expr::lower_type_expr_in_ns(
            db,
            &imp.interface_target.expr,
            pkg_items,
            namespace_path,
            &generic_param_names,
            &mut iface_lower_errs,
        );
        // Surface errors lowering the interface target (unknown type, wrong generic
        // arity, …) and stop — exactly as the for-target does above. Otherwise an
        // out-of-body `implement BadIface<a, b> for Bar` would silently swallow these
        // and run the orphan check on a degraded `iface_ty` (the in-body path reports
        // them, so this kept the two paths inconsistent).
        if !iface_lower_errs.is_empty() {
            for error in iface_lower_errs {
                diagnostics.push(
                    Diagnostic::error(tir_type_error_to_diagnostic_id(&error), error.to_string())
                        .with_primary_span(Span {
                            file_id,
                            range: imp.interface_target.span,
                        })
                        .with_phase(DiagnosticPhase::Type),
                );
            }
            return;
        }
        let iface_args: Vec<Ty> = match expand_type_alias(&iface_ty, aliases) {
            Ty::Interface(_, args, _, _) => {
                args.iter().map(|a| expand_type_alias(a, aliases)).collect()
            }
            _ => Vec::new(),
        };
        let orphan_diag = |detail: String| {
            Diagnostic::error(
                DiagnosticId::ImplViolatesOrphanRule,
                format!(
                    "cannot implement foreign interface `{}` for `{}`: {detail} (orphan rule)",
                    imp.interface_target.expr, imp.for_target.expr
                ),
            )
            .with_primary_span(Span {
                file_id,
                range: imp.for_target.span,
            })
            .with_phase(DiagnosticPhase::Type)
        };
        match orphan_check(current_package, &iface_qtn, &resolved_target, &iface_args) {
            OrphanCheck::Ok => {}
            OrphanCheck::UncoveredParam(param) => diagnostics.push(orphan_diag(format!(
                "the type parameter `{param}` is not covered by a type local to this package; \
                 a local type must appear before any uncovered type parameter"
            ))),
            OrphanCheck::NoLocalType => diagnostics.push(orphan_diag(
                "neither the interface nor any type in the impl is defined in this package"
                    .to_string(),
            )),
        }
    }

    let iface_file = resolved_iface.loc.file(db);
    let iface = resolved_iface.iface;
    let iface_pkg_info = baml_compiler2_hir::file_package::file_package(db, iface_file);
    let iface_pkg_id =
        baml_compiler2_hir::package::PackageId::new(db, iface_pkg_info.package.clone());
    let iface_pkg_items = baml_compiler2_hir::package::package_items(db, iface_pkg_id);
    let iface_namespace_path = iface_pkg_info.namespace_path;
    let generic_subst: std::collections::HashMap<Name, baml_compiler2_ast::TypeExpr> =
        match &imp.interface_target.expr {
            baml_compiler2_ast::TypeExpr::Path { generic_args, .. } => iface
                .generic_params
                .iter()
                .zip(generic_args.iter())
                .map(|(p, a)| (p.clone(), a.clone()))
                .collect(),
            _ => std::collections::HashMap::new(),
        };
    let target_associated_type_bindings = match &imp.interface_target.expr {
        baml_compiler2_ast::TypeExpr::Path {
            associated_type_bindings,
            ..
        } => associated_type_bindings.as_slice(),
        _ => &[][..],
    };
    validate_no_associated_type_bindings_on_implements_target(
        file_id,
        imp.interface_target.span,
        target_associated_type_bindings,
        diagnostics,
    );
    validate_associated_type_binding_defs(
        db,
        file_id,
        iface_file.file_id(db),
        imp.interface_target.span,
        &iface,
        &iface_display_name,
        iface_pkg_items,
        pkg_items,
        &iface_namespace_path,
        namespace_path,
        &generic_param_names,
        &generic_bounds,
        &generic_subst,
        &imp.associated_type_bindings,
        aliases,
        diagnostics,
    );
    let subst =
        associated_type_subst_from_bindings(&iface, &generic_subst, &imp.associated_type_bindings);
    let members = collect_interface_members_with_subst(
        db,
        &iface,
        iface_file,
        pkg_items,
        namespace_path,
        &subst,
        &[],
    );

    if !members.fields.is_empty() {
        diagnostics.push(
            Hir2Diagnostic::OutOfBodyImplementsFieldInterface {
                target_name: target_name.to_string(),
                interface_name: iface_display_name,
                span: imp.interface_target.span,
            }
            .to_diagnostic(file_id),
        );
        return;
    }

    let mut provided_method_names: HashSet<Name> = HashSet::new();
    for method in &imp.methods {
        let expected_sig = members
            .required_methods
            .iter()
            .find_map(|(_, name, sig)| {
                if *name == method.name {
                    Some(sig.clone())
                } else {
                    None
                }
            })
            .or_else(|| {
                members.default_methods.iter().find_map(|(_, name, sig)| {
                    if *name == method.name {
                        Some(sig.clone())
                    } else {
                        None
                    }
                })
            });
        match expected_sig {
            None => diagnostics.push(
                Hir2Diagnostic::UnknownInterfaceMember {
                    interface_name: iface_display_name.clone(),
                    method_name: method.name.clone(),
                    span: method.name_span,
                }
                .to_diagnostic(file_id),
            ),
            Some(expected) => {
                let actual_subst = subst_without_names(&subst, &generic_param_names);
                let actual = MethodSignature::from_params_and_return_with_subst(
                    &method.generic_params,
                    &method.generic_param_bounds,
                    &method.params,
                    method.return_type.as_ref(),
                    method.throws.as_ref(),
                    &actual_subst,
                );
                if !expected.matches(
                    &actual,
                    SignatureMatchContext {
                        db,
                        expected_pkg_items: iface_pkg_items,
                        expected_namespace_path: &iface_namespace_path,
                        actual_pkg_items: pkg_items,
                        actual_namespace_path: namespace_path,
                        aliases,
                        ignore_param_names: false,
                        outer_generic_params: &generic_param_names,
                    },
                ) {
                    diagnostics.push(
                        Hir2Diagnostic::InterfaceMethodSignatureMismatch {
                            class_name: target_name.clone(),
                            interface_name: iface_display_name.clone(),
                            method_name: method.name.clone(),
                            actual: actual.render(),
                            expected: expected.render(),
                            span: method.name_span,
                        }
                        .to_diagnostic(file_id),
                    );
                }
            }
        }
        provided_method_names.insert(method.name.clone());
    }

    for (origin, req_name, _sig) in &members.required_methods {
        // Only `implements`-block members (+ inherited defaults) satisfy a
        // requirement; a same-named method on the target class does not (BEP-044).
        if provided_method_names.contains(req_name) {
            continue;
        }
        if origin.qualified_name != iface_qtn
            && has_sibling_implements_for_origin(&ctx, imp, all_items, origin)
        {
            continue;
        }
        diagnostics.push(
            Hir2Diagnostic::MissingInterfaceMethod {
                class_name: target_name.clone(),
                interface_name: origin.display_name(),
                method_name: req_name.clone(),
                span: imp.span,
            }
            .to_diagnostic(file_id),
        );
    }

    if !iface.requires.is_empty() {
        let missing: Vec<Name> = iface
            .requires
            .iter()
            .filter_map(|parent_te| {
                let required_parent = substitute_type_vars(&parent_te.expr, &subst);
                let baml_compiler2_ast::TypeExpr::Path { segments, .. } = &required_parent else {
                    return None;
                };
                let parent_name =
                    resolve_interface_path(db, &required_parent, pkg_items, &iface_namespace_path)
                        .map(|_| Name::new(required_parent.to_string()))
                        .or_else(|| segments.last().cloned())?;
                let target_implements_it = all_items.iter().any(|item| {
                    item_implements_required_parent_for_target(
                        &ctx,
                        item,
                        imp,
                        &generic_param_names,
                        &required_parent,
                        &iface_namespace_path,
                    )
                });
                if target_implements_it {
                    None
                } else {
                    Some(parent_name)
                }
            })
            .collect();
        if !missing.is_empty() {
            diagnostics.push(
                Hir2Diagnostic::MissingRequiredInterface {
                    class_name: target_name,
                    interface_name: iface_display_name,
                    missing_parents: missing,
                    span: imp.interface_target.span,
                }
                .to_diagnostic(file_id),
            );
        }
    }
}

fn validate_class_implements<'db>(
    db: &'db dyn Db,
    file_id: FileId,
    class: &baml_compiler2_ast::ClassDef,
    pkg_items: &baml_compiler2_hir::package::PackageItems<'db>,
    namespace_path: &[Name],
    aliases: &std::collections::HashMap<QualifiedTypeName, Ty>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    use baml_compiler2_hir::diagnostic::Hir2Diagnostic;

    let ctx = InterfaceValidationCtx {
        db,
        pkg_items,
        namespace_path,
        aliases,
    };
    let mut seen_targets: IndexMap<String, (Name, Vec<TextRange>)> = IndexMap::new();
    for block in &class.implements {
        let Some(resolved_iface) =
            resolve_interface_path(db, &block.target.expr, pkg_items, namespace_path)
        else {
            continue;
        };
        let iface_display_name = resolved_iface.display_name();
        // Use the resolved interface identity plus its *lowered* type arguments
        // for duplicate detection: `Foo` and `ns.Foo` collide when they resolve
        // to the same interface, `Converter<A>` (where `type A = int`) and
        // `Converter<int>` collide as the same view, while `Converter<int>` and
        // `Converter<float>` stay distinct. Keying on the lowered (alias-resolved)
        // args rather than the source text is what makes the alias case collide.
        let type_arg_key = lower_path_generic_args(
            db,
            &block.target.expr,
            pkg_items,
            namespace_path,
            &class.generic_params,
        )
        .iter()
        .map(|arg| expand_type_alias(arg, aliases).to_string())
        .collect::<Vec<_>>()
        .join(",");
        let key = format!(
            "{}:{}<{}>",
            resolved_iface.loc.file(db).file_id(db).as_u32(),
            resolved_iface.loc.id(db).as_u32(),
            type_arg_key
        );
        seen_targets
            .entry(key)
            .or_insert_with(|| (iface_display_name, Vec::new()))
            .1
            .push(block.target.span);
    }
    for (_target, (name, sites)) in &seen_targets {
        if sites.len() > 1 {
            diagnostics.push(
                Hir2Diagnostic::DuplicateImplementsBlock {
                    class_name: class.name.clone(),
                    interface_name: name.clone(),
                    sites: sites.clone(),
                }
                .to_diagnostic(file_id),
            );
        }
    }

    for block in &class.implements {
        let Some(resolved_iface) =
            resolve_interface_path(db, &block.target.expr, pkg_items, namespace_path)
        else {
            // Distinguish "name doesn't exist" (E0112) from "name exists
            // but isn't an interface" (E0119).
            let is_non_interface =
                is_non_interface_type(&block.target.expr, pkg_items, namespace_path);
            if is_non_interface {
                diagnostics.push(
                    Hir2Diagnostic::NotAnInterface {
                        class_name: class.name.clone(),
                        target_name: format!("{}", block.target.expr),
                        span: block.target.span,
                    }
                    .to_diagnostic(file_id),
                );
            } else {
                diagnostics.push(
                    Hir2Diagnostic::UnknownInterface {
                        class_name: class.name.clone(),
                        target_name: format!("{}", block.target.expr),
                        span: block.target.span,
                    }
                    .to_diagnostic(file_id),
                );
            }
            continue;
        };
        // E0002: the interface name resolved, but its generic type arguments
        // must themselves be resolvable types. `implements Container<Bogus>`
        // is rejected here just like a field type `x: Bogus` is. Without this,
        // an unresolvable type argument in an `implements` clause is silently
        // dropped — the only code that lowers the target (dispatch-source
        // collection) discards these diagnostics.
        {
            let mut arg_errors = Vec::new();
            baml_compiler2_tir::lower_type_expr::lower_type_expr_in_ns(
                db,
                &block.target.expr,
                pkg_items,
                namespace_path,
                &class.generic_params,
                &mut arg_errors,
            );
            for error in arg_errors {
                diagnostics.push(
                    Diagnostic::error(tir_type_error_to_diagnostic_id(&error), error.to_string())
                        .with_primary_span(Span {
                            file_id,
                            range: block.target.span,
                        })
                        .with_phase(DiagnosticPhase::Type),
                );
            }
        }
        let iface_display_name = resolved_iface.display_name();
        let iface_qtn = resolved_iface.qtn.clone();
        let iface_file = resolved_iface.loc.file(db);
        let iface = resolved_iface.iface;
        let iface_pkg_info = baml_compiler2_hir::file_package::file_package(db, iface_file);
        let iface_pkg_id =
            baml_compiler2_hir::package::PackageId::new(db, iface_pkg_info.package.clone());
        let iface_pkg_items = baml_compiler2_hir::package::package_items(db, iface_pkg_id);
        let iface_namespace_path = iface_pkg_info.namespace_path;
        let mut interface_generic_params = class.generic_params.clone();
        interface_generic_params.extend(iface.generic_params.clone());
        // Build a T → concrete-type substitution from the implements target's
        // generic args. `implements Container<int>` gives `{T → int}` so
        // signature comparisons see the concrete shape.
        let generic_subst: std::collections::HashMap<Name, baml_compiler2_ast::TypeExpr> =
            match &block.target.expr {
                baml_compiler2_ast::TypeExpr::Path { generic_args, .. } => iface
                    .generic_params
                    .iter()
                    .zip(generic_args.iter())
                    .map(|(p, a)| (p.clone(), a.clone()))
                    .collect(),
                _ => std::collections::HashMap::new(),
            };
        let target_associated_type_bindings = match &block.target.expr {
            baml_compiler2_ast::TypeExpr::Path {
                associated_type_bindings,
                ..
            } => associated_type_bindings.as_slice(),
            _ => &[][..],
        };
        validate_no_associated_type_bindings_on_implements_target(
            file_id,
            block.target.span,
            target_associated_type_bindings,
            diagnostics,
        );
        validate_associated_type_binding_defs(
            db,
            file_id,
            iface_file.file_id(db),
            block.target.span,
            &iface,
            &iface_display_name,
            iface_pkg_items,
            pkg_items,
            &iface_namespace_path,
            namespace_path,
            &class.generic_params,
            &generic_bound_expr_map(&class.generic_params, &class.generic_param_bounds),
            &generic_subst,
            &block.associated_type_bindings,
            aliases,
            diagnostics,
        );
        let mut subst = associated_type_subst_from_bindings(
            &iface,
            &generic_subst,
            &block.associated_type_bindings,
        );
        augment_subst_with_class_required_parent_associated_types(
            db,
            class,
            &iface,
            pkg_items,
            namespace_path,
            &mut subst,
        );
        let members = collect_interface_members_with_subst(
            db,
            &iface,
            iface_file,
            pkg_items,
            namespace_path,
            &subst,
            &class.generic_params,
        );
        if block.is_out_of_body && !members.fields.is_empty() {
            diagnostics.push(
                Hir2Diagnostic::OutOfBodyImplementsFieldInterface {
                    target_name: class.name.to_string(),
                    interface_name: iface_display_name.clone(),
                    span: block.target.span,
                }
                .to_diagnostic(file_id),
            );
            continue;
        }

        // Check that every method declared in `implements I {}` actually
        // exists on `I` (required or default), and matches the interface's
        // declared signature.
        let mut provided_method_names: HashSet<Name> = HashSet::new();
        for m in &block.methods {
            let expected_sig = members
                .required_methods
                .iter()
                .find_map(|(_, n, s)| if *n == m.name { Some(s.clone()) } else { None })
                .or_else(|| {
                    members.default_methods.iter().find_map(|(_, n, s)| {
                        if *n == m.name { Some(s.clone()) } else { None }
                    })
                });
            match expected_sig {
                None => diagnostics.push(
                    Hir2Diagnostic::UnknownInterfaceMember {
                        interface_name: iface_display_name.clone(),
                        method_name: m.name.clone(),
                        span: m.name_span,
                    }
                    .to_diagnostic(file_id),
                ),
                Some(expected) => {
                    let actual_subst = subst_without_names(&subst, &class.generic_params);
                    let actual = MethodSignature::from_params_and_return_with_subst(
                        &m.generic_params,
                        &m.generic_param_bounds,
                        &m.params,
                        m.return_type.as_ref(),
                        m.throws.as_ref(),
                        &actual_subst,
                    );
                    if !expected.matches(
                        &actual,
                        SignatureMatchContext {
                            db,
                            expected_pkg_items: iface_pkg_items,
                            expected_namespace_path: &iface_namespace_path,
                            actual_pkg_items: pkg_items,
                            actual_namespace_path: namespace_path,
                            aliases,
                            ignore_param_names: false,
                            outer_generic_params: &class.generic_params,
                        },
                    ) {
                        diagnostics.push(
                            Hir2Diagnostic::InterfaceMethodSignatureMismatch {
                                class_name: class.name.clone(),
                                interface_name: iface_display_name.clone(),
                                method_name: m.name.clone(),
                                actual: actual.render(),
                                expected: expected.render(),
                                span: m.name_span,
                            }
                            .to_diagnostic(file_id),
                        );
                    }
                }
            }
            provided_method_names.insert(m.name.clone());
        }

        // Check that every required method has a body — provided in this
        // `implements` block, by an inherited default, or by a separate
        // `implements` block targeting the originating interface. A class's own
        // (non-block) method does NOT satisfy a requirement (BEP-044: only
        // `implements`-block members do); a same-named inherent method is unrelated.
        for (origin, req_name, _sig) in &members.required_methods {
            if provided_method_names.contains(req_name) {
                continue;
            }
            // If the method originates from a parent interface that this
            // class explicitly implements in a separate block, skip the
            // check — that block is responsible for providing the method.
            if origin.qualified_name != iface_qtn
                && class.implements.iter().any(|candidate| {
                    interface_origin_matches_target_expr(
                        db,
                        &candidate.target.expr,
                        pkg_items,
                        namespace_path,
                        &class.generic_params,
                        aliases,
                        origin,
                    )
                })
            {
                continue;
            }
            diagnostics.push(
                Hir2Diagnostic::MissingInterfaceMethod {
                    class_name: class.name.clone(),
                    interface_name: origin.display_name(),
                    method_name: req_name.clone(),
                    span: block.span,
                }
                .to_diagnostic(file_id),
            );
        }

        // BEP-044 v2: interface fields are satisfied by class fields. The
        // implements block may contain only explicit `field as class_field`
        // links; an absent link auto-links by exact field name.
        let class_fields: IndexMap<Name, &baml_compiler2_ast::FieldDef> =
            class.fields.iter().map(|f| (f.name.clone(), f)).collect();

        let own_fields: IndexMap<Name, Option<baml_compiler2_ast::SpannedTypeExpr>> = iface
            .fields
            .iter()
            .map(|f| {
                let substituted =
                    f.type_expr
                        .as_ref()
                        .map(|te| baml_compiler2_ast::SpannedTypeExpr {
                            expr: substitute_type_vars(&te.expr, &subst),
                            span: te.span,
                        });
                (f.name.clone(), substituted)
            })
            .collect();

        let mut link_sites: IndexMap<Name, Vec<TextRange>> = IndexMap::new();
        for link in &block.field_links {
            link_sites
                .entry(link.interface_field.clone())
                .or_default()
                .push(link.interface_field_span);
        }
        for (field_name, sites) in &link_sites {
            if sites.len() > 1 {
                diagnostics.push(
                    Hir2Diagnostic::DuplicateInterfaceFieldLink {
                        interface_name: iface_display_name.clone(),
                        field_name: field_name.clone(),
                        sites: sites.clone(),
                    }
                    .to_diagnostic(file_id),
                );
            }
        }

        let mut explicit_links: IndexMap<Name, &baml_compiler2_ast::InterfaceFieldLinkDef> =
            IndexMap::new();
        for link in &block.field_links {
            if !own_fields.contains_key(&link.interface_field) {
                diagnostics.push(
                    Hir2Diagnostic::UnknownInterfaceFieldLink {
                        interface_name: iface_display_name.clone(),
                        field_name: link.interface_field.clone(),
                        span: link.interface_field_span,
                    }
                    .to_diagnostic(file_id),
                );
                continue;
            }
            let Some(class_field) = class_fields.get(&link.class_field) else {
                diagnostics.push(
                    Hir2Diagnostic::UnknownClassFieldInInterfaceLink {
                        class_name: class.name.clone(),
                        interface_name: iface_display_name.clone(),
                        field_name: link.class_field.clone(),
                        span: link.class_field_span,
                    }
                    .to_diagnostic(file_id),
                );
                explicit_links.insert(link.interface_field.clone(), link);
                continue;
            };
            if let (Some(iface_te), Some(class_te)) = (
                own_fields
                    .get(&link.interface_field)
                    .and_then(std::option::Option::as_ref),
                class_field.type_expr.as_ref(),
            ) {
                // C12: `Self`-in-field is reported separately (E0136); skip the
                // invariance check here to avoid the contradictory cascade.
                if type_expr_contains_self(&iface_te.expr) {
                    continue;
                }
                if !type_exprs_compatible(
                    db,
                    iface_pkg_items,
                    &iface_namespace_path,
                    &interface_generic_params,
                    &iface_te.expr,
                    pkg_items,
                    namespace_path,
                    &class.generic_params,
                    &class_te.expr,
                    aliases,
                ) {
                    diagnostics.push(
                        Hir2Diagnostic::InterfaceFieldTypeMismatch {
                            class_name: class.name.clone(),
                            field_name: link.class_field.clone(),
                            interface_field_name: link.interface_field.clone(),
                            interface_name: iface_display_name.clone(),
                            class_type: format!("{}", class_te.expr),
                            interface_type: format!("{}", iface_te.expr),
                            span: class_te.span,
                        }
                        .to_diagnostic(file_id),
                    );
                }
            }
            explicit_links
                .entry(link.interface_field.clone())
                .or_insert(link);
        }

        for (field_name, iface_te) in &own_fields {
            if explicit_links.contains_key(field_name) {
                continue;
            }
            let Some(class_field) = class_fields.get(field_name) else {
                diagnostics.push(
                    Hir2Diagnostic::MissingInterfaceField {
                        class_name: class.name.clone(),
                        interface_name: iface_display_name.clone(),
                        field_name: field_name.clone(),
                        span: block.span,
                    }
                    .to_diagnostic(file_id),
                );
                continue;
            };
            if let (Some(iface_te), Some(class_te)) =
                (iface_te.as_ref(), class_field.type_expr.as_ref())
            {
                // C12: `Self` in the interface field type is rejected separately
                // (E0136); skip the invariance check so it doesn't ALSO demand
                // the class field be `Self?` (the old contradictory cascade).
                if type_expr_contains_self(&iface_te.expr) {
                    continue;
                }
                if !type_exprs_compatible(
                    db,
                    iface_pkg_items,
                    &iface_namespace_path,
                    &interface_generic_params,
                    &iface_te.expr,
                    pkg_items,
                    namespace_path,
                    &class.generic_params,
                    &class_te.expr,
                    aliases,
                ) {
                    diagnostics.push(
                        Hir2Diagnostic::InterfaceFieldTypeMismatch {
                            class_name: class.name.clone(),
                            field_name: class_field.name.clone(),
                            // Non-aliased: the interface and class field share a name.
                            interface_field_name: field_name.clone(),
                            interface_name: iface_display_name.clone(),
                            class_type: format!("{}", class_te.expr),
                            interface_type: format!("{}", iface_te.expr),
                            span: class_te.span,
                        }
                        .to_diagnostic(file_id),
                    );
                }
            }
        }

        // E0125: check that all `requires` parents are explicitly
        // implemented by this class.
        if !iface.requires.is_empty() {
            let missing: Vec<Name> = iface
                .requires
                .iter()
                .filter_map(|parent_te| {
                    let required_parent = substitute_type_vars(&parent_te.expr, &subst);
                    let baml_compiler2_ast::TypeExpr::Path { segments, .. } = &required_parent
                    else {
                        return None;
                    };
                    let parent_name = resolve_interface_path(
                        db,
                        &required_parent,
                        pkg_items,
                        &iface_namespace_path,
                    )
                    .map(|_| Name::new(required_parent.to_string()))
                    .or_else(|| segments.last().cloned())?;
                    let class_implements_it = class.implements.iter().any(|candidate| {
                        interface_target_matches_required_parent(
                            &ctx,
                            &candidate.target.expr,
                            namespace_path,
                            &candidate.associated_type_bindings,
                            &required_parent,
                            &iface_namespace_path,
                            &class.generic_params,
                        )
                    });
                    if class_implements_it {
                        None
                    } else {
                        Some(parent_name)
                    }
                })
                .collect();
            if !missing.is_empty() {
                diagnostics.push(
                    Hir2Diagnostic::MissingRequiredInterface {
                        class_name: class.name.clone(),
                        interface_name: iface_display_name.clone(),
                        missing_parents: missing,
                        span: block.target.span,
                    }
                    .to_diagnostic(file_id),
                );
            }
        }
    }

    // BEP-044 §"Method Disambiguation": same-named methods declared in
    // two `implements` blocks are NOT a class-level error. The class
    // compiles; the ambiguity surfaces at the call site instead — see
    // `resolve_member` in TIR for the unqualified-call diagnostic.
}

/// Invariant compatibility check for interface fields.
///
/// Interface fields are writeable through the interface view, so the class
/// storage type must match the interface field type exactly. The exactness
/// rule lives in TIR normalization so LSP and compiler diagnostics agree on
/// semantic equality without permitting assignment subtyping.
#[allow(clippy::too_many_arguments)]
fn type_exprs_compatible(
    db: &dyn Db,
    lhs_pkg_items: &baml_compiler2_hir::package::PackageItems<'_>,
    lhs_namespace_path: &[Name],
    lhs_generic_params: &[Name],
    lhs: &baml_compiler2_ast::TypeExpr,
    rhs_pkg_items: &baml_compiler2_hir::package::PackageItems<'_>,
    rhs_namespace_path: &[Name],
    rhs_generic_params: &[Name],
    rhs: &baml_compiler2_ast::TypeExpr,
    aliases: &std::collections::HashMap<QualifiedTypeName, Ty>,
) -> bool {
    let mut diagnostics = Vec::new();
    let lhs_ty = baml_compiler2_tir::lower_type_expr::lower_type_expr_in_ns(
        db,
        lhs,
        lhs_pkg_items,
        lhs_namespace_path,
        lhs_generic_params,
        &mut diagnostics,
    );
    let lhs_lowered = diagnostics.is_empty();
    diagnostics.clear();
    let rhs_ty = baml_compiler2_tir::lower_type_expr::lower_type_expr_in_ns(
        db,
        rhs,
        rhs_pkg_items,
        rhs_namespace_path,
        rhs_generic_params,
        &mut diagnostics,
    );

    if !lhs_lowered || !diagnostics.is_empty() {
        // Resolution diagnostics are emitted elsewhere. Avoid inventing a
        // misleading interface-field mismatch on partially unresolved types.
        return lhs.to_string() == rhs.to_string();
    }

    baml_compiler2_tir::normalize::is_same_normalized_type(&lhs_ty, &rhs_ty, aliases)
}

/// Nominal subtype check used for `throws` covariance: `sub <: sup` when the
/// types normalize equal, or when `sup` is an interface that `sub` implements
/// (via the implements registry). Recurses for the bound check.
fn ty_nominal_subtype(
    db: &dyn Db,
    sub: &Ty,
    sup: &Ty,
    aliases: &std::collections::HashMap<QualifiedTypeName, Ty>,
) -> bool {
    if baml_compiler2_tir::normalize::is_same_normalized_type(sub, sup, aliases) {
        return true;
    }
    if let Ty::Union(members, _) = sub {
        return !members.is_empty()
            && members
                .iter()
                .all(|member| ty_nominal_subtype(db, member, sup, aliases));
    }
    if let Ty::Union(members, _) = sup {
        return members
            .iter()
            .any(|member| ty_nominal_subtype(db, sub, member, aliases));
    }
    if let (
        Ty::Interface(sub_qtn, sub_args, sub_assoc, _),
        Ty::Interface(sup_qtn, sup_args, sup_assoc, _),
    ) = (sub, sup)
        && interface_ty_requires_nominal(InterfaceRequiresQuery {
            db,
            sub_qtn,
            sub_args,
            sub_assoc,
            sup_qtn,
            sup_args,
            sup_assoc,
            aliases,
        })
    {
        return true;
    }
    if let Ty::Interface(iface_qtn, _, _, _) = sup {
        let registry = baml_compiler2_tir::interfaces::package_implements_registry(
            db,
            baml_compiler2_hir::package::PackageId::new(db, iface_qtn.package().clone()),
        );
        return registry.type_implements_interface_via_rule(sub, sup, aliases, |a, b| {
            ty_nominal_subtype(db, a, b, aliases)
        });
    }
    false
}

fn interface_ty_requires_nominal(query: InterfaceRequiresQuery<'_>) -> bool {
    let pkg_id =
        baml_compiler2_hir::package::PackageId::new(query.db, query.sub_qtn.package().clone());
    let pkg_items = baml_compiler2_hir::package::package_items(query.db, pkg_id);
    let Some(baml_compiler2_hir::contributions::Definition::Interface(sub_loc)) =
        pkg_items.lookup_type(query.sub_qtn.namespace(), query.sub_qtn.name())
    else {
        return false;
    };
    let pkg = baml_compiler2_hir::file_package::file_package(query.db, sub_loc.file(query.db));
    baml_compiler2_tir::interfaces::interface_closure_locs_with_args_and_assoc(
        query.db,
        sub_loc,
        query.sub_args,
        query.sub_assoc,
        pkg_items,
        &pkg.namespace_path,
    )
    .into_iter()
    .any(|(iface_loc, iface_args, iface_assoc)| {
        let tree = baml_compiler2_hir::file_item_tree(query.db, iface_loc.file(query.db));
        let Some(iface) = tree.interfaces.get(&iface_loc.id(query.db)) else {
            return false;
        };
        let iface_qtn = baml_compiler2_tir::lower_type_expr::qualify_def(
            query.db,
            baml_compiler2_hir::contributions::Definition::Interface(iface_loc),
            &iface.name,
        );
        iface_qtn == *query.sup_qtn
            && iface_args.len() == query.sup_args.len()
            && iface_args.iter().zip(query.sup_args).all(|(a, b)| {
                baml_compiler2_tir::normalize::is_same_normalized_type(a, b, query.aliases)
            })
            && query.sup_assoc.iter().all(|(sup_name, sup_ty)| {
                iface_assoc
                    .iter()
                    .find(|(iface_name, _)| iface_name == sup_name)
                    .is_some_and(|(_, iface_ty)| {
                        baml_compiler2_tir::normalize::is_same_normalized_type(
                            iface_ty,
                            sup_ty,
                            query.aliases,
                        )
                    })
            })
    })
}

/// BEP-044 wf3 #G9a: `throws` is covariant — an interface impl may declare a
/// *narrower* throws than the interface (`throws NetworkError` satisfying
/// `throws IError` when `NetworkError implements IError`). Every member of the
/// impl's throws union must be a nominal subtype of some member of the
/// interface's. Falls back to string equality when a type can't be lowered.
fn throws_covariant_compatible(
    ctx: SignatureMatchContext<'_, '_>,
    generic_params: &[Name],
    iface_throws: &baml_compiler2_ast::TypeExpr,
    impl_throws: &baml_compiler2_ast::TypeExpr,
) -> bool {
    let mut diagnostics = Vec::new();
    let iface_ty = baml_compiler2_tir::lower_type_expr::lower_type_expr_in_ns(
        ctx.db,
        iface_throws,
        ctx.expected_pkg_items,
        ctx.expected_namespace_path,
        generic_params,
        &mut diagnostics,
    );
    let ok = diagnostics.is_empty();
    diagnostics.clear();
    let impl_ty = baml_compiler2_tir::lower_type_expr::lower_type_expr_in_ns(
        ctx.db,
        impl_throws,
        ctx.actual_pkg_items,
        ctx.actual_namespace_path,
        generic_params,
        &mut diagnostics,
    );
    if !ok || !diagnostics.is_empty() {
        return iface_throws.to_string() == impl_throws.to_string();
    }
    if baml_compiler2_tir::normalize::is_same_normalized_type(&impl_ty, &iface_ty, ctx.aliases) {
        return true;
    }
    let members = |ty: &Ty| -> Vec<Ty> {
        match ty {
            Ty::Union(m, _) => m.clone(),
            other => vec![other.clone()],
        }
    };
    let iface_members = members(&iface_ty);
    members(&impl_ty).iter().all(|im| {
        iface_members
            .iter()
            .any(|sup| ty_nominal_subtype(ctx.db, im, sup, ctx.aliases))
    })
}

/// Collect single-segment path names in `expr` that are members of
/// `type_params` (i.e. references to declared generic parameters), recursing
/// through containers and generic args. Used to check that an out-of-body
/// `implements<T> … for …` binds every declared param in the implementor type.
fn collect_type_var_names(
    expr: &baml_compiler2_ast::TypeExpr,
    type_params: &[Name],
    out: &mut HashSet<Name>,
) {
    use baml_compiler2_ast::TypeExpr;
    match expr {
        TypeExpr::Path {
            segments,
            generic_args,
            ..
        } => {
            if segments.len() == 1 && type_params.contains(&segments[0]) {
                out.insert(segments[0].clone());
            }
            for a in generic_args {
                collect_type_var_names(a, type_params, out);
            }
        }
        TypeExpr::List { inner, .. } | TypeExpr::Optional { inner, .. } => {
            collect_type_var_names(inner, type_params, out);
        }
        TypeExpr::Map { key, value, .. } => {
            collect_type_var_names(key, type_params, out);
            collect_type_var_names(value, type_params, out);
        }
        TypeExpr::Union { variants, .. } => {
            for v in variants {
                collect_type_var_names(v, type_params, out);
            }
        }
        TypeExpr::Function {
            params,
            ret,
            throws,
            ..
        } => {
            for p in params {
                collect_type_var_names(&p.ty, type_params, out);
            }
            collect_type_var_names(ret, type_params, out);
            if let Some(throws) = throws {
                collect_type_var_names(throws, type_params, out);
            }
        }
        _ => {}
    }
}

fn type_expr_contains_self(expr: &baml_compiler2_ast::TypeExpr) -> bool {
    use baml_compiler2_ast::TypeExpr;
    match expr {
        TypeExpr::Path {
            segments,
            generic_args,
            ..
        } => {
            (segments.len() == 1 && segments[0].as_str() == "Self")
                || generic_args.iter().any(type_expr_contains_self)
        }
        TypeExpr::Optional { inner, .. } | TypeExpr::List { inner, .. } => {
            type_expr_contains_self(inner)
        }
        TypeExpr::Map { key, value, .. } => {
            type_expr_contains_self(key) || type_expr_contains_self(value)
        }
        TypeExpr::Union { variants, .. } => variants.iter().any(type_expr_contains_self),
        TypeExpr::Function {
            params,
            ret,
            throws,
            ..
        } => {
            params.iter().any(|p| type_expr_contains_self(&p.ty))
                || type_expr_contains_self(ret)
                || throws
                    .as_ref()
                    .is_some_and(|throws| type_expr_contains_self(throws))
        }
        _ => false,
    }
}

fn check_jinja_templates(
    db: &dyn Db,
    file_id: FileId,
    item_tree: &baml_compiler2_hir::item_tree::ItemTree,
    pkg_items: &baml_compiler2_hir::package::PackageItems<'_>,
    namespace_path: &[Name],
    source_text: &str,
) -> Vec<Diagnostic> {
    let base_types = build_jinja_types(db, pkg_items, namespace_path);
    let mut diagnostics = Vec::new();

    for func_data in item_tree.functions.values() {
        diagnostics.extend(check_llm_prompt_template(
            db,
            file_id,
            func_data,
            pkg_items,
            namespace_path,
            &base_types,
            source_text,
        ));
    }

    for template in item_tree.template_strings.values() {
        let Some(body) = &template.body else {
            continue;
        };

        let mut types = base_types.clone();
        types.start_scope();
        for param in &template.params {
            let ty = param
                .type_expr
                .as_ref()
                .map(|type_expr| {
                    jinja_type_from_type_expr(db, &type_expr.expr, pkg_items, namespace_path)
                })
                .unwrap_or(sys_jinja_types::Type::Unknown);
            types.add_variable(param.name.as_str(), ty);
        }

        let range_hint = template.span;
        diagnostics.extend(render_jinja_validation_result(
            file_id,
            source_text,
            range_hint,
            body,
            sys_jinja_types::validate_template(template.name.as_str(), body, &mut types),
        ));
    }

    diagnostics
}

fn check_llm_prompt_template(
    db: &dyn Db,
    file_id: FileId,
    func_data: &baml_compiler2_hir::item_tree::Function,
    pkg_items: &baml_compiler2_hir::package::PackageItems<'_>,
    namespace_path: &[Name],
    base_types: &sys_jinja_types::PredefinedTypes,
    source_text: &str,
) -> Vec<Diagnostic> {
    let Some(baml_compiler2_ast::DeclarativeMeta::Llm(llm)) = &func_data.declarative_meta else {
        return Vec::new();
    };
    let Some(prompt) = &llm.prompt else {
        return Vec::new();
    };

    let mut types = base_types.clone();
    types.start_scope();
    for param in &func_data.params {
        let ty = param
            .type_expr
            .as_ref()
            .map(|type_expr| {
                jinja_type_from_type_expr(db, &type_expr.expr, pkg_items, namespace_path)
            })
            .unwrap_or(sys_jinja_types::Type::Unknown);
        types.add_variable(param.name.as_str(), ty);
    }

    render_jinja_validation_result(
        file_id,
        source_text,
        prompt.span,
        &prompt.text,
        sys_jinja_types::validate_template(func_data.name.as_str(), &prompt.text, &mut types),
    )
}

fn build_jinja_types(
    db: &dyn Db,
    pkg_items: &baml_compiler2_hir::package::PackageItems<'_>,
    namespace_path: &[Name],
) -> sys_jinja_types::PredefinedTypes {
    use baml_compiler2_hir::contributions::Definition;

    let mut types =
        sys_jinja_types::PredefinedTypes::default(sys_jinja_types::JinjaContext::Prompt);
    types.add_variable("baml", sys_jinja_types::Type::Unknown);

    let Some(ns_items) = pkg_items.namespaces.get(namespace_path) else {
        return types;
    };

    for def in ns_items.types.values() {
        let Definition::Class(class_loc) = *def else {
            continue;
        };
        let file = class_loc.file(db);
        let item_tree = baml_compiler2_ppir::file_item_tree(db, file);
        let class_data = &item_tree[class_loc.id(db)];
        let class_namespace =
            baml_compiler2_hir::file_package::file_package(db, file).namespace_path;
        let fields = class_data
            .fields
            .iter()
            .map(|field| {
                let ty = field
                    .type_expr
                    .as_ref()
                    .map(|type_expr| {
                        jinja_type_from_type_expr(db, &type_expr.expr, pkg_items, &class_namespace)
                    })
                    .unwrap_or(sys_jinja_types::Type::Unknown);
                (field.name.to_string(), ty)
            })
            .collect::<IndexMap<_, _>>();
        types.add_class(class_data.name.as_str(), fields);
    }

    for def in ns_items.types.values() {
        let Definition::Enum(enum_loc) = *def else {
            continue;
        };
        let item_tree = baml_compiler2_ppir::file_item_tree(db, enum_loc.file(db));
        let enum_data = &item_tree[enum_loc.id(db)];
        types.add_enum(
            enum_data.name.as_str(),
            enum_data
                .variants
                .iter()
                .map(|variant| variant.name.to_string())
                .collect(),
        );
    }

    for def in ns_items.types.values() {
        let Definition::TypeAlias(alias_loc) = *def else {
            continue;
        };
        let file = alias_loc.file(db);
        let item_tree = baml_compiler2_ppir::file_item_tree(db, file);
        let alias_data = &item_tree[alias_loc.id(db)];
        if let Some(type_expr) = &alias_data.type_expr {
            let alias_namespace =
                baml_compiler2_hir::file_package::file_package(db, file).namespace_path;
            types.add_alias(
                alias_data.name.as_str(),
                jinja_type_from_type_expr(db, &type_expr.expr, pkg_items, &alias_namespace),
            );
        }
    }

    for def in ns_items.values.values() {
        let Definition::TemplateString(template_loc) = *def else {
            continue;
        };
        let file = template_loc.file(db);
        let item_tree = baml_compiler2_ppir::file_item_tree(db, file);
        let template = &item_tree[template_loc.id(db)];
        let template_namespace =
            baml_compiler2_hir::file_package::file_package(db, file).namespace_path;
        let args = template
            .params
            .iter()
            .map(|param| {
                let ty = param
                    .type_expr
                    .as_ref()
                    .map(|type_expr| {
                        jinja_type_from_type_expr(
                            db,
                            &type_expr.expr,
                            pkg_items,
                            &template_namespace,
                        )
                    })
                    .unwrap_or(sys_jinja_types::Type::Unknown);
                (param.name.to_string(), ty)
            })
            .collect();
        types.add_function(template.name.as_str(), sys_jinja_types::Type::String, args);
    }

    types
}

fn jinja_type_from_type_expr(
    db: &dyn Db,
    type_expr: &baml_compiler2_ast::TypeExpr,
    pkg_items: &baml_compiler2_hir::package::PackageItems<'_>,
    namespace_path: &[Name],
) -> sys_jinja_types::Type {
    jinja_type_from_type_expr_inner(
        db,
        type_expr,
        pkg_items,
        namespace_path,
        &mut HashSet::new(),
    )
}

fn jinja_type_from_type_expr_inner(
    db: &dyn Db,
    type_expr: &baml_compiler2_ast::TypeExpr,
    pkg_items: &baml_compiler2_hir::package::PackageItems<'_>,
    namespace_path: &[Name],
    resolving_aliases: &mut HashSet<String>,
) -> sys_jinja_types::Type {
    use baml_compiler2_ast::TypeExpr;
    use baml_compiler2_hir::contributions::Definition;
    use sys_jinja_types::Type;

    match type_expr {
        TypeExpr::Int { .. } => Type::Int,
        TypeExpr::Bigint { .. } => Type::Bigint,
        TypeExpr::Float { .. } => Type::Float,
        TypeExpr::String { .. } => Type::String,
        TypeExpr::Bool { .. } => Type::Bool,
        TypeExpr::Null { .. } => Type::None,
        TypeExpr::Media { kind, .. } => match kind {
            baml_base::MediaKind::Image => Type::Image,
            baml_base::MediaKind::Audio => Type::Audio,
            _ => Type::Unknown,
        },
        TypeExpr::Literal { value, .. } => Type::Literal(value.clone()),
        TypeExpr::Optional { inner, .. } => Type::merge([
            Type::None,
            jinja_type_from_type_expr_inner(
                db,
                inner,
                pkg_items,
                namespace_path,
                resolving_aliases,
            ),
        ]),
        TypeExpr::List { inner, .. } => Type::List(Box::new(jinja_type_from_type_expr_inner(
            db,
            inner,
            pkg_items,
            namespace_path,
            resolving_aliases,
        ))),
        TypeExpr::Map { value, .. } => Type::Map(
            Box::new(Type::String),
            Box::new(jinja_type_from_type_expr_inner(
                db,
                value,
                pkg_items,
                namespace_path,
                resolving_aliases,
            )),
        ),
        TypeExpr::Union { variants, .. } => Type::merge(variants.iter().map(|variant| {
            jinja_type_from_type_expr_inner(
                db,
                variant,
                pkg_items,
                namespace_path,
                resolving_aliases,
            )
        })),
        TypeExpr::Path { segments, .. } if !segments.is_empty() => {
            let (lookup_namespace, name) = jinja_lookup_path(namespace_path, segments);
            let key = format!(
                "{}::{}",
                lookup_namespace
                    .iter()
                    .map(Name::as_str)
                    .collect::<Vec<_>>()
                    .join("::"),
                name
            );
            let name_obj = Name::new(name.as_str());
            match pkg_items.lookup_type(&lookup_namespace, &name_obj) {
                Some(Definition::Class(_)) => Type::ClassRef(name),
                Some(Definition::Enum(_)) => Type::EnumTypeRef(name),
                Some(Definition::TypeAlias(alias_loc)) => {
                    if !resolving_aliases.insert(key.clone()) {
                        return Type::RecursiveTypeAlias(name);
                    }
                    let file = alias_loc.file(db);
                    let item_tree = baml_compiler2_ppir::file_item_tree(db, file);
                    let alias = &item_tree[alias_loc.id(db)];
                    let alias_namespace =
                        baml_compiler2_hir::file_package::file_package(db, file).namespace_path;
                    let resolved = alias
                        .type_expr
                        .as_ref()
                        .map(|spanned| {
                            jinja_type_from_type_expr_inner(
                                db,
                                &spanned.expr,
                                pkg_items,
                                &alias_namespace,
                                resolving_aliases,
                            )
                        })
                        .unwrap_or(Type::Unknown);
                    resolving_aliases.remove(&key);
                    Type::Alias {
                        name,
                        target: Box::new(resolved.clone()),
                        resolved: Box::new(resolved),
                    }
                }
                _ => Type::Unknown,
            }
        }
        TypeExpr::Function { .. } | TypeExpr::AssociatedTypeProjection { .. } => Type::Unknown,
        TypeExpr::Uint8Array { .. }
        | TypeExpr::Never { .. }
        | TypeExpr::Void { .. }
        | TypeExpr::BuiltinUnknown { .. }
        | TypeExpr::Type { .. }
        | TypeExpr::Rust { .. }
        | TypeExpr::Error { .. }
        | TypeExpr::Unknown { .. }
        | TypeExpr::Path { .. } => Type::Unknown,
    }
}

fn jinja_lookup_path(current_namespace: &[Name], segments: &[Name]) -> (Vec<Name>, String) {
    let (name, namespace) = segments
        .split_last()
        .expect("caller guarantees at least one segment");
    if namespace.is_empty() {
        (current_namespace.to_vec(), name.to_string())
    } else {
        (namespace.to_vec(), name.to_string())
    }
}

fn render_jinja_validation_result(
    file_id: FileId,
    source_text: &str,
    raw_string_range: TextRange,
    _template: &str,
    result: Result<(), sys_jinja_types::ValidationError>,
) -> Vec<Diagnostic> {
    let Err(error) = result else {
        return Vec::new();
    };

    if let Some(parse_error) = error.parsing_errors {
        let range = parse_error
            .range()
            .map(|range| jinja_offset_range(source_text, raw_string_range, range.start, range.end))
            .unwrap_or(raw_string_range);
        return vec![
            Diagnostic::error(
                DiagnosticId::JinjaParseError,
                format!("Error parsing jinja template: {parse_error}"),
            )
            .with_primary_span(Span { file_id, range })
            .with_phase(DiagnosticPhase::Type),
        ];
    }

    error
        .errors
        .into_iter()
        .map(|error| {
            let span = error.span();
            let range = jinja_offset_range(
                source_text,
                raw_string_range,
                span.start_offset as usize,
                span.end_offset as usize,
            );
            Diagnostic::warning(jinja_diagnostic_id(error.message()), error.message())
                .with_primary_span(Span { file_id, range })
                .with_phase(DiagnosticPhase::Type)
        })
        .collect()
}

fn raw_string_content_start(source_text: &str, raw_string_range: TextRange) -> TextSize {
    let start: usize = raw_string_range.start().into();
    let end: usize = raw_string_range.end().into();
    let Some(raw_text) = source_text.get(start..end) else {
        return raw_string_range.start();
    };
    let quote_offset = raw_text.find('"').unwrap_or(0);
    raw_string_range.start() + TextSize::from(u32::try_from(quote_offset + 1).unwrap_or(u32::MAX))
}

fn jinja_offset_range(
    source_text: &str,
    raw_string_range: TextRange,
    start_offset: usize,
    end_offset: usize,
) -> TextRange {
    let content_start = raw_string_content_start(source_text, raw_string_range);
    TextRange::new(
        content_start + TextSize::from(u32::try_from(start_offset).unwrap_or(u32::MAX)),
        content_start
            + TextSize::from(u32::try_from(end_offset.max(start_offset + 1)).unwrap_or(u32::MAX)),
    )
}

fn jinja_diagnostic_id(message: &str) -> DiagnosticId {
    if message.starts_with("Variable `") {
        DiagnosticId::JinjaUnresolvedVariable
    } else if message.contains("referenced without parentheses") {
        DiagnosticId::JinjaFunctionReferenceWithoutCall
    } else if message.starts_with("Filter '") {
        DiagnosticId::JinjaInvalidFilter
    } else if message.contains("expects argument") {
        DiagnosticId::JinjaWrongArgType
    } else if message.contains("expects ") && message.contains(" arguments") {
        DiagnosticId::JinjaWrongArgCount
    } else if message.contains("property ") {
        DiagnosticId::JinjaPropertyNotDefined
    } else if message.contains("enum") && message.contains("string") {
        DiagnosticId::JinjaEnumStringComparison
    } else {
        DiagnosticId::JinjaInvalidType
    }
}

fn collect_type_aliases_for_resolution_context<'db>(
    db: &'db dyn Db,
    res_ctx: &'db baml_compiler2_tir::package_interface::PackageResolutionContext<'db>,
) -> std::collections::HashMap<baml_compiler2_tir::ty::QualifiedTypeName, baml_compiler2_tir::ty::Ty>
{
    let mut aliases = baml_compiler2_tir::inference::collect_type_aliases(db, &res_ctx.own_items);
    for (_dep_name, dep_iface) in &res_ctx.dep_interfaces {
        for types_in_ns in dep_iface.types.values() {
            for exported in types_in_ns.values() {
                if let baml_compiler2_tir::package_interface::ExportedType::TypeAlias {
                    qtn,
                    resolved,
                } = exported
                {
                    aliases.insert(qtn.clone(), resolved.clone());
                }
            }
        }
    }
    aliases
}

fn function_scope_id<'db>(
    index: &baml_compiler2_hir::semantic_index::FileSemanticIndex<'db>,
    func_data: &baml_compiler2_hir::item_tree::Function,
) -> Option<baml_compiler2_hir::scope::ScopeId<'db>> {
    index
        .scopes
        .iter()
        .zip(index.scope_ids.iter())
        .find_map(|(scope, scope_id)| {
            (matches!(scope.kind, ScopeKind::Function)
                && scope.range == func_data.span
                && scope.name.as_ref() == Some(&func_data.name))
            .then_some(*scope_id)
        })
}

fn is_function_default_signature_diagnostic(
    diag: &baml_compiler2_tir::infer_context::TirDiagnostic<'_>,
) -> bool {
    matches!(&diag.primary, DiagnosticLocation::Span(_))
        && matches!(
            &diag.error,
            TirTypeError::RequiredParamAfterDefault { .. }
                | TirTypeError::SelfParamDefault
                | TirTypeError::DefaultParamForwardReference { .. }
                | TirTypeError::TypeMismatch { .. }
        )
}

/// Convert a `RenderedTirDiagnostic` to the shared `Diagnostic` type.
///
/// `RenderedTirDiagnostic` has already resolved arena IDs to `TextRange`.
/// We add the `file_id` to form a full `Span` for the primary annotation.
///
fn tir_rendered_to_diagnostic(
    rendered: baml_compiler2_tir::infer_context::RenderedTirDiagnostic,
    file_id: FileId,
) -> Diagnostic {
    let unknown_member_access_member = match &rendered.error {
        TirTypeError::UnresolvedMember {
            base_type: Ty::BuiltinUnknown { .. },
            member,
        } => Some(member.clone()),
        _ => None,
    };
    let span = Span {
        file_id,
        range: rendered.range,
    };
    let diag = match rendered.severity {
        baml_compiler2_tir::infer_context::DiagnosticSeverity::Warning => Diagnostic::warning(
            tir_type_error_to_diagnostic_id(&rendered.error),
            rendered.message,
        ),
        baml_compiler2_tir::infer_context::DiagnosticSeverity::Error => Diagnostic::error(
            tir_type_error_to_diagnostic_id(&rendered.error),
            rendered.message,
        ),
    };
    let diag = if let Some(member) = &unknown_member_access_member {
        diag.with_primary(
            span,
            format!("use `match` to narrow this value before accessing `{member}`"),
        )
    } else {
        diag.with_primary_span(span)
    };
    rendered
        .related
        .into_iter()
        .fold(diag, |diag, related| {
            let span = Span {
                file_id: related.file_id,
                range: related.range,
            };
            let message = related.message;
            let diag = if unknown_member_access_member.is_some() {
                diag.with_secondary(span, message.clone())
            } else {
                diag
            };
            diag.with_related(span, message)
        })
        .with_phase(DiagnosticPhase::Type)
}

fn tir_rendered_to_diagnostic_for_file(
    db: &dyn Db,
    file: SourceFile,
    mut rendered: baml_compiler2_tir::infer_context::RenderedTirDiagnostic,
) -> Diagnostic {
    rendered.message = source_aware_tir_type_error_message(db, file, &rendered.error);
    tir_rendered_to_diagnostic(rendered, file.file_id(db))
}

fn source_aware_tir_type_error_message(
    db: &dyn Db,
    file: SourceFile,
    error: &TirTypeError,
) -> String {
    let ty = |ty: &Ty| crate::utils::display_ty_for_file(db, file, ty);
    match error {
        TirTypeError::TypeMismatch { expected, got } => {
            format!("type mismatch: expected {}, got {}", ty(expected), ty(got))
        }
        TirTypeError::UnresolvedMember {
            base_type: Ty::BuiltinUnknown { .. },
            member,
        } => {
            format!("cannot access field `{member}` on `unknown`")
        }
        TirTypeError::UnresolvedMember { base_type, member } => {
            format!("type `{}` has no member `{member}`", ty(base_type))
        }
        TirTypeError::NotCallable { ty: callee_ty } => {
            format!(
                "`{}` is not a function — it cannot be called",
                ty(callee_ty)
            )
        }
        TirTypeError::NotIterable { ty: iter_ty } => {
            format!("cannot iterate over type `{}`", ty(iter_ty))
        }
        TirTypeError::NotIndexable { ty: index_ty } => {
            format!("type `{}` is not indexable", ty(index_ty))
        }
        TirTypeError::InvalidBinaryOp { op, lhs, rhs } => {
            format!(
                "operator `{op}` cannot be applied to `{}` and `{}`",
                ty(lhs),
                ty(rhs)
            )
        }
        TirTypeError::InvalidUnaryOp { op, operand } => {
            format!("operator `{op}` cannot be applied to `{}`", ty(operand))
        }
        TirTypeError::OrderingDifferentTypes { op, lhs, rhs } => {
            format!(
                "cannot order `{}` and `{}` with `{op}`: ordering requires both operands \
                 to have the same type",
                ty(lhs),
                ty(rhs)
            )
        }
        TirTypeError::OrderingRequiresCompare { op, ty: operand_ty } => {
            format!(
                "`{}` does not implement `Compare`, so it cannot be ordered with `{op}`",
                ty(operand_ty)
            )
        }
        TirTypeError::ComparisonAlwaysDisjoint { op, lhs, rhs } => {
            let always = if matches!(op, baml_compiler2_ast::BinaryOp::Ne) {
                "true"
            } else {
                "false"
            };
            format!(
                "`{}` and `{}` share no value, so this comparison is always {always}",
                ty(lhs),
                ty(rhs)
            )
        }
        TirTypeError::MissingReturn { expected } => {
            format!("missing return value of type {}", ty(expected))
        }
        TirTypeError::NonExhaustiveMatch {
            scrutinee_type,
            missing_cases,
        } => {
            if missing_cases.is_empty() {
                format!("non-exhaustive match on type {}", ty(scrutinee_type))
            } else {
                format!(
                    "non-exhaustive match on type {}; missing: {}",
                    ty(scrutinee_type),
                    missing_cases.join(", ")
                )
            }
        }
        TirTypeError::NonExhaustiveCatchAll {
            caught_type,
            missing_cases,
        } => {
            if missing_cases.is_empty() {
                format!("non-exhaustive catch_all on type {}", ty(caught_type))
            } else {
                format!(
                    "non-exhaustive catch_all on type {}; missing: {}",
                    ty(caught_type),
                    missing_cases.join(", ")
                )
            }
        }
        TirTypeError::OrPatternBindingTypeMismatch {
            name,
            first_type,
            other_type,
        } => {
            format!(
                "or-pattern binding `{name}` has conflicting types: {} and {}",
                ty(first_type),
                ty(other_type)
            )
        }
        TirTypeError::ThrowsContractViolation {
            declared,
            extra_types,
        } => {
            format!(
                "declared throws is `{}`, but this function may also throw `{}`",
                ty(declared),
                extra_types.join(" | ")
            )
        }
        TirTypeError::CallbackThrowsContractViolation {
            callback_name,
            declared,
            concrete_throws,
        } => {
            if let Some(concrete_throws) = concrete_throws {
                format!(
                    "this body may throw through callback `{callback_name}`, but declared throws is `{}`. Add `throws {}` to the callback, catch the call, or make the callback non-throwing.",
                    ty(declared),
                    ty(concrete_throws)
                )
            } else {
                format!(
                    "this body may throw through callback `{callback_name}`, but declared throws is `{}`. The callback type does not say what it can throw. If `{callback_name}` is an infallible host callback, annotate it with `throws never`; otherwise catch the call or let the enclosing function declare/propagate the callback's throws.",
                    ty(declared)
                )
            }
        }
        TirTypeError::InvalidInterfaceUpcastTarget { target } => {
            format!("`.as<T>` requires an interface target, got {}", ty(target))
        }
        _ => error.to_string(),
    }
}

/// Map a `TirTypeError` to an approximate `DiagnosticId` for structural items.
///
/// This is used when we have access to the typed `TirTypeError` (for class field
/// and type alias diagnostics) rather than just the rendered string.
fn tir_type_error_to_diagnostic_id(
    error: &baml_compiler2_tir::infer_context::TirTypeError,
) -> DiagnosticId {
    use baml_compiler2_tir::infer_context::TirTypeError;
    match error {
        TirTypeError::TypeMismatch { .. } => DiagnosticId::TypeMismatch,
        TirTypeError::UnresolvedMember { .. } => DiagnosticId::NoSuchField,
        TirTypeError::UnresolvedName { .. } => DiagnosticId::UnknownVariable,
        TirTypeError::DeadCode { .. } => DiagnosticId::TypeMismatch,
        TirTypeError::VoidUsedAsValue => DiagnosticId::TypeMismatch,
        TirTypeError::VoidFunctionResultUsed => DiagnosticId::TypeMismatch,
        TirTypeError::SpawnWithNotATransformer { .. } => DiagnosticId::TypeMismatch,
        TirTypeError::NotCallable { .. } => DiagnosticId::NotCallable,
        TirTypeError::NotIterable { .. } => DiagnosticId::NotCallable,
        TirTypeError::NotIndexable { .. } => DiagnosticId::NotIndexable,
        TirTypeError::InvalidBinaryOp { .. } => DiagnosticId::InvalidOperator,
        TirTypeError::OrderingDifferentTypes { .. }
        | TirTypeError::OrderingRequiresCompare { .. }
        | TirTypeError::ComparisonAlwaysDisjoint { .. } => DiagnosticId::InvalidOperator,
        TirTypeError::InvalidUnaryOp { .. } => DiagnosticId::InvalidOperator,
        TirTypeError::UnresolvedType { .. } => DiagnosticId::UnknownType,
        TirTypeError::NonInterfaceProjectionQualifier => DiagnosticId::TypeMismatch,
        TirTypeError::UnknownAssociatedType { .. } => DiagnosticId::UnknownType,
        TirTypeError::AmbiguousAssociatedTypeProjection { .. } => DiagnosticId::TypeMismatch,
        TirTypeError::ArgumentCountMismatch { .. }
        | TirTypeError::PositionalArgumentAfterNamed
        | TirTypeError::DuplicateNamedArgument { .. }
        | TirTypeError::UnknownNamedArgument { .. }
        | TirTypeError::DefaultedParamPassedPositionally { .. }
        | TirTypeError::MissingRequiredArgument { .. } => DiagnosticId::ArgumentCountMismatch,
        TirTypeError::RequiredParamAfterDefault { .. }
        | TirTypeError::SelfParamDefault
        | TirTypeError::DefaultParamForwardReference { .. } => DiagnosticId::TypeMismatch,
        TirTypeError::MissingReturn { .. } => DiagnosticId::MissingReturnExpression,
        TirTypeError::AliasCycle { .. } => DiagnosticId::AliasCycle,
        TirTypeError::ClassCycle { .. } => DiagnosticId::ClassCycle,
        TirTypeError::NonExhaustiveMatch { .. } | TirTypeError::NonExhaustiveCatchAll { .. } => {
            DiagnosticId::NonExhaustiveMatch
        }
        TirTypeError::UnreachableArm => DiagnosticId::UnreachableArm,
        TirTypeError::OrPatternBindingTypeMismatch { .. } => DiagnosticId::TypeMismatch,
        TirTypeError::GenericClassDestructureRequiresTypeArgs { .. } => DiagnosticId::TypeMismatch,
        TirTypeError::RestSubPatternNotSupported => DiagnosticId::TypeMismatch,
        TirTypeError::RefutablePatternInLet { .. } => DiagnosticId::RefutablePatternInLet,
        TirTypeError::IrrefutablePatternInIfLet => DiagnosticId::IrrefutablePatternInIfLet,
        TirTypeError::LetElseMustDiverge { .. } => DiagnosticId::LetElseMustDiverge,
        TirTypeError::IrrefutablePatternInLetElse => DiagnosticId::IrrefutablePatternInLetElse,
        TirTypeError::IrrefutablePatternInWhileLet => DiagnosticId::IrrefutablePatternInWhileLet,
        TirTypeError::InvalidCatchBindingType { .. } => DiagnosticId::InvalidCatchBindingType,
        TirTypeError::DeferControlFlowEscape { .. } => DiagnosticId::DeferControlFlowEscape,
        TirTypeError::ThrowsContractViolation { .. }
        | TirTypeError::CallbackThrowsContractViolation { .. } => {
            DiagnosticId::ThrowsContractViolation
        }
        TirTypeError::ExtraneousThrowsDeclaration { .. } => DiagnosticId::ThrowsContractExtraneous,
        TirTypeError::CannotInferTypeParameter { .. } => DiagnosticId::UnknownType,
        TirTypeError::TypeParamShadowed { .. } => DiagnosticId::TypeMismatch,
        TirTypeError::CannotInferLambdaParamType { .. } => DiagnosticId::UnknownType,
        TirTypeError::WrongNumberOfTypeArgs { .. } => DiagnosticId::TypeMismatch,
        TirTypeError::TypeIsNotGeneric { .. } => DiagnosticId::TypeMismatch,
        TirTypeError::GenericFunctionValueNotSpecialized { .. } => DiagnosticId::TypeMismatch,
        TirTypeError::WrongTypeArgArity { .. } => DiagnosticId::ArgumentCountMismatch,
        // Optional chaining diagnostics
        TirTypeError::UnnecessaryOptionalChaining { .. } => DiagnosticId::InvalidOperator,
        TirTypeError::UnnecessaryNullCoalesce { .. } => DiagnosticId::InvalidOperator,
        TirTypeError::SuggestNullCoalesce { .. } => DiagnosticId::InvalidOperator,
        TirTypeError::NullCoalesceWithNull { .. } => DiagnosticId::InvalidOperator,
        TirTypeError::NullableMemberAccess { .. } => DiagnosticId::TypeMismatch,
        TirTypeError::TaggedTagNotAFunction { .. } => DiagnosticId::NotCallable,
        TirTypeError::TaggedTagNotMarked { .. } | TirTypeError::TaggedTagBadBodyParam { .. } => {
            DiagnosticId::TypeMismatch
        }
        TirTypeError::InterpolatedValueMaybeNull { .. }
        | TirTypeError::TypeNotInterpolatable { .. } => DiagnosticId::TypeMismatch,
        TirTypeError::AmbiguousInterfaceMethod { .. } => DiagnosticId::AmbiguousInterfaceMethod,
        TirTypeError::AmbiguousInterfaceField { .. } => DiagnosticId::AmbiguousInterfaceField,
        TirTypeError::InterfaceFieldRequiresProjection { .. } => DiagnosticId::NoSuchField,
        TirTypeError::InterfaceFieldRequiresQualifiedConstruction { .. } => {
            DiagnosticId::NoSuchField
        }
        TirTypeError::DeprecatedInterfaceProjection { .. } => DiagnosticId::NoSuchField,
        TirTypeError::InvalidInterfaceUpcastTarget { .. } => DiagnosticId::TypeMismatch,
        TirTypeError::InterfaceMemberRequiresReceiver { .. } => DiagnosticId::NoSuchField,
        TirTypeError::InvalidSelfCallThroughInterface { .. } => DiagnosticId::TypeMismatch,
        TirTypeError::DefaultOnRequiredMethod { .. } => DiagnosticId::DefaultOnRequiredMethod,
        TirTypeError::BareDefaultKeyword => DiagnosticId::BareDefaultKeyword,
        TirTypeError::TypeDoesNotImplementInterface { .. } => DiagnosticId::TypeMismatch,
        TirTypeError::BlanketBoundNotSatisfied { .. } => DiagnosticId::TypeMismatch,
        TirTypeError::AmbiguousInterfaceInstantiation { .. } => DiagnosticId::OverlappingImplements,
        // `$id` special-form misuse: an invalid assignment/access shape, not
        // a name-resolution failure.
        TirTypeError::RuntimeIdCompoundAssignment
        | TirTypeError::RuntimeIdMemberAccess { .. }
        | TirTypeError::RuntimeIdCallSiteArgument => DiagnosticId::TypeMismatch,
        TirTypeError::IntegerLiteralOutOfRange { .. } => DiagnosticId::IntegerLiteralOutOfRange,
        TirTypeError::GenericBoundNotInterface { .. } => DiagnosticId::GenericBoundNotInterface,
    }
}

#[cfg(test)]
mod tests {
    use baml_compiler_diagnostics::Severity;
    use baml_compiler2_tir::infer_context::{DiagnosticSeverity, RenderedTirDiagnostic};
    use text_size::{TextRange, TextSize};

    use super::*;
    use crate::testing::CursorTest;

    fn dummy_file_id() -> FileId {
        // Use index 0 — sufficient for span construction in unit tests.
        FileId::new(0)
    }

    fn dummy_rendered(severity: DiagnosticSeverity) -> RenderedTirDiagnostic {
        RenderedTirDiagnostic {
            error: baml_compiler2_tir::infer_context::TirTypeError::TypeMismatch {
                expected: baml_compiler2_tir::ty::Ty::Never {
                    attr: baml_compiler2_tir::ty::TyAttr::default(),
                },
                got: baml_compiler2_tir::ty::Ty::Never {
                    attr: baml_compiler2_tir::ty::TyAttr::default(),
                },
            },
            message: "test message".to_string(),
            range: TextRange::new(TextSize::from(0u32), TextSize::from(5u32)),
            severity,
            related: Vec::new(),
        }
    }

    #[test]
    fn tir_warning_severity_maps_to_warning_diagnostic() {
        let rendered = dummy_rendered(DiagnosticSeverity::Warning);
        let diag = tir_rendered_to_diagnostic(rendered, dummy_file_id());
        assert_eq!(
            diag.severity,
            Severity::Warning,
            "DiagnosticSeverity::Warning must produce a warning-level Diagnostic"
        );
    }

    #[test]
    fn tir_error_severity_maps_to_error_diagnostic() {
        let rendered = dummy_rendered(DiagnosticSeverity::Error);
        let diag = tir_rendered_to_diagnostic(rendered, dummy_file_id());
        assert_eq!(
            diag.severity,
            Severity::Error,
            "DiagnosticSeverity::Error must produce an error-level Diagnostic"
        );
    }

    #[test]
    fn check_file_reports_concrete_callback_throws_violation() {
        // The callback's effective throws (`string`) now binds the callee's
        // effect param, so a COVERED callback throw (`demo() throws string`)
        // no longer reports at all (the old "current limitation" is lifted),
        // and a genuine violation reports the precise concrete type.
        let test = CursorTest::new(
            r#"function forward(cb: (x: int) -> int) -> int {
  return cb(1)
}

function demo() -> int throws never {
  return forward((x: int) -> int {
    throw "boom"
  })
}
<[CURSOR]"#,
        );

        let diagnostics = check_file(&test.db, test.cursor.file);
        let diag = diagnostics
            .iter()
            .find(|diag| {
                diag.id == DiagnosticId::ThrowsContractViolation
                    && diag.message.contains("declared throws is `never`")
                    && diag.message.contains("`string`")
            })
            .expect("concrete callback throws violation");
        // The concrete-type violation carries no callback-provenance related
        // info (that path only fires when the thrown type cannot be named).
        assert!(diag.related_info.is_empty());
    }

    #[test]
    fn check_file_reports_unknown_prompt_variable() {
        let test = CursorTest::new(
            r##"client GPT4o {
  provider openai
  options {
    model "gpt-5"
    api_key env.OPENAI_API_KEY
  }
}

class GuessResponse {
  game_won bool
  text string
}

function TakeGuess(user_guess: string, famous_person_name: string) -> GuessResponse {
  client GPT4o
  prompt #"
    {{ famouse_person_name | lower }}

    {{ ctx.output_format }}
  "#
}
<[CURSOR]"##,
        );

        let diagnostics = check_file(&test.db, test.cursor.file);
        let diag = diagnostics
            .iter()
            .find(|diag| diag.id == DiagnosticId::JinjaUnresolvedVariable)
            .expect("unknown prompt variable diagnostic");

        assert!(diag.message.contains("`famouse_person_name`"));
        assert!(diag.message.contains("does not exist"));
        let span = diag.primary_span().expect("primary span");
        let text = test.cursor.file.text(&test.db);
        let start: usize = span.range.start().into();
        let end: usize = span.range.end().into();
        assert_eq!(&text[start..end], "famouse_person_name");
    }

    #[test]
    fn check_file_allows_template_string_call_in_prompt() {
        let test = CursorTest::new(
            r##"client GPT4o {
  provider openai
  options {
    model "gpt-5"
    api_key env.OPENAI_API_KEY
  }
}

template_string GuessHeader(name: string) #"
  Guess the person: {{ name }}
"#

function TakeGuess(famous_person_name: string) -> string {
  client GPT4o
  prompt #"
    {{ GuessHeader(famous_person_name) }}
  "#
}
<[CURSOR]"##,
        );

        let diagnostics = check_file(&test.db, test.cursor.file);
        assert!(
            diagnostics
                .iter()
                .all(|diag| diag.id != DiagnosticId::JinjaUnresolvedVariable),
            "template string call should not be reported as an unknown prompt variable: {diagnostics:#?}"
        );
    }

    #[test]
    fn check_file_reports_unknown_template_string_argument() {
        let test = CursorTest::new(
            r##"client GPT4o {
  provider openai
  options {
    model "gpt-5"
    api_key env.OPENAI_API_KEY
  }
}

template_string GuessHeader(name: string) #"
  Guess the person: {{ name }}
"#

function TakeGuess(famous_person_name: string) -> string {
  client GPT4o
  prompt #"
    {{ GuessHeader(famouse_person_name) }}
  "#
}
<[CURSOR]"##,
        );

        let diagnostics = check_file(&test.db, test.cursor.file);
        let diag = diagnostics
            .iter()
            .find(|diag| diag.id == DiagnosticId::JinjaUnresolvedVariable)
            .expect("unknown template string argument diagnostic");

        assert!(diag.message.contains("`famouse_person_name`"));
        assert!(diag.message.contains("does not exist"));
        let span = diag.primary_span().expect("primary span");
        let text = test.cursor.file.text(&test.db);
        let start: usize = span.range.start().into();
        let end: usize = span.range.end().into();
        assert_eq!(&text[start..end], "famouse_person_name");
    }

    #[test]
    fn check_file_reports_template_string_call_errors() {
        let test = CursorTest::new(
            r##"template_string WithParams(a: int) #"
  ...
"#

template_string BadCall1() #"
  {{ WithParams(a=2, b=2) }}
"#

template_string BadCall2() #"
  {{ WithParams("a") }}
"#
<[CURSOR]"##,
        );

        let diagnostics = check_file(&test.db, test.cursor.file);
        assert!(
            diagnostics
                .iter()
                .any(|diag| diag.id == DiagnosticId::JinjaWrongArgCount
                    && diag.message.contains("expects 1 arguments")),
            "{diagnostics:#?}"
        );
        assert!(
            diagnostics
                .iter()
                .any(|diag| diag.id == DiagnosticId::JinjaWrongArgType
                    && diag.message.contains("expects argument 'a'")),
            "{diagnostics:#?}"
        );
    }

    #[test]
    fn check_file_resolves_cross_file_jinja_template_strings() {
        let mut builder = CursorTest::builder();
        builder.source(
            "shared.baml",
            r##"class Person {
  name string
}

template_string PersonHeader(person: Person) #"
  {{ person.name }}
"#
"##,
        );
        builder.source(
            "main.baml",
            r##"client GPT4o {
  provider openai
  options {
    model "gpt-5"
    api_key env.OPENAI_API_KEY
  }
}

function TakeGuess(person: Person) -> string {
  client GPT4o
  prompt #"
    {{ PersonHeader(person) }}
  "#
}
<[CURSOR]"##,
        );
        let test = builder.build();

        let diagnostics = check_file(&test.db, test.cursor.file);
        assert!(
            diagnostics
                .iter()
                .all(|diag| diag.id != DiagnosticId::JinjaUnresolvedVariable),
            "cross-file template string calls should resolve in prompts: {diagnostics:#?}"
        );
    }

    #[test]
    fn check_file_reports_builtin_function_default_constraints() {
        let test = CursorTest::new(
            r#"function BadBuiltin(
  a: int = b,
  b: int = 1,
  label: string = 2,
  required: int
) -> int {
  $rust_function
}
<[CURSOR]"#,
        );

        let messages = check_file(&test.db, test.cursor.file)
            .into_iter()
            .map(|diag| diag.message)
            .collect::<Vec<_>>();

        assert!(
            messages.iter().any(|message| message
                == "default for parameter `a` cannot reference later parameter `b`"),
            "missing forward-reference diagnostic; got {messages:#?}"
        );
        assert!(
            messages
                .iter()
                .any(|message| message == "type mismatch: expected string, got 2"),
            "missing default type-mismatch diagnostic; got {messages:#?}"
        );
        assert!(
            messages.iter().any(|message| message
                == "required parameter `required` cannot appear after a defaulted parameter"),
            "missing required-after-default diagnostic; got {messages:#?}"
        );
    }

    #[test]
    fn check_file_reports_builtin_self_default_constraint() {
        let test = CursorTest::new(
            r#"class Counter {
  value int

  function Current(self = null) -> int {
    $rust_function
  }
}
<[CURSOR]"#,
        );

        let messages = check_file(&test.db, test.cursor.file)
            .into_iter()
            .map(|diag| diag.message)
            .collect::<Vec<_>>();

        assert!(
            messages
                .iter()
                .any(|message| message == "`self` cannot have a default value"),
            "missing self-default diagnostic; got {messages:#?}"
        );
    }
}
