use baml_types::{TypeValue, UnresolvedValue as UnresolvedValueBase};
use internal_baml_diagnostics::Diagnostics;

type UnresolvedValue = UnresolvedValueBase<Span>;

use crate::ast::Span;
use bstd::dedent;
use std::fmt;

use super::{Identifier, WithName, WithSpan};
use baml_types::JinjaExpression;

#[derive(Debug, Clone)]
pub struct RawString {
    raw_span: Span,
    #[allow(dead_code)]
    pub raw_value: String,
    pub inner_value: String,

    /// If set indicates the language of the raw string.
    /// By default it is a text string.
    pub language: Option<(String, Span)>,

    // This is useful for getting the final offset.
    pub indent: usize,
    inner_span_start: usize,
}

impl WithSpan for RawString {
    fn span(&self) -> &Span {
        &self.raw_span
    }
}

impl RawString {
    pub(crate) fn new(value: String, span: Span, language: Option<(String, Span)>) -> Self {
        let dedented_value = value.trim_start_matches(['\n', '\r']);
        let start_trim_count = value.len() - dedented_value.len();
        let dedented_value = dedented_value.trim_end();
        let dedented = dedent(dedented_value);
        Self {
            raw_span: span,
            raw_value: value,
            inner_value: dedented.content,
            indent: dedented.indent_size,
            inner_span_start: start_trim_count,
            language,
        }
    }

    pub fn value(&self) -> &str {
        &self.inner_value
    }

    pub fn raw_value(&self) -> &str {
        &self.raw_value
    }

    pub fn to_raw_span(&self, span: pest::Span<'_>) -> Span {
        let start_idx = span.start();
        let end_idx = span.end();
        // Count number of \n in the raw string before the start of the span.
        let start_line_count = self.value()[..start_idx]
            .lines()
            .filter(|line| !line.trim().is_empty())
            .count();
        let end_line_count = self.value()[..end_idx]
            .lines()
            .filter(|line| !line.trim().is_empty())
            .count();

        Span {
            file: self.raw_span.file.clone(),
            start: self.raw_span.start
                + self.inner_span_start
                + self.indent * start_line_count
                + span.start(),
            end: self.raw_span.start
                + self.inner_span_start
                + self.indent * end_line_count
                + span.end(),
        }
    }

    pub fn assert_eq_up_to_span(&self, other: &RawString) {
        assert_eq!(self.inner_value, other.inner_value);
        assert_eq!(self.raw_value, other.raw_value);
        assert_eq!(self.language, other.language);
        assert_eq!(self.indent, other.indent);
    }
}

/// Represents arbitrary, even nested, expressions.
#[derive(Debug, Clone)]
pub enum Expression {
    /// Boolean values aka true or false
    BoolValue(bool, Span),
    /// Any numeric value e.g. floats or ints.
    NumericValue(String, Span),
    /// An identifier
    Identifier(Identifier),
    /// Any string value.
    StringValue(String, Span),
    /// Any string value.
    RawStringValue(RawString),
    /// An array of other values.
    Array(Vec<Expression>, Span),
    /// A mapping function.
    Map(Vec<(Expression, Expression)>, Span),
    /// A JinjaExpression. e.g. "this|length > 5".
    JinjaExpressionValue(JinjaExpression, Span),
}

impl fmt::Display for Expression {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Expression::Identifier(id) => fmt::Display::fmt(id.name(), f),
            Expression::BoolValue(val, _) => fmt::Display::fmt(val, f),
            Expression::NumericValue(val, _) => fmt::Display::fmt(val, f),
            Expression::StringValue(val, _) => write!(f, "{}", crate::string_literal(val)),
            Expression::RawStringValue(val, ..) => {
                write!(f, "{}", crate::string_literal(val.value()))
            }
            Expression::JinjaExpressionValue(val, ..) => fmt::Display::fmt(val, f),
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
                    .map(|(k, v)| format!("{k}: {v}"))
                    .collect::<Vec<_>>()
                    .join(",");
                write!(f, "{{{vals}}}")
            }
        }
    }
}

