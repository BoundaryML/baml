//! HIR2 per-file diagnostics.
//!
//! These are produced during `SemanticIndexBuilder::build()` and stored in
//! `FileSemanticIndex::extra`. They use `TextRange` (not `Span`) because
//! the file is known from context. Conversion to the shared `Diagnostic`
//! type happens lazily via `to_diagnostic(file_id)`.

use baml_base::{FileId, Name, Span};
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
    /// Two or more members of a class or enum serialize to the same JSON key —
    /// either two members share an `@alias("k")`, or one member's name equals
    /// another member's `@alias`. Because an aliased member's real name is never
    /// used for matching (see `bex_sap`'s `AnnotatedField::key_matches`), such
    /// members are indistinguishable in the serialized schema: `ctx.output_format`
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
    /// `implements I {}` names something that isn't a known interface
    /// (name does not exist at all).
    UnknownInterface {
        /// Class that contains the bad `implements` block.
        class_name: Name,
        /// What the user wrote after `implements`.
        target_name: String,
        span: TextRange,
    },
    /// `implements X {}` references a type that exists but is not an interface
    /// (e.g. a class or enum).
    NotAnInterface {
        class_name: Name,
        target_name: String,
        span: TextRange,
    },
    /// `interface I requires X` where `X` exists but is not an interface (a
    /// class or enum). Reported at the `requires` clause, not at an implementor.
    InterfaceRequiresNonInterface {
        interface_name: Name,
        target_name: String,
        span: TextRange,
    },
    /// `interface I requires X` where `X` does not name any type at all.
    /// Distinct from [`Self::InterfaceRequiresNonInterface`] (which is for a
    /// real-but-wrong-kind target) so the message matches the `implements`
    /// path's "no interface with that name is in scope" (E0112) instead of
    /// wrongly claiming `X` "is not an interface".
    UnknownRequiredInterface {
        interface_name: Name,
        target_name: String,
        span: TextRange,
    },
    /// A class is missing a body for a required (no-default) interface method.
    MissingInterfaceMethod {
        class_name: Name,
        interface_name: Name,
        method_name: Name,
        /// Span of the offending `implements I { ... }` block.
        span: TextRange,
    },
    /// The same `implements I` block appears twice on a class.
    DuplicateImplementsBlock {
        class_name: Name,
        interface_name: Name,
        /// All sites in source order; first is the "winner".
        sites: Vec<TextRange>,
    },
    /// A method body inside `implements I { ... }` names something that
    /// isn't declared on I (neither required nor default).
    UnknownInterfaceMember {
        interface_name: Name,
        method_name: Name,
        span: TextRange,
    },
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
    /// A class declares a `cleanup` method whose signature is not the reserved
    /// magic-finalizer shape `cleanup(self) -> void` (BEP-042). `cleanup` is a
    /// reserved magic method name, so it must have that exact shape.
    CleanupMagicMethodSignature { class_name: Name, span: TextRange },
    /// A class field has a different type than the interface declares for
    /// that name.
    InterfaceFieldTypeMismatch {
        class_name: Name,
        field_name: Name,
        /// The interface's own field name, which differs from `field_name`
        /// when the implements block aliases it (`name as name_count`).
        interface_field_name: Name,
        interface_name: Name,
        /// User-rendered class type.
        class_type: String,
        /// User-rendered interface type.
        interface_type: String,
        span: TextRange,
    },
    /// Two interfaces require the same field name with different types and a
    /// class implements both — there is no single type to inject.
    ConflictingInterfaceFieldTypes {
        class_name: Name,
        field_name: Name,
        first_interface: Name,
        first_type: String,
        second_interface: Name,
        second_type: String,
        span: TextRange,
    },
    /// An interface's `extends` chain forms a cycle (`A extends B`, `B extends A`).
    InterfaceExtendsCycle { chain: Vec<Name>, span: TextRange },
    /// An interface inherits conflicting types for the same field name from
    /// two parent interfaces in its `extends` list.
    InterfaceExtendsFieldConflict {
        interface_name: Name,
        field_name: Name,
        first_interface: Name,
        first_type: String,
        second_interface: Name,
        second_type: String,
        span: TextRange,
    },
    /// A method declared in an `implements` block has a different signature
    /// from the one the interface requires.
    InterfaceMethodSignatureMismatch {
        class_name: Name,
        interface_name: Name,
        method_name: Name,
        /// Human-readable rendering of the implementing signature.
        actual: String,
        /// Human-readable rendering of the interface signature.
        expected: String,
        span: TextRange,
    },
    /// A class has the same method name declared in two or more `implements`
    /// blocks. The user must qualify (`obj.A.foo()` vs `obj.B.foo()`) at
    /// call sites (BEP-044 §"Method Disambiguation").
    ///
    /// `sources` is `(interface_name, declaration_span)` for every block
    /// that declares `method_name`, in source order. There can be more than
    /// two — e.g. a class implementing three interfaces each redeclaring
    /// `to_string`.
    AmbiguousInterfaceMethod {
        class_name: Name,
        method_name: Name,
        sources: Vec<(Name, TextRange)>,
    },
    /// An `implements` block is missing a required field from the interface.
    MissingInterfaceField {
        class_name: Name,
        interface_name: Name,
        field_name: Name,
        /// Span of the `implements` block.
        span: TextRange,
    },
    /// The left side of `field as class_field` does not name a field on the
    /// target interface.
    UnknownInterfaceFieldLink {
        interface_name: Name,
        field_name: Name,
        span: TextRange,
    },
    /// The right side of `field as class_field` does not name a class field.
    UnknownClassFieldInInterfaceLink {
        class_name: Name,
        interface_name: Name,
        field_name: Name,
        span: TextRange,
    },
    /// The same interface field is linked more than once in one implements block.
    DuplicateInterfaceFieldLink {
        interface_name: Name,
        field_name: Name,
        sites: Vec<TextRange>,
    },
    /// A class implements an interface whose `requires` parents are not
    /// all explicitly implemented by the same class.
    MissingRequiredInterface {
        class_name: Name,
        interface_name: Name,
        missing_parents: Vec<Name>,
        span: TextRange,
    },
    /// Top-level `implements I for T` cannot implement an interface that
    /// declares fields, because `T`'s data shape is already fixed.
    OutOfBodyImplementsFieldInterface {
        target_name: String,
        interface_name: Name,
        span: TextRange,
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
                        let kind_list: Vec<&str> =
                            sites.iter().map(|s| s.kind.as_str()).collect();
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
            Hir2Diagnostic::UnknownInternalAttribute {
                attr_name,
                span,
                valid_attributes,
            } => Diagnostic::error(
                DiagnosticId::UnknownAttribute,
                format!(
                    "unknown attribute `@@{}`. Valid builtin internal attributes are: {}",
                    attr_name,
                    valid_attributes.join(", ")
                ),
            )
            .with_primary(Span { file_id, range: *span }, "unknown attribute")
            .with_phase(DiagnosticPhase::Hir),
            Hir2Diagnostic::UnknownTypeAttribute { attr_name, span } => Diagnostic::error(
                DiagnosticId::UnknownAttribute,
                format!("unknown attribute `@{attr_name}`"),
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
                    "attribute `@@{attr_name}` is not valid on {context}. Allowed contexts: {allowed_contexts}",
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
                    Span { file_id, range: *or_span },
                    "alternatives bind different names",
                )
                .with_phase(DiagnosticPhase::Hir)
            }
            Hir2Diagnostic::UnknownInterface {
                class_name,
                target_name,
                span,
            } => Diagnostic::error(
                DiagnosticId::UnknownInterface,
                format!(
                    "class `{class_name}` cannot implement `{target_name}`: \
                     no interface with that name is in scope"
                ),
            )
            .with_primary(
                Span {
                    file_id,
                    range: *span,
                },
                "interface not found",
            )
            .with_phase(DiagnosticPhase::Hir),
            Hir2Diagnostic::NotAnInterface {
                class_name,
                target_name,
                span,
            } => Diagnostic::error(
                DiagnosticId::NotAnInterface,
                format!(
                    "`{target_name}` is not an interface; \
                     class `{class_name}` can only implement interfaces"
                ),
            )
            .with_primary(
                Span {
                    file_id,
                    range: *span,
                },
                "not an interface",
            )
            .with_phase(DiagnosticPhase::Hir),
            Hir2Diagnostic::InterfaceRequiresNonInterface {
                interface_name,
                target_name,
                span,
            } => Diagnostic::error(
                DiagnosticId::InterfaceRequiresNonInterface,
                format!(
                    "`{target_name}` is not an interface; \
                     interface `{interface_name}` can only require interfaces"
                ),
            )
            .with_primary(
                Span {
                    file_id,
                    range: *span,
                },
                "not an interface",
            )
            .with_phase(DiagnosticPhase::Hir),
            Hir2Diagnostic::UnknownRequiredInterface {
                interface_name,
                target_name,
                span,
            } => Diagnostic::error(
                DiagnosticId::UnknownInterface,
                format!(
                    "interface `{interface_name}` cannot require `{target_name}`: \
                     no interface with that name is in scope"
                ),
            )
            .with_primary(
                Span {
                    file_id,
                    range: *span,
                },
                "interface not found",
            )
            .with_phase(DiagnosticPhase::Hir),
            Hir2Diagnostic::MissingInterfaceMethod {
                class_name,
                interface_name,
                method_name,
                span,
            } => Diagnostic::error(
                DiagnosticId::MissingInterfaceMethod,
                format!(
                    "class `{class_name}` does not implement required method `{method_name}` of \
                     interface `{interface_name}`"
                ),
            )
            .with_primary(
                Span {
                    file_id,
                    range: *span,
                },
                format!("missing `{method_name}` here"),
            )
            .with_phase(DiagnosticPhase::Hir),
            Hir2Diagnostic::DuplicateImplementsBlock {
                class_name,
                interface_name,
                sites,
            } => {
                let first = sites.first().copied().unwrap_or_default();
                let rest = sites.get(1..).unwrap_or(&[]);
                let mut diag = Diagnostic::error(
                    DiagnosticId::DuplicateImplementsBlock,
                    format!(
                        "class `{class_name}` has multiple `implements {interface_name}` blocks; \
                         a class may implement each interface at most once"
                    ),
                )
                .with_secondary(
                    Span {
                        file_id,
                        range: first,
                    },
                    "first `implements` block here",
                );
                for r in rest {
                    diag = diag.with_primary(
                        Span {
                            file_id,
                            range: *r,
                        },
                        "duplicate `implements` block",
                    );
                }
                diag.with_phase(DiagnosticPhase::Hir)
            }
            Hir2Diagnostic::UnknownInterfaceMember {
                interface_name,
                method_name,
                span,
            } => Diagnostic::error(
                DiagnosticId::UnknownInterfaceMember,
                format!(
                    "method `{method_name}` is not declared on interface `{interface_name}`; \
                     remove it or add it to the interface"
                ),
            )
            .with_primary(
                Span {
                    file_id,
                    range: *span,
                },
                "not a member of the interface",
            )
            .with_phase(DiagnosticPhase::Hir),

            Hir2Diagnostic::ToStringMustImplementInterface { class_name, span } => Diagnostic::error(
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
            .with_phase(DiagnosticPhase::Hir),

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

            Hir2Diagnostic::FromJsonMustImplementInterface { class_name, span } => Diagnostic::error(
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
            .with_phase(DiagnosticPhase::Hir),

            Hir2Diagnostic::CleanupMagicMethodSignature { class_name, span } => Diagnostic::error(
                DiagnosticId::CleanupMagicMethodSignature,
                format!(
                    "`cleanup` on class `{class_name}` must have the signature \
                     `cleanup(self) -> void`; it is a reserved magic finalizer name"
                ),
            )
            .with_primary(
                Span {
                    file_id,
                    range: *span,
                },
                "expected `cleanup(self) -> void`",
            )
            .with_phase(DiagnosticPhase::Hir),
            Hir2Diagnostic::InterfaceFieldTypeMismatch {
                class_name,
                field_name,
                interface_field_name,
                interface_name,
                class_type,
                interface_type,
                span,
            } => Diagnostic::error(
                DiagnosticId::InterfaceFieldTypeMismatch,
                format!(
                    "class `{class_name}` declares field `{field_name}: {class_type}` but \
                     interface `{interface_name}` requires `{interface_field_name}` to have type `{interface_type}`"
                ),
            )
            .with_primary(
                Span {
                    file_id,
                    range: *span,
                },
                format!("expected `{interface_type}`, found `{class_type}`"),
            )
            .with_phase(DiagnosticPhase::Hir),
            Hir2Diagnostic::ConflictingInterfaceFieldTypes {
                class_name,
                field_name,
                first_interface,
                first_type,
                second_interface,
                second_type,
                span,
            } => Diagnostic::error(
                DiagnosticId::ConflictingInterfaceFieldTypes,
                format!(
                    "class `{class_name}` cannot implement both `{first_interface}` and \
                     `{second_interface}`: field `{field_name}` is declared as `{first_type}` \
                     in `{first_interface}` but `{second_type}` in `{second_interface}`"
                ),
            )
            .with_primary(
                Span {
                    file_id,
                    range: *span,
                },
                "conflicting field types from two interfaces",
            )
            .with_phase(DiagnosticPhase::Hir),
            Hir2Diagnostic::InterfaceExtendsCycle { chain, span } => {
                let chain_str = chain
                    .iter()
                    .map(std::string::ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(" -> ");
                Diagnostic::error(
                    DiagnosticId::InterfaceExtendsCycle,
                    format!("interface `requires` chain forms a cycle: {chain_str}"),
                )
                .with_primary(
                    Span {
                        file_id,
                        range: *span,
                    },
                    "cyclic `requires`",
                )
                .with_phase(DiagnosticPhase::Hir)
            }
            Hir2Diagnostic::InterfaceExtendsFieldConflict {
                interface_name,
                field_name,
                first_interface,
                first_type,
                second_interface,
                second_type,
                span,
            } => Diagnostic::error(
                DiagnosticId::InterfaceExtendsFieldConflict,
                format!(
                    "interface `{interface_name}` inherits conflicting types for field `{field_name}`: \
                     `{first_type}` from `{first_interface}`, `{second_type}` from `{second_interface}`"
                ),
            )
            .with_primary(
                Span {
                    file_id,
                    range: *span,
                },
                "conflicting field types inherited via `requires`",
            )
            .with_phase(DiagnosticPhase::Hir),
            Hir2Diagnostic::InterfaceMethodSignatureMismatch {
                class_name,
                interface_name,
                method_name,
                actual,
                expected,
                span,
            } => Diagnostic::error(
                DiagnosticId::InterfaceMethodSignatureMismatch,
                format!(
                    "method `{method_name}` in class `{class_name}` does not match interface \
                     `{interface_name}`: expected `{expected}`, found `{actual}`"
                ),
            )
            .with_primary(
                Span {
                    file_id,
                    range: *span,
                },
                "signature mismatch",
            )
            .with_phase(DiagnosticPhase::Hir),
            Hir2Diagnostic::AmbiguousInterfaceMethod {
                class_name,
                method_name,
                sources,
            } => {
                let iface_list = sources
                    .iter()
                    .map(|(iface, _)| format!("`{iface}`"))
                    .collect::<Vec<_>>()
                    .join(", ");
                let hint = sources
                    .iter()
                    .map(|(iface, _)| format!("obj.{iface}.{method_name}()"))
                    .collect::<Vec<_>>()
                    .join(" or ");
                let mut diag = Diagnostic::error(
                    DiagnosticId::AmbiguousInterfaceMethod,
                    format!(
                        "method `{method_name}` on class `{class_name}` is declared by multiple \
                         interfaces: {iface_list}; unqualified calls will be ambiguous — use \
                         {hint}"
                    ),
                );
                if let Some(((first_iface, first_span), rest)) = sources.split_first() {
                    diag = diag.with_secondary(
                        Span {
                            file_id,
                            range: *first_span,
                        },
                        format!("first declared in `implements {first_iface}` here"),
                    );
                    for (iface, span) in rest {
                        diag = diag.with_primary(
                            Span {
                                file_id,
                                range: *span,
                            },
                            format!("also declared in `implements {iface}`"),
                        );
                    }
                }
                diag.with_phase(DiagnosticPhase::Hir)
            }
            Hir2Diagnostic::MissingInterfaceField {
                class_name,
                interface_name,
                field_name,
                span,
            } => Diagnostic::error(
                DiagnosticId::MissingInterfaceField,
                format!(
                    "class `{class_name}` does not provide field `{field_name}` required by \
                     interface `{interface_name}`"
                ),
            )
            .with_primary(
                Span {
                    file_id,
                    range: *span,
                },
                format!("add class field `{field_name}`, or link it with `{field_name} as class_field`"),
            )
            .with_phase(DiagnosticPhase::Hir),
            Hir2Diagnostic::UnknownInterfaceFieldLink {
                interface_name,
                field_name,
                span,
            } => Diagnostic::error(
                DiagnosticId::UnknownInterfaceFieldLink,
                format!("interface `{interface_name}` has no field `{field_name}`"),
            )
            .with_primary(
                Span {
                    file_id,
                    range: *span,
                },
                "not a field of the interface",
            )
            .with_phase(DiagnosticPhase::Hir),
            Hir2Diagnostic::UnknownClassFieldInInterfaceLink {
                class_name,
                interface_name,
                field_name,
                span,
            } => Diagnostic::error(
                DiagnosticId::UnknownClassFieldInInterfaceLink,
                format!(
                    "class `{class_name}` has no field `{field_name}` to link for interface `{interface_name}`"
                ),
            )
            .with_primary(
                Span {
                    file_id,
                    range: *span,
                },
                "not a field of the class",
            )
            .with_phase(DiagnosticPhase::Hir),
            Hir2Diagnostic::DuplicateInterfaceFieldLink {
                interface_name,
                field_name,
                sites,
            } => {
                let mut diag = Diagnostic::error(
                    DiagnosticId::DuplicateInterfaceFieldLink,
                    format!(
                        "field `{field_name}` of interface `{interface_name}` is linked more than once"
                    ),
                );
                if let Some((first, rest)) = sites.split_first() {
                    diag = diag.with_secondary(
                        Span {
                            file_id,
                            range: *first,
                        },
                        "first link here",
                    );
                    for span in rest {
                        diag = diag.with_primary(
                            Span {
                                file_id,
                                range: *span,
                            },
                            "duplicate link here",
                        );
                    }
                }
                diag.with_phase(DiagnosticPhase::Hir)
            }
            Hir2Diagnostic::MissingRequiredInterface {
                class_name,
                interface_name,
                missing_parents,
                span,
            } => {
                let list = missing_parents
                    .iter()
                    .map(|n| format!("`{n}`"))
                    .collect::<Vec<_>>()
                    .join(", ");
                Diagnostic::error(
                    DiagnosticId::MissingRequiredInterface,
                    format!(
                        "class `{class_name}` implements `{interface_name}`, which requires \
                         {list}, but `{class_name}` does not implement them"
                    ),
                )
                .with_primary(
                    Span {
                        file_id,
                        range: *span,
                    },
                    format!("requires {list}"),
                )
                .with_phase(DiagnosticPhase::Hir)
            }
            Hir2Diagnostic::OutOfBodyImplementsFieldInterface {
                target_name,
                interface_name,
                span,
            } => Diagnostic::error(
                DiagnosticId::OutOfBodyImplementsFieldInterface,
                format!(
                    "`implements {interface_name} for {target_name}` cannot implement \
                     field-bearing interface `{interface_name}` outside the class body"
                ),
            )
            .with_primary(
                Span {
                    file_id,
                    range: *span,
                },
                "field-bearing interfaces must be implemented inside the class body",
            )
            .with_phase(DiagnosticPhase::Hir),
        }
    }
}
