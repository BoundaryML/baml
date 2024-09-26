use baml_types::TypeValue;
use internal_baml_diagnostics::DatamodelError;

use super::{
    traits::WithAttributes, Attribute, Comment, Identifier, Span, WithDocumentation,
    WithIdentifier, WithName, WithSpan,
};

/// A field definition in a model or a composite type.
#[derive(Debug, Clone)]
pub struct Field<T> {
    /// The field's type.
    ///
    /// ```ignore
    /// name String
    ///      ^^^^^^
    /// ```
    pub expr: Option<T>,
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

impl<T> Field<T> {
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

impl<T> WithIdentifier for Field<T> {
    fn identifier(&self) -> &Identifier {
        &self.name
    }
}

impl<T> WithSpan for Field<T> {
    fn span(&self) -> &Span {
        &self.span
    }
}

impl<T> WithAttributes for Field<T> {
    fn attributes(&self) -> &[Attribute] {
        &self.attributes
    }
}

impl<T> WithDocumentation for Field<T> {
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

#[derive(Debug, Clone)]
pub enum FieldType {
    Symbol(FieldArity, Identifier, Option<Vec<Attribute>>),
    Primitive(FieldArity, TypeValue, Span, Option<Vec<Attribute>>),
    // The second field is the number of dims for the list
    List(Box<FieldType>, u32, Span, Option<Vec<Attribute>>),
    Tuple(FieldArity, Vec<FieldType>, Span, Option<Vec<Attribute>>),
    // Unions don't have arity, as they can be flattened.
    Union(FieldArity, Vec<FieldType>, Span, Option<Vec<Attribute>>),
    Map(Box<(FieldType, FieldType)>, Span, Option<Vec<Attribute>>),
}

impl FieldType {
    pub fn name(&self) -> String {
        match self {
            FieldType::Symbol(_, name, ..) => name.name().to_string(),
            FieldType::Primitive(_, name, ..) => name.to_string(),
            _ => "Unknown".to_string(),
        }
    }

    pub fn span(&self) -> &Span {
        match self {
            FieldType::Primitive(.., span, _) => span,
            FieldType::Symbol(.., idn, _) => idn.span(),
            FieldType::Union(.., span, _) => span,
            FieldType::Tuple(.., span, _) => span,
            FieldType::Map(.., span, _) => span,
            FieldType::List(.., span, _) => span,
        }
    }

    pub fn to_nullable(&self) -> Result<Self, DatamodelError> {
        if self.is_nullable() {
            return Ok(self.to_owned());
        }
        match self {
            FieldType::Symbol(_arity, idn, attributes) => Ok(FieldType::Symbol(
                FieldArity::Optional,
                Identifier::Local(idn.name().to_string(), idn.span().clone()),
                attributes.to_owned(),
            )),
            FieldType::Primitive(_arity, type_value, span, attributes) => Ok(FieldType::Primitive(
                FieldArity::Optional,
                type_value.to_owned(),
                span.to_owned(),
                attributes.to_owned(),
            )),
            FieldType::Union(arity, items, span, attributes) => {
                let mut items = items.clone();

                items.push(FieldType::Primitive(
                    FieldArity::Required,
                    TypeValue::Null,
                    span.clone(),
                    None,
                ));
                Ok(FieldType::Union(
                    *arity,
                    items,
                    span.to_owned(),
                    attributes.to_owned(),
                ))
            }
            FieldType::Tuple(_arity, options, span, attributes) => Ok(FieldType::Tuple(
                FieldArity::Optional,
                options.to_owned(),
                span.to_owned(),
                attributes.to_owned(),
            )),
            FieldType::Map(_, span, _) => Err(DatamodelError::new_validation_error(
                "Dictionaries can not be optional",
                span.clone(),
            )),
            FieldType::List(_, _, span, _) => Err(DatamodelError::new_validation_error(
                "Lists can not be optional",
                span.clone(),
            )),
        }
    }

