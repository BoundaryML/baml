//! Diagnostic sink for a single scope inference run.
//!
//! `InferContext` is held inside `TypeInferenceBuilder` and accumulates
//! type errors discovered during expression walking. Consuming `finish()`
//! returns the accumulated `TypeCheckDiagnostics`.
//!
//! Diagnostics are Salsa-stable (no `TextRange`) — locations are stored as
//! arena IDs. The LSP layer maps them to source ranges at display time.

use std::{cell::RefCell, fmt};

use baml_base::Name;
use baml_compiler2_ast::{AstSourceMap, ExprId, StmtId, TypeAnnotId};
use baml_compiler2_hir::{
    contributions::Definition,
    loc::{ClassLoc, FunctionLoc},
    scope::ScopeId,
};
use text_size::TextRange;

use crate::ty::Ty;

// ── Error kinds ──────────────────────────────────────────────────────────────

/// What went wrong — no location info, just the semantic error.
///
/// `TirTypeError` is intentionally span-free for Salsa cacheability.
/// Each error is paired with a primary `ExprId` in `TirDiagnostic`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TirTypeError {
    /// Type mismatch: expected vs actual.
    TypeMismatch { expected: Ty, got: Ty },
    /// Member not found on a known type.
    ///
    /// Reported when the base type IS resolved (known class/enum) but the
    /// member doesn't exist. Error messages are tailored by base type:
    /// - Class: "Class `X` has no member `y`"
    /// - Enum: "Enum `X` has no variant `y`"
    UnresolvedMember { base_type: Ty, member: Name },
    /// Name could not be resolved at all.
    UnresolvedName { name: Name },
    /// Unreachable code after a diverging statement (return/break/continue).
    DeadCode {
        after: StmtId,
        unreachable_count: usize,
    },
    /// A `void` expression (e.g. `if` without `else`) was used where a value
    /// is required — assigned to a variable, passed as an argument, or returned.
    VoidUsedAsValue,
    /// Expression is not callable (e.g. `42(1)` or `Foo(1)` where Foo is a class).
    NotCallable { ty: Ty },
    /// Expression is not indexable (e.g. `true[0]`).
    NotIndexable { ty: Ty },
    /// Invalid operand types for a binary operator (e.g. `true + false`).
    InvalidBinaryOp {
        op: baml_compiler2_ast::BinaryOp,
        lhs: Ty,
        rhs: Ty,
    },
    /// Invalid operand type for a unary operator (e.g. `-"hello"`).
    InvalidUnaryOp {
        op: baml_compiler2_ast::UnaryOp,
        operand: Ty,
    },
    /// A type name in a type annotation could not be resolved.
    UnresolvedType { name: Name },
    /// Wrong number of arguments in a function call.
    ArgumentCountMismatch { expected: usize, got: usize },
    /// Function body ends without returning a value.
    MissingReturn { expected: Ty },
    /// Type alias participates in an invalid (unguarded) cycle.
    ///
    /// Examples: `type A = A`, `type A = B; type B = A`.
    /// Valid recursion through containers (`type JSON = string | JSON[]`) does NOT
    /// trigger this — only cycles with no base case.
    AliasCycle { name: Name },
    /// Class participates in an unconstructable required-field cycle.
    ///
    /// Examples: `class A { b B }; class B { a A }`.
    /// Cycles through optional, list, or map fields are valid since those can
    /// be null/empty, breaking the construction dependency.
    ClassCycle { name: Name, cycle_path: String },
    /// `match` is missing arms for one or more values.
    NonExhaustiveMatch {
        scrutinee_type: Ty,
        missing_cases: Vec<String>,
    },
    /// A `match`/`catch` arm can never execute because previous arms are exhaustive.
    UnreachableArm,
    /// Catch binding cannot be typed as `any` or `unknown`.
    InvalidCatchBindingType { type_name: String },
    /// Inferred escaping throws are not covered by the declared throws contract.
    ThrowsContractViolation {
        declared: Ty,
        extra_types: Vec<String>,
    },
    /// Declared throws contains extra types that never escape.
    ExtraneousThrowsDeclaration { extra_types: Vec<String> },
}

