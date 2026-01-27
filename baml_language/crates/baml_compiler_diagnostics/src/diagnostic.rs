//! Unified diagnostic type for all BAML compiler phases.
//!
//! This module provides a single `Diagnostic` type that can represent any
//! compiler error across all phases (parsing, HIR lowering, type checking).
//! This enables centralized rendering and consistent error handling.

use baml_base::{FileId, Span};

// ============================================================================
// DiagnosticPhase - Tracks which compiler phase produced a diagnostic
// ============================================================================

/// The compiler phase that produced a diagnostic.
///
/// This enables grouping diagnostics by phase for display purposes
/// (e.g., in `tools_onionskin` TUI or `baml_tests` snapshots).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum DiagnosticPhase {
    /// Parsing phase errors (syntax errors from the parser)
    #[default]
    Parse,
    /// HIR lowering phase (per-file validation like duplicate fields)
    Hir,
    /// Cross-file validation (duplicate names across files)
    Validation,
    /// Type inference phase (type mismatches, unknown variables)
    Type,
}

impl DiagnosticPhase {
    /// Get a short display name for the phase.
    pub fn name(&self) -> &'static str {
        match self {
            DiagnosticPhase::Parse => "parse",
            DiagnosticPhase::Hir => "hir",
            DiagnosticPhase::Validation => "validation",
            DiagnosticPhase::Type => "type",
        }
    }
}

/// Unique identifier for a diagnostic category.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DiagnosticId {
    // Parse errors (E0009, E0010)
    UnexpectedEof,
    UnexpectedToken,
    InvalidSyntax,

    // Type errors (E0001-E0008)
    TypeMismatch,
    UnknownType,
    UnknownVariable,
    InvalidOperator,
    ArgumentCountMismatch,
    NotCallable,
    NoSuchField,
    NotIndexable,
    InvalidMapKeyType,

    // Name errors (E0011)
    DuplicateName,

    // HIR diagnostics (E0012-E0027)
    DuplicateField,
    DuplicateVariant,
    DuplicateAttribute,
    UnknownAttribute,
    InvalidAttributeContext,
    UnknownGeneratorProperty,
    MissingGeneratorProperty,
    InvalidGeneratorPropertyValue,
    ReservedFieldName,
    FieldNameMatchesTypeName,
    InvalidClientResponseType,
    HttpConfigNotBlock,
    UnknownHttpConfigField,
    NegativeTimeout,
    MissingProvider,
    UnknownClientProperty,

    // Pattern matching errors (E0062-E0066)
    NonExhaustiveMatch,
    UnreachableArm,
    UnknownEnumVariant,
    WatchOnNonVariable,
    WatchOnUnwatchedVariable,

    // Syntax errors (E0028-E0031)
    MissingSemicolon,
    MissingConditionParens,
    UnmatchedDelimiter,

    // Return expression errors (E0029)
    MissingReturnExpression,

    // Constraint attribute errors (E0032)
    InvalidConstraintSyntax,

    // Attribute value errors (E0037-E0038)
    InvalidAttributeArg,
    UnexpectedAttributeArg,

    // Type literal errors (E0033)
    UnsupportedFloatLiteral,

    // Map type errors (E0039)
    InvalidMapArity,

    // Test diagnostics (E0034-E0036)
    UnknownTestProperty,
    MissingTestProperty,
    TestFieldAttribute,

    // Type builder diagnostics (E0040-E0043)
    TypeBuilderInNonTestContext,
    DuplicateTypeBuilderBlock,
    IncompleteDynamicDefinition,
    TypeBuilderSyntaxError,
}

