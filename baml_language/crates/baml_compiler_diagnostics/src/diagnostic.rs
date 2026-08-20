//! Unified diagnostic type for all BAML compiler phases.
//!
//! This module provides a single `Diagnostic` type that can represent any
//! compiler error across all phases (parsing, HIR lowering, type checking).
//! This enables centralized rendering and consistent error handling.

use std::borrow::Cow;

use baml_base::{FileId, Span};
use borsh::{BorshDeserialize, BorshSerialize};

use crate::message::{DiagnosticMessageHighlight, DiagnosticText};

// ============================================================================
// DiagnosticPhase - Tracks which compiler phase produced a diagnostic
// ============================================================================

/// The compiler phase that produced a diagnostic.
///
/// This enables grouping diagnostics by phase for display purposes
/// (e.g., in `baml_tests` snapshots).
///
/// The Borsh derives serialize the variant as a declaration-order
/// discriminant for the per-file diagnostics cache; reordering variants is a
/// wire-format break gated by the cache's `FORMAT_VERSION`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, BorshSerialize, BorshDeserialize)]
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

/// Unique identifier for a diagnostic category.
///
/// The Borsh derives serialize the variant as a declaration-order
/// discriminant for the per-file diagnostics cache; reordering variants is a
/// wire-format break gated by the cache's `FORMAT_VERSION`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, BorshSerialize, BorshDeserialize)]
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
    DuplicateMethod,
    DuplicateBinding,
    RefutablePatternInLet,
    /// `if let` pattern that always matches — the `else` branch is dead.
    IrrefutablePatternInIfLet,
    /// `let … else` whose else block has a non-`!` type. The else must
    /// diverge (return / throw / break / continue / panic / infinite loop)
    /// so that fall-through past the binding is unreachable.
    LetElseMustDiverge,
    /// `let … else` pattern that covers every value — the else branch is
    /// dead. Suggest replacing with a plain `let` binding.
    IrrefutablePatternInLetElse,
    /// `while let` pattern that always matches — the loop never exits via
    /// pattern failure. Suggest a plain `while`/`loop`.
    IrrefutablePatternInWhileLet,
    DuplicateAttribute,
    UnknownAttribute,
    InvalidAttributeContext,
    /// A `generator { … }` block was found in `.baml`; code generators are
    /// now configured in `baml.toml` under `[generator.<name>]`.
    GeneratorBlockUnsupported,
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
    /// `remap_roles` must be a map.
    RemapRolesNotMap,
    /// `remap_role` values must be strings.
    RemapRoleValueNotString,
    /// Invalid role not in `allowed_roles` list.
    RemapRoleNotAllowed,
    /// `allowed_roles` must not be empty.
    AllowedRolesEmpty,
    /// `allowed_roles` values must be strings.
    AllowedRoleNotString,
    /// Unknown provider in client definition.
    UnknownProvider,
    /// Missing required client option(s).
    MissingClientOptions,
    /// Composite client has empty strategy.
    EmptyStrategy,
    /// Unknown retry policy reference.
    UnknownRetryPolicy,
    /// Strategy array element is not a valid client name.
    InvalidStrategyElement,

    // Pattern matching errors (E0062-E0066)
    NonExhaustiveMatch,
    UnreachableArm,
    NonExhaustiveCatch,
    UnreachableCatchArm,
    UnknownEnumVariant,

    // Control-flow diagnostics (E0146)
    UnreachableCode,

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

    // Integer literal out of `int` (i63) range (E0139)
    IntegerLiteralOutOfRange,

    // Map type errors (E0039)
    InvalidMapArity,

    // Test diagnostics (E0034-E0036, E0088)
    UnknownTestProperty,
    MissingTestProperty,
    TestFieldAttribute,
    UnknownFunctionInTest,

    // Reserved prefix diagnostics
    ReservedStreamPrefix,

    // Cycle detection diagnostics (E0068-E0069)
    AliasCycle,
    ClassCycle,

    // Catch binding errors (E0093)
    InvalidCatchBindingType,

    // Throws contract errors (E0096-E0097)
    ThrowsContractViolation,
    ThrowsContractExtraneous,

    // VIR lowering errors (E0089)
    LoweringError,

    // Removed feature errors (E0098) — shared by all removed-syntax
    // diagnostics (`instanceof`, legacy BEP-066 TypeBuilder syntax, ...).
    RemovedFeature,

    // Namespace diagnostics (E0099)
    NamespaceShadow,

    // Lowering diagnostics (E0101-E0105)
    MissingName,
    UnparseableType,
    MissingConfigBlock,
    MissingConfigKey,
    MalformedAttribute,

    // Attribute disambiguation (E0106)
    FieldAttributeInTypePosition,

    // Byte string literal errors (E0109)
    InvalidByteStringEscape,

    // Void type position errors (E0110)
    VoidInNonReturnPosition,

    // Wildcard `_` type in a non-inferable position (E0147)
    WildcardTypeNotAllowed,

    // Interface diagnostics (BEP-044)
    /// `implements I {}` references an interface that does not exist.
    UnknownInterface,
    /// A class is missing the body of a required interface method.
    MissingInterfaceMethod,
    /// A class implements the same interface in two blocks.
    DuplicateImplementsBlock,
    /// A method declared in an `implements` block does not exist on the target interface.
    UnknownInterfaceMember,
    /// A class field's type does not match the interface field it satisfies.
    InterfaceFieldTypeMismatch,
    /// Two interfaces contribute conflicting types for the same field.
    ConflictingInterfaceFieldTypes,
    /// An interface's `extends` chain forms a cycle.
    InterfaceExtendsCycle,
    /// `extends Foo` references a type that is not an interface.
    NotAnInterface,
    /// A method body in `implements I {}` has a signature that doesn't match
    /// the interface's declared signature for that method.
    InterfaceMethodSignatureMismatch,
    /// A `$rust_io_function` (sys-op) method in an `implements` block declares
    /// its own generic parameters. Such a method is reached only through
    /// interface (virtual) dispatch, which cannot carry the sys-op's
    /// method-level type arguments.
    GenericSysOpMethodInInterfaceImpl,
    /// Two `implements` blocks on the same class declare methods with the
    /// same name — unqualified calls would be ambiguous (BEP-044
    /// §"Method Disambiguation").
    AmbiguousInterfaceMethod,
    /// An interface's `extends` list inherits conflicting types for the same
    /// field from two parent interfaces.
    InterfaceExtendsFieldConflict,
    /// `default.method()` references a required method that has no default body.
    DefaultOnRequiredMethod,
    /// Bare `default` used as a value rather than `default.method(...)`.
    BareDefaultKeyword,
    /// An out-of-body `implements<T> …` declares a generic parameter not
    /// determined by the implementor (`for`) type — an unconstrained/phantom param.
    UnconstrainedImplTypeParam,
    /// `Self` used in an interface FIELD type (only valid in method signatures).
    SelfInInterfaceField,
    /// Bare `Self` used in an associated type's DEFAULT. `Self` is universal, so it
    /// cannot be resolved where the interface is used as an interface-existential
    /// type (the implementor is hidden).
    SelfInAssociatedTypeDefault,
    /// An `implements … for <target>` whose `for` target is not a single concrete
    /// type — a union, optional, interface ("dyn"), or `unknown`. Interfaces can
    /// only be implemented for a concrete type (or a concrete type constructor
    /// such as `T[]` / `map<K, V>`, or a blanket type parameter).
    ImplTargetNotConcrete,
    /// `return`/`break`/`continue` inside a `defer` body that would escape the
    /// defer (BEP-042). Only `throw` may leave a defer.
    DeferControlFlowEscape,

    /// An out-of-body `implement<P..> I<args..> for T` violates the orphan rule
    /// (BEP-044, Rust's RFC 2451 "covered" rule): the interface is foreign and no
    /// type local to this package appears in `[T, args..]` before any uncovered
    /// type parameter.
    ImplViolatesOrphanRule,
    /// An `implements` block is missing a required interface field.
    MissingInterfaceField,
    /// A class implements an interface that `requires` other interfaces,
    /// but doesn't explicitly implement them.
    MissingRequiredInterface,
    /// Top-level `implements I for T` attempted to implement an interface
    /// that declares fields.
    OutOfBodyImplementsFieldInterface,
    /// A field declaration appeared inside an `implements` block.
    InterfaceFieldDeclaredInImplementsBlock,
    /// The left side of `field as class_field` is not an interface field.
    UnknownInterfaceFieldLink,
    /// The right side of `field as class_field` is not a class field.
    UnknownClassFieldInInterfaceLink,
    /// The same interface field is linked more than once in one impl block.
    DuplicateInterfaceFieldLink,
    /// Interface field access is ambiguous.
    AmbiguousInterfaceField,
    /// Two interface implementation rules can apply to the same receiver/interface.
    OverlappingImplements,
    /// An interface `requires` a type that is not an interface (e.g. a class or enum).
    InterfaceRequiresNonInterface,
    /// A generic parameter's bound (`T extends X`) is not an interface. Bounds
    /// must be interfaces (BEP-044).
    GenericBoundNotInterface,
    /// A class declares a `to_string` method directly; it must be provided by
    /// implementing the `baml.ToString` interface instead.
    ToStringMustImplementInterface,
    /// A class declares a `to_json` method directly; it must be provided by
    /// implementing the `baml.ToJson` interface instead.
    ToJsonMustImplementInterface,
    /// A class declares a `from_json` method directly; it must be provided by
    /// implementing the `baml.FromJson` interface instead.
    FromJsonMustImplementInterface,
    /// A class declares a `cleanup` method whose signature is not the reserved
    /// magic-finalizer shape `cleanup(self) -> void` (BEP-042).
    CleanupMagicMethodSignature,

    // Aliasing lints (E0148)
    /// `baml.Array.filled(n, value)` was called with a mutable literal (`[]`,
    /// `{}`, or a class-instance literal). Every slot aliases the *same* object
    /// reference, so mutating one slot mutates all of them (Linear B-548). This
    /// is a lint (warning), not a type error.
    ArrayFilledAliasing,

    // Function-type throws requirement (E0151)
    /// A function type in a position where its error type cannot be inferred
    /// (type alias, class field, `let` annotation, nested or return position)
    /// omits its `throws` clause (`TYPE_SYSTEM.md` rule 5). Only an immediate
    /// callback parameter of a function declaration may omit it (rule 4).
    FunctionTypeMissingThrows,

    // Serialized-key collision (E0149)
    /// Two or more fields of a class serialize to the same JSON key — either two
    /// fields share an `@alias`, or one field's name equals another field's
    /// `@alias`. Such a schema is unsatisfiable: an aliased field's real name is
    /// never matched, so `ctx.output_format` renders duplicate keys and a
    /// required shadowed field can never be parsed (Linear B-615).
    DuplicateFieldAlias,

    // Numeric literal validation (E0152)
    /// A numeric literal token failed validation in `baml_base::num_lit`:
    /// uppercase base prefix (`0X1F`), no digits after the prefix (`0x`),
    /// a digit invalid for the base (`0b12`), or an integer literal whose
    /// magnitude exceeds `i64::MAX`.
    InvalidNumericLiteral,

    // Builtin interfaces (BEP-062, E0153/E0154)
    /// An `implements` block targets a compiler-builtin interface
    /// (`baml.AnyFunction`), whose conformance is derived by the compiler
    /// (every function type implements it) and cannot be written by hand.
    BuiltinInterfaceNotImplementable,
    /// A generic parameter's bound (`T extends X`) names a compiler-builtin
    /// interface (`baml.AnyFunction`) that is only legal as a value type
    /// (an existential), never as a bound.
    BuiltinInterfaceNotABound,

    // Mounted packages (BEP-066 mounted-package linking, E0158)
    /// A call whose callee resolves into a MOUNTED (source-less) dependency
    /// package. References type from the mounted interface; callables without
    /// a loc-free bytecode link contract report this diagnostic.
    MountedPackageCallUnsupported,

    // Projection bases (E0156)
    /// The dotted projection shorthand (`Base.Member`) was written with the
    /// interface itself as the base (`Iterator.Element`). A projection's base
    /// is an implementor type, a bounded type variable, or `Self` — naming
    /// the interface explicitly takes a qualified projection
    /// (`(Base as Iterator).Element`). Rust's E0223 analog.
    InterfaceProjectionBase,

    // Reflection render diagnostics (BEP-066, E0159+).
    /// An enum definition reached an LLM schema boundary without any values.
    /// Empty enums are legal declarations/constructions, but have no output
    /// representation and therefore fail at render time (BEP-066 R-4).
    EmptyEnumAtRender,
    /// An interface method (required or default) omits its `throws` clause.
    InterfaceMethodMissingThrows,

    /// A runtime reflection union constructor received no members. Static
    /// source cannot spell this defect, so BEP-066 reserves a surface code.
    RuntimeEmptyUnion,

    /// An interface-typed occurrence reached an LLM output schema renderer.
    OpenInterfaceAtRender,

    /// Two non-equivalent definitions with the same displayed qualified name
    /// reached one LLM render/parse context.
    ConflictingTypeDefinitionAtRender,
    /// A top-level declaration ($init) can reach a yielding io sysop.
    InitIoNotAllowed,

    /// A non-data type reached an LLM output schema renderer. These types are
    /// valid in BAML's type system but have no output-format representation.
    NonDataTypeAtRender,
    /// Reflection attempted to extract or dynamically invoke a generic
    /// callable without a complete runtime type-argument frame.
    UnspecializedReflectedGeneric,
    /// A class literal named one of the builtin companion carriers
    /// (`baml.Int`, `baml.Map`, …). They exist to hang methods on a builtin
    /// type, never to be instantiated.
    CannotConstructBuiltinCompanion,
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
            DiagnosticId::DuplicateMethod => "E0093",
            DiagnosticId::DuplicateBinding => "E0094",
            DiagnosticId::RefutablePatternInLet => "E0111",
            DiagnosticId::IrrefutablePatternInIfLet => "E0112",
            DiagnosticId::LetElseMustDiverge => "E0113",
            DiagnosticId::IrrefutablePatternInLetElse => "E0114",
            DiagnosticId::IrrefutablePatternInWhileLet => "E0137",
            DiagnosticId::DuplicateAttribute => "E0014",
            DiagnosticId::UnknownAttribute => "E0015",
            DiagnosticId::InvalidAttributeContext => "E0016",
            DiagnosticId::GeneratorBlockUnsupported => "E0017",
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
            DiagnosticId::RemapRolesNotMap
            | DiagnosticId::RemapRoleValueNotString
            | DiagnosticId::RemapRoleNotAllowed
            | DiagnosticId::AllowedRolesEmpty
            | DiagnosticId::AllowedRoleNotString => "E0044",
            DiagnosticId::UnknownProvider => "E0102",
            DiagnosticId::MissingClientOptions => "E0101",
            DiagnosticId::EmptyStrategy => "E0090",
            DiagnosticId::UnknownRetryPolicy => "E0091",
            DiagnosticId::InvalidStrategyElement => "E0092",

            // Pattern matching errors
            DiagnosticId::NonExhaustiveMatch => "E0062",
            DiagnosticId::UnreachableArm => "E0063",
            DiagnosticId::NonExhaustiveCatch => "E0094",
            DiagnosticId::UnreachableCatchArm => "E0095",
            DiagnosticId::UnknownEnumVariant => "E0064",

            // Control-flow diagnostics
            DiagnosticId::UnreachableCode => "E0146",

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
            // E0139 is the orphan-rule code (`ImplViolatesOrphanRule`, BEP-044); this
            // literal error previously collided with it — moved to the next free code.
            DiagnosticId::IntegerLiteralOutOfRange => "E0150",

            // Map type errors
            DiagnosticId::InvalidMapArity => "E0039",

            // Test diagnostics
            DiagnosticId::UnknownTestProperty => "E0034",
            DiagnosticId::MissingTestProperty => "E0035",
            DiagnosticId::TestFieldAttribute => "E0036",
            DiagnosticId::UnknownFunctionInTest => "E0088",

            // Cycle detection diagnostics
            DiagnosticId::AliasCycle => "E0068",
            DiagnosticId::ClassCycle => "E0069",

            // Reserved prefix errors
            DiagnosticId::ReservedStreamPrefix => "E0100",

            // Catch binding errors
            DiagnosticId::InvalidCatchBindingType => "E0093",

            // Throws contract errors
            DiagnosticId::ThrowsContractViolation => "E0096",
            DiagnosticId::ThrowsContractExtraneous => "E0097",

            // VIR lowering errors
            DiagnosticId::LoweringError => "E0089",

            // Removed feature errors
            DiagnosticId::RemovedFeature => "E0098",

            DiagnosticId::NamespaceShadow => "E0099",

            // Lowering diagnostics
            DiagnosticId::MissingName => "E0107",
            DiagnosticId::UnparseableType => "E0108",
            DiagnosticId::MissingConfigBlock => "E0103",
            DiagnosticId::MissingConfigKey => "E0104",
            DiagnosticId::MalformedAttribute => "E0105",

            // Attribute disambiguation
            DiagnosticId::FieldAttributeInTypePosition => "E0106",

            // Byte string literal errors
            DiagnosticId::InvalidByteStringEscape => "E0109",

            // Void type position errors
            DiagnosticId::VoidInNonReturnPosition => "E0110",
            DiagnosticId::WildcardTypeNotAllowed => "E0147",

            // Interface diagnostics
            DiagnosticId::UnknownInterface => "E0112",
            DiagnosticId::MissingInterfaceMethod => "E0113",
            DiagnosticId::DuplicateImplementsBlock => "E0114",
            DiagnosticId::UnknownInterfaceMember => "E0115",
            DiagnosticId::InterfaceFieldTypeMismatch => "E0116",
            DiagnosticId::ConflictingInterfaceFieldTypes => "E0117",
            DiagnosticId::InterfaceExtendsCycle => "E0118",
            DiagnosticId::NotAnInterface => "E0119",
            DiagnosticId::InterfaceMethodSignatureMismatch => "E0120",
            DiagnosticId::AmbiguousInterfaceMethod => "E0121",
            DiagnosticId::InterfaceExtendsFieldConflict => "E0122",
            DiagnosticId::DefaultOnRequiredMethod => "E0123",
            DiagnosticId::MissingInterfaceField => "E0124",
            DiagnosticId::MissingRequiredInterface => "E0125",
            DiagnosticId::OutOfBodyImplementsFieldInterface => "E0126",
            DiagnosticId::InterfaceFieldDeclaredInImplementsBlock => "E0127",
            DiagnosticId::UnknownInterfaceFieldLink => "E0128",
            DiagnosticId::UnknownClassFieldInInterfaceLink => "E0129",
            DiagnosticId::DuplicateInterfaceFieldLink => "E0130",
            DiagnosticId::AmbiguousInterfaceField => "E0131",
            DiagnosticId::OverlappingImplements => "E0132",
            DiagnosticId::InterfaceRequiresNonInterface => "E0133",
            DiagnosticId::BareDefaultKeyword => "E0134",
            DiagnosticId::UnconstrainedImplTypeParam => "E0135",
            DiagnosticId::SelfInInterfaceField => "E0136",
            // E0137 is taken by `IrrefutablePatternInWhileLet`; use the next free code.
            DiagnosticId::ImplTargetNotConcrete => "E0138",
            DiagnosticId::ImplViolatesOrphanRule => "E0139",
            DiagnosticId::ToStringMustImplementInterface => "E0140",
            DiagnosticId::SelfInAssociatedTypeDefault => "E0157",
            DiagnosticId::DeferControlFlowEscape => "E0141",
            DiagnosticId::ToJsonMustImplementInterface => "E0142",
            DiagnosticId::FromJsonMustImplementInterface => "E0143",
            DiagnosticId::CleanupMagicMethodSignature => "E0144",
            DiagnosticId::RuntimeEmptyUnion => "E0160",
            DiagnosticId::OpenInterfaceAtRender => "E0161",
            DiagnosticId::ConflictingTypeDefinitionAtRender => "E0162",
            DiagnosticId::InitIoNotAllowed => "E0163",
            DiagnosticId::NonDataTypeAtRender => "E0164",
            DiagnosticId::GenericBoundNotInterface => "E0145",
            DiagnosticId::GenericSysOpMethodInInterfaceImpl => "E0153",

            // Aliasing lints
            DiagnosticId::ArrayFilledAliasing => "E0148",

            // Serialized-key collision
            DiagnosticId::DuplicateFieldAlias => "E0149",

            // Function-type throws requirement
            DiagnosticId::FunctionTypeMissingThrows => "E0151",

            // Numeric literal validation
            DiagnosticId::InvalidNumericLiteral => "E0152",
            // BUG(e-code-collision): "E0153" is also assigned to
            // `GenericSysOpMethodInInterfaceImpl` above. One of the two needs
            // a fresh code; renumbering changes user-facing diagnostics, so it
            // deserves its own change.
            DiagnosticId::BuiltinInterfaceNotImplementable => "E0153",
            DiagnosticId::BuiltinInterfaceNotABound => "E0154",
            DiagnosticId::InterfaceProjectionBase => "E0156",
            DiagnosticId::MountedPackageCallUnsupported => "E0158",
            DiagnosticId::EmptyEnumAtRender => "E0159",
            // E0164 is owned by the non-data output-format diagnostic in #4470.
            DiagnosticId::UnspecializedReflectedGeneric => "E0165",
            DiagnosticId::CannotConstructBuiltinCompanion => "E0166",
            DiagnosticId::InterfaceMethodMissingThrows => "E0167",
        }
    }
}

