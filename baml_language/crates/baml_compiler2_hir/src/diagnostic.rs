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
                    (Some(s), true) => format!("{}.{}", s, name),
                    _ => name.to_string(),
                };
                let in_scope = match (scope, use_dot) {
                    (Some(s), false) => format!(" in `{}`", s),
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
        }
    }
}
