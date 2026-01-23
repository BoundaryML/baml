// ============================================================================
// Type Errors
// ============================================================================
//
use std::fmt::Debug;
use std::hash::Hash;

use baml_base::Span;

/// Context trait that ties together type and location representations.
///
/// Different compiler phases use different representations:
/// - HIR uses `TypeRef` for types and `Span` for locations
/// - TIR uses `Ty` for types and `ErrorLocation` (with ExprId etc.) for locations
///
/// By parameterizing `TypeError` over this trait, we can:
/// 1. Keep errors in a single enum definition
/// 2. Use position-independent IDs in TIR for Salsa cache stability
/// 3. Convert to Span-based errors only at diagnostic rendering time
pub trait ErrorContext: Debug + Clone + PartialEq + Eq + Hash {
    /// The type representation (e.g., `TypeRef` in HIR, `Ty` in TIR).
    type Ty: Debug + Clone + PartialEq + Eq + Hash;
    /// The location representation (e.g., `Span` in HIR, `ErrorLocation` in TIR).
    type Location: Debug + Clone + Copy + PartialEq + Eq + Hash;
}

/// Default error context using Span for locations.
///
/// This is used when we need a simple `TypeError` with spans,
/// such as in early compiler phases or for diagnostic output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SpanContext<T>(std::marker::PhantomData<T>);

impl<T: Debug + Clone + PartialEq + Eq + Hash> ErrorContext for SpanContext<T> {
    type Ty = T;
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

impl<C: ErrorContext> TypeError<C> {
    /// Transform this error to a different context by mapping types and locations.
    ///
    /// This is used to convert between compiler phases, e.g., from TIR errors
    /// (with `ErrorLocation`) to renderable errors (with `Span`).
    pub fn map_context<D, TyFn, LocFn>(&self, ty_fn: TyFn, loc_fn: LocFn) -> TypeError<D>
    where
        D: ErrorContext,
        TyFn: Fn(&C::Ty) -> D::Ty,
        LocFn: Fn(C::Location) -> D::Location,
    {
        match self {
            TypeError::TypeMismatch {
                expected,
                found,
                location,
                info_location,
            } => TypeError::TypeMismatch {
                expected: ty_fn(expected),
                found: ty_fn(found),
                location: loc_fn(*location),
                info_location: info_location.map(loc_fn),
            },
            TypeError::UnknownType { name, location } => TypeError::UnknownType {
                name: name.clone(),
                location: loc_fn(*location),
            },
            TypeError::UnknownVariable { name, location } => TypeError::UnknownVariable {
                name: name.clone(),
                location: loc_fn(*location),
            },
            TypeError::InvalidBinaryOp {
                op,
                lhs,
                rhs,
                location,
            } => TypeError::InvalidBinaryOp {
                op: op.clone(),
                lhs: ty_fn(lhs),
                rhs: ty_fn(rhs),
                location: loc_fn(*location),
            },
            TypeError::InvalidUnaryOp {
                op,
                operand,
                location,
            } => TypeError::InvalidUnaryOp {
                op: op.clone(),
                operand: ty_fn(operand),
                location: loc_fn(*location),
            },
            TypeError::ArgumentCountMismatch {
                expected,
                found,
                location,
            } => TypeError::ArgumentCountMismatch {
                expected: *expected,
                found: *found,
                location: loc_fn(*location),
            },
            TypeError::NotCallable { ty, location } => TypeError::NotCallable {
                ty: ty_fn(ty),
                location: loc_fn(*location),
            },
            TypeError::NoSuchField {
                ty,
                field,
                location,
            } => TypeError::NoSuchField {
                ty: ty_fn(ty),
                field: field.clone(),
                location: loc_fn(*location),
            },
            TypeError::NotIndexable { ty, location } => TypeError::NotIndexable {
                ty: ty_fn(ty),
                location: loc_fn(*location),
            },
            TypeError::NonExhaustiveMatch {
                scrutinee_type,
                missing_cases,
                location,
            } => TypeError::NonExhaustiveMatch {
                scrutinee_type: ty_fn(scrutinee_type),
                missing_cases: missing_cases.clone(),
                location: loc_fn(*location),
            },
            TypeError::UnreachableArm { location } => TypeError::UnreachableArm {
                location: loc_fn(*location),
            },
            TypeError::UnknownEnumVariant {
                enum_name,
                variant_name,
                location,
            } => TypeError::UnknownEnumVariant {
                enum_name: enum_name.clone(),
                variant_name: variant_name.clone(),
                location: loc_fn(*location),
            },
            TypeError::WatchOnNonVariable { location } => TypeError::WatchOnNonVariable {
                location: loc_fn(*location),
            },
            TypeError::WatchOnUnwatchedVariable { name, location } => {
                TypeError::WatchOnUnwatchedVariable {
                    name: name.clone(),
                    location: loc_fn(*location),
                }
            }
            TypeError::MissingReturnExpression { expected, location } => {
                TypeError::MissingReturnExpression {
                    expected: ty_fn(expected),
                    location: loc_fn(*location),
                }
            }
        }
    }
}

/// Convenience methods for SpanContext errors (the common case for diagnostics).
impl<T: Debug + Clone + PartialEq + Eq + Hash> TypeError<SpanContext<T>> {
    /// Map a function over the type parameter, keeping locations as Spans.
    ///
    /// This preserves the original `fmap` behavior for backward compatibility.
    pub fn fmap<U: Debug + Clone + PartialEq + Eq + Hash, F: Fn(&T) -> U>(
        &self,
        f: F,
    ) -> TypeError<SpanContext<U>> {
        self.map_context(f, |loc| loc)
    }
}
