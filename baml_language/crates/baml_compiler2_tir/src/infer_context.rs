//! Diagnostic sink for a single scope inference run.
//!
//! `InferContext` is held inside `TypeInferenceBuilder` and accumulates
//! type errors discovered during expression walking. Consuming `finish()`
//! returns the accumulated `TypeCheckDiagnostics`.
//!
//! Diagnostics are Salsa-stable (no `TextRange`) — locations are stored as
//! arena IDs. The LSP layer maps them to source ranges at display time.

use std::cell::RefCell;

use baml_compiler2_ast::{AstSourceMap, ExprId, StmtId};
use baml_compiler2_hir::scope::ScopeId;
use text_size::TextRange;

pub use baml_compiler2_hir_ty::diagnostics::{
    AssocContainer, DiagnosticLocation, DiagnosticSeverity, RelatedLocation, RelatedNote,
    RenderedRelatedInformation, RenderedTirDiagnostic, SelfCallPosition, ShadowedParamOwner,
    TirDiagnostic, TirTypeError, TypeCheckDiagnostics,
};

// ── InferContext ─────────────────────────────────────────────────────────────

/// Diagnostic sink for a single scope inference run.
///
/// Held inside `TypeInferenceBuilder` — one per `infer_scope_types` call.
/// Modeled after Ty's `InferContext` (`context.rs:37-46`).
pub struct InferContext<'db> {
    db: &'db dyn crate::Db,
    scope: ScopeId<'db>,
    diagnostics: RefCell<TypeCheckDiagnostics<'db>>,
    /// When `true`, suppress diagnostics that arise from synthesized
    /// references to user types/names/members. Set while inferring an
    /// auto-derived function body (synthesized `to_json` / `from_json`):
    /// those bodies reference user fields by name, so when a class has a
    /// malformed field, the synthesizer's `self.<f>.to_json()` and
    /// `baml.json.from_json<F>(...)` calls surface duplicate
    /// `UnresolvedType` / `UnresolvedMember` / `NotCallable` errors whose
    /// spans point back at the user's class — confusing because the user
    /// didn't write that code. The user's underlying field declaration
    /// already reports the real error.
    suppress_member_lookup_errors: std::cell::Cell<bool>,
}

/// Returns `true` for diagnostic kinds that may arise spuriously from
/// auto-derived function bodies (synthesized code referencing user types).
/// We suppress these inside auto-derive bodies; the user's underlying type
/// declaration already reports the same condition without the synthesized
/// span confusion.
fn is_synthesized_code_diag(error: &TirTypeError) -> bool {
    matches!(
        error,
        TirTypeError::UnresolvedMember { .. }
            | TirTypeError::UnresolvedType { .. }
            | TirTypeError::UnresolvedName { .. }
            | TirTypeError::NotCallable { .. }
    )
}

impl<'db> InferContext<'db> {
    pub fn new(db: &'db dyn crate::Db, scope: ScopeId<'db>) -> Self {
        Self {
            db,
            scope,
            diagnostics: RefCell::new(TypeCheckDiagnostics::default()),
            suppress_member_lookup_errors: std::cell::Cell::new(false),
        }
    }

    /// Toggle suppression of `UnresolvedMember` diagnostics for the
    /// current inference run. See `suppress_member_lookup_errors`.
    pub fn set_suppress_member_lookup_errors(&self, value: bool) {
        self.suppress_member_lookup_errors.set(value);
    }

