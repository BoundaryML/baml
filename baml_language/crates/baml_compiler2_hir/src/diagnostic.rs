//! HIR2 per-file diagnostics.
//!
//! These are produced during `SemanticIndexBuilder::build()` and stored in
//! `FileSemanticIndex::extra`. They use `TextRange` (not `Span`) because
//! the file is known from context. Conversion to the shared `Diagnostic`
//! type happens lazily via `to_diagnostic(file_id)`.

use baml_base::{AttributePosition, FileId, Name, Span};
use baml_compiler_diagnostics::{
    diagnostic::{Diagnostic, DiagnosticId, DiagnosticPhase},
    runtime_type::{self, DuplicateMemberKind, SerializedKeyContainer},
};
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
    /// An attribute the language does not define. Attributes are a fixed
    /// vocabulary (`baml_base::SCHEMA_ATTRIBUTE_SPECS`), so an unknown name is
    /// a misspelling or a removed attribute, never a pass-through annotation.
    UnknownAttribute {
        attr_name: Name,
        position: AttributePosition,
        span: TextRange,
    },
    /// A schema attribute written at a declaration position it has no meaning
    /// at: `@stream.done` on an enum variant, `@@skip` on a class.
    AttributeNotAllowedHere {
        attr_name: Name,
        position: AttributePosition,
        allowed: &'static [AttributePosition],
        span: TextRange,
    },
    /// Builtin-only syntax used outside builtin stdlib files.
    BuiltinOnlySyntax { feature: String, span: TextRange },
    /// A binding, item, or type parameter named `root`, the path to the
    /// package itself.
    RootIsReserved { span: TextRange },
    /// A file under a directory `ns_root/`: a namespace named `root`.
    RootNamespace { span: TextRange },
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
    /// Two or more members of a class or enum serialize to the same JSON key —
    /// either two members share an `@alias("k")`, or one member's name equals
    /// another member's `@alias`. Because an aliased member's real name is never
    /// used for matching (see `bex_sap`'s `AnnotatedField::key_matches`), such
    /// members are indistinguishable in the serialized schema: `ctx.output_format()`
    /// renders duplicate keys and only one member can ever be populated at parse
    /// time (for classes) or produced during parsing (for enum variants).
    ///
    /// `key` is the shared serialized key. `sites` lists the name span of every
    /// contributing member in source order; the first is treated as the original
    /// and the rest as duplicates.
    DuplicateFieldAlias {
        key: String,
        sites: Vec<TextRange>,
        container: SerializedKeyContainer,
    },
    /// A single declaration (class, enum, field, or variant) carries the same
    /// single-valued schema attribute more than once — e.g. two `@alias`, two
    /// `@description`, or two `@skip`. These attributes take effect at most once
    /// (for valued attrs the last write silently wins, discarding the earlier
    /// ones), so a repeat is always a mistake (Linear B-648).
    ///
    /// `attr_name` is the duplicated attribute name (without the leading `@`).
    /// `sites` lists the span of every occurrence in source order; the first is
    /// treated as the original and the rest as duplicates.
    DuplicateAttribute {
        attr_name: String,
        sites: Vec<TextRange>,
    },
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

    // ─── Interface diagnostics (BEP-044) ────────────────────────────────────
    /// A class declares a `to_string` method directly in its body. `to_string`
    /// is provided by the `baml.ToString` interface, not as a magic method, so
    /// it must live inside an `implements baml.ToString { ... }` block.
    ToStringMustImplementInterface { class_name: Name, span: TextRange },
    /// A class declares a `to_json` method directly in its body. `to_json` is
    /// provided by the `baml.ToJson` interface, not as a magic method, so it must
    /// live inside an `implements baml.ToJson { ... }` block.
    ToJsonMustImplementInterface { class_name: Name, span: TextRange },
    /// A class declares a `from_json` method directly in its body. `from_json` is
    /// provided by the `baml.FromJson` interface, not as a magic method, so it
    /// must live inside an `implements baml.FromJson { ... }` block. (The
    /// auto-derived structural-default `from_json` delegate is exempt.)
    FromJsonMustImplementInterface { class_name: Name, span: TextRange },
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
                let mut diag = if kinds_match
                    && first.kind == DefinitionKind::Field
                    && let Some(scope) = scope
                {
                    runtime_type::duplicate_member(
                        DuplicateMemberKind::Field,
                        scope.as_str(),
                        name.as_str(),
                    )
                } else if kinds_match
                    && first.kind == DefinitionKind::Variant
                    && let Some(scope) = scope
                {
                    runtime_type::duplicate_member(
                        DuplicateMemberKind::Variant,
                        scope.as_str(),
                        name.as_str(),
                    )
                } else {
                    let message = if kinds_match {
                        format!("duplicate {} `{}`{}", first.kind, qualified, in_scope)
                    } else {
                        let kind_list: Vec<&str> = sites.iter().map(|s| s.kind.as_str()).collect();
                        format!(
                            "name `{}`{} defined {} times as: {}",
                            qualified,
                            in_scope,
                            sites.len(),
                            kind_list.join(", ")
                        )
                    };
                    Diagnostic::error(DiagnosticId::DuplicateField, message)
                };
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
            Hir2Diagnostic::UnknownAttribute {
                attr_name,
                position,
                span,
            } => {
                let sigil = position.sigil();
                let valid = baml_base::schema_attribute_specs_at(*position)
                    .map(|spec| format!("`{sigil}{}`", spec.name))
                    .collect::<Vec<_>>();
                let valid = if valid.is_empty() {
                    format!("no attribute is valid on {}", position.describe())
                } else {
                    format!("valid on {}: {}", position.describe(), valid.join(", "))
                };
                Diagnostic::error(
                    DiagnosticId::UnknownAttribute,
                    format!("unknown attribute `{sigil}{attr_name}`; {valid}"),
                )
                .with_primary(
                    Span {
                        file_id,
                        range: *span,
                    },
                    "unknown attribute",
                )
                .with_phase(DiagnosticPhase::Hir)
            }
            Hir2Diagnostic::AttributeNotAllowedHere {
                attr_name,
                position,
                allowed,
                span,
            } => {
                let allowed = allowed
                    .iter()
                    .map(|allowed| {
                        format!("`{}{attr_name}` on {}", allowed.sigil(), allowed.describe())
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                Diagnostic::error(
                    DiagnosticId::InvalidAttributeContext,
                    format!(
                        "attribute `{}{attr_name}` is not valid on {}; allowed: {allowed}",
                        position.sigil(),
                        position.describe(),
                    ),
                )
                .with_primary(
                    Span {
                        file_id,
                        range: *span,
                    },
                    "not valid here",
                )
                .with_phase(DiagnosticPhase::Hir)
            }
            Hir2Diagnostic::RootIsReserved { span } => Diagnostic::error(
                DiagnosticId::RootIsReserved,
                "`root` is the path to this package and cannot name a binding, an item, or a \
                 type parameter"
                    .to_string(),
            )
            .with_primary(
                Span {
                    file_id,
                    range: *span,
                },
                "`root` is this package",
            )
            .with_phase(DiagnosticPhase::Hir),
            Hir2Diagnostic::RootNamespace { span } => Diagnostic::error(
                DiagnosticId::RootIsReserved,
                "`root` is the path to this package and cannot name a namespace: rename the \
                 directory `ns_root/`"
                    .to_string(),
            )
            .with_primary(
                Span {
                    file_id,
                    range: *span,
                },
                "this file is in `ns_root/`",
            )
            .with_phase(DiagnosticPhase::Hir),
            Hir2Diagnostic::BuiltinOnlySyntax { feature, span } => Diagnostic::error(
                DiagnosticId::InvalidAttributeContext,
                format!("builtin-only syntax `{feature}` is only allowed in builtin stdlib files"),
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
                    format!("duplicate binding `{name}` in pattern"),
                )
                .with_secondary(
                    Span {
                        file_id,
                        range: first,
                    },
                    format!("`{name}` first bound here"),
                );
                for range in rest {
                    diag = diag.with_primary(
                        Span {
                            file_id,
                            range: *range,
                        },
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
                    format!("duplicate field `{name}` in class destructure pattern"),
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
            Hir2Diagnostic::DuplicateFieldAlias {
                key,
                sites,
                container,
            } => {
                let first = sites.first().copied().unwrap_or_default();
                let rest = sites.get(1..).unwrap_or(&[]);
                let mut diag = runtime_type::duplicate_serialized_key(key, *container)
                    .with_secondary(
                        Span {
                            file_id,
                            range: first,
                        },
                        format!("key `{key}` first serialized here"),
                    );
                for range in rest {
                    diag = diag.with_primary(
                        Span {
                            file_id,
                            range: *range,
                        },
                        format!("also serialized as `{key}` here"),
                    );
                }
                diag.with_phase(DiagnosticPhase::Hir)
            }
            Hir2Diagnostic::DuplicateAttribute { attr_name, sites } => {
                let first = sites.first().copied().unwrap_or_default();
                let rest = sites.get(1..).unwrap_or(&[]);
                let mut diag = Diagnostic::error(
                    DiagnosticId::DuplicateAttribute,
                    format!("duplicate attribute `@{attr_name}`"),
                )
                .with_secondary(
                    Span {
                        file_id,
                        range: first,
                    },
                    format!("`@{attr_name}` first applied here"),
                );
                for range in rest {
                    diag = diag.with_primary(
                        Span {
                            file_id,
                            range: *range,
                        },
                        format!("duplicate `@{attr_name}` — only the last takes effect"),
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
                        "or-pattern alternatives must bind the same names. \
                         Inconsistent across branches: {names_str}",
                    ),
                )
                .with_primary(
                    Span {
                        file_id,
                        range: *or_span,
                    },
                    "alternatives bind different names",
                )
                .with_phase(DiagnosticPhase::Hir)
            }
            Hir2Diagnostic::ToStringMustImplementInterface { class_name, span } => {
                Diagnostic::error(
                    DiagnosticId::ToStringMustImplementInterface,
                    format!(
                        "`to_string` cannot be defined as a method on class `{class_name}`; \
                     implement the `baml.ToString` interface instead"
                    ),
                )
                .with_primary(
                    Span {
                        file_id,
                        range: *span,
                    },
                    "move this into `implements baml.ToString { ... }`",
                )
                .with_phase(DiagnosticPhase::Hir)
            }

            Hir2Diagnostic::ToJsonMustImplementInterface { class_name, span } => Diagnostic::error(
                DiagnosticId::ToJsonMustImplementInterface,
                format!(
                    "`to_json` cannot be defined as a method on class `{class_name}`; \
                     implement the `baml.ToJson` interface instead"
                ),
            )
            .with_primary(
                Span {
                    file_id,
                    range: *span,
                },
                "move this into `implements baml.ToJson { ... }`",
            )
            .with_phase(DiagnosticPhase::Hir),

            Hir2Diagnostic::FromJsonMustImplementInterface { class_name, span } => {
                Diagnostic::error(
                    DiagnosticId::FromJsonMustImplementInterface,
                    format!(
                        "`from_json` cannot be defined as a method on class `{class_name}`; \
                     implement the `baml.FromJson` interface instead"
                    ),
                )
                .with_primary(
                    Span {
                        file_id,
                        range: *span,
                    },
                    "move this into `implements baml.FromJson { ... }`",
                )
                .with_phase(DiagnosticPhase::Hir)
            }
        }
    }
}
