//! Diagnostics emitted during CST → AST lowering.
//!
//! Uses `TextRange` (file-relative) — `FileId` is added at the conversion
//! boundary in `check_file()`.

use baml_base::{FileId, Span};
use baml_compiler_diagnostics::diagnostic::{Diagnostic, DiagnosticId, DiagnosticPhase, Severity};
use text_size::TextRange;

/// Diagnostic emitted during CST → AST lowering.
///
/// These are structural problems ("missing name token", "unparseable type")
/// rather than semantic ones ("duplicate definition", "type mismatch").
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoweringDiagnostic {
    /// A top-level item (class, function, enum, etc.) has no name token.
    MissingItemName {
        item_kind: &'static str,
        span: TextRange,
    },

    /// A class field has no name token.
    MissingFieldName { class_name: String, span: TextRange },

    /// A type expression could not be parsed — lowered to `TypeExpr::Unknown`.
    UnparseableType {
        context: String, // "field `Foo.bar`", "return type", "parameter `x`"
        span: TextRange,
    },

    /// A function parameter has no name token.
    MissingParamName {
        function_name: String,
        span: TextRange,
    },

    /// A parameter default was parsed in a context that does not support defaults.
    UnsupportedParameterDefault { context: String, span: TextRange },

    /// A user-authored LLM function declared `client`, which is reserved for
    /// the compiler-injected client override parameter.
    ReservedLlmClientParam {
        function_name: String,
        param_name: String,
        span: TextRange,
    },

    /// An enum variant has no name token.
    MissingVariantName { enum_name: String, span: TextRange },

    /// An attribute could not be lowered (missing name token).
    MalformedAttribute {
        context: String, // "field `Foo.bar`", "class `Foo`"
        span: TextRange,
    },

    /// A config block item (client, generator, test, `retry_policy`) has a
    /// missing key.
    MissingConfigKey {
        block_kind: &'static str,
        block_name: String,
        span: TextRange,
    },

    /// A required config block is missing entirely (e.g., client without
    /// options block, `retry_policy` without config block).
    MissingConfigBlock {
        block_kind: &'static str,
        block_name: String,
        span: TextRange,
    },

    /// The client's `provider` value is not a recognized provider name.
    UnknownProvider {
        client_name: String,
        provider: String,
        span: TextRange,
    },

    /// A client is missing required provider-specific options.
    MissingClientOptions {
        client_name: String,
        message: String,
        span: TextRange,
    },

    /// A field attribute (`@alias`, `@description`, `@skip`) appeared in a type
    /// expression where only type attributes are valid (e.g. inside parens, on a
    /// non-final union member, or in a function signature).
    FieldAttributeInTypePosition { attr_name: String, span: TextRange },

    /// A byte string literal contains an invalid escape sequence.
    InvalidByteStringEscape { message: String, span: TextRange },

    /// The `instanceof` operator was used; it has been removed. Use `match` instead.
    InstanceofRemoved { span: TextRange },

    /// `void` was used outside of a function return type position.
    VoidInNonReturnPosition { context: String, span: TextRange },

    /// A `:` type ascription was applied to a pattern that doesn't accept
    /// one. Only `let x: T` and `[…]: T` are supported. Things like
    /// `_: T`, `int: T`, `Class { … }: T`, `(a | b): T`, and progressive
    /// chains (`let x: T1: T2`) are all rejected.
    InvalidPatternAscription {
        reason: &'static str,
        span: TextRange,
    },

    /// `const` currently parses as a non-immutable alias for `let`.
    ConstBindingIntroducer { span: TextRange },

    /// `const` is reserved as future language surface and cannot be used as a
    /// binding name.
    ReservedConstBindingName { span: TextRange },

    /// `$id` is the runtime-identity special form (reads lower to
    /// `baml.id.current()`, writes to `baml.id.set(...)`); a binding named
    /// `$id` would be silently dead, so it is rejected.
    ReservedRuntimeIdBindingName { span: TextRange },

    /// Top-level `implements I for T` where `T` does not match any class in the file.
    UnresolvedImplementsForTarget {
        interface_name: String,
        target_name: String,
        span: TextRange,
    },

    /// Top-level `implements I for T` tried to add fields to a non-class
    /// target. Primitive and fixed-shape types may only provide methods.
    InvalidImplementsForFieldsTarget {
        target_name: String,
        span: TextRange,
    },

    /// Field declarations inside `implements` blocks are the obsolete
    /// qualified-field model. Interface fields are satisfied by class fields
    /// or explicit `field as class_field` links.
    InterfaceFieldDeclaredInImplementsBlock {
        interface_name: String,
        field_name: String,
        span: TextRange,
    },
}