impl fmt::Display for TirTypeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TirTypeError::TypeMismatch { expected, got } => {
                write!(f, "type mismatch: expected {expected}, got {got}")
            }
            TirTypeError::UnresolvedMember { base_type, member } => {
                write!(f, "unresolved member: {base_type}.{member}")
            }
            TirTypeError::UnresolvedName { name } => {
                write!(f, "unresolved name: {name}")
            }
            TirTypeError::DeadCode {
                unreachable_count, ..
            } => {
                write!(
                    f,
                    "unreachable code: {unreachable_count} statement(s) after diverging statement"
                )
            }
            TirTypeError::VoidUsedAsValue => {
                write!(
                    f,
                    "`if` without `else` cannot be used as a value; add an `else` branch"
                )
            }
            TirTypeError::NotCallable { ty } => {
                write!(f, "type `{ty}` is not callable")
            }
            TirTypeError::NotIndexable { ty } => {
                write!(f, "type `{ty}` is not indexable")
            }
            TirTypeError::InvalidBinaryOp { op, lhs, rhs } => {
                write!(
                    f,
                    "operator `{op:?}` cannot be applied to `{lhs}` and `{rhs}`"
                )
            }
            TirTypeError::InvalidUnaryOp { op, operand } => {
                write!(f, "operator `{op:?}` cannot be applied to `{operand}`")
            }
            TirTypeError::UnresolvedType { name } => {
                write!(f, "unresolved type: {name}")
            }
            TirTypeError::ArgumentCountMismatch { expected, got } => {
                write!(f, "expected {expected} argument(s), got {got}")
            }
            TirTypeError::MissingReturn { expected } => {
                write!(f, "missing return: expected `{expected}`")
            }
            TirTypeError::AliasCycle { name } => {
                write!(f, "recursive type alias cycle: {name}")
            }
            TirTypeError::ClassCycle { cycle_path, .. } => {
                write!(f, "class cycle: {cycle_path}")
            }
            TirTypeError::NonExhaustiveMatch {
                scrutinee_type,
                missing_cases,
            } => {
                write!(
                    f,
                    "non-exhaustive match on `{scrutinee_type}`; missing: {}",
                    missing_cases.join(", ")
                )
            }
            TirTypeError::UnreachableArm => write!(f, "unreachable arm"),
            TirTypeError::InvalidCatchBindingType { type_name } => write!(
                f,
                "invalid catch binding type `{type_name}`; use a concrete type instead"
            ),
            TirTypeError::ThrowsContractViolation {
                declared,
                extra_types,
            } => write!(
                f,
                "throws contract violation: `{declared}` is missing {}",
                extra_types.join(", ")
            ),
            TirTypeError::ExtraneousThrowsDeclaration { extra_types } => write!(
                f,
                "extraneous throws declaration: {}",
                extra_types.join(", ")
            ),
        }
    }
}

/// Diagnostic severity used by compiler2 TIR diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticSeverity {
    Error,
    Warning,
}

// ── Multi-span diagnostic structure ─────────────────────────────────────────

/// A location that may be in the current scope, another scope, or another file.
///
/// All variants use Salsa-stable IDs — no `TextRange`s. The LSP layer maps
/// each variant to `(File, TextRange)` via the appropriate source map.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RelatedLocation<'db> {
    /// Expression in the same scope's `ExprBody`.
    Expr(ExprId),
    /// Statement in the same scope's `ExprBody`.
    Stmt(StmtId),
    /// A function parameter (possibly in another file).
    Param(FunctionLoc<'db>, usize),
    /// A class field definition.
    ClassField(ClassLoc<'db>, Name),
    /// Any top-level item definition (class, enum, function, etc.).
    Item(Definition<'db>),
}

/// Primary location for a diagnostic — either an expression, a statement,
/// or a raw source span (for type annotations that lack an ExprId).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiagnosticLocation {
    Expr(ExprId),
    Stmt(StmtId),
    TypeAnnot(TypeAnnotId),
    Span(TextRange),
}

/// A single type-check diagnostic with primary location and optional related spans.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TirDiagnostic<'db> {
    /// What went wrong.
    pub error: TirTypeError,
    /// Severity level.
    pub severity: DiagnosticSeverity,
    /// Primary location — where the error was detected.
    pub primary: DiagnosticLocation,
    /// Related locations — secondary spans with explanatory messages.
    pub related: Vec<(RelatedLocation<'db>, &'static str)>,
}