impl Expression {
    pub fn from_json(value: serde_json::Value, span: Span, empty_span: Span) -> Expression {
        match value {
            serde_json::Value::Null => Expression::StringValue("Null".to_string(), empty_span),
            serde_json::Value::Bool(b) => Expression::BoolValue(b, span),
            serde_json::Value::Number(n) => Expression::NumericValue(n.to_string(), span),
            serde_json::Value::String(s) => Expression::StringValue(s, span),
            serde_json::Value::Array(arr) => {
                let arr = arr
                    .into_iter()
                    .map(|v| Expression::from_json(v, empty_span.clone(), empty_span.clone()))
                    .collect();
                Expression::Array(arr, span)
            }
            serde_json::Value::Object(obj) => {
                let obj = obj
                    .into_iter()
                    .map(|(k, v)| {
                        (
                            Expression::StringValue(k, empty_span.clone()),
                            Expression::from_json(v, empty_span.clone(), empty_span.clone()),
                        )
                    })
                    .collect();
                Expression::Map(obj, span)
            }
        }
    }
    pub fn as_array(&self) -> Option<(&[Expression], &Span)> {
        match self {
            Expression::Array(arr, span) => Some((arr, span)),
            _ => None,
        }
    }

    pub fn as_path_value(&self) -> Option<(&str, &Span)> {
        match self {
            Expression::StringValue(s, span) if !(s == "true" || s == "false") => Some((s, span)),
            Expression::RawStringValue(s) => Some((s.value(), s.span())),
            Expression::Identifier(Identifier::String(id, span)) => Some((id, span)),
            Expression::Identifier(Identifier::Invalid(id, span)) => Some((id, span)),
            Expression::Identifier(Identifier::Local(id, span)) => Some((id, span)),
            Expression::Identifier(Identifier::Ref(id, span)) => Some((&id.full_name, span)),
            _ => None,
        }
    }

    pub fn as_string_value(&self) -> Option<(&str, &Span)> {
        match self {
            Expression::StringValue(s, span) if !(s == "true" || s == "false") => Some((s, span)),
            Expression::RawStringValue(s) => Some((s.value(), s.span())),
            Expression::Identifier(Identifier::String(id, span)) => Some((id, span)),
            Expression::Identifier(Identifier::Invalid(id, span)) => Some((id, span)),
            Expression::Identifier(Identifier::Local(id, span)) => Some((id, span)),
            _ => None,
        }
    }

    pub fn as_raw_string_value(&self) -> Option<&RawString> {
        match self {
            Expression::RawStringValue(s) => Some(s),
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
            Expression::RawStringValue(s) => Some((s.value(), s.span())),
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
            Self::BoolValue(_, span) => span,
            Self::NumericValue(_, span) => span,
            Self::StringValue(_, span) => span,
            Self::RawStringValue(r) => r.span(),
            Self::JinjaExpressionValue(_, span) => span,
            Self::Identifier(id) => id.span(),
            Self::Map(_, span) => span,
            Self::Array(_, span) => span,
        }
    }

    pub fn is_env_expression(&self) -> bool {
        matches!(self, Self::Identifier(Identifier::ENV(..)))
    }

    /// Creates a friendly readable representation for a value's type.
    pub fn describe_value_type(&self) -> &'static str {
        match self {
            Expression::BoolValue(_, _) => "boolean",
            Expression::NumericValue(_, _) => "numeric",
            Expression::StringValue(_, _) => "string",
            Expression::RawStringValue(_) => "raw_string",
            Expression::JinjaExpressionValue(_, _) => "jinja_expression",
            Expression::Identifier(id) => match id {
                Identifier::String(_, _) => "string",
                Identifier::Local(_, _) => "local_type",
                Identifier::Ref(_, _) => "ref_type",
                Identifier::ENV(_, _) => "env_type",
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
                | Expression::RawStringValue(_)
                | Expression::Identifier(Identifier::String(_, _))
                | Expression::Identifier(Identifier::Invalid(_, _))
                | Expression::Identifier(Identifier::Local(_, _))
        )
    }

