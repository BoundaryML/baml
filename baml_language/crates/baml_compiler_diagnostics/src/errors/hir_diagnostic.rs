// ============================================================================
// HIR Lowering Diagnostics
// ============================================================================

use baml_base::Span;

/// Diagnostics that occur during HIR lowering (per-file validation).
///
/// These are distinct from `NameError` which handles project-wide validation
/// like duplicate class/function names across files. `HirDiagnostic` handles
/// within-item validation like duplicate fields or attributes.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum HirDiagnostic {
    /// Duplicate field in a class.
    DuplicateField {
        class_name: String,
        field_name: String,
        first_span: Span,
        second_span: Span,
    },

    /// Duplicate method in a class.
    DuplicateMethod {
        class_name: String,
        method_name: String,
        first_span: Span,
        second_span: Span,
    },

    /// Duplicate let-binding in the same scope.
    DuplicateBinding {
        name: String,
        first_span: Span,
        second_span: Span,
    },

    /// Duplicate variant in an enum.
    DuplicateVariant {
        enum_name: String,
        variant_name: String,
        first_span: Span,
        second_span: Span,
    },

    /// Duplicate block attribute (@@) on an item.
    DuplicateBlockAttribute {
        item_kind: &'static str, // "class", "enum", "function"
        item_name: String,
        attr_name: String,
        first_span: Span,
        second_span: Span,
    },

    /// Duplicate field attribute (@) on a field/variant.
    DuplicateFieldAttribute {
        container_kind: &'static str, // "class", "enum"
        container_name: String,
        field_name: String,
        attr_name: String,
        first_span: Span,
        second_span: Span,
    },

    /// Unknown attribute.
    UnknownAttribute {
        attr_name: String,
        span: Span,
        valid_attributes: Vec<&'static str>,
    },

    /// Attribute used in wrong context.
    InvalidAttributeContext {
        attr_name: String,
        context: &'static str,          // "type alias", "test field"
        allowed_contexts: &'static str, // "class fields, enum variants"
        span: Span,
    },

    // ============ Generator Diagnostics ============
    /// Unknown property in generator block.
    UnknownGeneratorProperty {
        generator_name: String,
        property_name: String,
        span: Span,
        valid_properties: Vec<&'static str>,
    },

    /// Required property missing in generator.
    MissingGeneratorProperty {
        generator_name: String,
        property_name: &'static str,
        span: Span,
    },

    /// Invalid value for generator property.
    InvalidGeneratorPropertyValue {
        generator_name: String,
        property_name: String,
        value: String,
        span: Span,
        valid_values: Option<Vec<String>>,
        help: Option<String>,
    },

    // ============ Reserved Names Diagnostics ============
    /// Field/parameter name is a reserved keyword in target language.
    ReservedFieldName {
        item_kind: &'static str, // "class", "enum", "function"
        item_name: String,
        field_name: String,
        span: Span,
        target_languages: Vec<String>,
    },

    /// Field name matches its type name (Python-specific issue).
    FieldNameMatchesTypeName {
        class_name: String,
        field_name: String,
        type_name: String,
        span: Span,
    },

    // ============ Client Diagnostics ============
    /// Invalid value for `client_response_type`.
    InvalidClientResponseType {
        client_name: String,
        value: String,
        span: Span,
        valid_values: Vec<&'static str>,
    },

    /// http configuration must be a block, not a scalar value.
    HttpConfigNotBlock { client_name: String, span: Span },

    /// Unrecognized field in http configuration block.
    UnknownHttpConfigField {
        client_name: String,
        field_name: String,
        span: Span,
        suggestion: Option<String>,
        is_composite: bool,
    },

    /// Negative timeout value in http configuration.
    NegativeTimeout {
        client_name: String,
        field_name: String,
        value: i64,
        span: Span,
    },

    /// Missing required `provider` field in client definition.
    MissingProvider { client_name: String, span: Span },

    /// Unknown field in client configuration block.
    UnknownClientProperty {
        client_name: String,
        field_name: String,
        span: Span,
    },

    /// `remap_roles` must be a map/block, not a scalar value.
    RemapRolesNotMap {
        client_name: String,
        actual_type: String,
        span: Span,
    },

    /// `remap_role` values must be strings.
    RemapRoleValueNotString { client_name: String, span: Span },

    /// `remap_roles` key is not in `allowed_roles`.
    RemapRoleNotAllowed {
        client_name: String,
        role_key: String,
        allowed_roles: Vec<String>,
        span: Span,
    },

    /// `allowed_roles` must not be empty.
    AllowedRolesEmpty { client_name: String, span: Span },

    /// `allowed_roles` values must be strings.
    AllowedRoleNotString { client_name: String, span: Span },

    /// Composite client (fallback/round-robin) has no sub-clients in strategy.
    EmptyStrategy {
        client_name: String,
        provider: String,
        span: Span,
    },

    /// Client references a retry policy that doesn't exist.
    UnknownRetryPolicy {
        client_name: String,
        policy_name: String,
        span: Span,
    },

    /// Strategy array element is not a valid client name.
    InvalidStrategyElement { client_name: String, span: Span },

    // ============ Syntax Diagnostics ============
    /// Statement missing required semicolon.
    /// In Rust-style blocks, all statements except the final expression need semicolons.
    MissingSemicolon { span: Span },

    /// Missing return expression in function.
    MissingReturnExpression { span: Span },

    /// Missing parentheses around control flow condition.
    /// Conditions in if/while/for require parentheses: `if (cond)`, `while (cond)`, `for (...)`.
    MissingConditionParens {
        /// The kind of control flow: "if", "while", or "for"
        kind: &'static str,
        span: Span,
    },

    /// Unmatched closing delimiter (e.g., `)` without `(`, `}` without `{`, `]` without `[`).
    UnmatchedDelimiter {
        /// The unmatched token: ")", "}", or "]"
        token: &'static str,
        span: Span,
    },

    // ============ Constraint Attribute Diagnostics ============
    /// Invalid syntax for @check or @assert attribute.
    /// These require a Jinja expression block {{ }}.
    InvalidConstraintSyntax { attr_name: String, span: Span },

    // ============ Attribute Value Diagnostics ============
    /// Attribute requires a single string literal but received something else.
    /// Covers cases like:
    /// - `@alias(some_var)` - identifier instead of string
    /// - `@alias("a", "b")` - multiple arguments
    /// - `@alias()` - no arguments
    /// - `@alias({{ expr }})` - expression instead of string
    InvalidAttributeArg {
        attr_name: String,
        span: Span,
        /// Human-readable description of what was received
        received: String,
    },

    /// Attribute takes no arguments but received some (e.g., @@dynamic("unexpected")).
    UnexpectedAttributeArg { attr_name: String, span: Span },

    // ============ Type Diagnostics ============
    /// Float literal used as a type, which is not supported.
    UnsupportedFloatLiteral { value: String, span: Span },

    /// Invalid map type arity (wrong number of type parameters).
    /// Maps require exactly 2 type parameters: `map<KeyType, ValueType>`
    InvalidMapArity {
        expected: usize,
        found: usize,
        span: Span,
    },

    // ============ Test Diagnostics ============
    /// Unknown property in test block.
    UnknownTestProperty {
        test_name: String,
        property_name: String,
        span: Span,
        valid_properties: Vec<&'static str>,
    },

    /// Required property missing in test.
    MissingTestProperty {
        test_name: String,
        property_name: &'static str,
        span: Span,
    },

    /// Attribute used on test config item field (not allowed).
    TestFieldAttribute { attr_name: String, span: Span },

    // ============ Type Builder Diagnostics ============
    /// `type_builder` block found outside of test context (e.g., in function or client).
    TypeBuilderInNonTestContext {
        context: &'static str, // "function", "client", "generator"
        span: Span,
    },

    /// Multiple `type_builder` blocks in the same test.
    DuplicateTypeBuilderBlock {
        test_name: String,
        first_span: Span,
        second_span: Span,
    },

    /// Incomplete dynamic type definition (e.g., `dynamic Foo` without `class` or `enum`).
    IncompleteDynamicDefinition { span: Span },

    /// Syntax error in `type_builder` block content.
    TypeBuilderSyntaxError { message: String, span: Span },

    // ============ Reserved Prefix Diagnostics ============
    /// User-defined item uses the reserved `stream_` prefix.
    ReservedStreamPrefix {
        item_kind: &'static str, // "class", "type alias", "enum", "function"
        item_name: String,
        span: Span,
    },
}
