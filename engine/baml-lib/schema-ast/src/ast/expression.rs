

use crate::ast::Span;
use std::fmt;

use super::{Identifier, WithName, WithSpan};

/// Represents arbitrary, even nested, expressions.
#[derive(Debug, Clone)]
pub enum Expression {
    /// Any numeric value e.g. floats or ints.
    NumericValue(String, Span),
    /// An identifier
    Identifier(Identifier),
    /// Any string value.
    StringValue(String, Span),
    /// Any string value.
    RawStringValue(String, Span),
    /// An array of other values.
    Array(Vec<Expression>, Span),
    /// A mapping function.
    Map(Vec<(Expression, Expression)>, Span),
}

impl fmt::Display for Expression {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Expression::Identifier(id) => fmt::Display::fmt(id.name(), f),
            Expression::NumericValue(val, _) => fmt::Display::fmt(val, f),
            Expression::StringValue(val, _) => write!(f, "{}", crate::string_literal(val)),
            Expression::RawStringValue(val, _) => write!(f, "{}", crate::string_literal(val)),
            Expression::Array(vals, _) => {
                let vals = vals
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(",");
                write!(f, "[{vals}]")
            }
            Expression::Map(vals, _) => {
                let vals = vals
                    .iter()
                    .map(|(k, v)| format!("{}: {}", k, v))
                    .collect::<Vec<_>>()
                    .join(",");
                write!(f, "{{{vals}}}")
            }
        }
    }
}

impl Expression {
    pub fn as_array(&self) -> Option<(&[Expression], &Span)> {
        match self {
            Expression::Array(arr, span) => Some((arr, span)),
            _ => None,
        }
    }

    pub fn as_path_value(&self) -> Option<(&str, &Span)> {
        match self {
            Expression::StringValue(s, span) => Some((s, span)),
            Expression::RawStringValue(s, span) if !(s == "true" || s == "false") => {
                Some((s, span))
            }
            Expression::Identifier(Identifier::String(id, span)) => Some((id, span)),
            Expression::Identifier(Identifier::Invalid(id, span)) => Some((id, span)),
            Expression::Identifier(Identifier::Local(id, span)) => Some((id, span)),
            Expression::Identifier(Identifier::Ref(id, span)) => Some((&id.full_name, span)),
            _ => None,
        }
    }

    pub fn as_string_value(&self) -> Option<(&str, &Span)> {
        match self {
            Expression::StringValue(s, span) => Some((s, span)),
            Expression::RawStringValue(s, span) if !(s == "true" || s == "false") => {
                Some((s, span))
            }
            Expression::Identifier(Identifier::String(id, span)) => Some((id, span)),
            Expression::Identifier(Identifier::Invalid(id, span)) => Some((id, span)),
            Expression::Identifier(Identifier::Local(id, span)) => Some((id, span)),
            _ => None,
        }
    }

    pub fn as_identifer(&self) -> Option<&Identifier> {
        match self {
            Expression::Identifier(id) => Some(id),
            _ => None,
        }
    }

    pub fn as_constant_value(&self) -> Option<(&str, &Span)> {
        match self {
            Expression::StringValue(val, span) => Some((val, span)),
            Expression::RawStringValue(val, span) => Some((val, span)),
            Expression::Identifier(idn) if idn.is_valid_value() => Some((idn.name(), idn.span())),
            _ => None,
        }
    }

    pub fn as_map(&self) -> Option<(&[(Expression, Expression)], &Span)> {
        match self {
            Expression::Map(map, span) => Some((map, span)),
            _ => None,
        }
    }

    pub fn as_numeric_value(&self) -> Option<(&str, &Span)> {
        match self {
            Expression::NumericValue(s, span) => Some((s, span)),
            _ => None,
        }
    }

    pub fn span(&self) -> &Span {
        match &self {
            Self::NumericValue(_, span) => span,
            Self::StringValue(_, span) => span,
            Self::RawStringValue(_, span) => span,
            Self::Identifier(id) => id.span(),
            Self::Map(_, span) => span,
            Self::Array(_, span) => span,
        }
    }

    pub fn is_env_expression(&self) -> bool {
        match &self {
            Self::Identifier(Identifier::ENV(..)) => true,
            _ => false,
        }
    }

    /// Creates a friendly readable representation for a value's type.
    pub fn describe_value_type(&self) -> &'static str {
        match self {
            Expression::NumericValue(_, _) => "numeric",
            Expression::StringValue(_, _) => "string",
            Expression::RawStringValue(_, _) => "raw_string",
            Expression::Identifier(id) => match id {
                Identifier::String(_, _) => "string",
                Identifier::Local(_, _) => "local_type",
                Identifier::Ref(_, _) => "ref_type",
                Identifier::ENV(_, _) => "env_type",
                Identifier::Primitive(_, _) => "primitive_type",
                Identifier::Invalid(_, _) => "invalid_type",
            },
            Expression::Map(_, _) => "map",
            Expression::Array(_, _) => "array",
        }
    }

    pub fn is_map(&self) -> bool {
        matches!(self, Expression::Map(_, _))
    }

    pub fn is_array(&self) -> bool {
        matches!(self, Expression::Array(_, _))
    }

    pub fn is_string(&self) -> bool {
        matches!(
            self,
            Expression::StringValue(_, _)
                | Expression::RawStringValue(_, _)
                | Expression::Identifier(Identifier::String(_, _))
                | Expression::Identifier(Identifier::Invalid(_, _))
                | Expression::Identifier(Identifier::Local(_, _))
        )
    }
}
