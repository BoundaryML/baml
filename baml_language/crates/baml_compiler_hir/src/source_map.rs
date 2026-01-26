//! Source maps for mapping HIR IDs back to source locations.
//!
//! The key insight from rust-analyzer: separate **what** something is (position-independent)
//! from **where** it is (spans). Type checking operates on the "what", diagnostics resolve
//! the "where" only at render time.
//!
//! This allows whitespace and comment changes to not invalidate type checking cache
//! (once `ExprBody` no longer contains spans).

use std::{collections::HashMap, hash::Hash};

use baml_base::Span;
use baml_compiler_diagnostics::ErrorContext;
use rowan::TextRange;

use crate::{ExprId, MatchArmId, MatchArmSpans, PatId, StmtId, TypeId};

// ============================================================================
// Error Location for TIR
// ============================================================================

/// Position-independent error location for TIR type errors.
///
/// This enum allows type errors to reference locations by ID rather than by
/// span, enabling Salsa to cache type inference results without invalidation
/// when whitespace or comments change.
///
/// At diagnostic rendering time, these locations are resolved to `Span` via
/// the `HirSourceMap`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ErrorLocation {
    /// Error at an expression.
    Expr(ExprId),
    /// Error at a match arm (for unreachable arm errors).
    MatchArm(MatchArmId),
    /// Fallback to a direct span (for errors from signatures or other non-body contexts).
    /// This should be minimized over time as we add more ID-based tracking.
    Span(Span),
}

impl ErrorLocation {
    /// Resolve this location to a `Span` using the provided source map.
    pub fn to_span(&self, source_map: &HirSourceMap) -> Span {
        match self {
            ErrorLocation::Expr(id) => source_map.expr_span(*id).unwrap_or_default(),
            ErrorLocation::MatchArm(id) => source_map
                .match_arm_spans(*id)
                .map(|s| s.arm_span)
                .unwrap_or_default(),
            ErrorLocation::Span(span) => *span,
        }
    }
}

impl From<ExprId> for ErrorLocation {
    fn from(id: ExprId) -> Self {
        ErrorLocation::Expr(id)
    }
}

impl From<MatchArmId> for ErrorLocation {
    fn from(id: MatchArmId) -> Self {
        ErrorLocation::MatchArm(id)
    }
}

impl From<Span> for ErrorLocation {
    fn from(span: Span) -> Self {
        ErrorLocation::Span(span)
    }
}

// ============================================================================
// TIR Error Context
// ============================================================================

/// Error context for TIR (Typed Intermediate Representation).
///
/// This implements `ErrorContext` with:
/// - `Ty` as the type (from `baml_compiler_tir`)
/// - `ErrorLocation` as the location (position-independent IDs)
///
/// Note: This is a marker type parameterized by `Ty` since we can't reference
/// the actual `Ty` type here (it would create a circular dependency).
/// TIR defines `type TirErrorContext = TirContext<Ty>`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TirContext<Ty>(std::marker::PhantomData<Ty>);

impl<Ty: std::fmt::Debug + Clone + PartialEq + Eq + Hash> ErrorContext for TirContext<Ty> {
    type Ty = Ty;
    type Location = ErrorLocation;
}

/// Source map for HIR expression bodies.
///
/// Maps HIR IDs back to their source spans, enabling accurate error locations
/// without storing spans in the cached HIR structures.
#[allow(clippy::struct_field_names)]
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HirSourceMap {
    /// Expression spans
    expr_spans: HashMap<ExprId, Span>,

    /// Statement spans
    stmt_spans: HashMap<StmtId, Span>,

    /// Pattern spans
    pattern_spans: HashMap<PatId, Span>,

    /// Match arm spans
    match_arm_spans: HashMap<MatchArmId, MatchArmSpans>,

    /// Type annotation spans
    type_spans: HashMap<TypeId, Span>,
}

impl HirSourceMap {
    /// Create a new empty source map.
    pub fn new() -> Self {
        Self::default()
    }

    // ========================================================================
    // Expression mappings
    // ========================================================================

    /// Insert an expression span.
    pub fn insert_expr(&mut self, id: ExprId, span: Span) {
        self.expr_spans.insert(id, span);
    }

    /// Get the span for an expression.
    pub fn expr_span(&self, id: ExprId) -> Option<Span> {
        self.expr_spans.get(&id).copied()
    }

    // ========================================================================
    // Statement mappings
    // ========================================================================

    /// Insert a statement span.
    pub fn insert_stmt(&mut self, id: StmtId, span: Span) {
        self.stmt_spans.insert(id, span);
    }

    /// Get the span for a statement.
    pub fn stmt_span(&self, id: StmtId) -> Option<Span> {
        self.stmt_spans.get(&id).copied()
    }

    // ========================================================================
    // Pattern mappings
    // ========================================================================

    /// Insert a pattern span.
    pub fn insert_pattern(&mut self, id: PatId, span: Span) {
        self.pattern_spans.insert(id, span);
    }

    /// Get the span for a pattern.
    pub fn pattern_span(&self, id: PatId) -> Option<Span> {
        self.pattern_spans.get(&id).copied()
    }

    // ========================================================================
    // Match arm mappings
    // ========================================================================

    /// Insert match arm spans.
    pub fn insert_match_arm(&mut self, id: MatchArmId, spans: MatchArmSpans) {
        self.match_arm_spans.insert(id, spans);
    }

    /// Get the spans for a match arm.
    pub fn match_arm_spans(&self, id: MatchArmId) -> Option<MatchArmSpans> {
        self.match_arm_spans.get(&id).copied()
    }

    // ========================================================================
    // Type annotation mappings
    // ========================================================================

    /// Insert a type annotation span.
    pub fn insert_type(&mut self, id: TypeId, span: Span) {
        self.type_spans.insert(id, span);
    }

    /// Get the span for a type annotation.
    pub fn type_span(&self, id: TypeId) -> Option<Span> {
        self.type_spans.get(&id).copied()
    }
}

// ============================================================================
// Signature Source Map
// ============================================================================

/// Source map for function signatures.
///
/// Maps signature components back to their source spans, enabling accurate
/// error locations without storing spans in the cached `FunctionSignature`.
#[allow(clippy::struct_field_names)]
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SignatureSourceMap {
    /// Span of the return type annotation
    return_type_span: Option<TextRange>,

    /// Spans of parameters, indexed by position
    param_spans: Vec<Option<TextRange>>,
}

impl SignatureSourceMap {
    /// Create a new empty signature source map.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the return type span.
    pub fn set_return_type_span(&mut self, span: TextRange) {
        self.return_type_span = Some(span);
    }

    /// Get the return type span.
    pub fn return_type_span(&self) -> Option<TextRange> {
        self.return_type_span
    }

    /// Add a parameter span.
    pub fn push_param_span(&mut self, span: Option<TextRange>) {
        self.param_spans.push(span);
    }

    /// Get a parameter span by index.
    pub fn param_span(&self, index: usize) -> Option<TextRange> {
        self.param_spans.get(index).copied().flatten()
    }
}
