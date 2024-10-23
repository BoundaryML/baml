use super::{
    traits::WithAttributes, Attribute, Comment, Expression, Field, FieldType, Identifier, Span,
    WithDocumentation, WithIdentifier, WithSpan,
};
use super::argument::ArgumentId;
use std::fmt::Display;
use std::fmt::Formatter;

/// An opaque identifier for a field in an AST model. Use the
/// `model[field_id]` syntax to resolve the id to an `ast::Field`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FieldId(pub u32);

impl FieldId {
    /// Used for range bounds when iterating over BTreeMaps.
    pub const MIN: FieldId = FieldId(0);
    /// Used for range bounds when iterating over BTreeMaps.
    pub const MAX: FieldId = FieldId(u32::MAX);
}

impl std::ops::Index<FieldId> for ValueExprBlock {
    type Output = Field<Expression>;

    fn index(&self, index: FieldId) -> &Self::Output {
        &self.fields[index.0 as usize]
    }
}

impl std::ops::Index<ArgumentId> for BlockArgs {
    type Output = (Identifier, BlockArg);

    fn index(&self, index: ArgumentId) -> &Self::Output {
        &self.args[index.0 as usize]
    }
}

impl std::ops::Index<ArgumentId> for BlockArg {
    type Output = BlockArg;

    fn index(&self, index: ArgumentId) -> &Self::Output {
        assert_eq!(index, ArgumentId(0), "BlockArg only has one arg");
        self
    }
}

#[derive(Debug, Clone)]
pub struct BlockArg {
    /// The field's type.
    pub field_type: FieldType,

    /// The location of this field in the text representation.
    pub(crate) span: Span,
}

impl BlockArg {
    pub fn name(&self) -> String {
        self.field_type.name()
    }
}
#[derive(Debug, Clone)]
pub struct BlockArgs {
    pub(crate) documentation: Option<Comment>,
    pub args: Vec<(Identifier, BlockArg)>,
    pub(crate) span: Span,
}

#[derive(Debug, Clone)]
pub enum ValueExprBlockType {
    Function,
    Client,
    Generator,
    RetryPolicy,
    Test,
}

impl Display for ValueExprBlockType {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            ValueExprBlockType::Function => write!(f, "function"),
            ValueExprBlockType::Client => write!(f, "client"),
            ValueExprBlockType::Generator => write!(f, "generator"),
            ValueExprBlockType::RetryPolicy => write!(f, "retry_policy"),
            ValueExprBlockType::Test => write!(f, "test"),
        }
    }
}

impl BlockArgs {
    pub fn flat_idns(&self) -> Vec<&Identifier> {
        self.args
            .iter()
            .flat_map(|(_, arg)| arg.field_type.flat_idns())
            .filter_map(|f| match f {
                Identifier::String(..) => None,
                _ => Some(f),
            })
            .collect()
    }
    pub fn iter_args(
        &self,
    ) -> impl ExactSizeIterator<Item = (ArgumentId, &(Identifier, BlockArg))> {
        self.args
            .iter()
            .enumerate()
            .map(|(id, arg)| (ArgumentId(id as u32), arg))
    }
}

/// A block declaration.
/// A complete Function, Client, Generator, Test, or RetryPolicy.
#[derive(Debug, Clone)]
pub struct ValueExprBlock {
    /// The name of the block.
    ///
    /// ```ignore
    /// function Foo(...) {...}
    ///          ^^^
    /// ```
    pub(crate) name: Identifier,
    /// The fields of the block.
    ///
    /// ```ignore
    /// class Foo {
    ///   id    Int    @id
    ///   ^^^^^^^^^^^^^^^^
    ///   field String
    ///   ^^^^^^^^^^^^
    /// }
    /// ```
    pub(crate) input: Option<BlockArgs>,
    pub(crate) output: Option<BlockArg>,
    /// The documentation for this block.
    ///
    /// ```ignore
    /// /// Lorem ipsum
    ///     ^^^^^^^^^^^
    /// class Foo {
    ///   id    Int    @id
    ///   field String
    /// }
    /// ```
    pub(crate) documentation: Option<Comment>,
    /// The attributes of this block.
    ///
    /// ```ignore
    /// class Foo {
    ///   id    Int    @id
    ///   field String
    ///
    ///   @@description("A Foo")
    ///   ^^^^^^^^^^^^^^^^^^^^^^
    ///   @@check({{ this.field|length < 100 }}, "Short field")
    ///   ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
    /// }
    /// ```
    pub attributes: Vec<Attribute>,
    /// The location of this block in the text representation.
    pub(crate) span: Span,
    pub fields: Vec<Field<Expression>>,

    pub block_type: ValueExprBlockType,
}

impl ValueExprBlock {
    pub fn input(&self) -> Option<&BlockArgs> {
        match &self.input {
            Some(input) => Some(input),
            None => None,
        }
    }
    pub fn output(&self) -> Option<&BlockArg> {
        match &self.output {
            Some(output) => Some(output),
            None => None,
        }
    }

    pub fn iter_fields(
        &self,
    ) -> impl ExactSizeIterator<Item = (FieldId, &Field<Expression>)> + Clone {
        self.fields
            .iter()
            .enumerate()
            .map(|(idx, field)| (FieldId(idx as u32), field))
    }

    pub fn fields(&self) -> &[Field<Expression>] {
        &self.fields
    }

    pub fn get_type(&self) -> &'static str {
        match &self.block_type {
            ValueExprBlockType::RetryPolicy => "retry_policy",
            ValueExprBlockType::Function => "function",
            ValueExprBlockType::Client => "client",
            ValueExprBlockType::Generator => "generator",
            ValueExprBlockType::Test => "test",
        }
    }
}

impl WithIdentifier for ValueExprBlock {
    fn identifier(&self) -> &Identifier {
        &self.name
    }
}

impl WithSpan for ValueExprBlock {
    fn span(&self) -> &Span {
        &self.span
    }
}

impl WithAttributes for ValueExprBlock {
    fn attributes(&self) -> &[Attribute] {
        &self.attributes
    }
}

impl WithDocumentation for ValueExprBlock {
    fn documentation(&self) -> Option<&str> {
        self.documentation.as_ref().map(|doc| doc.text.as_str())
    }
}

impl WithSpan for BlockArgs {
    fn span(&self) -> &Span {
        &self.span
    }
}
impl WithSpan for BlockArg {
    fn span(&self) -> &Span {
        &self.span
    }
}

impl WithDocumentation for BlockArgs {
    fn documentation(&self) -> Option<&str> {
        self.documentation.as_ref().map(|doc| doc.text.as_str())
    }
}

impl WithDocumentation for BlockArg {
    fn documentation(&self) -> Option<&str> {
        None
    }
}
