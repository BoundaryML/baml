//! HIR2 per-file diagnostics.
//!
//! These are produced during `SemanticIndexBuilder::build()` and stored in
//! `FileSemanticIndex::extra`. They use `TextRange` (not `Span`) because
//! the file is known from context. Conversion to the shared `Diagnostic`
//! type happens lazily via `to_diagnostic(file_id)`.

use baml_base::{FileId, Name, Span};
use baml_compiler_diagnostics::diagnostic::{Diagnostic, DiagnosticId, DiagnosticPhase};
use text_size::TextRange;

use crate::contributions::DefinitionKind;

/// A definition site within a scope, with its kind tag and source range.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemberSite {
    pub range: TextRange,
    pub kind: DefinitionKind,
}

/// Per-file diagnostic produced during HIR2 semantic indexing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Hir2Diagnostic {
    /// A name is defined more than once within the same scope.
    ///
    /// The `sites` vector contains all definition sites in source order;
    /// the first entry is the "winner" (kept for downstream resolution).
    ///
    /// `scope` is the parent scope name (e.g. `Some("Foo")` for members
    /// inside `class Foo`). `None` for file-level scopes.
    DuplicateDefinition {
        name: Name,
        scope: Option<Name>,
        sites: Vec<MemberSite>,
    },
    /// Unknown builtin-internal attribute.
    UnknownInternalAttribute {
        attr_name: Name,
        span: TextRange,
        valid_attributes: Vec<&'static str>,
    },
    /// An attribute on a type expression is not a known type or field attribute.
    UnknownTypeAttribute { attr_name: Name, span: TextRange },
    /// Builtin-internal attribute used in the wrong place.
    InvalidAttributeContext {
        attr_name: Name,
        context: &'static str,
        allowed_contexts: &'static str,
        span: TextRange,
    },
    /// Builtin-only syntax used outside builtin stdlib files.
    BuiltinOnlySyntax { feature: String, span: TextRange },
    /// Generic single-span diagnostic for builtin contract validation.
    DiagnosticMessage {
        diagnostic_id: DiagnosticId,
        message: String,
        span: TextRange,
    },
    /// A pattern introduces the same name more than once (e.g.
    /// `Foo { a, a }`, `let Foo { x }: let x = ...`). Each binding inside a
    /// single pattern must use a unique name — otherwise it would shadow
    /// itself within the same scope.
    ///
    /// `sites` lists every binding site for `name` in source order. The
    /// first site is treated as the original; the rest are reported as
    /// duplicates.
    DuplicatePatternBinding { name: Name, sites: Vec<TextRange> },
    /// A class destructure names the same field more than once.
    DuplicatePatternField { name: Name, sites: Vec<TextRange> },
    /// An `Or` pattern's alternatives don't all bind the same name set.
    /// A name introduced in some alternatives but not others would only
    /// sometimes be in scope in the arm body — semantically incoherent.
    ///
    /// Example: `(Foo { a } | Bar { a, b })` — `b` is bound by the `Bar`
    /// alternative but not by `Foo`.
    OrPatternBindingMismatch {
        /// Span of the `Or` pattern itself.
        or_span: TextRange,
        /// Names that appear in some alternatives but not all.
        mismatched_names: Vec<Name>,
    },
}