impl DiagnosticId {
    /// Returns the error code as a string (e.g., "E0001").
    pub fn code(&self) -> &'static str {
        match self {
            // Parse errors
            DiagnosticId::UnexpectedEof => "E0009",
            DiagnosticId::UnexpectedToken => "E0010",
            DiagnosticId::InvalidSyntax => "E0010",

            // Type errors
            DiagnosticId::TypeMismatch => "E0001",
            DiagnosticId::UnknownType => "E0002",
            DiagnosticId::UnknownVariable => "E0003",
            DiagnosticId::InvalidOperator => "E0004",
            DiagnosticId::ArgumentCountMismatch => "E0005",
            DiagnosticId::NotCallable => "E0006",
            DiagnosticId::NoSuchField => "E0007",
            DiagnosticId::NotIndexable => "E0008",
            DiagnosticId::InvalidMapKeyType => "E0067",

            // Name errors
            DiagnosticId::DuplicateName => "E0011",

            // HIR diagnostics
            DiagnosticId::DuplicateField => "E0012",
            DiagnosticId::DuplicateVariant => "E0013",
            DiagnosticId::DuplicateAttribute => "E0014",
            DiagnosticId::UnknownAttribute => "E0015",
            DiagnosticId::InvalidAttributeContext => "E0016",
            DiagnosticId::UnknownGeneratorProperty => "E0017",
            DiagnosticId::MissingGeneratorProperty => "E0018",
            DiagnosticId::InvalidGeneratorPropertyValue => "E0019",
            DiagnosticId::ReservedFieldName => "E0020",
            DiagnosticId::FieldNameMatchesTypeName => "E0021",
            DiagnosticId::InvalidClientResponseType => "E0022",
            DiagnosticId::HttpConfigNotBlock => "E0023",
            DiagnosticId::UnknownHttpConfigField => "E0024",
            DiagnosticId::NegativeTimeout => "E0025",
            DiagnosticId::MissingProvider => "E0026",
            DiagnosticId::UnknownClientProperty => "E0027",

            // Pattern matching errors
            DiagnosticId::NonExhaustiveMatch => "E0062",
            DiagnosticId::UnreachableArm => "E0063",
            DiagnosticId::UnknownEnumVariant => "E0064",
            DiagnosticId::WatchOnNonVariable => "E0065",
            DiagnosticId::WatchOnUnwatchedVariable => "E0066",

            // Syntax errors
            DiagnosticId::MissingSemicolon => "E0028",
            DiagnosticId::MissingConditionParens => "E0030",
            DiagnosticId::UnmatchedDelimiter => "E0031",

            // Return expression errors
            DiagnosticId::MissingReturnExpression => "E0029",

            // Constraint attribute errors
            DiagnosticId::InvalidConstraintSyntax => "E0032",

            // Attribute value errors
            DiagnosticId::InvalidAttributeArg => "E0037",
            DiagnosticId::UnexpectedAttributeArg => "E0038",

            // Type literal errors
            DiagnosticId::UnsupportedFloatLiteral => "E0033",

            // Map type errors
            DiagnosticId::InvalidMapArity => "E0039",

            // Test diagnostics
            DiagnosticId::UnknownTestProperty => "E0034",
            DiagnosticId::MissingTestProperty => "E0035",
            DiagnosticId::TestFieldAttribute => "E0036",

            // Type builder diagnostics
            DiagnosticId::TypeBuilderInNonTestContext => "E0040",
            DiagnosticId::DuplicateTypeBuilderBlock => "E0041",
            DiagnosticId::IncompleteDynamicDefinition => "E0042",
            DiagnosticId::TypeBuilderSyntaxError => "E0043",
        }
    }
}

/// Severity level of a diagnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Severity {
    /// An error that prevents compilation.
    Error,
    /// A warning that doesn't prevent compilation.
    Warning,
    /// Informational message.
    Info,
}

/// An annotation pointing to a span in the source code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Annotation {
    /// The span this annotation refers to.
    pub span: Span,
    /// Message for this annotation (optional).
    pub message: Option<String>,
    /// Whether this is the primary annotation.
    pub is_primary: bool,
}

impl Annotation {
    /// Create a primary annotation with a message.
    pub fn primary(span: Span, message: impl Into<String>) -> Self {
        Self {
            span,
            message: Some(message.into()),
            is_primary: true,
        }
    }

    /// Create a primary annotation without a message.
    pub fn primary_no_msg(span: Span) -> Self {
        Self {
            span,
            message: None,
            is_primary: true,
        }
    }

    /// Create a secondary annotation with a message.
    pub fn secondary(span: Span, message: impl Into<String>) -> Self {
        Self {
            span,
            message: Some(message.into()),
            is_primary: false,
        }
    }
}

/// Related diagnostic information (for cross-file references).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelatedInfo {
    /// The span of the related location.
    pub span: Span,
    /// The message describing this related location.
    pub message: String,
    /// Optional file path for display purposes.
    pub file_path: Option<String>,
}

impl RelatedInfo {
    /// Create a new related info with a span and message.
    pub fn new(span: Span, message: impl Into<String>) -> Self {
        Self {
            span,
            message: message.into(),
            file_path: None,
        }
    }

    /// Create a new related info with file path for display.
    pub fn with_path(span: Span, message: impl Into<String>, path: impl Into<String>) -> Self {
        Self {
            span,
            message: message.into(),
            file_path: Some(path.into()),
        }
    }
}

/// A unified diagnostic that can represent any BAML compiler error.
///
/// This type is inspired by `ruff_db::Diagnostic` and enables:
/// - Centralized diagnostic collection via `Project::check()`
/// - Multi-format rendering (Ariadne for CLI, LSP types for editors)
/// - Consistent error handling across all compiler phases
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    /// The diagnostic category/id.
    pub id: DiagnosticId,
    /// The severity level.
    pub severity: Severity,
    /// The main error message.
    pub message: String,
    /// Annotations pointing to relevant source locations.
    pub annotations: Vec<Annotation>,
    /// Related information (e.g., "first defined here").
    pub related_info: Vec<RelatedInfo>,
    /// The compiler phase that produced this diagnostic.
    pub phase: DiagnosticPhase,
}