impl<'db> TirDiagnostic<'db> {
    /// Resolve this diagnostic's arena IDs to source ranges and produce a
    /// rendered diagnostic with a human-readable message and `TextRange`.
    ///
    /// `source_map` is the `AstSourceMap` for the function body that owns
    /// the expressions/statements referenced by `self.primary`.
    pub fn render(&self, source_map: Option<&AstSourceMap>) -> RenderedTirDiagnostic {
        let primary_range = match &self.primary {
            DiagnosticLocation::Expr(id) => {
                source_map.map(|sm| sm.expr_span(*id)).unwrap_or_default()
            }
            DiagnosticLocation::Stmt(id) => {
                source_map.map(|sm| sm.stmt_span(*id)).unwrap_or_default()
            }
            DiagnosticLocation::TypeAnnot(id) => source_map
                .map(|sm| sm.type_annotation_span(*id))
                .unwrap_or_default(),
            DiagnosticLocation::Span(range) => *range,
        };

        RenderedTirDiagnostic {
            message: self.error.to_string(),
            range: primary_range,
            severity: self.severity,
        }
    }
}

/// A fully rendered diagnostic — ready for display / LSP.
///
/// Contains the human-readable message and the resolved source `TextRange`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderedTirDiagnostic {
    /// Human-readable error message (e.g. "type mismatch: expected int, got string").
    pub message: String,
    /// Source range within the file (resolved from `ExprId`/`StmtId`).
    pub range: TextRange,
    /// Severity level for rendering.
    pub severity: DiagnosticSeverity,
}

impl fmt::Display for RenderedTirDiagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let start = u32::from(self.range.start());
        let end = u32::from(self.range.end());
        write!(f, "{start}..{end}: {}", self.message)
    }
}

/// Accumulated diagnostics for a single scope inference run.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TypeCheckDiagnostics<'db> {
    pub diagnostics: Vec<TirDiagnostic<'db>>,
}

impl<'db> TypeCheckDiagnostics<'db> {
    pub fn is_empty(&self) -> bool {
        self.diagnostics.is_empty()
    }

    pub fn extend(&mut self, other: &TypeCheckDiagnostics<'db>) {
        self.diagnostics.extend(other.diagnostics.iter().cloned());
    }
}

// ── InferContext ─────────────────────────────────────────────────────────────

/// Diagnostic sink for a single scope inference run.
///
/// Held inside `TypeInferenceBuilder` — one per `infer_scope_types` call.
/// Modeled after Ty's `InferContext` (`context.rs:37-46`).
pub struct InferContext<'db> {
    db: &'db dyn crate::Db,
    scope: ScopeId<'db>,
    diagnostics: RefCell<TypeCheckDiagnostics<'db>>,
}

impl<'db> InferContext<'db> {
    pub fn new(db: &'db dyn crate::Db, scope: ScopeId<'db>) -> Self {
        Self {
            db,
            scope,
            diagnostics: RefCell::new(TypeCheckDiagnostics::default()),
        }
    }

    pub fn db(&self) -> &'db dyn crate::Db {
        self.db
    }

    pub fn scope(&self) -> ScopeId<'db> {
        self.scope
    }

    /// Report a type error at a specific expression, with optional related locations.
    pub fn report(
        &self,
        error: TirTypeError,
        at: ExprId,
        related: Vec<(RelatedLocation<'db>, &'static str)>,
    ) {
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

    /// Report a type error at a type annotation location.
    pub fn report_at_type_annot(&self, error: TirTypeError, at: TypeAnnotId) {
        self.diagnostics
            .borrow_mut()
            .diagnostics
            .push(TirDiagnostic {
                error,
                severity: DiagnosticSeverity::Error,
                primary: DiagnosticLocation::TypeAnnot(at),
                related: Vec::new(),
            });
    }

    /// Report a type error at a raw source span (for type annotations).
    pub fn report_at_span(&self, error: TirTypeError, span: TextRange) {
        self.diagnostics
            .borrow_mut()
            .diagnostics
            .push(TirDiagnostic {
                error,
                severity: DiagnosticSeverity::Error,
                primary: DiagnosticLocation::Span(span),
                related: Vec::new(),
            });
    }

    /// Report a type error at a specific statement.
    pub fn report_at_stmt(&self, error: TirTypeError, at: StmtId) {
        self.diagnostics
            .borrow_mut()
            .diagnostics
            .push(TirDiagnostic {
                error,
                severity: DiagnosticSeverity::Error,
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
