//! Source maps for mapping HIR IDs back to source locations.
//!
//! The key insight from rust-analyzer: separate **what** something is (position-independent)
//! from **where** it is (spans). Type checking operates on the "what", diagnostics resolve
//! the "where" only at render time.
//!
//! This allows whitespace and comment changes to not invalidate type checking cache
//! (once `ExprBody` no longer contains spans).

use std::{collections::HashMap, hash::Hash};

use baml_base::{FileId, Name, Span};
use baml_compiler_diagnostics::ErrorContext;
use rowan::TextRange;

use crate::{ExprId, MatchArmId, MatchArmSpans, PatId, StmtId, TypeId};

// ============================================================================
// Span Resolution Context
// ============================================================================

/// Context for resolving `ErrorLocation` to `Span`.
///
/// Bundles all the information needed to resolve any location type, avoiding
/// the need for multiple `to_span` method variants.
pub struct SpanResolutionContext<'a> {
    /// Source map for expression function bodies (maps `ExprId`, `StmtId`, etc. to spans).
    /// Empty for LLM functions and template strings.
    pub expr_fn_source_map: &'a HirSourceMap,

    /// Maps type item names (classes, enums, type aliases) to their definition spans.
    pub type_spans: &'a HashMap<Name, Span>,

    /// Maps (`class_name`, `field_index`) to the field's type annotation span.
    pub field_type_spans: &'a HashMap<(Name, usize), Span>,

    /// Maps (`alias_name`, `path`) to the span of a specific type within a type alias RHS.
    /// The path navigates nested type constructors (see `ErrorLocation::TypeAliasType`).
    pub type_alias_type_spans: &'a HashMap<(Name, Vec<usize>), Span>,

    /// File ID for constructing spans (needed for `JinjaTemplate` errors).
    pub jinja_file_id: FileId,

    /// For `JinjaTemplate` errors: the file offset where the template text starts.
    /// This is looked up from the CST at diagnostic rendering time.
    /// None for expression functions (which don't have Jinja templates).
    pub template_file_offset: Option<u32>,
}

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
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ErrorLocation {
    /// Error at an expression.
    Expr(ExprId),
    /// Error at a match arm (for unreachable arm errors).
    MatchArm(MatchArmId),
    /// Error at a top-level type item (type alias or class).
    ///
    /// Used for validation errors about type definitions (e.g., cycle detection).
    /// The Name is resolved to a span during diagnostic rendering.
    TypeItem(Name),
    /// Error at a class field's type annotation.
    ///
    /// Used for unknown type errors in class field declarations.
    /// Contains (`class_name`, `field_index`) for position-independent lookup.
    ClassFieldType {
        class_name: Name,
        field_index: usize, // TODO: use a full path here, not just field index.
    },
    /// Error at a specific type within a type alias's RHS definition.
    ///
    /// Used for unknown type errors in type alias declarations.
    /// The `path` navigates to the specific type within nested type constructors:
    /// - For List: index 0 is the element type
    /// - For Map: index 0 is the key type, index 1 is the value type
    /// - For Union: index is the variant number (0, 1, 2, ...)
    /// - For Optional: index 0 is the inner type
    /// - Empty path means the entire RHS type expression
    TypeAliasType {
        alias_name: Name,
        /// Path to the specific type within nested type constructors.
        path: Vec<usize>,
    },
    /// Error within a Jinja template (LLM function prompt or template string).
    ///
    /// Contains offsets relative to the start of the template text, not absolute file positions.
    /// This allows the error to be cached independently of the template's position in the file.
    /// At diagnostic rendering time, the template's actual file offset is looked up from the CST.
    JinjaTemplate {
        /// Offset from template start where the error begins
        start_offset: u32,
        /// Offset from template start where the error ends
        end_offset: u32,
    },
    /// Error at a pattern (e.g., a typed binding in a match arm).
    Pattern(PatId),
    /// Fallback to a direct span (for errors from signatures or other non-body contexts).
    /// This should be minimized over time as we add more ID-based tracking.
    Span(Span),
}

impl ErrorLocation {
    /// Resolve this location to a `Span`.
    ///
    /// Uses the `SpanResolutionContext` to resolve any location type:
    /// - Expression locations (Expr, `MatchArm`) use the `expr_fn_source_map`
    /// - Type-level locations (`TypeItem`, `ClassFieldType`, `TypeAliasType`) use the `type_spans`
    /// - Jinja template locations use the `template_file_offset`
    pub fn to_span(&self, ctx: &SpanResolutionContext<'_>) -> Span {
        match self {
            ErrorLocation::Expr(id) => ctx.expr_fn_source_map.expr_span(*id).unwrap_or_default(),
            ErrorLocation::MatchArm(id) => ctx
                .expr_fn_source_map
                .match_arm_spans(*id)
                .map(|s| s.arm_span)
                .unwrap_or_default(),
            ErrorLocation::TypeItem(name) => ctx
                .type_spans
                .get(name)
                .copied()
                .unwrap_or_else(Span::default),
            ErrorLocation::ClassFieldType {
                class_name,
                field_index,
            } => {
                // Look up the field's type span, falling back to the class span
                ctx.field_type_spans
                    .get(&(class_name.clone(), *field_index))
                    .copied()
                    .or_else(|| ctx.type_spans.get(class_name).copied())
                    .unwrap_or_else(Span::default)
            }
            ErrorLocation::TypeAliasType { alias_name, path } => {
                // Try to find the specific type span using the path
                ctx.type_alias_type_spans
                    .get(&(alias_name.clone(), path.clone()))
                    .copied()
                    // Fall back to the whole RHS (empty path)
                    .or_else(|| {
                        ctx.type_alias_type_spans
                            .get(&(alias_name.clone(), vec![]))
                            .copied()
                    })
                    // Fall back to the type alias name span
                    .or_else(|| ctx.type_spans.get(alias_name).copied())
                    .unwrap_or_else(Span::default)
            }
            ErrorLocation::JinjaTemplate {
                start_offset,
                end_offset,
            } => {
                if let Some(base_offset) = ctx.template_file_offset {
                    let start = base_offset + start_offset;
                    let end = base_offset + end_offset;
                    Span::new(ctx.jinja_file_id, TextRange::new(start.into(), end.into()))
                } else {
                    Span::default()
                }
            }
            ErrorLocation::Pattern(id) => {
                ctx.expr_fn_source_map.pattern_span(*id).unwrap_or_default()
            }
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

    /// Spans of parameters (entire param including name), indexed by position
    param_spans: Vec<Option<TextRange>>,

    /// Spans of parameter types only (not including name), indexed by position
    param_type_spans: Vec<Option<TextRange>>,
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

    /// Add a parameter span (entire parameter including name).
    pub fn push_param_span(&mut self, span: Option<TextRange>) {
        self.param_spans.push(span);
    }

    /// Add a parameter type span (just the type, not including name).
    pub fn push_param_type_span(&mut self, span: Option<TextRange>) {
        self.param_type_spans.push(span);
    }

    /// Get a parameter span by index (entire parameter including name).
    pub fn param_span(&self, index: usize) -> Option<TextRange> {
        self.param_spans.get(index).copied().flatten()
    }

    /// Get a parameter type span by index (just the type).
    pub fn param_type_span(&self, index: usize) -> Option<TextRange> {
        self.param_type_spans.get(index).copied().flatten()
    }
}
