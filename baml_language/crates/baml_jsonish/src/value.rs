//! JSON-ish value type.

use std::hash::{Hash, Hasher};
use baml_types::CompletionState;

/// Fixes applied during parsing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fixes {
    /// JSON was extracted from markdown code blocks.
    ExtractedFromMarkdown,
    /// An array was inferred from multiple values.
    InferredArray,
    /// Trailing comma was removed.
    RemovedTrailingComma,
    /// Single quotes were converted to double quotes.
    ConvertedQuotes,
}

/// A JSON-ish value that may be partially complete.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    /// A string value.
    String(String, CompletionState),
    /// A number value (using serde_json::Number for precision).
    Number(serde_json::Number, CompletionState),
    /// A boolean value.
    Boolean(bool),
    /// A null value.
    Null,
    /// An object (map) with key-value pairs.
    Object(Vec<(String, Value)>, CompletionState),
    /// An array of values.
    Array(Vec<Value>, CompletionState),
    /// JSON extracted from markdown code block.
    Markdown(String, Box<Value>, CompletionState),
    /// JSON that had fixes applied during parsing.
    FixedJson(Box<Value>, Vec<Fixes>),
    /// Multiple possible values (for ambiguous parses).
    AnyOf(Vec<Value>, String),
}

impl Hash for Value {
    fn hash<H: Hasher>(&self, state: &mut H) {
        std::mem::discriminant(self).hash(state);

        match self {
            Value::String(s, _) => s.hash(state),
            Value::Number(n, _) => n.to_string().hash(state),
            Value::Boolean(b) => b.hash(state),
            Value::Null => "null".hash(state),
            Value::Object(pairs, _) => {
                for (k, v) in pairs {
                    k.hash(state);
                    v.hash(state);
                }
            }
            Value::Array(items, _) => {
                for v in items {
                    v.hash(state);
                }
            }
            Value::Markdown(tag, v, _) => {
                tag.hash(state);
                v.hash(state);
            }
            Value::FixedJson(v, _) => v.hash(state),
            Value::AnyOf(items, _) => {
                for item in items {
                    item.hash(state);
                }
            }
        }
    }
}

impl Value {
    /// Get the type name of this value.
    pub fn type_name(&self) -> String {
        match self {
            Value::String(_, _) => "string".to_string(),
            Value::Number(_, _) => "number".to_string(),
            Value::Boolean(_) => "boolean".to_string(),
            Value::Null => "null".to_string(),
            Value::Object(_, _) => "object".to_string(),
            Value::Array(_, _) => "array".to_string(),
            Value::Markdown(tag, _, _) => format!("markdown:{}", tag),
            Value::FixedJson(inner, _) => inner.type_name(),
            Value::AnyOf(_, _) => "anyOf".to_string(),
        }
    }

    /// Get the completion state of this value.
    pub fn completion_state(&self) -> CompletionState {
        match self {
            Value::String(_, s) => *s,
            Value::Number(_, s) => *s,
            Value::Boolean(_) => CompletionState::Complete,
            Value::Null => CompletionState::Complete,
            Value::Object(_, s) => *s,
            Value::Array(_, s) => *s,
            Value::Markdown(_, _, s) => *s,
            Value::FixedJson(inner, _) => inner.completion_state(),
            Value::AnyOf(choices, _) => {
                if choices.iter().any(|c| c.completion_state() == CompletionState::Incomplete) {
                    CompletionState::Incomplete
                } else {
                    CompletionState::Complete
                }
            }
        }
    }

    /// Mark this value (and all nested values) as complete.
    pub fn mark_complete(&mut self) {
        match self {
            Value::String(_, s) => *s = CompletionState::Complete,
            Value::Number(_, s) => *s = CompletionState::Complete,
            Value::Boolean(_) | Value::Null => {}
            Value::Object(pairs, s) => {
                *s = CompletionState::Complete;
                for (_, v) in pairs {
                    v.mark_complete();
                }
            }
            Value::Array(items, s) => {
                *s = CompletionState::Complete;
                for v in items {
                    v.mark_complete();
                }
            }
            Value::Markdown(_, inner, s) => {
                *s = CompletionState::Complete;
                inner.mark_complete();
            }
            Value::FixedJson(inner, _) => inner.mark_complete(),
            Value::AnyOf(choices, _) => {
                for v in choices {
                    v.mark_complete();
                }
            }
        }
    }

    /// Check if this value is a string.
    pub fn is_string(&self) -> bool {
        matches!(self, Value::String(_, _))
    }

    /// Check if this value is a number.
    pub fn is_number(&self) -> bool {
        matches!(self, Value::Number(_, _))
    }

    /// Check if this value is an object.
    pub fn is_object(&self) -> bool {
        matches!(self, Value::Object(_, _))
    }

    /// Check if this value is an array.
    pub fn is_array(&self) -> bool {
        matches!(self, Value::Array(_, _))
    }

    /// Get the string value if this is a string.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::String(s, _) => Some(s),
            _ => None,
        }
    }

    /// Get the object fields if this is an object.
    pub fn as_object(&self) -> Option<&Vec<(String, Value)>> {
        match self {
            Value::Object(pairs, _) => Some(pairs),
            _ => None,
        }
    }

    /// Get the array items if this is an array.
    pub fn as_array(&self) -> Option<&Vec<Value>> {
        match self {
            Value::Array(items, _) => Some(items),
            _ => None,
        }
    }
}

impl std::fmt::Display for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Value::String(s, _) => write!(f, "\"{}\"", s),
            Value::Number(n, _) => write!(f, "{}", n),
            Value::Boolean(b) => write!(f, "{}", b),
            Value::Null => write!(f, "null"),
            Value::Object(pairs, _) => {
                write!(f, "{{")?;
                for (i, (k, v)) in pairs.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "\"{}\": {}", k, v)?;
                }
                write!(f, "}}")
            }
            Value::Array(items, _) => {
                write!(f, "[")?;
                for (i, v) in items.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", v)?;
                }
                write!(f, "]")
            }
            Value::Markdown(tag, inner, _) => {
                write!(f, "```{}\n{}\n```", tag, inner)
            }
            Value::FixedJson(inner, _) => write!(f, "{}", inner),
            Value::AnyOf(choices, raw) => {
                if choices.is_empty() {
                    write!(f, "{}", raw)
                } else {
                    write!(f, "{}", choices[0])
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_value_type_name() {
        assert_eq!(Value::String("test".into(), CompletionState::Complete).type_name(), "string");
        assert_eq!(Value::Boolean(true).type_name(), "boolean");
        assert_eq!(Value::Null.type_name(), "null");
    }

    #[test]
    fn test_value_completion_state() {
        let complete = Value::String("test".into(), CompletionState::Complete);
        assert_eq!(complete.completion_state(), CompletionState::Complete);

        let incomplete = Value::String("test".into(), CompletionState::Incomplete);
        assert_eq!(incomplete.completion_state(), CompletionState::Incomplete);
    }
}