    pub fn is_nullable(&self) -> bool {
        match self {
            FieldType::Symbol(arity, ..) => arity.is_optional(),
            FieldType::Union(arity, f, _, _) => {
                arity.is_optional() || f.iter().any(|t| t.is_nullable())
            }
            FieldType::Tuple(arity, _, _, _) => arity.is_optional(),
            FieldType::Primitive(arity, _, _, _) => arity.is_optional(),
            // Lists can't be nullable
            FieldType::Map(_kv, _, _) => false,
            FieldType::List(_t, _, _, _) => false,
        }
    }
    // Whether the field could theoretically be made optional.
    pub fn can_be_null(&self) -> bool {
        match self {
            FieldType::Symbol(_arity, t, ..) => true,
            FieldType::Primitive(_arity, ..) => true,
            // There's a bug with unions where we cant parse optionals in unions right now
            FieldType::Union(_arity, _f, ..) => false,
            FieldType::Tuple(_arity, ..) => true,
            // Lists can't be nullable
            FieldType::Map(_kv, ..) => false,
            FieldType::List(_t, ..) => false,
        }
    }

    // All the identifiers used in this type.
    pub fn flat_idns(&self) -> Vec<&Identifier> {
        match self {
            FieldType::Symbol(_, idn, ..) => {
                vec![&idn]
            }

            FieldType::Union(_, f, ..) => f.iter().flat_map(|t| t.flat_idns()).collect(),
            FieldType::Tuple(_, f, ..) => f.iter().flat_map(|t| t.flat_idns()).collect(),
            FieldType::Map(kv, ..) => {
                let mut idns = kv.1.flat_idns();
                idns.extend(kv.0.flat_idns());
                idns
            }
            FieldType::List(t, ..) => t.flat_idns(),
            FieldType::Primitive(..) => vec![],
        }
    }

    pub fn attributes(&self) -> &[Attribute] {
        match self {
            FieldType::Symbol(.., attr)
            | FieldType::Primitive(.., attr)
            | FieldType::Union(.., attr)
            | FieldType::Tuple(.., attr)
            | FieldType::Map(.., attr)
            | FieldType::List(.., attr) => attr.as_deref().unwrap_or(&[]),
        }
    }

    pub fn reset_attributes(&mut self) {
        match self {
            FieldType::Symbol(.., attr)
            | FieldType::Primitive(.., attr)
            | FieldType::Union(.., attr)
            | FieldType::Tuple(.., attr)
            | FieldType::Map(.., attr)
            | FieldType::List(.., attr) => *attr = None,
        }
    }

    pub fn set_attributes(&mut self, attributes: Vec<Attribute>) {
        match self {
            FieldType::Symbol(.., attr)
            | FieldType::Primitive(.., attr)
            | FieldType::Union(.., attr)
            | FieldType::Tuple(.., attr)
            | FieldType::Map(.., attr)
            | FieldType::List(.., attr) => *attr = Some(attributes),
        }
    }

    pub fn extend_attributes(&mut self, attributes: Vec<Attribute>) {
        match self {
            FieldType::Symbol(.., attr)
            | FieldType::Primitive(.., attr)
            | FieldType::Union(.., attr)
            | FieldType::Tuple(.., attr)
            | FieldType::Map(.., attr)
            | FieldType::List(.., attr) => match attr.as_mut() {
                Some(ats) => ats.extend(attributes),
                None => { *attr = Some(attributes) }
            },
        }
    }

