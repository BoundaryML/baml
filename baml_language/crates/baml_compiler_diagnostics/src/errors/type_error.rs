// ============================================================================
// Type Errors
// ============================================================================
//
use baml_base::Span;

/// Context trait that ties together type and location representations.
///
/// Different compiler phases use different representations:
/// - HIR uses `TypeRef` for types and `Span` for locations
/// - TIR uses `Ty` for types and `ErrorLocation` (with `ExprId` etc.) for locations
///
/// By parameterizing `TypeError` over this trait, we can:
/// 1. Keep errors in a single enum definition
/// 2. Use position-independent IDs in TIR for Salsa cache stability
/// 3. Convert to Span-based errors only at diagnostic rendering time
pub trait ErrorContext {
    /// The type representation (e.g., `TypeRef` in HIR, `Ty` in TIR).
    type Ty;
    /// The location representation (e.g., `Span` in HIR, `ErrorLocation` in TIR).
    type Location;
}

/// Default error context using Span for locations.
///
/// This is used when we need a simple `TypeError` with spans,
/// such as in early compiler phases or for diagnostic output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SpanContext;

impl ErrorContext for SpanContext {
    type Ty = String;
    type Location = Span;
}

/// Type errors that can occur during type checking.
///
/// Parameterized over an `ErrorContext` that determines both the type
/// representation and location representation. This enables:
/// - TIR to use position-independent IDs for Salsa cache stability
/// - Conversion to Span-based errors only at diagnostic rendering time
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TypeError<C: ErrorContext> {
    /// Type mismatch between expected and found types.
    ///
    /// - `location`: Location of the expression with the wrong type
    /// - `info_location`: Optional location of the type constraint source (e.g., return type annotation)
    TypeMismatch {
        expected: C::Ty,
        found: C::Ty,
        location: C::Location,
        info_location: Option<C::Location>,
    },
    /// Reference to an unknown type name.
    UnknownType { name: String, location: C::Location },
    /// Reference to an unknown variable.
    UnknownVariable { name: String, location: C::Location },
    /// Invalid binary operation.
    InvalidBinaryOp {
        op: String,
        lhs: C::Ty,
        rhs: C::Ty,
        location: C::Location,
    },
    /// Invalid unary operation.
    InvalidUnaryOp {
        op: String,
        operand: C::Ty,
        location: C::Location,
    },
    /// Wrong number of arguments in function call.
    ArgumentCountMismatch {
        expected: usize,
        found: usize,
        location: C::Location,
    },
    /// Calling a non-callable type.
    NotCallable { ty: C::Ty, location: C::Location },
    /// Field access on non-class type.
    NoSuchField {
        ty: C::Ty,
        field: String,
        location: C::Location,
    },
    /// Index access on non-indexable type.
    NotIndexable { ty: C::Ty, location: C::Location },
    /// Match expression is not exhaustive - some cases are not covered.
    NonExhaustiveMatch {
        scrutinee_type: C::Ty,
        missing_cases: Vec<String>,
        location: C::Location,
    },
    /// Match arm is unreachable - it can never match because previous arms cover all cases.
    UnreachableArm { location: C::Location },
    /// Reference to an unknown enum variant.
    UnknownEnumVariant {
        enum_name: String,
        variant_name: String,
        location: C::Location,
    },
    /// Using $watch on a non-variable expression (e.g., `arr[0].$watch`).
    WatchOnNonVariable { location: C::Location },
    /// Using $watch on a variable not declared with `watch let`.
    WatchOnUnwatchedVariable { name: String, location: C::Location },
    /// Function body has no return expression but requires a non-void return type.
    MissingReturnExpression {
        expected: C::Ty,
        location: C::Location,
    },
}