impl LoweringDiagnostic {
    /// Convert to the shared `Diagnostic` type for rendering.
    ///
    /// `file_id` is the file this diagnostic was produced in — needed to
    /// construct `Span` values from the stored `TextRange`s.
    pub fn to_diagnostic(&self, file_id: FileId) -> Diagnostic {
        let (id, severity, message, range, label) = match self {
            LoweringDiagnostic::MissingItemName { item_kind, span } => (
                DiagnosticId::MissingName,
                Severity::Error,
                format!("{item_kind} is missing a name"),
                *span,
                "expected a name here",
            ),
            LoweringDiagnostic::MissingFieldName { class_name, span } => (
                DiagnosticId::MissingName,
                Severity::Error,
                format!("field in class `{class_name}` is missing a name"),
                *span,
                "expected a field name",
            ),
            LoweringDiagnostic::UnparseableType { context, span } => (
                DiagnosticId::UnparseableType,
                Severity::Error,
                format!("could not parse type expression for {context}"),
                *span,
                "unparseable type",
            ),
            LoweringDiagnostic::MissingParamName {
                function_name,
                span,
            } => (
                DiagnosticId::MissingName,
                Severity::Error,
                format!("parameter in function `{function_name}` is missing a name"),
                *span,
                "expected a parameter name",
            ),
            LoweringDiagnostic::UnsupportedParameterDefault { context, span } => (
                DiagnosticId::InvalidSyntax,
                Severity::Error,
                format!("parameter defaults are not supported in {context}"),
                *span,
                "default value is not allowed here",
            ),
            LoweringDiagnostic::ReservedLlmClientParam {
                function_name,
                param_name,
                span,
            } => (
                DiagnosticId::InvalidSyntax,
                Severity::Error,
                format!(
                    "LLM function `{function_name}` cannot declare a parameter named `{param_name}`; `client` is reserved for the compiler-injected LLM client override"
                ),
                *span,
                "`client` is reserved here",
            ),
            LoweringDiagnostic::MissingVariantName { enum_name, span } => (
                DiagnosticId::MissingName,
                Severity::Error,
                format!("variant in enum `{enum_name}` is missing a name"),
                *span,
                "expected a variant name",
            ),
            LoweringDiagnostic::MalformedAttribute { context, span } => (
                DiagnosticId::MalformedAttribute,
                Severity::Error,
                format!("attribute on {context} is missing a name"),
                *span,
                "malformed attribute",
            ),
            LoweringDiagnostic::MissingConfigKey {
                block_kind,
                block_name,
                span,
            } => (
                DiagnosticId::MissingConfigKey,
                Severity::Error,
                format!("config entry in {block_kind} `{block_name}` is missing a key"),
                *span,
                "expected a key",
            ),
            LoweringDiagnostic::MissingConfigBlock {
                block_kind,
                block_name,
                span,
            } => (
                DiagnosticId::MissingConfigBlock,
                Severity::Error,
                format!("{block_kind} `{block_name}` is missing a required config block"),
                *span,
                "expected a config block",
            ),
            LoweringDiagnostic::UnknownProvider {
                client_name: _,
                provider,
                span,
            } => (
                DiagnosticId::UnknownProvider,
                Severity::Error,
                format!("unknown provider '{provider}'"),
                *span,
                "unknown provider",
            ),
            LoweringDiagnostic::MissingClientOptions {
                client_name: _,
                message,
                span,
            } => (
                DiagnosticId::MissingClientOptions,
                Severity::Error,
                message.clone(),
                *span,
                "missing options",
            ),
            LoweringDiagnostic::FieldAttributeInTypePosition { attr_name, span } => (
                DiagnosticId::FieldAttributeInTypePosition,
                Severity::Error,
                format!(
                    "`@{attr_name}` is only allowed on class fields and enum variants; \
                     remove it here"
                ),
                *span,
                "field attribute here",
            ),
            LoweringDiagnostic::InvalidByteStringEscape { message, span } => (
                DiagnosticId::InvalidByteStringEscape,
                Severity::Error,
                format!("invalid byte string literal: {message}"),
                *span,
                "invalid escape",
            ),
            LoweringDiagnostic::InstanceofRemoved { span } => (
                DiagnosticId::InstanceofRemoved,
                Severity::Error,
                "`instanceof` is no longer supported. Use a `match` expression for type checking instead.".to_string(),
                *span,
                "use `match` instead",
            ),
            LoweringDiagnostic::VoidInNonReturnPosition { context, span } => (
                DiagnosticId::VoidInNonReturnPosition,
                Severity::Error,
                format!("`void` can only be used as a function return type, not as {context}"),
                *span,
                "`void` not allowed here",
            ),
            LoweringDiagnostic::InvalidPatternAscription { reason, span } => (
                DiagnosticId::TypeMismatch,
                Severity::Error,
                format!("invalid pattern type ascription: {reason}"),
                *span,
                "type ascription not allowed here",
            ),
            LoweringDiagnostic::ConstBindingIntroducer { span } => (
                DiagnosticId::InvalidSyntax,
                Severity::Warning,
                "`const` is currently treated like `let`; BAML does not enforce immutability yet. Use `let` for current BAML semantics.".to_string(),
                *span,
                "`const` behaves like `let` for now",
            ),
            LoweringDiagnostic::ReservedConstBindingName { span } => (
                DiagnosticId::InvalidSyntax,
                Severity::Error,
                "`const` is reserved and cannot be used as a binding name".to_string(),
                *span,
                "`const` is reserved here",
            ),
            LoweringDiagnostic::ReservedRuntimeIdBindingName { span } => (
                DiagnosticId::InvalidSyntax,
                Severity::Error,
                "`$id` is the runtime identity and cannot be used as a binding name".to_string(),
                *span,
                "`$id` is reserved here",
            ),
            LoweringDiagnostic::UnresolvedImplementsForTarget {
                interface_name,
                target_name,
                span,
            } => (
                DiagnosticId::UnknownType,
                Severity::Error,
                format!("`implements {interface_name} for {target_name}`: type `{target_name}` not found"),
                *span,
                "unknown target type",
            ),
            LoweringDiagnostic::InvalidImplementsForFieldsTarget { target_name, span } => (
                DiagnosticId::TypeMismatch,
                Severity::Error,
                format!(
                    "`implements for {target_name}` cannot declare fields; only class targets can add interface fields"
                ),
                *span,
                "fields are not allowed for this target",
            ),
            LoweringDiagnostic::InterfaceFieldDeclaredInImplementsBlock {
                interface_name,
                field_name,
                span,
            } => (
                DiagnosticId::InterfaceFieldDeclaredInImplementsBlock,
                Severity::Error,
                format!(
                    "field `{field_name}` cannot be declared inside `implements {interface_name}`"
                ),
                *span,
                "add a class field, or link it with `field as class_field`",
            ),
        };

        Diagnostic::new(id, severity, message)
            .with_primary(Span { file_id, range }, label)
            .with_phase(DiagnosticPhase::Hir)
    }
}
