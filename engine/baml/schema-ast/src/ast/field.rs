use super::{
    traits::WithAttributes, Attribute, Comment, Identifier, Span, WithDocumentation,
    WithIdentifier, WithName, WithSpan,
};

/// A field definition in a model or a composite type.
#[derive(Debug, Clone)]
pub struct Field {
    /// The field's type.
    ///
    /// ```ignore
    /// name String
    ///      ^^^^^^
    /// ```
    pub field_type: FieldType,
    /// The name of the field.
    ///
    /// ```ignore
    /// name String
    /// ^^^^
    /// ```
    pub(crate) name: Identifier,
    /// The arity of the field.
    pub arity: FieldArity,
    /// The comments for this field.
    ///
    /// ```ignore
    /// /// Lorem ipsum
    ///     ^^^^^^^^^^^
    /// name String @id @default("lol")
    /// ```
    pub(crate) documentation: Option<Comment>,
    /// The attributes of this field.
    ///
    /// ```ignore
    /// name String @id @default("lol")
    ///             ^^^^^^^^^^^^^^^^^^^
    /// ```
    pub attributes: Vec<Attribute>,
    /// The location of this field in the text representation.
    pub(crate) span: Span,
}

impl Field {
    /// Finds the position span of the argument in the given field attribute.
    pub fn span_for_argument(&self, attribute: &str, argument: &str) -> Option<Span> {
        self.attributes
            .iter()
            .filter(|a| a.name() == attribute)
            .flat_map(|a| a.arguments.iter())
            .map(|(_, a)| a.span.clone())
            .next()
    }

    /// Finds the position span of the given attribute.
    pub fn span_for_attribute(&self, attribute: &str) -> Option<Span> {
        self.attributes
            .iter()
            .filter(|a| a.name() == attribute)
            .map(|a| a.span.clone())
            .next()
    }

    /// The name of the field
    pub fn name(&self) -> &str {
        &self.name.name
    }
}

impl WithIdentifier for Field {
    fn identifier(&self) -> &Identifier {
        &self.name
    }
}

impl WithSpan for Field {
    fn span(&self) -> &Span {
        &self.span
    }
}

impl WithAttributes for Field {
    fn attributes(&self) -> &[Attribute] {
        &self.attributes
    }
}

impl WithDocumentation for Field {
    fn documentation(&self) -> Option<&str> {
        self.documentation.as_ref().map(|doc| doc.text.as_str())
    }
}

/// An arity of a data model field.
#[derive(Copy, Debug, Clone, PartialEq, Eq, Hash)]
pub enum FieldArity {
    /// The field either must be in an insert statement, or the field must have
    /// a default value for the insert to succeed.
    ///
    /// ```ignore
    /// name String
    /// ```
    Required,
    /// The field does not need to be in an insert statement for the write to
    /// succeed.
    ///
    /// ```ignore
    /// name String?
    /// ```
    Optional,
    /// The field can have multiple values stored in the same column.
    ///
    /// ```ignore
    /// name String[]
    /// ```
    List,
}

impl FieldArity {
    pub fn is_list(&self) -> bool {
        matches!(self, &FieldArity::List)
    }

    pub fn is_optional(&self) -> bool {
        matches!(self, &FieldArity::Optional)
    }

    pub fn is_required(&self) -> bool {
        matches!(self, &FieldArity::Required)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum TypeValue {
    String,
    Int,
    Float,
    Boolean,
    Char,
}

#[derive(Debug, Clone, PartialEq)]
pub enum FieldType {
    PrimitiveType(TypeValue, Span),
    Union(Vec<(FieldArity, FieldType)>, Span),
    Supported(Identifier),
    /// Unsupported("...")
    Unsupported(String, Span),
}

impl FieldType {
    pub fn span(&self) -> &Span {
        match self {
            FieldType::Union(_, span) => span,
            FieldType::PrimitiveType(_, span) => span,
            FieldType::Supported(ident) => &ident.span,
            FieldType::Unsupported(_, span) => span,
        }
    }

    pub fn as_unsupported(&self) -> Option<(&str, &Span)> {
        match self {
            FieldType::Unsupported(name, span) => Some((name, span)),
            FieldType::Union(_, _) => None,
            FieldType::PrimitiveType(_, _) => None,
            FieldType::Supported(_) => None,
        }
    }
}