    pub fn assert_eq_up_to_span(&self, other: &Expression) {
        use Expression::*;
        match (self, other) {
            (BoolValue(v1, _), BoolValue(v2, _)) => assert_eq!(v1, v2),
            (BoolValue(_, _), _) => panic!("Types do not match: {self:?} and {other:?}"),
            (NumericValue(n1, _), NumericValue(n2, _)) => assert_eq!(n1, n2),
            (NumericValue(_, _), _) => panic!("Types do not match: {self:?} and {other:?}"),
            (Identifier(i1), Identifier(i2)) => assert_eq!(i1, i2),
            (Identifier(_), _) => panic!("Types do not match: {self:?} and {other:?}"),
            (StringValue(s1, _), StringValue(s2, _)) => assert_eq!(s1, s2),
            (StringValue(_, _), _) => panic!("Types do not match: {self:?} and {other:?}"),
            (RawStringValue(s1), RawStringValue(s2)) => s1.assert_eq_up_to_span(s2),
            (RawStringValue(_), _) => panic!("Types do not match: {self:?} and {other:?}"),
            (JinjaExpressionValue(j1, _), JinjaExpressionValue(j2, _)) => assert_eq!(j1, j2),
            (JinjaExpressionValue(_, _), _) => {
                panic!("Types do not match: {self:?} and {other:?}")
            }
            (Array(xs, _), Array(ys, _)) => {
                assert_eq!(xs.len(), ys.len());
                xs.iter().zip(ys).for_each(|(x, y)| {
                    x.assert_eq_up_to_span(y);
                })
            }
            (Array(_, _), _) => panic!("Types do not match: {self:?} and {other:?}"),
            (Map(m1, _), Map(m2, _)) => {
                assert_eq!(m1.len(), m2.len());
                m1.iter().zip(m2).for_each(|((k1, v1), (k2, v2))| {
                    k1.assert_eq_up_to_span(k2);
                    v1.assert_eq_up_to_span(v2);
                });
            }
            (Map(_, _), _) => panic!("Types do not match: {self:?} and {other:?}"),
        }
    }

    pub fn to_unresolved_value(
        &self,
        _diagnostics: &mut internal_baml_diagnostics::Diagnostics,
    ) -> Option<UnresolvedValue> {
        use baml_types::StringOr;

        match self {
            Expression::BoolValue(val, span) => Some(UnresolvedValue::Bool(*val, span.clone())),
            Expression::NumericValue(val, span) => {
                Some(UnresolvedValue::Numeric(val.clone(), span.clone()))
            }
            Expression::Identifier(identifier) => match identifier {
                Identifier::ENV(val, span) => Some(UnresolvedValue::String(
                    StringOr::EnvVar(val.to_string()),
                    span.clone(),
                )),
                Identifier::Ref(ref_identifier, span) => Some(UnresolvedValue::String(
                    StringOr::Value(ref_identifier.full_name.as_str().to_string()),
                    span.clone(),
                )),
                Identifier::Invalid(val, span)
                | Identifier::String(val, span)
                | Identifier::Local(val, span) => match val.as_str() {
                    "null" => Some(UnresolvedValue::Null(span.clone())),
                    "true" => Some(UnresolvedValue::Bool(true, span.clone())),
                    "false" => Some(UnresolvedValue::Bool(false, span.clone())),
                    _ => Some(UnresolvedValue::String(
                        StringOr::Value(val.to_string()),
                        span.clone(),
                    )),
                },
            },
            Expression::StringValue(val, span) => Some(UnresolvedValue::String(
                StringOr::Value(val.to_string()),
                span.clone(),
            )),
            Expression::RawStringValue(raw_string) => {
                // Do standard dedenting / trimming.
                let val = raw_string.value();
                Some(UnresolvedValue::String(
                    StringOr::Value(val.to_string()),
                    raw_string.span().clone(),
                ))
            }
            Expression::Array(vec, span) => {
                let values = vec
                    .iter()
                    .filter_map(|e| e.to_unresolved_value(_diagnostics))
                    .collect::<Vec<_>>();
                Some(UnresolvedValue::Array(values, span.clone()))
            }
            Expression::Map(map, span) => {
                let values = map
                    .iter()
                    .filter_map(|(k, v)| {
                        let key = k.to_unresolved_value(_diagnostics);
                        if let Some(UnresolvedValue::String(StringOr::Value(key), key_span)) = key {
                            if let Some(value) = v.to_unresolved_value(_diagnostics) {
                                return Some((key, (key_span, value)));
                            }
                        }
                        None
                    })
                    .collect::<_>();
                Some(UnresolvedValue::Map(values, span.clone()))
            }
            Expression::JinjaExpressionValue(jinja_expression, span) => {
                Some(UnresolvedValue::String(
                    StringOr::JinjaExpression(jinja_expression.clone()),
                    span.clone(),
                ))
            }
        }
    }
}