impl Diagnostic {
    /// Create a new diagnostic with a single primary span.
    pub fn new(id: DiagnosticId, severity: Severity, message: impl Into<String>) -> Self {
        Self {
            id,
            severity,
            message: message.into(),
            annotations: Vec::new(),
            related_info: Vec::new(),
            phase: DiagnosticPhase::default(),
        }
    }

    /// Create an error diagnostic.
    pub fn error(id: DiagnosticId, message: impl Into<String>) -> Self {
        Self::new(id, Severity::Error, message)
    }

    /// Create a warning diagnostic.
    pub fn warning(id: DiagnosticId, message: impl Into<String>) -> Self {
        Self::new(id, Severity::Warning, message)
    }

    /// Set the compiler phase for this diagnostic.
    #[must_use]
    pub fn with_phase(mut self, phase: DiagnosticPhase) -> Self {
        self.phase = phase;
        self
    }

    /// Add a primary annotation at a span with a message.
    #[must_use]
    pub fn with_primary(mut self, span: Span, message: impl Into<String>) -> Self {
        self.annotations.push(Annotation::primary(span, message));
        self
    }

    /// Add a primary annotation at a span using the main message.
    #[must_use]
    pub fn with_primary_span(mut self, span: Span) -> Self {
        self.annotations
            .push(Annotation::primary(span, self.message.clone()));
        self
    }

    /// Add a secondary annotation at a span.
    #[must_use]
    pub fn with_secondary(mut self, span: Span, message: impl Into<String>) -> Self {
        self.annotations.push(Annotation::secondary(span, message));
        self
    }

    /// Add related information.
    #[must_use]
    pub fn with_related(mut self, span: Span, message: impl Into<String>) -> Self {
        self.related_info.push(RelatedInfo::new(span, message));
        self
    }

    /// Add related information with file path.
    #[must_use]
    pub fn with_related_path(
        mut self,
        span: Span,
        message: impl Into<String>,
        path: impl Into<String>,
    ) -> Self {
        self.related_info
            .push(RelatedInfo::with_path(span, message, path));
        self
    }

    /// Get the error code string.
    pub fn code(&self) -> &'static str {
        self.id.code()
    }

    /// Get the primary span (first primary annotation).
    pub fn primary_span(&self) -> Option<Span> {
        self.annotations
            .iter()
            .find(|a| a.is_primary)
            .map(|a| a.span)
    }

    /// Get the primary file ID.
    pub fn file_id(&self) -> Option<FileId> {
        self.primary_span().map(|s| s.file_id)
    }
}

/// Trait for converting error types to the unified Diagnostic type.
pub trait ToDiagnostic {
    /// Convert this error to a unified Diagnostic.
    fn to_diagnostic(&self) -> Diagnostic;
}

#[cfg(test)]
mod tests {
    use text_size::TextRange;

    use super::*;

    fn test_span() -> Span {
        Span {
            file_id: FileId::new(0),
            range: TextRange::new(0.into(), 10.into()),
        }
    }

    #[test]
    fn test_diagnostic_builder() {
        let diag = Diagnostic::error(DiagnosticId::TypeMismatch, "Expected int, found string")
            .with_primary_span(test_span());

        assert_eq!(diag.severity, Severity::Error);
        assert_eq!(diag.code(), "E0001");
        assert_eq!(diag.message, "Expected int, found string");
        assert_eq!(diag.annotations.len(), 1);
        assert!(diag.annotations[0].is_primary);
    }

    #[test]
    fn test_diagnostic_with_related() {
        let first_span = test_span();
        let second_span = Span {
            file_id: FileId::new(1),
            range: TextRange::new(20.into(), 30.into()),
        };

        let diag = Diagnostic::error(DiagnosticId::DuplicateName, "Duplicate class 'Foo'")
            .with_primary_span(second_span)
            .with_related(first_span, "First defined here");

        assert_eq!(diag.related_info.len(), 1);
        assert_eq!(diag.related_info[0].message, "First defined here");
    }

    #[test]
    fn test_all_error_codes() {
        // Ensure all DiagnosticId variants have unique error codes
        let ids = vec![
            DiagnosticId::TypeMismatch,
            DiagnosticId::UnknownType,
            DiagnosticId::UnknownVariable,
            DiagnosticId::InvalidOperator,
            DiagnosticId::ArgumentCountMismatch,
            DiagnosticId::NotCallable,
            DiagnosticId::NoSuchField,
            DiagnosticId::NotIndexable,
            DiagnosticId::UnexpectedEof,
            DiagnosticId::UnexpectedToken,
            DiagnosticId::DuplicateName,
        ];

        for id in ids {
            let code = id.code();
            assert!(code.starts_with('E'), "Code should start with E: {code}");
        }
    }
}
