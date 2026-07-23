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

use std::collections::HashSet;

use baml_base::{FileId, Name, SourceFile, Span};
use baml_compiler_diagnostics::{
    Diagnostic, DiagnosticId, DiagnosticPhase, ParseError, ToDiagnostic,
};
use baml_compiler2_hir::{file_semantic_index, scope::ScopeKind};
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
    //
    // Suppress type-inference diagnostics for any scope whose body failed to
    // parse. When a function or lambda body contains a syntax error, parser
    // error recovery produces a malformed AST that then yields cascading,
    // misleading type errors. For example, the braceless lambda
    // `(x: int) => x + 1` makes the parser consume `x` as the lambda's *return
    // type* and leaves `+ 1` dangling, emitting `unresolved type: x` and
    // `operator + cannot be applied to a function` *before* the real
    // `Expected lambda body '{'` syntax error. Tainting the innermost executable
    // scope containing each parse error (plus its descendant scopes) keeps the
    // syntax error as the only diagnostic surfaced for the broken body. Parse
    // errors at structural positions (class/enum names, top-level declarations)
    // never taint a scope, so unrelated type errors elsewhere in the file are
    // unaffected.
    let tainted = parse_error_tainted_scopes(index, &parse_errors);
    // Drive inference with PPIR's canonical (post-`$stream`-expansion) ScopeIds,
    // not HIR's. `ScopeId` is a Salsa *tracked* struct, so HIR's index and
    // PPIR's expanded index mint distinct Salsa IDs for the same
    // (file, FileScopeId) pair; keying `infer_scope_types` with HIR IDs here
    // made every scope in a `$stream`-expanded file get inferred a second time
    // when TIR/MIR later asked with the PPIR ID. The original file's scopes are
    // a prefix of the expanded index — the same invariant `infer_scope_types`
    // relies on when it resolves a `FileScopeId` in the expanded arena — and we
    // iterate only that prefix, so synthetic `*$stream` scopes are never
    // visited and diagnostics are unchanged.
    let ppir_index = baml_compiler2_ppir::file_semantic_index(db, file);
    for (file_scope_idx, scope_id) in ppir_index
        .scope_ids
        .iter()
        .take(index.scopes.len())
        .enumerate()
    {
        if tainted.contains(&file_scope_idx) {
            continue;
        }
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

    let pkg_info = baml_compiler2_hir::file_package::file_package(db, file);
    let pkg_id = baml_compiler2_hir::package::PackageId::new(db, pkg_info.package.clone());
    let res_ctx = baml_compiler2_tir::package_interface::package_resolution_context(db, pkg_id);
    let pkg_items = &res_ctx.own_items;
    // Salsa-cached per package — previously rebuilt (and cloned per function
    // below) on every file check.
    let aliases = baml_compiler2_tir::inference::package_resolved_aliases(db, pkg_id);
    // Reuse the memoized CST → AST lowering instead of re-lowering here.
    let ast_items = &baml_compiler2_hir::file_ast(db, file).items;
    diagnostics.extend(validate_associated_type_bindings_in_items(
        db,
        file_id,
        ast_items,
        pkg_items,
        &pkg_info.namespace_path,
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
        file,
        pkg_items,
        &pkg_info.namespace_path,
        source_text,
    ));

    // ── 6. Function signature diagnostics ────────────────────────────────────
    //
    // FIXME(lsp-validation-antipattern): this re-implements TIR signature
    // checking off the item-data firewall. Validation logic belongs in TIR/HIR
    // with check.rs only surfacing diagnostics; it stays here because the
    // default-signature diagnostics it emits have no TIR home yet.
    //
    // Build a method → enclosing class list so we can merge class generic params.
    let mut method_to_class = Vec::new();
    for &class_loc in baml_compiler2_ppir::item_data::file_classes(db, file) {
        let class_data = baml_compiler2_ppir::item_data::class_data(db, class_loc);
        for &method_loc in &class_data.methods {
            method_to_class.push((method_loc, class_loc));
        }
    }
    // Out-of-body `implement Interface for Type` methods: their `Self` resolves
    // to the `for` target and the block's generic params are in scope. Bodied
    // (`$rust_function`/builtin) impl methods skip the scope-inference path, so
    // without this their signatures would leave `Self` unresolved here.
    let mut method_to_impl = Vec::new();
    for &impl_loc in baml_compiler2_ppir::item_data::file_free_impls(db, file) {
        let block = baml_compiler2_ppir::item_data::impl_block_data(db, impl_loc);
        for &method_loc in &block.methods {
            method_to_impl.push((method_loc, impl_loc));
        }
    }

    for &func_loc in baml_compiler2_ppir::item_data::file_functions(db, file) {
        let func_data = baml_compiler2_ppir::item_data::function_data(db, func_loc);
        // Expression-body functions already have their signatures checked during
        // scope inference (step 3); only check non-expr bodies here. `function_body`
        // is tracked (its `Arc` is cached per revision), so this body-kind test is
        // O(1) on repeat rather than re-cloning the `ExprBody` arena each call.
        if matches!(
            baml_compiler2_ppir::function_body(db, func_loc).as_ref(),
            baml_compiler2_hir::body::FunctionBody::Expr(..)
        ) {
            continue;
        }

        let func_source_map = baml_compiler2_ppir::item_data::function_source_map(db, func_loc);
        let mut type_errors = Vec::new();
        let mut param_types = Vec::new();

        // Compute the effective generic params: method params + enclosing class params.
        let mut generic_params = func_data.generic_params.clone();
        let enclosing_class = method_to_class
            .iter()
            .find(|(mid, _)| *mid == func_loc)
            .map(|(_, class_loc)| *class_loc);
        if let Some(class_loc) = enclosing_class {
            let class_data = baml_compiler2_ppir::item_data::class_data(db, class_loc);
            // Prepend class generic params (class params come first, method params after)
            let mut merged = class_data.generic_params.clone();
            merged.extend(generic_params);
            generic_params = merged;
        }
        // BEP-044: inside an out-of-body `implement Interface for Type` block,
        // `Self` is the `for` target and the block's generic params are in scope.
        let enclosing_impl = method_to_impl
            .iter()
            .find(|(mid, _)| *mid == func_loc)
            .map(|(_, imp)| *imp);
        let enclosing_impl_data =
            enclosing_impl.map(|imp| baml_compiler2_ppir::item_data::impl_block_data(db, imp));
        if let Some(block) = enclosing_impl_data
            && let baml_compiler2_ppir::item_data::ImplSubjectData::Free { generics, .. } =
                &block.subject
        {
            let mut merged: Vec<Name> = generics.iter().map(|g| g.name.clone()).collect();
            merged.extend(generic_params);
            generic_params = merged;
        }
        // Pre-resolve `Self` to the enclosing impl's `for` target before lowering
        // signature types, mirroring the body path in `tir::inference`. The
        // for-target lives in the impl block's own type-ref arena.
        let self_replacement: Option<(
            &baml_compiler2_hir::type_ref::TypeRefStore,
            baml_compiler2_hir::type_ref::TypeRefId,
        )> = enclosing_impl_data.and_then(|block| match &block.subject {
            baml_compiler2_ppir::item_data::ImplSubjectData::Free { for_target, .. } => {
                Some((&block.type_refs, *for_target))
            }
            baml_compiler2_ppir::item_data::ImplSubjectData::InClass { .. } => None,
        });
        // BEP-044: `Self` in an out-of-body impl method's signature is the impl's
        // `for` target. The lowering context carries it as `self_ty`, so `Self`
        // resolves during lowering (no textual substitution).
        let self_ty: Option<Ty> = self_replacement.map(|(store, id)| {
            baml_compiler2_tir::lower_type_expr::lower_type_ref(
                store,
                id,
                &baml_compiler2_tir::lower_type_expr::ScopeCtx {
                    db,
                    package_items: pkg_items,
                    ns_context: &pkg_info.namespace_path,
                    generic_params: &generic_params,
                    bounds: &baml_compiler2_tir::lower_type_expr::TypeVarBoundsMap::default(),
                    self_ty: None,
                },
                &mut Vec::new(),
            )
        });
        let lower_sig_ref = |id: baml_compiler2_hir::type_ref::TypeRefId,
                             generic_params: &[Name],
                             diags: &mut Vec<baml_compiler2_tir::infer_context::TirTypeError>|
         -> Ty {
            baml_compiler2_tir::lower_type_expr::lower_type_ref(
                &func_data.type_refs,
                id,
                &baml_compiler2_tir::lower_type_expr::ScopeCtx {
                    db,
                    package_items: pkg_items,
                    ns_context: &pkg_info.namespace_path,
                    generic_params,
                    bounds: &baml_compiler2_tir::lower_type_expr::TypeVarBoundsMap::default(),
                    self_ty: self_ty.clone(),
                },
                diags,
            )
        };

        // Check return type — anchor diagnostics at the return type's source span.
        if let Some(ret_id) = func_data.return_type {
            lower_sig_ref(ret_id, &generic_params, &mut type_errors);
            if !type_errors.is_empty() {
                let range = func_source_map.type_refs.span(ret_id);
                for error in type_errors.drain(..) {
                    diagnostics.push(
                        Diagnostic::error(
                            tir_type_error_to_diagnostic_id(&error),
                            error.to_string(),
                        )
                        .with_primary_span(Span { file_id, range })
                        .with_phase(DiagnosticPhase::Type),
                    );
                }
            }
        }

        // Check parameter types — anchor diagnostics at each annotation's span.
        for param in &func_data.params {
            type_errors.clear();
            let param_ty = if param.name.as_str() == "self" && param.type_ref.is_none() {
                // `self`'s type is the enclosing receiver: the class for an in-body
                // method, or the impl's `for` target for an out-of-body
                // `implement I for C` method (mirroring the body path in
                // `tir::inference`). Falling back to `Unknown` would otherwise
                // leave `self` untyped in the latter case.
                if let Some(class_loc) = enclosing_class {
                    let class_data = baml_compiler2_ppir::item_data::class_data(db, class_loc);
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
                } else if let Some((store, id)) = self_replacement {
                    baml_compiler2_tir::lower_type_expr::lower_type_ref(
                        store,
                        id,
                        &baml_compiler2_tir::lower_type_expr::ScopeCtx {
                            db,
                            package_items: pkg_items,
                            ns_context: &pkg_info.namespace_path,
                            generic_params: &generic_params,
                            bounds: &baml_compiler2_tir::lower_type_expr::TypeVarBoundsMap::default(
                            ),
                            self_ty: None,
                        },
                        &mut type_errors,
                    )
                } else {
                    Ty::Unknown {
                        attr: TyAttr::default(),
                    }
                }
            } else if let Some(param_id) = param.type_ref {
                lower_sig_ref(param_id, &generic_params, &mut type_errors)
            } else {
                // A missing annotation lowered to the `Unknown` sentinel, which
                // yields `Ty::Unknown` with no diagnostic.
                Ty::Unknown {
                    attr: TyAttr::default(),
                }
            };
            if !type_errors.is_empty() {
                if let Some(type_id) = param.type_ref {
                    let range = func_source_map.type_refs.span(type_id);
                    for error in type_errors.drain(..) {
                        diagnostics.push(
                            Diagnostic::error(
                                tir_type_error_to_diagnostic_id(&error),
                                error.to_string(),
                            )
                            .with_primary_span(Span { file_id, range })
                            .with_phase(DiagnosticPhase::Type),
                        );
                    }
                }
            }
            param_types.push((param.name.clone(), param_ty));
        }

        if let Some(scope_id) = baml_compiler2_ppir::item_data::function_scope(db, func_loc) {
            let context = baml_compiler2_tir::infer_context::InferContext::new(db, scope_id);
            let mut builder = baml_compiler2_tir::builder::TypeInferenceBuilder::new(
                context, res_ctx, pkg_id, scope_id, aliases,
            );
            builder.set_generic_params(generic_params);
            for (name, ty) in &param_types {
                builder.add_local(name.clone(), ty.clone());
                builder.param_types.push((name.clone(), ty.clone()));
            }
            let parameter_defaults =
                baml_compiler2_hir::signature::function_parameter_defaults(db, func_loc);
            builder.check_function_parameter_defaults(
                &baml_compiler2_ppir::item_data::function_data(db, func_loc).params,
                &baml_compiler2_ppir::item_data::function_source_map(db, func_loc).param_spans,
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
                _nested_lambda_inference,
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

    // ── 7. Interface impl + coherence diagnostics (BEP-044) ──────────────────
    //
    // tir2 owns all of these; check.rs is only the surfacing host. See
    // `check_interfaces` for the three families it surfaces (structural impl
    // diagnostics, coherence overlap, and impl signature/type conformance).
    diagnostics.extend(check_interfaces(db, file, file_id));

    // Deduplicate: multiple steps can produce the same diagnostic (e.g. scope
    // inference + signature validation for the same unresolved return type).
    diagnostics.dedup_by(|a, b| {
        a.code() == b.code() && a.message == b.message && a.primary_span() == b.primary_span()
    });

    diagnostics
}

/// Indices (into the file's scope arena) of scopes whose TIR type-inference
/// diagnostics should be suppressed because the scope body failed to parse.
///
/// Parser error recovery turns broken syntax into a malformed AST that then
/// produces cascading, misleading type errors. We taint the innermost
/// *executable* scope containing each parse error, plus all of its descendant
/// scopes, so a single syntax error doesn't drag a pile of spurious type errors
/// along with it. Parse errors at structural positions (class/enum names,
/// top-level declarations, type-alias bodies) resolve to a structural scope and
/// are ignored here, leaving genuine type errors elsewhere in the file intact.
fn parse_error_tainted_scopes(
    index: &baml_compiler2_hir::semantic_index::FileSemanticIndex<'_>,
    parse_errors: &[ParseError],
) -> HashSet<usize> {
    let mut tainted = HashSet::new();
    for err in parse_errors {
        let span = match err {
            ParseError::UnexpectedToken { span, .. }
            | ParseError::UnexpectedEof { span, .. }
            | ParseError::InvalidSyntax { span, .. } => span,
        };
        let fsid = index.scope_at_offset(span.range.start(), None);
        let scope = &index.scopes[fsid.index() as usize];
        if !matches!(
            scope.kind,
            ScopeKind::Function
                | ScopeKind::Lambda
                | ScopeKind::Block
                | ScopeKind::Let
                | ScopeKind::MatchArm
                | ScopeKind::CatchClause
                | ScopeKind::CatchArm
        ) {
            continue;
        }
        tainted.insert(fsid.index() as usize);
        for d in scope.descendants.start.index()..scope.descendants.end.index() {
            tainted.insert(d as usize);
        }
    }
    tainted
}

/// Surface tir2's interface `implements` diagnostics for a single file.
///
/// check.rs owns none of the validation here — tir2 computes it and this
/// function only maps each span-free diagnostic origin onto a source range.
/// Three complementary families are surfaced (this is the *only* place any of
/// them reaches the LSP):
///
/// 1. **Structural impl diagnostics** — `impl_data(loc).diagnostics`: the
///    name/membership checks (unknown interface member, missing/extra override,
///    field-link and associated-binding hygiene). `impl_data` owns these even
///    when the impl fails to fully resolve.
/// 2. **Coherence (E0132)** — `package_coherence_diagnostics`: overlapping
///    `implements` for the same receiver/interface. A per-package property over
///    the whole dependency closure, computed once and filtered to the impls in
///    this file.
/// 3. **Signature / type conformance** — `validate_impl_signatures(loc)`: field
///    and method *type* mismatches (E0116 / E0120), missing-required-interface,
///    non-concrete target, unconstrained/orphan params, cyclic impl headers, and
///    associated-binding-bound violations. Nothing else in the workspace calls
///    this query, so it must be surfaced here.
fn check_interfaces(db: &dyn Db, file: SourceFile, file_id: FileId) -> Vec<Diagnostic> {
    use baml_compiler2_tir::interfaces::ImplDataError;

    let mut diagnostics = Vec::new();

    // ── 1. Coherence (E0132) ─────────────────────────────────────────────────
    //
    // Overlap is a per-package property over the whole dependency closure, not a
    // per-file one. Compute it once for the package and surface the violations
    // whose offending impl lives in this file (its conflicting partner may be in
    // another file or a dependency).
    let package = baml_compiler2_hir::file_package::file_package(db, file).package;
    let pkg_id = baml_compiler2_hir::package::PackageId::new(db, package);
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

    // ── 2 + 3. Per-impl structural + signature/conformance diagnostics ────────
    //
    // `impl_data(loc).diagnostics` (name/membership) and
    // `validate_impl_signatures(loc)` (type conformance) each yield
    // `(TirTypeError, ImplDiagnosticLocation)` pairs anchored via the same source
    // map; a `Method` / field-link / binding location may mark several sites.
    for &impl_loc in baml_compiler2_ppir::item_data::file_impls(db, file) {
        let sm = baml_compiler2_tir::interfaces::impl_data_source_map(db, impl_loc);
        // `impl_data` owns an impl's structural diagnostics whether or not it
        // fully resolves: an unresolved interface target still carries the
        // diagnostics it lowered (the bad target, the for-target, the bounds). A
        // cyclic header carries none — `validate_impl_signatures` re-detects and
        // surfaces `CyclicImplHeader` for it.
        let structural = match baml_compiler2_tir::interfaces::impl_data(db, impl_loc).as_ref() {
            Ok(data) => Some(&data.diagnostics),
            Err(ImplDataError::InterfaceUnresolved { diagnostics }) => Some(diagnostics),
            Err(ImplDataError::CyclicHeader | ImplDataError::Malformed) => None,
        };
        let signatures = baml_compiler2_tir::interfaces::validate_impl_signatures(db, impl_loc);
        for (error, loc) in structural.into_iter().flatten().chain(signatures) {
            for span in impl_diagnostic_spans(loc, sm) {
                diagnostics.push(
                    Diagnostic::error(tir_type_error_to_diagnostic_id(error), error.to_string())
                        .with_primary_span(span)
                        .with_phase(DiagnosticPhase::Type),
                );
            }
        }
    }

    diagnostics
}

/// Resolve an [`ImplDiagnosticLocation`] to the source span(s) it marks, via an
/// impl block's [`ImplDataSourceMap`]. The name-keyed locations (`Method`,
/// field links, associated bindings) can mark several same-named sites; an empty
/// or missing entry falls back to the whole-block span.
fn impl_diagnostic_spans(
    loc: &baml_compiler2_tir::interfaces::ImplDiagnosticLocation,
    sm: &baml_compiler2_tir::interfaces::ImplDataSourceMap,
) -> Vec<Span> {
    use baml_compiler2_tir::interfaces::ImplDiagnosticLocation;

    let named = |spans: Option<&Vec<Span>>| -> Vec<Span> {
        spans
            .filter(|v| !v.is_empty())
            .cloned()
            .unwrap_or_else(|| vec![sm.impl_span])
    };
    match loc {
        ImplDiagnosticLocation::InterfaceTarget => vec![sm.interface_target_span],
        ImplDiagnosticLocation::ForTarget => vec![sm.for_target_span.unwrap_or(sm.impl_span)],
        ImplDiagnosticLocation::Bound => vec![sm.impl_span],
        ImplDiagnosticLocation::Method(name) => named(sm.method_spans.get(name)),
        ImplDiagnosticLocation::InterfaceFieldLink(name) => {
            named(sm.interface_field_link_spans.get(name))
        }
        ImplDiagnosticLocation::ClassFieldLink(name) => named(sm.class_field_link_spans.get(name)),
        ImplDiagnosticLocation::AssociatedBinding(name) => {
            named(sm.associated_binding_spans.get(name))
        }
    }
}

/// Resolve a `TypeExprKind::Path` to an interface, by name, walking the package.
///
/// Returns `None` if the path doesn't name an interface (including: name
/// doesn't exist, or resolves to a class/enum/etc.).
#[derive(Debug, Clone)]
struct ResolvedInterfaceData<'db> {
    loc: baml_compiler2_hir::loc::InterfaceLoc<'db>,
    qtn: QualifiedTypeName,
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
    Some(ResolvedInterfaceData {
        loc: resolved.loc,
        qtn: resolved.qtn,
    })
}

/// The `TypeRef`-arena twin of [`resolve_interface_path`], for walking a
/// `requires` target held as firewall data (`interface_data(…).type_refs` plus a
/// `TypeRefId`) rather than an AST node.
fn resolve_interface_ref<'db>(
    db: &'db dyn Db,
    store: &baml_compiler2_hir::type_ref::TypeRefStore,
    target: baml_compiler2_hir::type_ref::TypeRefId,
    pkg_items: &baml_compiler2_hir::package::PackageItems<'db>,
    namespace_path: &[Name],
) -> Option<ResolvedInterfaceData<'db>> {
    let resolved = baml_compiler2_tir::interfaces::resolve_ref_to_interface_identity(
        db,
        store,
        target,
        pkg_items,
        namespace_path,
    )?;
    Some(ResolvedInterfaceData {
        loc: resolved.loc,
        qtn: resolved.qtn,
    })
}