    pub fn db(&self) -> &'db dyn crate::Db {
        self.db
    }

    /// Number of diagnostics recorded so far. Paired with
    /// [`truncate_diagnostics`](Self::truncate_diagnostics) to run a
    /// speculative resolution (e.g. probing one member of a union) and roll
    /// back any diagnostics it emitted.
    pub fn diagnostic_count(&self) -> usize {
        self.diagnostics.borrow().diagnostics.len()
    }

    /// Drop every diagnostic recorded after index `n` (see
    /// [`diagnostic_count`](Self::diagnostic_count)).
    pub fn truncate_diagnostics(&self, n: usize) {
        self.diagnostics.borrow_mut().diagnostics.truncate(n);
    }

    /// Freeze the source spans of diagnostics recorded at index `[start..]`,
    /// resolving their arena-relative locations against `source_map` and
    /// replacing them with absolute [`DiagnosticLocation::Span`]s.
    ///
    /// Used when a nested lambda body is inferred *inline* in an enclosing
    /// scope (`infer_lambda_body`): those diagnostics carry the lambda's own
    /// arena IDs but are recorded in the enclosing scope's diagnostic set, so
    /// at render time they'd be resolved against the *enclosing* scope's source
    /// map — which can't resolve a nested-arena ID, collapsing the span to
    /// `0..0`. Resolving them here, while the lambda's source map is in hand,
    /// makes them render correctly regardless of which scope renders them.
    /// Already-frozen (`Span`) locations and deeper-lambda diagnostics (frozen
    /// by their own `infer_lambda_body`) are left unchanged.
    pub fn freeze_diagnostic_spans_from(&self, start: usize, source_map: &AstSourceMap) {
        let mut diags = self.diagnostics.borrow_mut();
        let len = diags.diagnostics.len();
        for d in &mut diags.diagnostics[start.min(len)..] {
            d.primary = Self::freeze_location(&d.primary, source_map);
        }
    }

    fn freeze_location(loc: &DiagnosticLocation, sm: &AstSourceMap) -> DiagnosticLocation {
        let span = match loc {
            DiagnosticLocation::Expr(id) => sm.expr_span(*id),
            DiagnosticLocation::ExprMember(id) => sm.member_access_member_span(*id),
            DiagnosticLocation::ExprSegment(id, seg) => sm.path_segment_span(*id, *seg),
            DiagnosticLocation::Stmt(id) => sm.stmt_span(*id),
            DiagnosticLocation::TypeAnnot(id) => sm.type_annotation_span(*id),
            DiagnosticLocation::Pat(id) => sm.pattern_span(*id),
            // TIR never emits TypeRef anchors (hir_ty's channel).
            DiagnosticLocation::TypeRef(_) => return loc.clone(),
            // Already absolute (e.g. a deeper lambda's frozen diagnostic, or a
            // class-field span) — leave it.
            DiagnosticLocation::Span(r) => *r,
        };
        DiagnosticLocation::Span(span)
    }

    pub fn scope(&self) -> ScopeId<'db> {
        self.scope
    }

    /// Report a type error at a specific expression, with optional related locations.
    pub fn report(&self, error: TirTypeError, at: ExprId, related: Vec<RelatedNote<'db>>) {
        if self.suppress_member_lookup_errors.get() && is_synthesized_code_diag(&error) {
            return;
        }
        self.diagnostics
            .borrow_mut()
            .diagnostics
            .push(TirDiagnostic {
                error,
                severity: DiagnosticSeverity::Error,
                primary: DiagnosticLocation::Expr(at),
                related,
            });
    }

    /// Convenience: report an error with no related locations.
    pub fn report_simple(&self, error: TirTypeError, at: ExprId) {
        self.report(error, at, Vec::new());
    }

    /// Report a type error at the member-name portion of a `MemberAccess` expression.
    pub fn report_at_member(
        &self,
        error: TirTypeError,
        at: ExprId,
        related: Vec<RelatedNote<'db>>,
    ) {
        if self.suppress_member_lookup_errors.get() && is_synthesized_code_diag(&error) {
            return;
        }
        self.diagnostics
            .borrow_mut()
            .diagnostics
            .push(TirDiagnostic {
                error,
                severity: DiagnosticSeverity::Error,
                primary: DiagnosticLocation::ExprMember(at),
                related,
            });
    }

    /// Convenience: report at member with no related locations.
    pub fn report_at_member_simple(&self, error: TirTypeError, at: ExprId) {
        self.report_at_member(error, at, Vec::new());
    }

    /// Report a type error at a specific segment of a multi-segment `Path` expression.
    /// `segment_idx` is the index into `path_segment_spans[at]`.
    pub fn report_at_segment(
        &self,
        error: TirTypeError,
        at: ExprId,
        segment_idx: usize,
        related: Vec<RelatedNote<'db>>,
    ) {
        if self.suppress_member_lookup_errors.get() && is_synthesized_code_diag(&error) {
            return;
        }
        self.diagnostics
            .borrow_mut()
            .diagnostics
            .push(TirDiagnostic {
                error,
                severity: DiagnosticSeverity::Error,
                primary: DiagnosticLocation::ExprSegment(at, segment_idx),
                related,
            });
    }

    /// Report a type error at a raw source span (for type annotations).
    pub fn report_at_span(&self, error: TirTypeError, span: TextRange) {
        self.report_at_span_with_related(error, span, Vec::new());
    }

    /// Report a type error at a raw source span with related notes.
    pub fn report_at_span_with_related(
        &self,
        error: TirTypeError,
        span: TextRange,
        related: Vec<RelatedNote<'db>>,
    ) {
        if self.suppress_member_lookup_errors.get() && is_synthesized_code_diag(&error) {
            return;
        }
        self.diagnostics
            .borrow_mut()
            .diagnostics
            .push(TirDiagnostic {
                error,
                severity: DiagnosticSeverity::Error,
                primary: DiagnosticLocation::Span(span),
                related,
            });
    }

    /// Report a warning-level diagnostic at a specific statement.
    pub fn report_warning_at_stmt(&self, error: TirTypeError, at: StmtId) {
        self.diagnostics
            .borrow_mut()
            .diagnostics
            .push(TirDiagnostic {
                error,
                severity: DiagnosticSeverity::Warning,
                primary: DiagnosticLocation::Stmt(at),
                related: Vec::new(),
            });
    }

    /// Report a warning-level diagnostic at an expression.
    pub fn report_warning_simple(&self, error: TirTypeError, at: ExprId) {
        self.diagnostics
            .borrow_mut()
            .diagnostics
            .push(TirDiagnostic {
                error,
                severity: DiagnosticSeverity::Warning,
                primary: DiagnosticLocation::Expr(at),
                related: Vec::new(),
            });
    }

    /// Report a warning-level diagnostic at a raw source span.
    pub fn report_warning_at_span(&self, error: TirTypeError, span: TextRange) {
        self.diagnostics
            .borrow_mut()
            .diagnostics
            .push(TirDiagnostic {
                error,
                severity: DiagnosticSeverity::Warning,
                primary: DiagnosticLocation::Span(span),
                related: Vec::new(),
            });
    }

    /// Consume the context and return accumulated diagnostics.
    pub fn finish(self) -> TypeCheckDiagnostics<'db> {
        self.diagnostics.into_inner()
    }
}