/// Severity level of a diagnostic.
///
/// The Borsh derives serialize the variant as a declaration-order
/// discriminant for the per-file diagnostics cache; reordering variants is a
/// wire-format break gated by the cache's `FORMAT_VERSION`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, BorshSerialize, BorshDeserialize)]
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
    /// Styled ranges within `message`.
    pub message_highlights: Vec<DiagnosticMessageHighlight>,
    /// Whether this is the primary annotation.
    pub is_primary: bool,
}

impl Annotation {
    /// Create a primary annotation with a message.
    pub fn primary(span: Span, message: impl Into<DiagnosticText>) -> Self {
        let (message, message_highlights) = message.into().into_parts();
        Self {
            span,
            message: Some(message),
            message_highlights,
            is_primary: true,
        }
    }

    /// Create a primary annotation without a label.
    pub fn primary_span(span: Span) -> Self {
        Self {
            span,
            message: None,
            message_highlights: Vec::new(),
            is_primary: true,
        }
    }

    /// Create a secondary annotation with a message.
    pub fn secondary(span: Span, message: impl Into<DiagnosticText>) -> Self {
        let (message, message_highlights) = message.into().into_parts();
        Self {
            span,
            message: Some(message),
            message_highlights,
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
    /// Styled ranges within `message`.
    pub message_highlights: Vec<DiagnosticMessageHighlight>,
    /// Optional file path for display purposes.
    pub file_path: Option<String>,
}

impl RelatedInfo {
    /// Create a new related info with a span and message.
    pub fn new(span: Span, message: impl Into<DiagnosticText>) -> Self {
        let (message, message_highlights) = message.into().into_parts();
        Self {
            span,
            message,
            message_highlights,
            file_path: None,
        }
    }
}

/// A unified diagnostic that can represent any BAML compiler error.
///
/// This type is inspired by `ruff_db::Diagnostic` and enables:
/// - Centralized diagnostic collection via `Project::check()`
/// - Multi-format rendering (Miette for CLI, LSP types for editors)
/// - Consistent error handling across all compiler phases
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    /// The diagnostic category/id.
    pub id: DiagnosticId,
    /// The severity level.
    pub severity: Severity,
    /// The main error message.
    pub message: String,
    /// Styled ranges within `message`.
    pub message_highlights: Vec<DiagnosticMessageHighlight>,
    /// Annotations pointing to relevant source locations.
    pub annotations: Vec<Annotation>,
    /// Related information (e.g., "first defined here").
    pub related_info: Vec<RelatedInfo>,
    /// The compiler phase that produced this diagnostic.
    pub phase: DiagnosticPhase,
}

impl Diagnostic {
    /// Create a new diagnostic with a single primary span.
    pub fn new(id: DiagnosticId, severity: Severity, message: impl Into<DiagnosticText>) -> Self {
        let (message, message_highlights) = message.into().into_parts();
        Self {
            id,
            severity,
            message,
            message_highlights,
            annotations: Vec::new(),
            related_info: Vec::new(),
            phase: DiagnosticPhase::default(),
        }
    }

