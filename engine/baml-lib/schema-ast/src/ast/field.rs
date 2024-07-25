use baml_types::TypeValue;
use internal_baml_diagnostics::DatamodelError;

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
    pub field_type: Option<FieldType>,
    /// The name of the field.
    ///
    /// ```ignore
    /// name String
    /// ^^^^
    /// ```
    pub(crate) name: Identifier,
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
    pub fn span_for_argument(&self, attribute: &str, _argument: &str) -> Option<Span> {
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
        self.name.name()
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
    Required,
    Optional,
}

impl FieldArity {
    pub fn is_optional(&self) -> bool {
        matches!(self, &FieldArity::Optional)
    }

    pub fn is_required(&self) -> bool {
        matches!(self, &FieldArity::Required)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum FieldType {
    Identifier(FieldArity, Identifier),
    // The second field is the number of dims for the list
    List(Box<FieldType>, u32, Span),
    Tuple(FieldArity, Vec<FieldType>, Span),
    // Unions don't have arity, as they are can be flattened.
    Union(FieldArity, Vec<FieldType>, Span),
    Map(Box<(FieldType, FieldType)>, Span),
}

impl FieldType {
    pub fn span(&self) -> &Span {
        match self {
            FieldType::Identifier(.., idn) => idn.span(),
            FieldType::Union(.., span) => span,
            FieldType::Tuple(.., span) => span,
            FieldType::Map(.., span) => span,
            FieldType::List(.., span) => span,
        }
    }

    pub fn to_nullable(&self) -> Result<Self, DatamodelError> {
        if self.is_nullable() {
            return Ok(self.to_owned());
        }
        match self {
            FieldType::Identifier(_arity, Identifier::Primitive(TypeValue::Null, _)) => {
                Ok(self.to_owned())
            }
            FieldType::Identifier(_arity, idn) => {
                Ok(FieldType::Identifier(FieldArity::Optional, idn.to_owned()))
            }
            FieldType::Union(arity, items, span) => {
                let mut items = items.clone();

                items.push(FieldType::Identifier(
                    FieldArity::Required,
                    Identifier::Primitive(TypeValue::Null, span.clone()),
                ));
                Ok(FieldType::Union(*arity, items, span.to_owned()))
            }
            FieldType::Tuple(_arity, options, span) => Ok(FieldType::Tuple(
                FieldArity::Optional,
                options.to_owned(),
                span.to_owned(),
            )),
            FieldType::Map(.., span) => Err(DatamodelError::new_validation_error(
                "Dictionaries can not be optional",
                span.clone(),
            )),
            FieldType::List(.., span) => Err(DatamodelError::new_validation_error(
                "Lists can not be optional",
                span.clone(),
            )),
        }
    }

    pub fn is_nullable(&self) -> bool {
        match self {
            FieldType::Identifier(arity, t) => {
                arity.is_optional() || matches!(t, Identifier::Primitive(TypeValue::Null, _))
            }
            FieldType::Union(arity, f, ..) => {
                arity.is_optional() || f.iter().any(|t| t.is_nullable())
            }
            FieldType::Tuple(arity, ..) => arity.is_optional(),
            // Lists can't be nullable
            FieldType::Map(_kv, _) => false,
            FieldType::List(_t, _, _) => false,
        }
    }

    // Whether the field could theoretically be made optional.
    pub fn can_be_null(&self) -> bool {
        match self {
            FieldType::Identifier(_arity, t) => match t {
                Identifier::Primitive(TypeValue::Null, _) => true,
                _ => true,
            },
            // There's a bug with unions where we cant parse optionals in unions right now
            FieldType::Union(_arity, _f, ..) => false,
            FieldType::Tuple(_arity, ..) => true,
            // Lists can't be nullable
            FieldType::Map(_kv, _) => false,
            FieldType::List(_t, _, _) => false,
        }
    }

    // All the identifiers used in this type.
    pub fn flat_idns(&self) -> Vec<&Identifier> {
        match self {
            FieldType::Identifier(.., idn) => vec![idn],
            FieldType::Union(_, f, ..) => f.iter().flat_map(|t| t.flat_idns()).collect(),
            FieldType::Tuple(_, f, ..) => f.iter().flat_map(|t| t.flat_idns()).collect(),
            FieldType::Map(kv, _) => {
                let mut idns = kv.1.flat_idns();
                idns.extend(kv.0.flat_idns());
                idns
            }
            FieldType::List(t, _, _) => t.flat_idns(),
        }
    }
}

// Impl display for FieldType
impl std::fmt::Display for FieldType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FieldType::Identifier(arity, idn) => {
                write!(
                    f,
                    "{}{}",
                    idn.name(),
                    if arity.is_optional() { "?" } else { "" }
                )
            }
            FieldType::Union(arity, ft, _) => {
                let mut ft = ft.iter().map(|t| t.to_string()).collect::<Vec<_>>();
                ft.sort();
                write!(
                    f,
                    "({}){}",
                    ft.join(" | "),
                    if arity.is_optional() { "?" } else { "" }
                )
            }
            FieldType::Tuple(arity, ft, _) => {
                let mut ft = ft.iter().map(|t| t.to_string()).collect::<Vec<_>>();
                ft.sort();
                write!(
                    f,
                    "({}){}",
                    ft.join(", "),
                    if arity.is_optional() { "?" } else { "" }
                )
            }
            FieldType::Map(kv, _) => write!(f, "map<{}, {}>", kv.0, kv.1),
            FieldType::List(t, _, _) => write!(f, "{}[]", t),
        }
    }
}
