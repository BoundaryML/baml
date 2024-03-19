use crate::ast::Span;
use std::fmt;

use super::{Identifier, WithName, WithSpan};

#[derive(Debug, Clone)]
pub struct RawString {
    raw_span: Span,
    #[allow(dead_code)]
    raw_value: String,
    inner_value: String,

    /// If set indicates the language of the raw string.
    /// By default it is a text string.
    pub language: Option<(String, Span)>,

    // This is useful for getting the final offset.
    indent: usize,
    inner_span_start: usize,
}

impl WithSpan for RawString {
    fn span(&self) -> &Span {
        &self.raw_span
    }
}

pub fn dedent(s: &str) -> (String, usize) {
    let mut prefix = "";
    let mut lines = s.lines();

    // We first search for a non-empty line to find a prefix.
    for line in &mut lines {
        let mut whitespace_idx = line.len();
        for (idx, ch) in line.char_indices() {
            if !ch.is_whitespace() {
                whitespace_idx = idx;
                break;
            }
        }

        // Check if the line had anything but whitespace
        if whitespace_idx < line.len() {
            prefix = &line[..whitespace_idx];
            break;
        }
    }

    // We then continue looking through the remaining lines to
    // possibly shorten the prefix.
    for line in &mut lines {
        let mut whitespace_idx = line.len();
        for ((idx, a), b) in line.char_indices().zip(prefix.chars()) {
            if a != b {
                whitespace_idx = idx;
                break;
            }
        }

        // Check if the line had anything but whitespace and if we
        // have found a shorter prefix
        if whitespace_idx < line.len() && whitespace_idx < prefix.len() {
            prefix = &line[..whitespace_idx];
        }
    }

    // We now go over the lines a second time to build the result.
    let mut result = String::new();
    for line in s.lines() {
        if line.starts_with(prefix) && line.chars().any(|c| !c.is_whitespace()) {
            let (_, tail) = line.split_at(prefix.len());
            result.push_str(tail);
        }
        result.push('\n');
    }

    if result.ends_with('\n') && !s.ends_with('\n') {
        let new_len = result.len() - 1;
        result.truncate(new_len);
    }

    (result, prefix.len())
}

impl RawString {
    pub(crate) fn new(value: String, span: Span, language: Option<(String, Span)>) -> Self {
        let dedented_value = value.trim_start_matches(|c| c == '\n' || c == '\r');
        let start_trim_count = value.len() - dedented_value.len();
        let dedented_value = dedented_value.trim_end();
        let (dedented_value, indent_size) = dedent(dedented_value);
        Self {
            raw_span: span,
            raw_value: value,
            inner_value: dedented_value,
            indent: indent_size,
            inner_span_start: start_trim_count,
            language,
        }
    }

    pub fn value(&self) -> &str {
        &self.inner_value
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
}

impl Expression {
    pub fn from_json(value: serde_json::Value, span: Span, empty_span: Span) -> Expression {
        match value {
            serde_json::Value::Null => {
                Expression::Identifier(Identifier::Primitive(super::TypeValue::Null, span))
            }
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
}

impl Into<serde_json::Value> for &Expression {
    fn into(self) -> serde_json::Value {
        match self {
            Expression::BoolValue(val, _) => serde_json::Value::Bool(*val),
            Expression::NumericValue(val, _) => serde_json::Value::Number(val.parse().unwrap()),
            Expression::StringValue(val, _) => serde_json::Value::String(val.clone()),
            Expression::RawStringValue(val) => serde_json::Value::String(val.value().to_string()),
            Expression::Identifier(id) => serde_json::Value::String(id.name().to_string()),
            Expression::Array(vals, _) => {
                serde_json::Value::Array(vals.iter().map(Into::into).collect())
            }
            Expression::Map(vals, _) => serde_json::Value::Object(
                vals.iter()
                    .map(|(k, v)| {
                        let k = Into::<serde_json::Value>::into(k);
                        let k = match k.as_str() {
                            Some(k) => k.to_string(),
                            None => k.to_string(),
                        };
                        (k, v.into())
                    })
                    .collect(),
            ),
        }
    }
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
            Expression::BoolValue(_, _) => "boolean",
            Expression::NumericValue(_, _) => "numeric",
            Expression::StringValue(_, _) => "string",
            Expression::RawStringValue(_) => "raw_string",
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
                | Expression::RawStringValue(_)
                | Expression::Identifier(Identifier::String(_, _))
                | Expression::Identifier(Identifier::Invalid(_, _))
                | Expression::Identifier(Identifier::Local(_, _))
        )
    }
}