    /// Create an error diagnostic.
    pub fn error(id: DiagnosticId, message: impl Into<DiagnosticText>) -> Self {
        Self::new(id, Severity::Error, message)
    }

    /// Create a warning diagnostic.
    pub fn warning(id: DiagnosticId, message: impl Into<DiagnosticText>) -> Self {
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
    pub fn with_primary(mut self, span: Span, message: impl Into<DiagnosticText>) -> Self {
        self.annotations.push(Annotation::primary(span, message));
        self
    }

    /// Add a primary annotation at a span using the main message.
    #[must_use]
    pub fn with_primary_span(mut self, span: Span) -> Self {
        self.annotations.push(Annotation::primary_span(span));
        self
    }

    /// Add a secondary annotation at a span.
    #[must_use]
    pub fn with_secondary(mut self, span: Span, message: impl Into<DiagnosticText>) -> Self {
        self.annotations.push(Annotation::secondary(span, message));
        self
    }

    /// Add related information.
    #[must_use]
    pub fn with_related(mut self, span: Span, message: impl Into<DiagnosticText>) -> Self {
        self.related_info.push(RelatedInfo::new(span, message));
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

    /// Return the headline plus a distinct primary annotation label.
    pub fn message_with_primary_label(&self) -> Cow<'_, str> {
        self.annotations
            .iter()
            .find(|annotation| annotation.is_primary)
            .and_then(|annotation| annotation.message.as_deref())
            .filter(|message| *message != self.message)
            .map_or_else(
                || Cow::Borrowed(self.message.as_str()),
                |message| Cow::Owned(format!("{}: {message}", self.message)),
            )
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
    fn message_with_primary_label_preserves_span_detail() {
        let diag = Diagnostic::error(DiagnosticId::TypeMismatch, "mismatched types")
            .with_primary(test_span(), "expected `int`, found `string`");

        assert_eq!(
            diag.message_with_primary_label(),
            "mismatched types: expected `int`, found `string`"
        );
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
            DiagnosticId::LoweringError,
            DiagnosticId::RemovedFeature,
            DiagnosticId::NamespaceShadow,
        ];

        for id in ids {
            let code = id.code();
            assert!(code.starts_with('E'), "Code should start with E: {code}");
        }
    }

    #[test]
    fn borsh_discriminants_are_stable() {
        // The per-file diagnostics cache serializes these fieldless enums as a
        // declaration-order discriminant. A snapshot of the first few variants
        // guards against a silent reorder (which must instead bump the cache's
        // `FORMAT_VERSION`). Borsh writes enum discriminants as a single byte.
        assert_eq!(
            borsh::to_vec(&DiagnosticId::UnexpectedEof).unwrap(),
            vec![0]
        );
        assert_eq!(borsh::to_vec(&DiagnosticId::TypeMismatch).unwrap(), vec![3]);
        assert_eq!(borsh::to_vec(&Severity::Error).unwrap(), vec![0]);
        assert_eq!(borsh::to_vec(&Severity::Warning).unwrap(), vec![1]);
        assert_eq!(borsh::to_vec(&DiagnosticPhase::Parse).unwrap(), vec![0]);
        assert_eq!(borsh::to_vec(&DiagnosticPhase::Type).unwrap(), vec![3]);

        // Round-trip every representative id to prove deserialize is the inverse.
        for id in [
            DiagnosticId::TypeMismatch,
            DiagnosticId::DuplicateFieldAlias,
            DiagnosticId::OverlappingImplements,
        ] {
            let bytes = borsh::to_vec(&id).unwrap();
            assert_eq!(borsh::from_slice::<DiagnosticId>(&bytes).unwrap(), id);
        }
    }
}
