use super::{Expression, Span, WithSpan};
use std::fmt::{Display, Formatter};

/// An opaque identifier for a value in an AST enum. Use the
/// `r#enum[enum_value_id]` syntax to resolve the id to an `ast::EnumValue`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ArguementId(pub u32);

impl ArguementId {
    /// Used for range bounds when iterating over BTreeMaps.
    pub const MIN: ArguementId = ArguementId(0);
    /// Used for range bounds when iterating over BTreeMaps.
    pub const MAX: ArguementId = ArguementId(u32::MAX);
}

impl std::ops::Index<ArguementId> for ArgumentsList {
    type Output = Argument;

    fn index(&self, index: ArguementId) -> &Self::Output {
        &self.arguments[index.0 as usize]
    }
}

/// A list of arguments inside parentheses.
#[derive(Debug, Clone, Default)]
pub struct ArgumentsList {
    /// The arguments themselves.
    ///
    /// ```ignore
    /// @@index([a, b, c], map: "myidix")
    ///         ^^^^^^^^^^^^^^^^^^^^^^^^
    /// ```
    pub arguments: Vec<Argument>,
}

impl ArgumentsList {
    pub fn iter(&self) -> impl ExactSizeIterator<Item = (ArguementId, &Argument)> {
        self.arguments
            .iter()
            .enumerate()
            .map(|(idx, field)| (ArguementId(idx as u32), field))
    }
}

/// An argument, either for attributes or for function call expressions.
#[derive(Debug, Clone)]
pub struct Argument {
    /// The argument value.
    ///
    /// ```ignore
    /// @id("myIndex")
    ///     ^^^^^^^^^
    /// ```
    pub value: Expression,
    /// Location of the argument in the text representation.
    pub span: Span,
}

impl Argument {
    pub fn assert_eq_up_to_span(&self, other: &Argument) {
        assert_eq!(self.to_string(), other.to_string())
    }
}

impl Display for Argument {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        Display::fmt(&self.value, f)
    }
}

impl WithSpan for Argument {
    fn span(&self) -> &Span {
        &self.span
    }
}
