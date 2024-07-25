use super::{
    traits::WithAttributes, Attribute, Comment, ConfigBlockProperty, FieldType, Identifier, Span,
    WithDocumentation, WithIdentifier, WithSpan,
};

/// An opaque identifier for a value in an AST enum. Use the
/// `r#enum[enum_value_id]` syntax to resolve the id to an `ast::EnumValue`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ArgumentId(pub u32);

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

impl std::ops::Index<FieldId> for ValueExp {
    type Output = ConfigBlockProperty;

    fn index(&self, index: FieldId) -> &Self::Output {
        &self.fields[index.0 as usize]
    }
}

impl ArgumentId {
    /// Used for range bounds when iterating over BTreeMaps.
    pub const MIN: ArgumentId = ArgumentId(0);
    /// Used for range bounds when iterating over BTreeMaps.
    pub const MAX: ArgumentId = ArgumentId(u32::MAX);
}

impl std::ops::Index<ArgumentId> for BlockArgList {
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

#[derive(Debug, Clone)]
pub struct BlockArgList {
    pub(crate) documentation: Option<Comment>,
    pub args: Vec<(Identifier, BlockArg)>,
    pub(crate) span: Span,
}
#[derive(Debug, Clone)]

pub enum SubValueExp {
    Function,
    Client,
    Generator,
    RetryPolicy,
    Test,
}

impl BlockArgList {
    pub fn iter_args(
        &self,
    ) -> impl ExactSizeIterator<Item = (ArgumentId, &(Identifier, BlockArg))> {
        self.args
            .iter()
            .enumerate()
            .map(|(id, arg)| (ArgumentId(id as u32), arg))
    }
}

#[derive(Debug, Clone)]
pub enum BlockArgs {
    Unnamed(BlockArg),
    Named(BlockArgList),
}

impl BlockArgs {
    pub fn flat_idns(&self) -> Vec<&Identifier> {
        match self {
            BlockArgs::Unnamed(arg) => arg
                .field_type
                .flat_idns()
                .iter()
                .filter_map(|f| match f {
                    Identifier::Primitive(..) => None,
                    _ => Some(*f),
                })
                .collect(),
            BlockArgs::Named(named) => named
                .args
                .iter()
                .flat_map(|(_, arg)| arg.field_type.flat_idns())
                .filter_map(|f| match f {
                    Identifier::Primitive(..) => None,
                    _ => Some(f),
                })
                .collect(),
        }
    }
}

/// A model declaration.
#[derive(Debug, Clone)]
pub struct ValueExp {
    /// The name of the model.
    ///
    /// ```ignore
    /// function Foo { .. }
    ///       ^^^
    /// ```
    pub(crate) name: Identifier,
    /// The fields of the model.
    ///
    /// ```ignore
    /// model Foo {
    ///   id    Int    @id
    ///   ^^^^^^^^^^^^^^^^
    ///   field String
    ///   ^^^^^^^^^^^^
    /// }
    /// ```
    pub(crate) input: Option<BlockArgs>,
    pub(crate) output: Option<BlockArgs>,
    /// The documentation for this model.
    ///
    /// ```ignore
    /// /// Lorem ipsum
    ///     ^^^^^^^^^^^
    /// model Foo {
    ///   id    Int    @id
    ///   field String
    /// }
    /// ```
    pub(crate) documentation: Option<Comment>,
    /// The attributes of this model.
    ///
    /// ```ignore
    /// model Foo {
    ///   id    Int    @id
    ///   field String
    ///
    ///   @@index([field])
    ///   ^^^^^^^^^^^^^^^^
    ///   @@map("Bar")
    ///   ^^^^^^^^^^^^
    /// }
    /// ```
    pub attributes: Vec<Attribute>,
    /// The location of this model in the text representation.
    pub(crate) span: Span,
    pub fields: Vec<ConfigBlockProperty>,

    pub sub_type: SubValueExp,
}

impl ValueExp {
    pub fn input(&self) -> Option<&BlockArgs> {
        match &self.input {
            Some(input) => Some(input),
            None => None,
        }
    }
    pub fn output(&self) -> Option<&BlockArgs> {
        match &self.output {
            Some(output) => Some(output),
            None => None,
        }
    }

    pub fn iter_fields(
        &self,
    ) -> impl ExactSizeIterator<Item = (FieldId, &ConfigBlockProperty)> + Clone {
        self.fields
            .iter()
            .enumerate()
            .map(|(idx, field)| (FieldId(idx as u32), field))
    }

    pub fn fields(&self) -> &[ConfigBlockProperty] {
        &self.fields
    }
}

impl WithIdentifier for ValueExp {
    fn identifier(&self) -> &Identifier {
        &self.name
    }
}

impl WithSpan for ValueExp {
    fn span(&self) -> &Span {
        &self.span
    }
}

impl WithAttributes for ValueExp {
    fn attributes(&self) -> &[Attribute] {
        &self.attributes
    }
}

impl WithDocumentation for ValueExp {
    fn documentation(&self) -> Option<&str> {
        self.documentation.as_ref().map(|doc| doc.text.as_str())
    }
}

impl WithSpan for BlockArgList {
    fn span(&self) -> &Span {
        &self.span
    }
}
impl WithSpan for BlockArg {
    fn span(&self) -> &Span {
        &self.span
    }
}

impl WithDocumentation for BlockArgList {
    fn documentation(&self) -> Option<&str> {
        self.documentation.as_ref().map(|doc| doc.text.as_str())
    }
}

impl WithDocumentation for BlockArg {
    fn documentation(&self) -> Option<&str> {
        None
    }
}
