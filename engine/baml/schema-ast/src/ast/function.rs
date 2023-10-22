use super::{
    traits::WithAttributes, Attribute, Comment, FieldArity, FieldType, Identifier, Span,
    WithDocumentation, WithIdentifier, WithSpan,
};

/// An opaque identifier for a value in an AST enum. Use the
/// `r#enum[enum_value_id]` syntax to resolve the id to an `ast::EnumValue`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FuncArguementId(pub u32);

impl FuncArguementId {
    /// Used for range bounds when iterating over BTreeMaps.
    pub const MIN: FuncArguementId = FuncArguementId(0);
    /// Used for range bounds when iterating over BTreeMaps.
    pub const MAX: FuncArguementId = FuncArguementId(u32::MAX);
}

impl std::ops::Index<FuncArguementId> for NamedFunctionArgList {
    type Output = (Identifier, FunctionArg);

    fn index(&self, index: FuncArguementId) -> &Self::Output {
        &self.args[index.0 as usize]
    }
}

impl std::ops::Index<FuncArguementId> for FunctionArg {
    type Output = FunctionArg;

    fn index(&self, index: FuncArguementId) -> &Self::Output {
        assert_eq!(index, FuncArguementId(0), "FunctionArg only has one arg");
        &self
    }
}

#[derive(Debug, Clone)]
pub struct FunctionArg {
    /// The arity of the field.
    pub arity: FieldArity,

    /// The field's type.
    pub field_type: FieldType,

    /// The location of this field in the text representation.
    pub(crate) span: Span,
}

#[derive(Debug, Clone)]
pub struct NamedFunctionArgList {
    pub(crate) documentation: Option<Comment>,
    pub args: Vec<(Identifier, FunctionArg)>,
    pub(crate) span: Span,
}

impl NamedFunctionArgList {
    pub fn iter_args(
        &self,
    ) -> impl ExactSizeIterator<Item = (FuncArguementId, &(Identifier, FunctionArg))> {
        self.args
            .iter()
            .enumerate()
            .map(|(id, arg)| (FuncArguementId(id as u32), arg))
    }
}

#[derive(Debug, Clone)]
pub enum FunctionArgs {
    Unnamed(FunctionArg),
    Named(NamedFunctionArgList),
}

/// A model declaration.
#[derive(Debug, Clone)]
pub struct Function {
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
    pub(crate) input: FunctionArgs,
    pub(crate) output: FunctionArgs,
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
}

impl Function {
    pub fn input(&self) -> &FunctionArgs {
        &self.input
    }
    pub fn output(&self) -> &FunctionArgs {
        &self.output
    }
}

impl WithIdentifier for Function {
    fn identifier(&self) -> &Identifier {
        &self.name
    }
}

impl WithSpan for Function {
    fn span(&self) -> &Span {
        &self.span
    }
}

impl WithAttributes for Function {
    fn attributes(&self) -> &[Attribute] {
        &self.attributes
    }
}

impl WithDocumentation for Function {
    fn documentation(&self) -> Option<&str> {
        self.documentation.as_ref().map(|doc| doc.text.as_str())
    }
}

impl WithSpan for NamedFunctionArgList {
    fn span(&self) -> &Span {
        &self.span
    }
}
impl WithSpan for FunctionArg {
    fn span(&self) -> &Span {
        &self.span
    }
}

impl WithDocumentation for NamedFunctionArgList {
    fn documentation(&self) -> Option<&str> {
        self.documentation.as_ref().map(|doc| doc.text.as_str())
    }
}

impl WithDocumentation for FunctionArg {
    fn documentation(&self) -> Option<&str> {
        None
    }
}
