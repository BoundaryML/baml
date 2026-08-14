//! Diagnostics emitted during CST → AST lowering.
//!
//! Uses `TextRange` (file-relative) — `FileId` is added at the conversion
//! boundary in `check_file()`.

use baml_base::{ClientOptionsValidationError, FileId, Span};
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

    /// A `generator { … }` block was found in `.baml`. Code generators are
    /// now configured in `baml.toml` under `[generator.<name>]`; the block is
    /// ignored. Carries the optional generator name for the message.
    GeneratorBlockInBaml {
        name: Option<String>,
        span: TextRange,
    },

    /// A config block item (client, test, `retry_policy`) has a missing key.
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
        error: ClientOptionsValidationError,
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

    /// A backtick template has a malformed block tag — an unclosed,
    /// mismatched, or stray `${for}`/`${if}` open/close.
    MalformedTemplateBlock {
        kind: baml_compiler_syntax::BacktickStructuralErrorKind,
        span: TextRange,
    },

    /// A `${}` interpolation with no expression inside.
    EmptyInterpolation { span: TextRange },

    /// `void` was used outside of a function return type position.
    VoidInNonReturnPosition { context: String, span: TextRange },

    /// The `_` wildcard type was used in a position where it cannot be inferred
    /// (a signature, field, or alias type, as opposed to a `let` binding
    /// annotation or a `throws`-clause member).
    WildcardTypeNotAllowed { context: String, span: TextRange },

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

    /// An assignment operator (`=`, `+=`, …) appeared in expression position,
    /// e.g. `(x = 5)`. Assignment is statement-only in BAML, so the expression
    /// has no value; without this diagnostic it would lower to a `Missing` that
    /// only fails at runtime.
    AssignmentInExpressionPosition { span: TextRange },

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

    /// Type arguments (`<...>`) were applied to an expression that is not a
    /// plain function reference (e.g. `(foo)<int>`). Only a path reference to a
    /// generic function may be specialized into a value (`foo<int>`).
    TypeArgsOnNonPathBase { span: TextRange },

    /// An LLM function's `tools` field cannot switch the function to the
    /// ai-package spec desugar: the function is missing a backtick prompt or a
    /// `"provider/model"` client string with a known provider prefix.
    InvalidLlmToolsField {
        function_name: String,
        reason: &'static str,
        span: TextRange,
    },

    /// A legacy `client<llm> Name { ... }` config block. Removed: clients are
    /// plain values now — `client Name = <expr>;`.
    ClientBlockRemoved { name: String, span: TextRange },

    /// A legacy `retry_policy` block. Removed: retry composes at the client
    /// boundary via `ai.Retry.new(inner, ...)`.
    RetryPolicyRemoved { name: String, span: TextRange },

    /// A legacy Jinja `#"..."#` prompt on an LLM function. Removed: prompts
    /// are backtick templates.
    LlmJinjaPromptRemoved { span: TextRange },

    /// A `${` in a quoted LLM prompt. Regular strings do not interpolate, so
    /// the marker reaches the model as literal text — almost always a prompt
    /// ported from a form where it did interpolate.
    QuotedPromptInterpolation { span: TextRange },

    /// A legacy `template_string` declaration. Removed: use a function
    /// returning a backtick string.
    TemplateStringRemoved { span: TextRange },

    /// The LLM function's `client` value cannot be used: unknown provider
    /// prefix, a string without a `provider/model` shape, or the removed
    /// unquoted shorthand.
    InvalidLlmClient {
        function_name: String,
        reason: String,
        span: TextRange,
    },

    /// A `${role(...)}` marker in an LLM prompt. Removed: the prompt is
    /// instructions-only; the conversation lives in the journal and roles are
    /// the client's wire concern.
    LlmRoleMarkerRemoved {
        function_name: String,
        span: TextRange,
    },

    /// A numeric literal token failed validation (`baml_base::num_lit`):
    /// uppercase base prefix, no digits after the prefix, a digit invalid
    /// for the base, or an integer magnitude exceeding `i64::MAX`. For
    /// `InvalidDigits` one diagnostic is pushed per offending digit, with
    /// `span` covering just that digit.
    InvalidNumericLiteral {
        error: baml_base::num_lit::IntLitError,
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
            LoweringDiagnostic::GeneratorBlockInBaml { name, span } => {
                let what = match name {
                    Some(n) => format!("`generator {n}`"),
                    None => "this `generator` block".to_string(),
                };
                return Diagnostic::new(
                    DiagnosticId::GeneratorBlockUnsupported,
                    Severity::Warning,
                    format!(
                        "{what} is ignored: code generators are configured in `baml.toml` now. \
                         Move it to a `[generator.<name>]` section in `baml.toml`."
                    ),
                )
                .with_primary(
                    Span {
                        file_id,
                        range: *span,
                    },
                    "move this to baml.toml",
                )
                .with_phase(DiagnosticPhase::Hir);
            }
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
                error,
                span,
            } => (
                DiagnosticId::MissingClientOptions,
                Severity::Error,
                error.to_string(),
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
            LoweringDiagnostic::MalformedTemplateBlock { kind, span } => {
                use baml_compiler_syntax::BacktickStructuralErrorKind as K;
                let (message, label): (&str, &str) = match kind {
                    K::UnclosedFor => (
                        "unclosed ${for} block — expected ${endfor}",
                        "this ${for} is never closed",
                    ),
                    K::UnclosedIf => (
                        "unclosed ${if} block — expected ${endif}",
                        "this ${if} is never closed",
                    ),
                    K::MismatchedForClose => (
                        "${for} block closed by ${endif} — expected ${endfor}",
                        "expected ${endfor} here",
                    ),
                    K::MismatchedIfClose => (
                        "${if} block closed by ${endfor} — expected ${endif}",
                        "expected ${endif} here",
                    ),
                    K::StrayEndfor => (
                        "stray ${endfor} with no matching ${for}",
                        "no open ${for} to close",
                    ),
                    K::StrayEndif => (
                        "stray ${endif} with no matching ${if}",
                        "no open ${if} to close",
                    ),
                    K::StrayElse => (
                        "stray ${else} outside an ${if} block",
                        "${else} must be inside ${if}…${endif}",
                    ),
                    K::StrayElseIf => (
                        "stray ${else if} outside an ${if} block",
                        "${else if} must be inside ${if}…${endif}",
                    ),
                    K::DuplicateElse => (
                        "duplicate ${else} in the same ${if} chain",
                        "this ${if} already has an ${else}",
                    ),
                    K::ElseIfAfterElse => (
                        "${else if} after ${else} in the same ${if} chain",
                        "move ${else if} branches before ${else}",
                    ),
                };
                (
                    DiagnosticId::InvalidSyntax,
                    Severity::Error,
                    message.to_string(),
                    *span,
                    label,
                )
            }
            LoweringDiagnostic::EmptyInterpolation { span } => (
                DiagnosticId::InvalidSyntax,
                // Advisory, not fatal: an empty `${}` renders to the empty
                // string (a valid, if pointless, template). Pre-canary the HIR
                // pipeline let this through; canary made severity authoritative,
                // so keep it a warning to preserve the render-empty behavior
                // (see bex_engine backtick_case_c_empty_interp_renders_empty).
                Severity::Warning,
                "empty interpolation ${} has no expression".to_string(),
                *span,
                "expected an expression inside ${…}",
            ),
            LoweringDiagnostic::VoidInNonReturnPosition { context, span } => (
                DiagnosticId::VoidInNonReturnPosition,
                Severity::Error,
                format!("`void` can only be used as a function return type, not as {context}"),
                *span,
                "`void` not allowed here",
            ),
            LoweringDiagnostic::WildcardTypeNotAllowed { context, span } => (
                DiagnosticId::WildcardTypeNotAllowed,
                Severity::Error,
                format!(
                    "the `_` wildcard type can only be inferred in a `let` binding or a `throws` clause, not in {context}"
                ),
                *span,
                "`_` cannot be inferred here",
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
            LoweringDiagnostic::AssignmentInExpressionPosition { span } => (
                DiagnosticId::InvalidSyntax,
                Severity::Error,
                "assignment is not allowed in expression position; assignment is a statement"
                    .to_string(),
                *span,
                "assignment not allowed here",
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
            LoweringDiagnostic::TypeArgsOnNonPathBase { span } => (
                DiagnosticId::InvalidSyntax,
                Severity::Error,
                "type arguments can only be applied to a function reference".to_string(),
                *span,
                "specialize a generic function directly, e.g. `foo<int>`",
            ),
            LoweringDiagnostic::InvalidLlmToolsField {
                function_name,
                reason,
                span,
            } => (
                DiagnosticId::InvalidSyntax,
                Severity::Error,
                format!("LLM function `{function_name}` cannot use a `tools` field: {reason}"),
                *span,
                "`tools` requires a backtick prompt and a \"provider/model\" client string",
            ),
            LoweringDiagnostic::ClientBlockRemoved { name, span } => (
                DiagnosticId::InvalidSyntax,
                Severity::Error,
                format!(
                    "`client<llm>` config blocks are removed; declare a client value instead: \
                     `client {name} = openai.OpenAiClient.new(model = \"...\");` \
                     (compose reliability with ai.Retry / ai.Fallback / ai.RoundRobin)"
                ),
                *span,
                "replace with `client Name = <expr>;`",
            ),
            LoweringDiagnostic::RetryPolicyRemoved { name, span } => (
                DiagnosticId::InvalidSyntax,
                Severity::Error,
                format!(
                    "`retry_policy` blocks are removed; wrap the client instead: \
                     `client Reliable = ai.Retry.new(<inner>, max_attempts = ...);` \
                     (`{name}` has no effect)"
                ),
                *span,
                "retry composes at the client boundary now",
            ),
            LoweringDiagnostic::LlmJinjaPromptRemoved { span } => (
                DiagnosticId::InvalidSyntax,
                Severity::Error,
                "Jinja `#\"...\"#` prompts are no longer supported. Use a backtick prompt with `${...}` interpolation instead."
                    .to_string(),
                *span,
                "use a backtick prompt instead",
            ),
            LoweringDiagnostic::QuotedPromptInterpolation { span } => (
                DiagnosticId::InvalidSyntax,
                // Advisory: the prompt is well-formed and renders, it just
                // renders the marker verbatim. Erroring would reject prompts
                // that legitimately want a literal `${`.
                Severity::Warning,
                r#"`${...}` in a quoted prompt is sent to the model as literal text — `"..."` strings do not interpolate. Use a backtick prompt to interpolate, or write `\${...}` in a backtick prompt to keep the literal `${`."#
                    .to_string(),
                *span,
                "literal text here, not an interpolation",
            ),
            LoweringDiagnostic::TemplateStringRemoved { span } => (
                DiagnosticId::InvalidSyntax,
                Severity::Error,
                "`template_string` declarations are no longer supported. Use a function returning a backtick string instead."
                    .to_string(),
                *span,
                "use a function returning a backtick string instead",
            ),
            LoweringDiagnostic::InvalidLlmClient {
                function_name,
                reason,
                span,
            } => (
                DiagnosticId::InvalidSyntax,
                Severity::Error,
                format!("LLM function `{function_name}` has an unusable `client`: {reason}"),
                *span,
                "use a \"provider/model\" string or an expression evaluating to ai.Client",
            ),
            LoweringDiagnostic::LlmRoleMarkerRemoved {
                function_name,
                span,
            } => (
                DiagnosticId::InvalidSyntax,
                Severity::Error,
                format!(
                    "`${{role(...)}}` markers are removed; the prompt of `{function_name}` is \
                     instructions-only — the conversation lives in the journal, and message \
                     roles are the client's wire concern"
                ),
                *span,
                "delete the role marker; write plain instructions",
            ),
            LoweringDiagnostic::InvalidNumericLiteral { error, span } => {
                use baml_base::num_lit::IntLitError;
                (
                    DiagnosticId::InvalidNumericLiteral,
                    Severity::Error,
                    error.message(),
                    *span,
                    match error {
                        IntLitError::UppercaseBasePrefix { .. } => {
                            "base prefixes (`0x`, `0o`, `0b`) are lowercase"
                        }
                        IntLitError::NoDigits => "expected at least one digit",
                        IntLitError::InvalidDigits { .. } => "invalid digit",
                        IntLitError::TooLarge => {
                            "does not fit in `int`; use a `bigint` literal (`n` suffix) for arbitrary precision"
                        }
                    },
                )
            }
        };

        Diagnostic::new(id, severity, message)
            .with_primary(Span { file_id, range }, label)
            .with_phase(DiagnosticPhase::Hir)
    }
}
