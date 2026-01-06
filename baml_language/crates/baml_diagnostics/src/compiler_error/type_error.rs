// ============================================================================
// Type Errors
// ============================================================================
//
use baml_base::Span;

/// Type errors that can occur during type checking.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TypeError<T> {
    /// Type mismatch between expected and found types.
    ///
    /// - `span`: Location of the expression with the wrong type
    /// - `info_span`: Optional location of the type constraint source (e.g., return type annotation)
    TypeMismatch {
        expected: T,
        found: T,
        span: Span,
        info_span: Option<Span>,
    },
    /// Reference to an unknown type name.
    UnknownType { name: String, span: Span },
    /// Reference to an unknown variable.
    UnknownVariable { name: String, span: Span },
    /// Invalid binary operation.
    InvalidBinaryOp {
        op: String,
        lhs: T,
        rhs: T,
        span: Span,
    },
    /// Invalid unary operation.
    InvalidUnaryOp { op: String, operand: T, span: Span },
    /// Wrong number of arguments in function call.
    ArgumentCountMismatch {
        expected: usize,
        found: usize,
        span: Span,
    },
    /// Calling a non-callable type.
    NotCallable { ty: T, span: Span },
    /// Field access on non-class type.
    NoSuchField { ty: T, field: String, span: Span },
    /// Index access on non-indexable type.
    NotIndexable { ty: T, span: Span },
    /// Match expression is not exhaustive - some cases are not covered.
    NonExhaustiveMatch {
        scrutinee_type: T,
        missing_cases: Vec<String>,
        span: Span,
    },
    /// Match arm is unreachable - it can never match because previous arms cover all cases.
    UnreachableArm { span: Span },
    /// Reference to an unknown enum variant.
    UnknownEnumVariant {
        enum_name: String,
        variant_name: String,
        span: Span,
    },
    /// Using $watch on a non-variable expression (e.g., `arr[0].$watch`).
    WatchOnNonVariable { span: Span },
    /// Using $watch on a variable not declared with `watch let`.
    WatchOnUnwatchedVariable { name: String, span: Span },
}

impl<T> TypeError<T> {
    /// Map a function over the type parameter, transforming `TypeError<T>` to `TypeError<U>`.
    pub fn fmap<U, F: Fn(&T) -> U>(&self, f: F) -> TypeError<U> {
        match self {
            TypeError::TypeMismatch {
                expected,
                found,
                span,
                info_span,
            } => TypeError::TypeMismatch {
                expected: f(expected),
                found: f(found),
                span: *span,
                info_span: *info_span,
            },
            TypeError::UnknownType { name, span } => TypeError::UnknownType {
                name: name.clone(),
                span: *span,
            },
            TypeError::UnknownVariable { name, span } => TypeError::UnknownVariable {
                name: name.clone(),
                span: *span,
            },
            TypeError::InvalidBinaryOp { op, lhs, rhs, span } => TypeError::InvalidBinaryOp {
                op: op.clone(),
                lhs: f(lhs),
                rhs: f(rhs),
                span: *span,
            },
            TypeError::InvalidUnaryOp { op, operand, span } => TypeError::InvalidUnaryOp {
                op: op.clone(),
                operand: f(operand),
                span: *span,
            },
            TypeError::ArgumentCountMismatch {
                expected,
                found,
                span,
            } => TypeError::ArgumentCountMismatch {
                expected: *expected,
                found: *found,
                span: *span,
            },
            TypeError::NotCallable { ty, span } => TypeError::NotCallable {
                ty: f(ty),
                span: *span,
            },
            TypeError::NoSuchField { ty, field, span } => TypeError::NoSuchField {
                ty: f(ty),
                field: field.clone(),
                span: *span,
            },
            TypeError::NotIndexable { ty, span } => TypeError::NotIndexable {
                ty: f(ty),
                span: *span,
            },
            TypeError::NonExhaustiveMatch {
                scrutinee_type,
                missing_cases,
                span,
            } => TypeError::NonExhaustiveMatch {
                scrutinee_type: f(scrutinee_type),
                missing_cases: missing_cases.clone(),
                span: *span,
            },
            TypeError::UnreachableArm { span } => TypeError::UnreachableArm { span: *span },
            TypeError::UnknownEnumVariant {
                enum_name,
                variant_name,
                span,
            } => TypeError::UnknownEnumVariant {
                enum_name: enum_name.clone(),
                variant_name: variant_name.clone(),
                span: *span,
            },
            TypeError::WatchOnNonVariable { span } => TypeError::WatchOnNonVariable { span: *span },
            TypeError::WatchOnUnwatchedVariable { name, span } => {
                TypeError::WatchOnUnwatchedVariable {
                    name: name.clone(),
                    span: *span,
                }
            }
        }
    }
}
