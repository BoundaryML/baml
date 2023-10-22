use std::collections::HashMap;

use super::{
    traits::WithAttributes, Attribute, Comment, FieldArity, FieldType, Identifier, Span,
    WithDocumentation, WithIdentifier, WithSpan,
};

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
