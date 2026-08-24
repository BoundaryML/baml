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
    Diagnostic, DiagnosticId, DiagnosticIdentifierKind, DiagnosticPhase, DiagnosticText,
    ParseError, ToDiagnostic, runtime_type,
};
use baml_compiler2_hir::{file_semantic_index, scope::ScopeKind};
use baml_compiler2_hir_ty::diagnostics::TirTypeError;
use baml_type::Ty;
use text_size::TextRange;

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
    // Body-owner granularity (hir_ty infers whole bodies, lambdas in the
    // owner's arena): an owner is suppressed when ITS scope or any
    // descendant scope is parse-tainted - the same cascades the per-scope
    // walk suppressed.
    //
    {
        use baml_compiler2_hir::body::BodyOwnerId;
        let mut owners: Vec<BodyOwnerId> = Vec::new();
        for owner in baml_compiler2_ppir::file_body_owners(db, file) {
            owners.push(owner);
            if let BodyOwnerId::Function(function) = owner {
                owners.push(BodyOwnerId::ParameterDefaults(function));
            }
        }
        for owner in owners {
            // A scopeless owner (a builtin declaration's parameter
            // defaults) has no parse-taint surface; it still infers.
            let owner_tainted = match baml_compiler2_ppir::body_scope(db, owner) {
                Some(scope) => {
                    let idx = scope.file_scope_id(db).index() as usize;
                    idx < index.scopes.len() && {
                        let descendants = &index.scopes[idx].descendants;
                        tainted.contains(&idx)
                            || (descendants.start.index()..descendants.end.index())
                                .any(|i| tainted.contains(&(i as usize)))
                    }
                }
                None if matches!(owner, BodyOwnerId::ParameterDefaults(_)) => false,
                None => continue,
            };
            let result = baml_compiler2_hir_ty::infer::infer_body(db, owner);
            if result.diagnostics.is_empty() {
                continue;
            }
            let source_map = baml_compiler2_ppir::body_source_map(db, owner);
            let type_ref_spans = baml_compiler2_ppir::body_type_ref_spans(db, owner);
            for diagnostic in &result.diagnostics {
                let rendered = diagnostic.render_with_type_refs(
                    db,
                    file,
                    source_map.as_ref(),
                    type_ref_spans.as_ref(),
                );
                // Inside a parse-tainted scope's span, inference findings
                // are cascades of the syntax error and stay suppressed -
                // but an UNRESOLVED TYPE still surfaces (a broken lambda's
                // mis-parsed annotation names its unknown type rather than
                // vanishing with the whole body). Unresolved NAMES stay
                // suppressed with the rest: recovery routinely rereads
                // stray tokens as value paths, and reporting those reads
                // as missing names is noise, not signal.
                if owner_tainted
                    && !matches!(
                        rendered.error,
                        baml_compiler2_hir_ty::diagnostics::TirTypeError::UnresolvedType { .. }
                    )
                {
                    continue;
                }
                diagnostics.push(tir_rendered_to_diagnostic_for_file(db, file, rendered));
            }
        }
        // SIGNATURE-side diagnostics: unresolved references (E0002) and
        // non-interface bounds (E0145), re-lowered with the sink - plus the
        // declaration-structural parameter-default rules (required-after-
        // default ordering, `self` defaults, forward references).
        for &func_loc in baml_compiler2_ppir::item_data::file_functions(db, file) {
            for (range, error) in
                baml_compiler2_hir_ty::defaults::parameter_default_diagnostics(db, func_loc)
                    .into_iter()
                    .chain(
                        baml_compiler2_hir_ty::lower::signature_lowering_diagnostics(db, func_loc),
                    )
            {
                let rendered = baml_compiler2_hir_ty::diagnostics::RenderedTirDiagnostic {
                    message: error.to_string(),
                    error,
                    range,
                    severity: baml_compiler2_hir_ty::diagnostics::DiagnosticSeverity::Error,
                    related: Vec::new(),
                };
                diagnostics.push(tir_rendered_to_diagnostic_for_file(db, file, rendered));
            }
        }
        // TOP-LEVEL DECLARATION effect diagnostics: io reachable from a
        // `client`/`let` initializer, which `$init` cannot run (E0158).
        //
        // Parse-tainted declarations are skipped for the same reason inference
        // findings are above: recovery rebuilds a broken initializer into some
        // other expression, and reporting what THAT reaches is noise on top of
        // the syntax error the user actually needs to see.
        for &let_loc in baml_compiler2_ppir::item_data::file_lets(db, file) {
            let tainted_decl = baml_compiler2_ppir::body_scope(
                db,
                baml_compiler2_hir::body::BodyOwnerId::Let(let_loc),
            )
            .is_some_and(|scope| {
                let idx = scope.file_scope_id(db).index() as usize;
                idx < index.scopes.len() && {
                    let descendants = &index.scopes[idx].descendants;
                    tainted.contains(&idx)
                        || (descendants.start.index()..descendants.end.index())
                            .any(|i| tainted.contains(&(i as usize)))
                }
            });
            if tainted_decl {
                continue;
            }
            for (range, error) in
                baml_compiler2_hir_ty::init_io::let_init_io_diagnostics(db, let_loc)
            {
                let rendered = baml_compiler2_hir_ty::diagnostics::RenderedTirDiagnostic {
                    message: error.to_string(),
                    error,
                    range,
                    severity: baml_compiler2_hir_ty::diagnostics::DiagnosticSeverity::Error,
                    related: Vec::new(),
                };
                diagnostics.push(tir_rendered_to_diagnostic_for_file(db, file, rendered));
            }
        }
        // CLASS generic-bound diagnostics.
        for &class_loc in baml_compiler2_ppir::item_data::file_classes(db, file) {
            for (range, error) in
                baml_compiler2_hir_ty::lower::class_lowering_diagnostics(db, class_loc)
            {
                let rendered = baml_compiler2_hir_ty::diagnostics::RenderedTirDiagnostic {
                    message: error.to_string(),
                    error,
                    range,
                    severity: baml_compiler2_hir_ty::diagnostics::DiagnosticSeverity::Error,
                    related: Vec::new(),
                };
                diagnostics.push(tir_rendered_to_diagnostic_for_file(db, file, rendered));
            }
        }
        // INTERFACE requires-clause diagnostics.
        for &iface_loc in baml_compiler2_ppir::item_data::file_interfaces(db, file) {
            for (range, error) in
                baml_compiler2_hir_ty::lower::interface_lowering_diagnostics(db, iface_loc)
            {
                let rendered = baml_compiler2_hir_ty::diagnostics::RenderedTirDiagnostic {
                    message: error.to_string(),
                    error,
                    range,
                    severity: baml_compiler2_hir_ty::diagnostics::DiagnosticSeverity::Error,
                    related: Vec::new(),
                };
                diagnostics.push(tir_rendered_to_diagnostic_for_file(db, file, rendered));
            }
        }
    }

    // ── 4. Declaration structural diagnostics ────────────────────────────────
    //
    // Type-alias bodies re-lowered with the sink (class field annotations
    // ride the layer-3 class walk).
    for (_name, contrib) in &index.symbol_contributions.types {
        use baml_compiler2_hir::contributions::Definition;
        let Definition::TypeAlias(alias_loc) = contrib.definition else {
            continue;
        };
        for (range, error) in
            baml_compiler2_hir_ty::lower::type_alias_lowering_diagnostics(db, alias_loc)
        {
            let rendered = baml_compiler2_hir_ty::diagnostics::RenderedTirDiagnostic {
                message: error.to_string(),
                error,
                range,
                severity: baml_compiler2_hir_ty::diagnostics::DiagnosticSeverity::Error,
                related: Vec::new(),
            };
            diagnostics.push(tir_rendered_to_diagnostic_for_file(db, file, rendered));
        }
    }

    let pkg_info = baml_compiler2_hir::file_package::file_package(db, file);
    let pkg_id = baml_compiler2_hir::package::PackageId::new(db, pkg_info.package.clone());
    let res_ctx = baml_compiler2_hir_ty::package_interface::package_resolution_context(db, pkg_id);
    let pkg_items = &res_ctx.own_items;
    // Salsa-cached per package — previously rebuilt (and cloned per function
    // below) on every file check.
    // Reuse the memoized CST → AST lowering instead of re-lowering here.
    let ast_items = &baml_compiler2_hir::file_ast(db, file).items;
    diagnostics.extend(validate_associated_type_bindings_in_items(
        db,
        file_id,
        ast_items,
        pkg_items,
        &pkg_info.namespace_path,
    ));

    // Backtick templates are parsed and type-checked by the compiler.

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
            | ParseError::InvalidSyntax { span, .. }
            | ParseError::RemovedFeature { span, .. } => span,
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
    use baml_compiler2_hir_ty::interfaces::ImplDataError;

    let mut diagnostics = Vec::new();

    // ── 1. Coherence (E0132) ─────────────────────────────────────────────────
    //
    // Overlap is a per-package property over the whole dependency closure, not a
    // per-file one. Compute it once for the package and surface the violations
    // whose offending impl lives in this file (its conflicting partner may be in
    // another file or a dependency).
    let package = baml_compiler2_hir::file_package::file_package(db, file).package;
    let pkg_id = baml_compiler2_hir::package::PackageId::new(db, package);
    for violation in baml_compiler2_hir_ty::interfaces::package_coherence_diagnostics(db, pkg_id) {
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
    // Mounted impls have no source span and therefore cannot enter the legacy
    // loc-paired coherence report above.  Compare each source impl against the
    // exported rows with hir_ty's shared overlap engine, anchoring only the
    // editable source side and retaining a structural description of its
    // mounted partner in the message.
    for &impl_loc in baml_compiler2_ppir::item_data::file_impls(db, file) {
        let Some(source) = baml_compiler2_hir_ty::impls::impl_facts(db, impl_loc) else {
            continue;
        };
        for mounted_package in baml_compiler2_hir::package::mounted_package_names(db) {
            let Some(interface) =
                baml_compiler2_hir_ty::package_interface::mounted_interface(db, &mounted_package)
            else {
                continue;
            };
            for mounted in &interface.impls {
                let overlap = baml_compiler2_hir_ty::coherence::source_mounted_impl_conflict(
                    db, pkg_id, source, mounted,
                );
                if overlap == baml_compiler2_hir_ty::coherence::Overlap::No {
                    continue;
                }
                let partner = format!(
                    "implement {} for {}",
                    mounted.interface.name, mounted.for_ty_pattern
                );
                let message = if overlap == baml_compiler2_hir_ty::coherence::Overlap::Unknown {
                    format!(
                        "these interface implementations are too complex to prove disjoint; \
simplify the types involved so coherence can be decided (conflicts with the mounted \
dependency's `{partner}`)"
                    )
                } else {
                    format!(
                        "overlapping interface implementations for the same receiver/interface \
(conflicts with the mounted dependency's `{partner}`)"
                    )
                };
                let range =
                    baml_compiler2_ppir::item_data::impl_block_source_map(db, impl_loc).span;
                diagnostics.push(
                    Diagnostic::error(DiagnosticId::OverlappingImplements, message)
                        .with_primary_span(Span::new(file_id, range))
                        .with_phase(DiagnosticPhase::Type),
                );
            }
        }
    }

    // ── 2 + 3. Per-impl structural + signature/conformance diagnostics ────────
    //
    // `impl_data(loc).diagnostics` (name/membership) and
    // `validate_impl_signatures(loc)` (type conformance) each yield
    // `(TirTypeError, ImplDiagnosticLocation)` pairs anchored via the same source
    // map; a `Method` / field-link / binding location may mark several sites.
    for &impl_loc in baml_compiler2_ppir::item_data::file_impls(db, file) {
        let sm = baml_compiler2_hir_ty::interfaces::impl_data_source_map(db, impl_loc);
        // The loc-based declaration validator cannot open a source-less
        // interface declaration. Replay its name-level conformance from the
        // exported row so mounted and source dependency modes retain the same
        // required/default/override surface.
        if let Some(source) = baml_compiler2_hir_ty::impls::impl_facts(db, impl_loc)
            && let Some(baml_compiler2_hir_ty::package_interface::ExportedType::Interface {
                fields,
                required_methods,
                default_methods,
                ..
            }) = baml_compiler2_hir_ty::package_interface::mounted_type_row(
                db,
                &source.interface.name,
            )
        {
            let block = baml_compiler2_ppir::item_data::impl_block_data(db, impl_loc);
            let override_names: Vec<Name> = block
                .methods
                .iter()
                .map(|method| {
                    baml_compiler2_ppir::item_data::function_data(db, *method)
                        .name
                        .clone()
                })
                .collect();
            let default_names: Vec<&Name> =
                default_methods.iter().map(|method| &method.name).collect();
            let mut mounted_structural = Vec::new();
            for required in required_methods {
                if !override_names.contains(&required.name)
                    && !default_names.iter().any(|name| **name == required.name)
                {
                    mounted_structural.push((
                        TirTypeError::MissingInterfaceMethod {
                            interface: source.interface.name.clone(),
                            method: required.name.clone(),
                        },
                        baml_compiler2_hir_ty::interfaces::ImplDiagnosticLocation::InterfaceTarget,
                    ));
                }
            }
            for (index, name) in override_names.iter().enumerate() {
                if override_names[..index].contains(name) {
                    continue;
                }
                let known = required_methods.iter().any(|method| method.name == *name)
                    || default_names.iter().any(|default| **default == *name);
                if !known {
                    mounted_structural.push((
                        TirTypeError::UnknownInterfaceMember {
                            interface: source.interface.name.clone(),
                            member: name.clone(),
                        },
                        baml_compiler2_hir_ty::interfaces::ImplDiagnosticLocation::Method(
                            name.clone(),
                        ),
                    ));
                }
            }
            let out_of_body = match &block.subject {
                baml_compiler2_ppir::item_data::ImplSubjectData::InClass {
                    out_of_body, ..
                } => *out_of_body,
                baml_compiler2_ppir::item_data::ImplSubjectData::Free { .. } => true,
            };
            if out_of_body && !fields.is_empty() {
                mounted_structural.push((
                    TirTypeError::OutOfBodyImplementsFieldInterface {
                        interface: source.interface.name.clone(),
                    },
                    baml_compiler2_hir_ty::interfaces::ImplDiagnosticLocation::InterfaceTarget,
                ));
            }
            for (error, loc) in &mounted_structural {
                for span in impl_diagnostic_spans(loc, sm) {
                    diagnostics.push(
                        Diagnostic::error(
                            tir_type_error_to_diagnostic_id(error),
                            error.to_string(),
                        )
                        .with_primary_span(span)
                        .with_phase(DiagnosticPhase::Type),
                    );
                }
            }
        }
        // `impl_data` owns an impl's structural diagnostics whether or not it
        // fully resolves: an unresolved interface target still carries the
        // diagnostics it lowered (the bad target, the for-target, the bounds). A
        // cyclic header carries none — `validate_impl_signatures` re-detects and
        // surfaces `CyclicImplHeader` for it.
        let structural = match baml_compiler2_hir_ty::interfaces::impl_data(db, impl_loc).as_ref() {
            Ok(data) => Some(&data.diagnostics),
            Err(ImplDataError::InterfaceUnresolved { diagnostics }) => Some(diagnostics),
            Err(ImplDataError::CyclicHeader | ImplDataError::Malformed) => None,
        };
        let signatures = baml_compiler2_hir_ty::interfaces::validate_impl_signatures(db, impl_loc);
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
    loc: &baml_compiler2_hir_ty::interfaces::ImplDiagnosticLocation,
    sm: &baml_compiler2_hir_ty::interfaces::ImplDataSourceMap,
) -> Vec<Span> {
    use baml_compiler2_hir_ty::interfaces::ImplDiagnosticLocation;

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
/// The interface an `extends` bound names, or `None` when it does not resolve to
/// one (diagnosed elsewhere). Only the `loc` is needed: rendering a declarer's
/// name goes through the compiler's `interface_loc_qtn`, so this does not carry
/// a second copy of the identity.
fn resolve_interface_path<'db>(
    db: &'db dyn Db,
    target: &baml_compiler2_ast::TypeExpr,
    pkg_items: &'db baml_compiler2_hir::package::PackageItems<'db>,
    namespace_path: &[Name],
) -> Option<baml_compiler2_hir::loc::InterfaceLoc<'db>> {
    baml_compiler2_hir_ty::interfaces::resolve_path_to_interface_identity(
        db,
        target,
        pkg_items,
        namespace_path,
    )
    .map(|resolved| resolved.loc)
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
                let outer_bounds = generic_bound_expr_map(&class.generic_params);
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
                let iface_bounds = generic_bound_expr_map(&iface.generic_params);
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
                let impl_bounds = generic_bound_expr_map(&imp.generic_params);
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
            .any(|param| param.name == assoc.name)
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

/// Each in-scope type variable's declared bound *conjunction* — `T extends A & B`
/// maps `T` to `[A, B]`, so a `T.member` projection is resolved against all of
/// them (and reports ambiguity when more than one supplies `member`).
type GenericBoundExprMap = std::collections::HashMap<Name, Vec<baml_compiler2_ast::TypeExpr>>;

fn generic_bound_expr_map(params: &[baml_compiler2_ast::GenericParam]) -> GenericBoundExprMap {
    params
        .iter()
        .filter(|param| !param.bounds.is_empty())
        .map(|param| (param.name.clone(), param.bounds.clone()))
        .collect()
}

fn extend_generic_bound_expr_map(
    outer: &GenericBoundExprMap,
    params: &[baml_compiler2_ast::GenericParam],
) -> GenericBoundExprMap {
    let mut merged = outer.clone();
    merged.extend(generic_bound_expr_map(params));
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
    let generic_bounds =
        extend_generic_bound_expr_map(outer_generic_bounds, &function.generic_params);
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
    let generic_bounds =
        extend_generic_bound_expr_map(outer_generic_bounds, &method.generic_params);
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
    generic_bounds: &GenericBoundExprMap,
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
                if let Some(bounds) = generic_bounds.get(base) {
                    // The legacy structural renderer below is loc-based. A
                    // mounted bound has no InterfaceLoc; hir_ty's ordinary
                    // projection lowering owns its loc-free requires walk and
                    // diagnostics, so do not manufacture an "unknown" here.
                    let has_mounted_bound = bounds.iter().any(|bound| {
                        let TypeExprKind::Path { segments, .. } = &bound.kind else {
                            return false;
                        };
                        segments.first().is_some_and(|package| {
                            baml_compiler2_hir_ty::package_interface::mounted_interface(db, package)
                                .is_some()
                        })
                    });
                    if has_mounted_bound {
                        return;
                    }
                    // `T extends A & B` — the member may come from any conjunct,
                    // so the declarers are collected across the whole conjunction:
                    // none means unknown, two or more is ambiguous. The compiler
                    // owns that walk (and its cross-conjunct deduplication, without
                    // which two bounds reaching the same declarer through `requires`
                    // would look like an ambiguity); this only renders the result.
                    let roots = bounds.iter().filter_map(|bound| {
                        resolve_interface_path(db, bound, pkg_items, namespace_path)
                    });
                    let sources: Vec<String> =
                        baml_compiler2_hir_ty::interfaces::interfaces_declaring_associated_type(
                            db, roots, member,
                        )
                        .into_iter()
                        .filter_map(|loc| {
                            baml_compiler2_hir_ty::interfaces::interface_loc_qtn(db, loc)
                                .map(|qtn| qtn.render_user_facing())
                        })
                        .collect();
                    if sources.is_empty() {
                        let rendered = bounds
                            .iter()
                            .map(ToString::to_string)
                            .collect::<Vec<_>>()
                            .join(" & ");
                        diagnostics.push(
                            Diagnostic::error(
                                DiagnosticId::UnknownType,
                                format!(
                                    "unknown associated type `{member}` for bound `{rendered}`"
                                ),
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

/// Convert a `RenderedTirDiagnostic` to the shared `Diagnostic` type.
///
/// `RenderedTirDiagnostic` has already resolved arena IDs to `TextRange`.
/// We add the `file_id` to form a full `Span` for the primary annotation.
///
#[cfg(test)]
fn tir_rendered_to_diagnostic(
    rendered: baml_compiler2_hir_ty::diagnostics::RenderedTirDiagnostic,
    file_id: FileId,
) -> Diagnostic {
    let message = DiagnosticText::from(rendered.message.clone());
    tir_rendered_to_diagnostic_with_message(rendered, file_id, message)
}

fn tir_rendered_to_diagnostic_with_message(
    rendered: baml_compiler2_hir_ty::diagnostics::RenderedTirDiagnostic,
    file_id: FileId,
    message: DiagnosticText,
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
    let warning = matches!(
        rendered.severity,
        baml_compiler2_hir_ty::diagnostics::DiagnosticSeverity::Warning
    );
    let mut diag = new_tir_diagnostic(&rendered.error, message, span, warning);
    let diag = if let Some(member) = &unknown_member_access_member {
        diag.annotations.clear();
        diag.with_primary(
            span,
            format!("use `match` to narrow this value before accessing `{member}`"),
        )
    } else {
        diag
    };
    rendered.related.into_iter().fold(diag, |diag, related| {
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
}

fn tir_rendered_to_diagnostic_for_file(
    db: &dyn Db,
    file: SourceFile,
    rendered: baml_compiler2_hir_ty::diagnostics::RenderedTirDiagnostic,
) -> Diagnostic {
    let message = rich_source_aware_tir_type_error_message(db, file, &rendered.error);
    tir_rendered_to_diagnostic_with_message(rendered, file.file_id(db), message)
}

fn new_tir_diagnostic(
    error: &TirTypeError,
    message: DiagnosticText,
    span: Span,
    warning: bool,
) -> Diagnostic {
    if let TirTypeError::ComputedGenericArgumentRequiresUnreflect { name } = error {
        return runtime_type::computed_generic_argument_requires_unreflect(name.as_str())
            .with_primary_span(span)
            .with_phase(DiagnosticPhase::Type);
    }
    if let TirTypeError::RuntimeTypeMustBeNamed { escape } = error {
        // The headline says what is wrong; the label at the `unreflect(...)`
        // slot says why the inline spelling cannot reach past this call —
        // naming whichever published type the runtime parameter reached, the
        // value or the error. The rewrite rides along as related info, built
        // from the file text in `render_with_type_refs`.
        return runtime_type::runtime_type_must_be_named()
            .with_primary(span, escape.note())
            .with_phase(DiagnosticPhase::Type);
    }
    if let TirTypeError::CannotConstructReflectionKind { class_name } = error {
        return runtime_type::cannot_construct_reflection_kind(&class_name.render_user_facing())
            .with_primary_span(span)
            .with_phase(DiagnosticPhase::Type);
    }
    if let TirTypeError::CannotConstructBuiltinCompanion {
        class_name,
        companion,
    } = error
    {
        return runtime_type::cannot_construct_builtin_companion(
            &class_name.render_user_facing(),
            companion.builtin,
            companion.origin,
            companion.carries_methods,
        )
        .with_primary_span(span)
        .with_phase(DiagnosticPhase::Type);
    }
    if matches!(error, TirTypeError::TypeMismatch { .. }) {
        let base = runtime_type::mismatched_types();
        let diagnostic = if warning {
            Diagnostic::warning(base.id, base.message)
        } else {
            base
        };
        return diagnostic
            .with_primary(span, message)
            .with_phase(DiagnosticPhase::Type);
    }
    let id = tir_type_error_to_diagnostic_id(error);
    let headline = match error {
        TirTypeError::MissingReturn { .. } => Some("missing return expression"),
        _ => None,
    };
    let diagnostic = match headline {
        Some(headline) => {
            let diagnostic = if warning {
                Diagnostic::warning(id, headline)
            } else {
                Diagnostic::error(id, headline)
            };
            diagnostic.with_primary(span, message)
        }
        None => {
            let diagnostic = if warning {
                Diagnostic::warning(id, message)
            } else {
                Diagnostic::error(id, message)
            };
            diagnostic.with_primary_span(span)
        }
    };
    diagnostic.with_phase(DiagnosticPhase::Type)
}

fn rich_source_aware_tir_type_error_message(
    db: &dyn Db,
    file: SourceFile,
    error: &TirTypeError,
) -> DiagnosticText {
    let ty = |ty: &Ty| crate::utils::display_ty_for_file(db, file, ty);
    match error {
        TirTypeError::TypeMismatch { expected, got } => DiagnosticText::new()
            .text("expected ")
            .type_expr(ty(expected))
            .text(", found ")
            .type_expr(ty(got)),
        TirTypeError::UnresolvedName { name, .. } => DiagnosticText::new()
            .text("unresolved name: ")
            .identifier(name, DiagnosticIdentifierKind::Variable),
        TirTypeError::UnresolvedMember {
            base_type, member, ..
        } => DiagnosticText::new()
            .text("type ")
            .type_expr(ty(base_type))
            .text(" has no member ")
            .identifier(member, DiagnosticIdentifierKind::Field),
        TirTypeError::NotCallable { ty: callee_ty } => DiagnosticText::new()
            .type_expr(ty(callee_ty))
            .text(" is not a function and cannot be called"),
        TirTypeError::NotIterable { ty: iter_ty } => DiagnosticText::new()
            .text("cannot iterate over type ")
            .type_expr(ty(iter_ty)),
        TirTypeError::NotIndexable { ty: index_ty } => DiagnosticText::new()
            .text("cannot index into type ")
            .type_expr(ty(index_ty)),
        TirTypeError::MissingReturn { expected } => DiagnosticText::new()
            .text("expected return value of type ")
            .type_expr(ty(expected)),
        _ => DiagnosticText::from(source_aware_tir_type_error_message(db, file, error)),
    }
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
        // Both spellings that write an interface qualifier report here:
        // `x.as<T>` and `(Base as T).item`.
        TirTypeError::InvalidInterfaceUpcastTarget { target } => {
            format!("expected an interface qualifier, got {}", ty(target))
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
    error: &baml_compiler2_hir_ty::diagnostics::TirTypeError,
) -> DiagnosticId {
    use baml_compiler2_hir_ty::diagnostics::TirTypeError;
    match error {
        TirTypeError::TypeMismatch { .. } => DiagnosticId::TypeMismatch,
        TirTypeError::UnresolvedMember { .. } => DiagnosticId::NoSuchField,
        TirTypeError::UnionMemberNoCommonInterface { .. } => DiagnosticId::NoSuchField,
        TirTypeError::UnknownClassField { .. } | TirTypeError::UnknownClassPatternField { .. } => {
            DiagnosticId::NoSuchField
        }
        TirTypeError::UnknownClassPropertyShorthand { .. } => DiagnosticId::NoSuchField,
        TirTypeError::UnresolvedName { .. } | TirTypeError::UnresolvedPropertyShorthand { .. } => {
            DiagnosticId::UnknownVariable
        }
        TirTypeError::ComputedGenericArgumentRequiresUnreflect { name } => {
            runtime_type::computed_generic_argument_requires_unreflect(name.as_str()).id
        }
        TirTypeError::RuntimeTypeMustBeNamed { .. } => {
            runtime_type::runtime_type_must_be_named().id
        }
        TirTypeError::MountedPackageCallUnsupported { path } => {
            runtime_type::mounted_package_call_unsupported(path.as_str()).id
        }
        TirTypeError::CannotConstructReflectionKind { .. } => DiagnosticId::TypeMismatch,
        TirTypeError::CannotConstructBuiltinCompanion { .. } => {
            DiagnosticId::CannotConstructBuiltinCompanion
        }
        TirTypeError::DeadCode { .. } => DiagnosticId::UnreachableCode,
        TirTypeError::ConditionAlwaysConstant { .. } => DiagnosticId::ConditionAlwaysConstant,
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
        TirTypeError::RemovedReflectSpelling { .. } => DiagnosticId::RemovedFeature,
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
        TirTypeError::RuntimeTypeArgumentOnStreamingCall { .. } => DiagnosticId::InvalidSyntax,
        TirTypeError::RuntimeTypeArgumentOnIndirectCall => {
            runtime_type::runtime_type_argument_on_indirect_call().id
        }
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
        TirTypeError::SelflessMethodNeedsConcreteSelf { .. } => DiagnosticId::TypeMismatch,
        TirTypeError::SelflessInstanceMember { .. } => DiagnosticId::TypeMismatch,
        TirTypeError::ErasedSelfMethodValue { .. } => DiagnosticId::TypeMismatch,
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
        TirTypeError::InterfaceProjectionBase { .. } => DiagnosticId::InterfaceProjectionBase,
        // `$init` effect rules.
        TirTypeError::InitIoNotAllowed { .. } => DiagnosticId::InitIoNotAllowed,
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
        TirTypeError::SelfInAssociatedTypeDefault { .. } => {
            DiagnosticId::SelfInAssociatedTypeDefault
        }
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
    use baml_compiler2_hir_ty::diagnostics::{DiagnosticSeverity, RenderedTirDiagnostic};
    use text_size::{TextRange, TextSize};

    use super::*;
    use crate::testing::{CursorTest, ProjectTest};

    fn dummy_file_id() -> FileId {
        // Use index 0 — sufficient for span construction in unit tests.
        FileId::new(0)
    }

    fn dummy_rendered(severity: DiagnosticSeverity) -> RenderedTirDiagnostic {
        RenderedTirDiagnostic {
            error: baml_compiler2_hir_ty::diagnostics::TirTypeError::TypeMismatch {
                expected: baml_type::Ty::Never {
                    attr: baml_type::TyAttr::default(),
                },
                got: baml_type::Ty::Never {
                    attr: baml_type::TyAttr::default(),
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

        let diagnostics = check_file(&test.db, test.cursor.file);
        let messages = diagnostics
            .iter()
            .map(|diag| diag.message.as_str())
            .collect::<Vec<_>>();

        assert!(
            messages.contains(&"default for parameter `a` cannot reference later parameter `b`"),
            "missing forward-reference diagnostic; got {messages:#?}"
        );
        assert!(
            diagnostics.iter().any(|diagnostic| {
                diagnostic.message == "mismatched types"
                    && diagnostic.annotations.iter().any(|annotation| {
                        annotation.message.as_deref() == Some("expected `string`, found `2`")
                    })
            }),
            "missing default type-mismatch diagnostic; got {diagnostics:#?}"
        );
        assert!(
            messages.contains(
                &"required parameter `required` cannot appear after a defaulted parameter"
            ),
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
        // A missing right-hand operand is a syntax error in the function body.
        // The function-body scope containing the parse error — and its
        // descendant lambda scope — must be tainted so cascading type errors
        // are suppressed.
        let mut builder = ProjectTest::builder();
        builder.source(
            "test.baml",
            "function Broken() -> int {\n  let broken = 1 + ;\n  let f = (x: int) -> { x + 1 };\n  f(2)\n}\n",
        );
        let test = builder.build();
        let file = test.files[0];

        let index = file_semantic_index(&test.db, file);
        let parse_errors = baml_compiler_parser::parse_errors(&test.db, file);
        assert!(
            !parse_errors.is_empty(),
            "missing right-hand operand should produce a parse error"
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
            .expect("a descendant lambda scope should exist");
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
