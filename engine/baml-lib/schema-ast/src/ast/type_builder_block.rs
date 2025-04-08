use internal_baml_diagnostics::Span;

use super::{Assignment, TypeExpressionBlock};

// TODO: #1343 Temporary solution until we implement scoping in the AST.
pub const DYNAMIC_TYPE_NAME_PREFIX: &str = "Dynamic::";

/// Blocks allowed in `type_builder` blocks.
#[derive(Debug, Clone)]
pub enum TypeBuilderEntry {
    /// An enum declaration.
    Enum(TypeExpressionBlock),
    /// A class declaration.
    Class(TypeExpressionBlock),
    /// Type alias expression.
    TypeAlias(Assignment),
    /// Dynamic block.
    Dynamic(TypeExpressionBlock),
}

/// The `type_builder` block.
///
/// ```ignore
/// test SomeTest {
///     type_builder {
///         // Contents
///     }
/// }
/// ```
#[derive(Debug, Clone)]
pub struct TypeBuilderBlock {
    pub entries: Vec<TypeBuilderEntry>,
    pub span: Span,
}