fn validate_associated_type_bindings_in_items(
    db: &dyn Db,
    file_id: FileId,
    items: &[baml_compiler2_ast::Item],
    pkg_items: &baml_compiler2_hir::package::PackageItems<'_>,
    namespace_path: &[Name],
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
                    &empty_bounds,
                    pkg_items,
                    namespace_path,
                    &mut diagnostics,
                );
            }
            baml_compiler2_ast::Item::Class(class) => {
                let outer_bounds =
                    generic_bound_expr_map(&class.generic_params, &class.generic_param_bounds);
                for method in &class.methods {
                    validate_associated_type_bindings_in_function(
                        db,
                        file_id,
                        method,
                        &outer_bounds,
                        pkg_items,
                        namespace_path,
                        &mut diagnostics,
                    );
                }
                for block in &class.implements {
                    for method in &block.methods {
                        validate_associated_type_bindings_in_function(
                            db,
                            file_id,
                            method,
                            &outer_bounds,
                            pkg_items,
                            namespace_path,
                            &mut diagnostics,
                        );
                    }
                }
            }
            baml_compiler2_ast::Item::Interface(iface) => {
                validate_associated_type_declaration_names(file_id, iface, &mut diagnostics);
                let iface_bounds =
                    generic_bound_expr_map(&iface.generic_params, &iface.generic_param_bounds);
                for method in &iface.required_methods {
                    validate_associated_type_bindings_in_method_sig(
                        db,
                        file_id,
                        method,
                        &iface_bounds,
                        pkg_items,
                        namespace_path,
                        &mut diagnostics,
                    );
                }
                for method in &iface.default_methods {
                    validate_associated_type_bindings_in_function(
                        db,
                        file_id,
                        method,
                        &iface_bounds,
                        pkg_items,
                        namespace_path,
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
                for method in &imp.methods {
                    validate_associated_type_bindings_in_function(
                        db,
                        file_id,
                        method,
                        &impl_bounds,
                        pkg_items,
                        namespace_path,
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

fn validate_associated_type_bindings_in_function(
    db: &dyn Db,
    file_id: FileId,
    function: &baml_compiler2_ast::FunctionDef,
    outer_generic_bounds: &GenericBoundExprMap,
    pkg_items: &baml_compiler2_hir::package::PackageItems<'_>,
    namespace_path: &[Name],
    diagnostics: &mut Vec<Diagnostic>,
) {
    let generic_bounds = extend_generic_bound_expr_map(
        outer_generic_bounds,
        &function.generic_params,
        &function.generic_param_bounds,
    );
    for param in &function.params {
        if let Some(te) = &param.type_expr {
            validate_ambiguous_typevar_associated_projection_in_type_expr(
                db,
                file_id,
                te,
                te.span,
                pkg_items,
                namespace_path,
                &generic_bounds,
                diagnostics,
            );
        }
    }
    if let Some(ret) = &function.return_type {
        validate_ambiguous_typevar_associated_projection_in_type_expr(
            db,
            file_id,
            ret,
            ret.span,
            pkg_items,
            namespace_path,
            &generic_bounds,
            diagnostics,
        );
    }
    if let Some(throws) = &function.throws {
        validate_ambiguous_typevar_associated_projection_in_type_expr(
            db,
            file_id,
            throws,
            throws.span,
            pkg_items,
            namespace_path,
            &generic_bounds,
            diagnostics,
        );
    }
}

fn validate_associated_type_bindings_in_method_sig(
    db: &dyn Db,
    file_id: FileId,
    method: &baml_compiler2_ast::MethodSigDef,
    outer_generic_bounds: &GenericBoundExprMap,
    pkg_items: &baml_compiler2_hir::package::PackageItems<'_>,
    namespace_path: &[Name],
    diagnostics: &mut Vec<Diagnostic>,
) {
    let generic_bounds = extend_generic_bound_expr_map(
        outer_generic_bounds,
        &method.generic_params,
        &method.generic_param_bounds,
    );
    for param in &method.params {
        if let Some(te) = &param.type_expr {
            validate_ambiguous_typevar_associated_projection_in_type_expr(
                db,
                file_id,
                te,
                te.span,
                pkg_items,
                namespace_path,
                &generic_bounds,
                diagnostics,
            );
        }
    }
    if let Some(ret) = &method.return_type {
        validate_ambiguous_typevar_associated_projection_in_type_expr(
            db,
            file_id,
            ret,
            ret.span,
            pkg_items,
            namespace_path,
            &generic_bounds,
            diagnostics,
        );
    }
    if let Some(throws) = &method.throws {
        validate_ambiguous_typevar_associated_projection_in_type_expr(
            db,
            file_id,
            throws,
            throws.span,
            pkg_items,
            namespace_path,
            &generic_bounds,
            diagnostics,
        );
    }
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
    use baml_compiler2_ast::TypeExprKind;

    match &expr.kind {
        TypeExprKind::Path {
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
        TypeExprKind::AssociatedTypeProjection {
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
        TypeExprKind::Optional { inner, .. } | TypeExprKind::List { inner, .. } => {
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
        TypeExprKind::Map { key, value, .. } => {
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
        TypeExprKind::Union { variants, .. } => {
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
        TypeExprKind::Function {
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
        let iface = baml_compiler2_ppir::item_data::interface_data(db, current.loc);
        if iface
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
        for &parent in &iface.requires {
            if let Some(parent) = resolve_interface_ref(
                db,
                &iface.type_refs,
                parent,
                current_pkg_items,
                &current_pkg.namespace_path,
            ) {
                stack.push(parent);
            }
        }
    }

    out
}

fn check_jinja_templates(
    db: &dyn Db,
    file_id: FileId,
    file: SourceFile,
    pkg_items: &baml_compiler2_hir::package::PackageItems<'_>,
    namespace_path: &[Name],
    source_text: &str,
) -> Vec<Diagnostic> {
    let base_types = build_jinja_types(db, pkg_items, namespace_path);
    let mut diagnostics = Vec::new();

    for &func_loc in baml_compiler2_ppir::item_data::file_functions(db, file) {
        diagnostics.extend(check_llm_prompt_template(
            db,
            file_id,
            func_loc,
            pkg_items,
            namespace_path,
            &base_types,
            source_text,
        ));
    }

    for &template_loc in baml_compiler2_ppir::item_data::file_template_strings(db, file) {
        let template = baml_compiler2_ppir::item_data::template_string_data(db, template_loc);
        let Some(body) = &template.body else {
            continue;
        };

        let mut types = base_types.clone();
        types.start_scope();
        for param in &template.params {
            let ty = param
                .type_ref
                .map(|id| {
                    jinja_type_from_type_ref(db, &template.type_refs, id, pkg_items, namespace_path)
                })
                .unwrap_or(sys_jinja_types::Type::Unknown);
            types.add_variable(param.name.as_str(), ty);
        }

        let range_hint =
            baml_compiler2_ppir::item_data::template_string_source_map(db, template_loc).span;
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
    func_loc: baml_compiler2_hir::loc::FunctionLoc<'_>,
    pkg_items: &baml_compiler2_hir::package::PackageItems<'_>,
    namespace_path: &[Name],
    base_types: &sys_jinja_types::PredefinedTypes,
    source_text: &str,
) -> Vec<Diagnostic> {
    let Some(prompt) = baml_compiler2_ppir::item_data::function_llm_prompt(db, func_loc) else {
        return Vec::new();
    };

    let func_data = baml_compiler2_ppir::item_data::function_data(db, func_loc);
    let mut types = base_types.clone();
    types.start_scope();
    for param in &func_data.params {
        let ty = param
            .type_ref
            .map(|id| {
                jinja_type_from_type_ref(db, &func_data.type_refs, id, pkg_items, namespace_path)
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
        let class_data = baml_compiler2_ppir::item_data::class_data(db, class_loc);
        let class_namespace =
            baml_compiler2_hir::file_package::file_package(db, file).namespace_path;
        let fields = class_data
            .fields
            .iter()
            .map(|field| {
                let ty = field
                    .type_ref
                    .map(|id| {
                        jinja_type_from_type_ref(
                            db,
                            &class_data.type_refs,
                            id,
                            pkg_items,
                            &class_namespace,
                        )
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
        let enum_data = baml_compiler2_ppir::item_data::enum_data(db, enum_loc);
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
        let alias_data = baml_compiler2_ppir::item_data::type_alias_data(db, alias_loc);
        if let Some(id) = alias_data.value {
            let alias_namespace =
                baml_compiler2_hir::file_package::file_package(db, file).namespace_path;
            types.add_alias(
                alias_data.name.as_str(),
                jinja_type_from_type_ref(
                    db,
                    &alias_data.type_refs,
                    id,
                    pkg_items,
                    &alias_namespace,
                ),
            );
        }
    }

    for def in ns_items.values.values() {
        let Definition::TemplateString(template_loc) = *def else {
            continue;
        };
        let file = template_loc.file(db);
        let template = baml_compiler2_ppir::item_data::template_string_data(db, template_loc);
        let template_namespace =
            baml_compiler2_hir::file_package::file_package(db, file).namespace_path;
        let args = template
            .params
            .iter()
            .map(|param| {
                let ty = param
                    .type_ref
                    .map(|id| {
                        jinja_type_from_type_ref(
                            db,
                            &template.type_refs,
                            id,
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

fn jinja_type_from_type_ref(
    db: &dyn Db,
    store: &baml_compiler2_hir::type_ref::TypeRefStore,
    id: baml_compiler2_hir::type_ref::TypeRefId,
    pkg_items: &baml_compiler2_hir::package::PackageItems<'_>,
    namespace_path: &[Name],
) -> sys_jinja_types::Type {
    jinja_type_from_type_ref_inner(
        db,
        store,
        id,
        pkg_items,
        namespace_path,
        &mut HashSet::new(),
    )
}

fn jinja_type_from_type_ref_inner(
    db: &dyn Db,
    store: &baml_compiler2_hir::type_ref::TypeRefStore,
    id: baml_compiler2_hir::type_ref::TypeRefId,
    pkg_items: &baml_compiler2_hir::package::PackageItems<'_>,
    namespace_path: &[Name],
    resolving_aliases: &mut HashSet<String>,
) -> sys_jinja_types::Type {
    use baml_compiler2_hir::{contributions::Definition, type_ref::TypeRefKind};
    use sys_jinja_types::Type;

    match &store[id].kind {
        TypeRefKind::Int => Type::Int,
        TypeRefKind::Bigint => Type::Bigint,
        TypeRefKind::Float => Type::Float,
        TypeRefKind::String => Type::String,
        TypeRefKind::Bool => Type::Bool,
        TypeRefKind::Null => Type::None,
        TypeRefKind::Media { kind } => match kind {
            baml_base::MediaKind::Image => Type::Image,
            baml_base::MediaKind::Audio => Type::Audio,
            _ => Type::Unknown,
        },
        TypeRefKind::Literal { value } => Type::Literal(value.clone()),
        TypeRefKind::Optional { inner } => Type::merge([
            Type::None,
            jinja_type_from_type_ref_inner(
                db,
                store,
                *inner,
                pkg_items,
                namespace_path,
                resolving_aliases,
            ),
        ]),
        TypeRefKind::List { inner } => Type::List(Box::new(jinja_type_from_type_ref_inner(
            db,
            store,
            *inner,
            pkg_items,
            namespace_path,
            resolving_aliases,
        ))),
        TypeRefKind::Map { value, .. } => Type::Map(
            Box::new(Type::String),
            Box::new(jinja_type_from_type_ref_inner(
                db,
                store,
                *value,
                pkg_items,
                namespace_path,
                resolving_aliases,
            )),
        ),
        TypeRefKind::Union { variants } => Type::merge(variants.iter().map(|&variant| {
            jinja_type_from_type_ref_inner(
                db,
                store,
                variant,
                pkg_items,
                namespace_path,
                resolving_aliases,
            )
        })),
        TypeRefKind::Path { segments, .. } if !segments.is_empty() => {
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
                    let alias = baml_compiler2_ppir::item_data::type_alias_data(db, alias_loc);
                    let alias_namespace =
                        baml_compiler2_hir::file_package::file_package(db, file).namespace_path;
                    let resolved = alias
                        .value
                        .map(|value_id| {
                            jinja_type_from_type_ref_inner(
                                db,
                                &alias.type_refs,
                                value_id,
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
        TypeRefKind::Function { .. } | TypeRefKind::AssociatedTypeProjection { .. } => {
            Type::Unknown
        }
        TypeRefKind::Uint8Array
        | TypeRefKind::Never
        | TypeRefKind::Void
        | TypeRefKind::BuiltinUnknown
        | TypeRefKind::Type
        | TypeRefKind::Rust
        | TypeRefKind::Error
        | TypeRefKind::Unknown
        | TypeRefKind::Infer
        | TypeRefKind::Path { .. } => Type::Unknown,
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
        TirTypeError::RuntimeIdArgumentTypeMismatch { got } => format!(
            "`$id` at a call site expects `boundary.LocalId`, got {}",
            ty(got)
        ),
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
        TirTypeError::UnionMemberNoCommonInterface { .. } => DiagnosticId::NoSuchField,
        TirTypeError::UnknownClassPatternField { .. } => DiagnosticId::NoSuchField,
        TirTypeError::UnresolvedName { .. } => DiagnosticId::UnknownVariable,
        TirTypeError::DeadCode { .. } => DiagnosticId::UnreachableCode,
        TirTypeError::VoidUsedAsValue => DiagnosticId::TypeMismatch,
        TirTypeError::VoidFunctionResultUsed => DiagnosticId::TypeMismatch,
        TirTypeError::SpawnWithNotATransformer { .. } => DiagnosticId::TypeMismatch,
        TirTypeError::NotCallable { .. } => DiagnosticId::NotCallable,
        TirTypeError::NotIterable { .. } => DiagnosticId::NotCallable,
        TirTypeError::NotIndexable { .. } => DiagnosticId::NotIndexable,
        TirTypeError::InvalidMapKeyType { .. } => DiagnosticId::InvalidMapKeyType,
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
        TirTypeError::RestSubPatternNotBinding => DiagnosticId::TypeMismatch,
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
        TirTypeError::InvalidInterfaceUpcastTarget { .. } => DiagnosticId::TypeMismatch,
        TirTypeError::InterfaceMemberRequiresReceiver { .. } => DiagnosticId::NoSuchField,
        TirTypeError::InvalidSelfCallThroughInterface { .. } => DiagnosticId::TypeMismatch,
        TirTypeError::DefaultOnRequiredMethod { .. } => DiagnosticId::DefaultOnRequiredMethod,
        TirTypeError::BareDefaultKeyword => DiagnosticId::BareDefaultKeyword,
        TirTypeError::TypeDoesNotImplementInterface { .. } => DiagnosticId::TypeMismatch,
        TirTypeError::BlanketBoundNotSatisfied { .. } => DiagnosticId::TypeMismatch,
        // `$id` special-form misuse: an invalid assignment/access shape, not
        // a name-resolution failure.
        TirTypeError::RuntimeIdCompoundAssignment
        | TirTypeError::RuntimeIdMemberAccess { .. }
        | TirTypeError::DuplicateRuntimeIdArgument
        | TirTypeError::RuntimeIdArgumentMustBeLast
        | TirTypeError::RuntimeIdArgumentTypeMismatch { .. } => DiagnosticId::TypeMismatch,
        TirTypeError::IntegerLiteralOutOfRange { .. } => DiagnosticId::IntegerLiteralOutOfRange,
        TirTypeError::GenericBoundNotInterface { .. } => DiagnosticId::GenericBoundNotInterface,
        // Builtin interfaces (BEP-062, E0153/E0154).
        TirTypeError::BuiltinInterfaceNotImplementable { .. } => {
            DiagnosticId::BuiltinInterfaceNotImplementable
        }
        TirTypeError::BuiltinInterfaceNotABound { .. } => DiagnosticId::BuiltinInterfaceNotABound,
        // A `_` placeholder in a non-inferable position.
        TirTypeError::CannotInferType => DiagnosticId::WildcardTypeNotAllowed,
        // Generic-parameter / associated-type declaration hygiene.
        TirTypeError::TypeParamShadowedImplParam { .. } => DiagnosticId::TypeMismatch,
        TirTypeError::DuplicateGenericParam { .. }
        | TirTypeError::DuplicateAssociatedType { .. }
        | TirTypeError::AssociatedTypeConflictsWithGenericParam { .. } => {
            DiagnosticId::DuplicateField
        }
        // Generic type-argument constraints.
        TirTypeError::BoundedTypeArgNotConcrete { .. }
        | TirTypeError::MissingAssociatedTypeBindings { .. }
        | TirTypeError::AmbiguousInterfacePatternBindings { .. } => DiagnosticId::TypeMismatch,
        // Interface impl conformance (BEP-044, E0113–E0139): tir2 owns these and
        // check.rs surfaces them via `check_interfaces`.
        TirTypeError::MissingInterfaceMethod { .. } => DiagnosticId::MissingInterfaceMethod,
        TirTypeError::GenericSysOpMethodInInterfaceImpl { .. } => {
            DiagnosticId::GenericSysOpMethodInInterfaceImpl
        }
        TirTypeError::OutOfBodyImplementsFieldInterface { .. } => {
            DiagnosticId::OutOfBodyImplementsFieldInterface
        }
        TirTypeError::UnknownInterfaceMember { .. } => DiagnosticId::UnknownInterfaceMember,
        TirTypeError::MissingInterfaceField { .. } => DiagnosticId::MissingInterfaceField,
        TirTypeError::InterfaceFieldTypeMismatch { .. } => DiagnosticId::InterfaceFieldTypeMismatch,
        TirTypeError::InterfaceMethodSignatureMismatch { .. }
        | TirTypeError::InterfaceMethodAddsGenericBound { .. } => {
            DiagnosticId::InterfaceMethodSignatureMismatch
        }
        TirTypeError::MissingRequiredInterface { .. } => DiagnosticId::MissingRequiredInterface,
        TirTypeError::ImplTargetNotInterface { .. } => DiagnosticId::NotAnInterface,
        TirTypeError::ImplTargetNotConcrete { .. } => DiagnosticId::ImplTargetNotConcrete,
        TirTypeError::UnconstrainedImplTypeParam { .. } => DiagnosticId::UnconstrainedImplTypeParam,
        TirTypeError::ImplViolatesOrphanRule { .. } => DiagnosticId::ImplViolatesOrphanRule,
        TirTypeError::UnknownInterfaceFieldLink { .. } => DiagnosticId::UnknownInterfaceFieldLink,
        TirTypeError::UnknownClassFieldInInterfaceLink { .. } => {
            DiagnosticId::UnknownClassFieldInInterfaceLink
        }
        TirTypeError::DuplicateInterfaceFieldLink { .. } => {
            DiagnosticId::DuplicateInterfaceFieldLink
        }
        TirTypeError::SelfInInterfaceField { .. } => DiagnosticId::SelfInInterfaceField,
        TirTypeError::InterfaceRequiresNonInterface { .. } => {
            DiagnosticId::InterfaceRequiresNonInterface
        }
        // The `requires` graph is the trait-model successor to `extends`; it shares E0118.
        TirTypeError::InterfaceRequiresCycle { .. } => DiagnosticId::InterfaceExtendsCycle,
        // Associated-binding hygiene and cyclic/throwless impl-header
        // well-formedness have no dedicated codes yet; surface as generic type
        // errors.
        TirTypeError::UnknownAssociatedTypeBinding { .. }
        | TirTypeError::DuplicateAssociatedTypeBinding { .. }
        | TirTypeError::MissingImplAssociatedTypeBinding { .. }
        | TirTypeError::AssociatedTypeBindingsOnImplementsTarget { .. }
        | TirTypeError::AssociatedTypeBindingViolatesBound { .. }
        | TirTypeError::AssociatedTypeDefaultViolatesBound { .. }
        | TirTypeError::CyclicImplHeader
        | TirTypeError::InterfaceMethodMissingThrows { .. } => DiagnosticId::TypeMismatch,
        TirTypeError::FunctionTypeMissingThrows => DiagnosticId::FunctionTypeMissingThrows,
    }
}

#[cfg(test)]
mod tests {
    use baml_compiler_diagnostics::Severity;
    use baml_compiler2_tir::infer_context::{DiagnosticSeverity, RenderedTirDiagnostic};
    use text_size::{TextRange, TextSize};

    use super::*;
    use crate::testing::{CursorTest, ProjectTest};

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

    #[test]
    fn check_file_renders_runtime_id_mismatch_type_in_file_namespace() {
        let test = CursorTest::with_filename(
            "billing/test.baml",
            r#"class WrongId {
  value int
}

function target(value: int) -> int {
  value
}

function main(value: WrongId) -> int {
  target(1, $id = value)
}
<[CURSOR]"#,
        );

        let diagnostics = check_file(&test.db, test.cursor.file);
        let diag = diagnostics
            .iter()
            .find(|diag| {
                diag.id == DiagnosticId::TypeMismatch
                    && diag.message.contains("expects `boundary.LocalId`")
            })
            .expect("runtime-id type mismatch diagnostic");

        assert_eq!(
            diag.message,
            "`$id` at a call site expects `boundary.LocalId`, got WrongId"
        );
    }

    #[test]
    fn parse_error_in_body_taints_scope_and_descendants() {
        // A braceless lambda is a syntax error; parser recovery leaves a
        // malformed lambda that would otherwise emit cascading type errors. The
        // function-body scope containing the parse error — and its descendant
        // lambda scope — must be tainted so those type errors are suppressed.
        let mut builder = ProjectTest::builder();
        builder.source(
            "test.baml",
            "function Braceless() -> int {\n  let f = (x: int) => x + 1;\n  f(2)\n}\n",
        );
        let test = builder.build();
        let file = test.files[0];

        let index = file_semantic_index(&test.db, file);
        let parse_errors = baml_compiler_parser::parse_errors(&test.db, file);
        assert!(
            !parse_errors.is_empty(),
            "braceless lambda should produce a parse error"
        );

        let tainted = parse_error_tainted_scopes(index, &parse_errors);
        assert!(
            !tainted.is_empty(),
            "the broken function body should be tainted"
        );

        // The lambda scope is a *descendant* of the function-body scope that
        // holds the parse error, so descendant tainting must reach it.
        let lambda_idx = index
            .scopes
            .iter()
            .position(|s| s.kind == ScopeKind::Lambda)
            .expect("a lambda scope should exist for the braceless lambda");
        assert!(
            tainted.contains(&lambda_idx),
            "descendant lambda scope (index {lambda_idx}) must be tainted, got {tainted:?}"
        );

        // Invariant the suppression relies on: every descendant of a tainted
        // scope is itself tainted.
        for &i in &tainted {
            let scope = &index.scopes[i];
            for d in scope.descendants.start.index()..scope.descendants.end.index() {
                assert!(
                    tainted.contains(&(d as usize)),
                    "descendant {d} of tainted scope {i} must also be tainted"
                );
            }
        }
    }

    #[test]
    fn structural_parse_error_does_not_taint_scopes() {
        // A parse error at a structural position (an invalid class name)
        // resolves to a structural scope, which must never be tainted — so a
        // type error in an unrelated executable body stays visible.
        let mut builder = ProjectTest::builder();
        builder.source(
            "test.baml",
            "class 123Bad {\n  field string\n}\n\nfunction Good() -> int {\n  42\n}\n",
        );
        let test = builder.build();
        let file = test.files[0];

        let index = file_semantic_index(&test.db, file);
        let parse_errors = baml_compiler_parser::parse_errors(&test.db, file);
        assert!(
            !parse_errors.is_empty(),
            "invalid class name should produce a parse error"
        );

        let tainted = parse_error_tainted_scopes(index, &parse_errors);
        assert!(
            tainted.is_empty(),
            "a structural parse error must not taint any scope, got {tainted:?}"
        );
    }
}