    pub fn eq_up_to_span(&self, other: &Self) {
        use FieldType::*;

        fn attrs_eq(attrs1: &Option<Vec<Attribute>>, attrs2: &Option<Vec<Attribute>>) {
            let attrs1 = attrs1.clone().unwrap_or(vec![]);
            let attrs2 = attrs2.clone().unwrap_or(vec![]);
            assert_eq!(
                attrs1.len(),
                attrs2.len(),
                "Attribute lengths are different"
            );
            for (x, y) in attrs1.iter().zip(attrs2) {
                x.eq_up_to_span(&y);
            }
        }
        match (self, other) {
            (Symbol(arity1, ident1, attrs1), Symbol(arity2, ident2, attrs2)) => {
                assert_eq!(arity1, arity2);
                assert_eq!(ident1, ident2);
                attrs_eq(attrs1, attrs2);
            }
            (Symbol(..), _) => {
                panic!(
                    "Different types:\n{}\n---\n{}",
                    self.to_string(),
                    other.to_string()
                )
            }
            (Primitive(arity1, prim_ty1, _, attrs1), Primitive(arity2, prim_ty2, _, attrs2)) => {
                assert_eq!(arity1, arity2);
                assert_eq!(prim_ty1, prim_ty2);
                attrs_eq(attrs1, attrs2);
            }
            (Primitive(..), _) => {
                panic!(
                    "Different types: \n{}\n---\n{}",
                    self.to_string(),
                    other.to_string()
                )
            }
            (List(inner1, dims1, _, attrs1), List(inner2, dims2, _, attrs2)) => {
                inner1.eq_up_to_span(inner2);
                assert_eq!(dims1, dims2);
                attrs_eq(attrs1, attrs2);
            }
            (List(..), _) => {
                panic!(
                    "Different types: \n{}\n---\n{}",
                    self.to_string(),
                    other.to_string()
                )
            }
            (Tuple(arity1, inner1, _, attrs1), Tuple(arity2, inner2, _, attrs2)) => {
                assert_eq!(arity1, arity2);
                for (t1, t2) in inner1.iter().zip(inner2) {
                    t1.eq_up_to_span(t2);
                }
                attrs_eq(attrs1, attrs2);
            }
            (Tuple(..), _) => {
                panic!(
                    "Different types: \n{}\n---\n{}",
                    self.to_string(),
                    other.to_string()
                )
            }
            (Union(arity1, variants1, _, attrs1), Union(arity2, variants2, _, attrs2)) => {
                assert_eq!(arity1, arity2);
                assert_eq!(
                    variants1.len(),
                    variants2.len(),
                    "Unions have the same number of variants"
                );
                for (v1, v2) in variants1.iter().zip(variants2) {
                    v1.eq_up_to_span(v2);
                }
                attrs_eq(attrs1, attrs2);
            }
            (Union(..), _) => {
                panic!(
                    "Different types: \n{}\n---\n{}",
                    self.to_string(),
                    other.to_string()
                )
            }
            (Map(kv1, _, attrs1), Map(kv2, _, attrs2)) => {
                kv1.0.eq_up_to_span(&kv2.0);
                kv1.1.eq_up_to_span(&kv2.1);
                attrs_eq(attrs1, attrs2);
            }
            (Map(..), _) => {
                panic!(
                    "Different types: \n{}\n---\n{}",
                    self.to_string(),
                    other.to_string()
                )
            }
        }
    }
}

// Impl display for FieldType
impl std::fmt::Display for FieldType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FieldType::Symbol(arity, idn, ..) => {
                write!(
                    f,
                    "{:#?}{}",
                    idn,
                    if arity.is_optional() { "?" } else { "" }
                )
            }
            FieldType::Union(arity, ft, ..) => {
                let mut ft = ft.iter().map(|t| t.to_string()).collect::<Vec<_>>();
                ft.sort(); // Greg: Why??
                write!(
                    f,
                    "({}){}",
                    ft.join(" | "),
                    if arity.is_optional() { "?" } else { "" }
                )
            }
            FieldType::Tuple(arity, ft, ..) => {
                let mut ft = ft.iter().map(|t| t.to_string()).collect::<Vec<_>>();
                ft.sort();
                write!(
                    f,
                    "({}){}",
                    ft.join(", "),
                    if arity.is_optional() { "?" } else { "" }
                )
            }
            FieldType::Map(kv, ..) => write!(f, "map<{}, {}>", kv.0, kv.1),
            FieldType::List(t, ..) => write!(f, "{}[]", t),
            FieldType::Primitive(arity, t, ..) => {
                write!(f, "{}{}", t, if arity.is_optional() { "?" } else { "" })
            }
        }
    }
}