impl Hir2Diagnostic {
    /// Convert to the shared `Diagnostic` type for rendering.
    ///
    /// `file_id` is the file this diagnostic was produced in — needed to
    /// construct `Span` values from the stored `TextRange`s.
    pub fn to_diagnostic(&self, file_id: FileId) -> Diagnostic {
        match self {
            Hir2Diagnostic::DuplicateDefinition { name, scope, sites } => {
                let first = &sites[0];
                let rest = &sites[1..];

                let use_dot = first.kind.is_member();
                let qualified = match (scope, use_dot) {
                    (Some(s), true) => format!("{s}.{name}"),
                    _ => name.to_string(),
                };
                let in_scope = match (scope, use_dot) {
                    (Some(s), false) => format!(" in `{s}`"),
                    _ => String::new(),
                };

                let kinds_match = rest.iter().all(|s| s.kind == first.kind);
                let message = if kinds_match {
                    format!("Duplicate {} `{}`{}", first.kind, qualified, in_scope)
                } else {
                    let kind_list: Vec<&str> = sites.iter().map(|s| s.kind.as_str()).collect();
                    format!(
                        "Name `{}`{} defined {} times as: {}",
                        qualified,
                        in_scope,
                        sites.len(),
                        kind_list.join(", ")
                    )
                };

                let mut diag = Diagnostic::error(DiagnosticId::DuplicateField, message);
                let first_span = Span {
                    file_id,
                    range: first.range,
                };
                diag = diag
                    .with_secondary(first_span, format!("first defined as {} here", first.kind));
                for site in rest {
                    let span = Span {
                        file_id,
                        range: site.range,
                    };
                    diag = diag.with_primary(span, format!("duplicate {} definition", site.kind));
                }
                diag.with_phase(DiagnosticPhase::Hir)
            }
            Hir2Diagnostic::UnknownInternalAttribute {
                attr_name,
                span,
                valid_attributes,
            } => Diagnostic::error(
                DiagnosticId::UnknownAttribute,
                format!(
                    "Unknown attribute `@@{}`. Valid builtin internal attributes are: {}",
                    attr_name,
                    valid_attributes.join(", ")
                ),
            )
            .with_primary(Span { file_id, range: *span }, "unknown attribute")
            .with_phase(DiagnosticPhase::Hir),
            Hir2Diagnostic::UnknownTypeAttribute { attr_name, span } => Diagnostic::error(
                DiagnosticId::UnknownAttribute,
                format!("Unknown attribute `@{attr_name}`"),
            )
            .with_primary(Span { file_id, range: *span }, "unknown attribute")
            .with_phase(DiagnosticPhase::Hir),
            Hir2Diagnostic::InvalidAttributeContext {
                attr_name,
                context,
                allowed_contexts,
                span,
            } => Diagnostic::error(
                DiagnosticId::InvalidAttributeContext,
                format!(
                    "Attribute `@@{attr_name}` is not valid on {context}. Allowed contexts: {allowed_contexts}",
                ),
            )
            .with_primary(
                Span {
                    file_id,
                    range: *span,
                },
                "invalid attribute context",
            )
            .with_phase(DiagnosticPhase::Hir),
            Hir2Diagnostic::BuiltinOnlySyntax { feature, span } => Diagnostic::error(
                DiagnosticId::InvalidAttributeContext,
                format!("Builtin-only syntax `{feature}` is only allowed in builtin stdlib files"),
            )
            .with_primary(
                Span {
                    file_id,
                    range: *span,
                },
                "builtin-only syntax",
            )
            .with_phase(DiagnosticPhase::Hir),
            Hir2Diagnostic::DiagnosticMessage {
                diagnostic_id,
                message,
                span,
            } => Diagnostic::error(*diagnostic_id, message.clone())
                .with_primary(
                    Span {
                        file_id,
                        range: *span,
                    },
                    "invalid builtin declaration",
                )
                .with_phase(DiagnosticPhase::Hir),
            Hir2Diagnostic::DuplicatePatternBinding { name, sites } => {
                let first = sites.first().copied().unwrap_or_default();
                let rest = sites.get(1..).unwrap_or(&[]);
                let mut diag = Diagnostic::error(
                    DiagnosticId::DuplicateBinding,
                    format!("Duplicate binding `{name}` in pattern"),
                )
                .with_secondary(
                    Span { file_id, range: first },
                    format!("`{name}` first bound here"),
                );
                for range in rest {
                    diag = diag.with_primary(
                        Span { file_id, range: *range },
                        format!("`{name}` bound again here"),
                    );
                }
                diag.with_phase(DiagnosticPhase::Hir)
            }
            Hir2Diagnostic::DuplicatePatternField { name, sites } => {
                let first = sites.first().copied().unwrap_or_default();
                let rest = sites.get(1..).unwrap_or(&[]);
                let mut diag = Diagnostic::error(
                    DiagnosticId::DuplicateField,
                    format!("Duplicate field `{name}` in class destructure pattern"),
                )
                .with_secondary(
                    Span {
                        file_id,
                        range: first,
                    },
                    format!("field `{name}` first destructured here"),
                );
                for range in rest {
                    diag = diag.with_primary(
                        Span {
                            file_id,
                            range: *range,
                        },
                        format!("field `{name}` destructured again here"),
                    );
                }
                diag.with_phase(DiagnosticPhase::Hir)
            }
            Hir2Diagnostic::OrPatternBindingMismatch {
                or_span,
                mismatched_names,
            } => {
                let names_str = mismatched_names
                    .iter()
                    .map(std::string::ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", ");
                Diagnostic::error(
                    DiagnosticId::DuplicateBinding,
                    format!(
                        "Or-pattern alternatives must bind the same names. \
                         Inconsistent across branches: {names_str}",
                    ),
                )
                .with_primary(
                    Span { file_id, range: *or_span },
                    "alternatives bind different names",
                )
                .with_phase(DiagnosticPhase::Hir)
            }
        }
    }
}
