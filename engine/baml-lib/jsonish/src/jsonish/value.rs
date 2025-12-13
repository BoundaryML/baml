use std::{
    collections::HashSet,
    hash::{Hash, Hasher},
};

use baml_types::{BamlMap, CompletionState};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fixes {
    GreppedForJSON,
    InferredArray,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    // Primitive Types
    String(String, CompletionState),
    Number(serde_json::Number, CompletionState),
    Boolean(bool),
    Null,

    // Complex Types
    // Note: Greg - should keys carry completion state?
    // During parsing, if we hare an incomplete key, does the parser
    // complete it and set its value to null? Or drop it?
    // If the parser drops it, we don't need to carry CompletionState.
    Object(Vec<(String, Value)>, CompletionState),
    Array(Vec<Value>, CompletionState),

    // Fixed types
    Markdown(String, Box<Value>, CompletionState),
    FixedJson(Box<Value>, Vec<Fixes>),
    AnyOf(Vec<Value>, String),
}

impl Hash for Value {
    // Hashing a Value ignores CompletationState.
    fn hash<H: Hasher>(&self, state: &mut H) {
        std::mem::discriminant(self).hash(state);

        match self {
            Value::String(s, _) => s.hash(state),
            Value::Number(n, _) => n.to_string().hash(state),
            Value::Boolean(b) => b.hash(state),
            Value::Null => "null".hash(state),
            Value::Object(o, _) => {
                for (k, v) in o {
                    k.hash(state);
                    v.hash(state);
                }
            }
            Value::Array(a, _) => {
                for v in a {
                    v.hash(state);
                }
            }
            Value::Markdown(s, v, _) => {
                s.hash(state);
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
    pub(super) fn simplify(self, is_done: bool) -> Self {
        match self {
            Value::AnyOf(items, s) => {
                let as_simple_str = |s: String| {
                    Value::String(
                        s,
                        if is_done {
                            CompletionState::Complete
                        } else {
                            CompletionState::Incomplete
                        },
                    )
                };
                let mut items = items
                    .into_iter()
                    .map(|v| v.simplify(is_done))
                    .collect::<Vec<_>>();
                match items.len() {
                    0 => as_simple_str(s),
                    1 => match items.pop().expect("Expected 1 item") {
                        Value::String(content, completion_state) if content == s => {
                            as_simple_str(s)
                        }
                        other => Value::AnyOf(vec![other], s),
                    },
                    _ => Value::AnyOf(items, s),
                }
            }
            _ => self,
        }
    }

    pub fn r#type(&self) -> String {
        match self {
            Value::String(_, _) => "String".to_string(),
            Value::Number(_, _) => "Number".to_string(),
            Value::Boolean(_) => "Boolean".to_string(),
            Value::Null => "Null".to_string(),
            Value::Object(k, _) => {
                let mut s = "Object{".to_string();
                for (key, value) in k.iter() {
                    s.push_str(&format!("{}: {}, ", key, value.r#type()));
                }
                s.push('}');
                s
            }
            Value::Array(i, _) => {
                let mut s = "Array[".to_string();
                let items = i
                    .iter()
                    .map(|v| v.r#type())
                    .collect::<HashSet<String>>()
                    .into_iter()
                    .collect::<Vec<String>>()
                    .join(" | ");
                s.push_str(&items);
                s.push(']');
                s
            }
            Value::Markdown(tag, item, _) => {
                format!("Markdown:{} - {}", tag, item.r#type())
            }
            Value::FixedJson(inner, fixes) => {
                format!("{} ({} fixes)", inner.r#type(), fixes.len())
            }
            Value::AnyOf(items, _) => {
                let mut s = "AnyOf[".to_string();
                for item in items {
                    s.push_str(&item.r#type());
                    s.push_str(", ");
                }
                s.push(']');
                s
            }
        }
    }

    pub fn completion_state(&self) -> &CompletionState {
        match self {
            Value::String(_, s) => s,
            Value::Number(_, s) => s,
            Value::Boolean(_) => &CompletionState::Complete,
            Value::Null => &CompletionState::Complete,
            Value::Object(_, s) => s,
            Value::Array(_, s) => s,
            Value::Markdown(_, _, s) => s,
            Value::FixedJson(_, _) => &CompletionState::Complete,
            Value::AnyOf(choices, _) => {
                if choices
                    .iter()
                    .any(|c| c.completion_state() == &CompletionState::Incomplete)
                {
                    &CompletionState::Incomplete
                } else {
                    &CompletionState::Complete
                }
            }
        }
    }

    pub fn complete_deeply(&mut self) {
        match self {
            Value::String(_, s) => *s = CompletionState::Complete,
            Value::Number(_, s) => *s = CompletionState::Complete,
            Value::Boolean(_) => {}
            Value::Null => {}
            Value::Object(kv_pairs, s) => {
                *s = CompletionState::Complete;
                kv_pairs.iter_mut().for_each(|(_, v)| v.complete_deeply());
            }
            Value::Array(elems, s) => {
                *s = CompletionState::Complete;
                elems.iter_mut().for_each(|v| v.complete_deeply());
            }
            Value::Markdown(_, _, s) => *s = CompletionState::Complete,
            Value::FixedJson(val, fixes) => {
                val.complete_deeply();
            }
            Value::AnyOf(choices, _) => choices.iter_mut().for_each(|v| v.complete_deeply()),
        }
    }
}

impl std::fmt::Display for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Value::String(s, _) => write!(f, "{s}"),
            Value::Number(n, _) => write!(f, "{n}"),
            Value::Boolean(b) => write!(f, "{b}"),
            Value::Null => write!(f, "null"),
            Value::Object(o, _) => {
                write!(f, "{{")?;
                for (i, (k, v)) in o.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{k}: {v}")?;
                }
                write!(f, "}}")
            }
            Value::Array(a, _) => {
                write!(f, "[")?;
                for (i, v) in a.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{v}")?;
                }
                write!(f, "]")
            }
            Value::Markdown(s, v, _) => write!(f, "{s}\n{v}"),
            Value::FixedJson(v, _) => write!(f, "{v}"),
            Value::AnyOf(items, s) => {
                write!(f, "AnyOf[{s},")?;
                for item in items {
                    write!(f, "{item},")?;
                }
                write!(f, "]")
            }
        }
    }
}

// The serde implementation is used as one of our parsing options.
// We deserialize into a "complete" value, and this property is
// true for nested values, because serde will call the same `deserialize`
// method on children of a serde container.
//
// Numbers should be considered Incomplete if they are encountered
// at the top level. Therefore the non-recursive callsite of `deserialize`
// is responsible for setting completion state to Incomplete for top-level
// strings and numbers.
//
// Lists, strings and objects at the top level are necessarily complete, because
// serde will not parse an array, string or an object unless the closing
// delimiter is present.

/// A serde Visitor that constructs Value directly from the deserializer,
/// avoiding the intermediate serde_json::Value allocation and double-parsing.
struct ValueVisitor;

impl<'de> serde::de::Visitor<'de> for ValueVisitor {
    type Value = Value;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("any valid JSON value")
    }

    fn visit_bool<E>(self, v: bool) -> Result<Self::Value, E> {
        Ok(Value::Boolean(v))
    }

    fn visit_i64<E>(self, v: i64) -> Result<Self::Value, E> {
        Ok(Value::Number(v.into(), CompletionState::Complete))
    }

    fn visit_u64<E>(self, v: u64) -> Result<Self::Value, E> {
        Ok(Value::Number(v.into(), CompletionState::Complete))
    }

    fn visit_f64<E>(self, v: f64) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        match serde_json::Number::from_f64(v) {
            Some(n) => Ok(Value::Number(n, CompletionState::Complete)),
            None => Err(serde::de::Error::custom(format!(
                "f64 value cannot be represented as JSON number: {}",
                v
            ))),
        }
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E> {
        Ok(Value::String(v.to_owned(), CompletionState::Complete))
    }

    fn visit_string<E>(self, v: String) -> Result<Self::Value, E> {
        Ok(Value::String(v, CompletionState::Complete))
    }

    fn visit_none<E>(self) -> Result<Self::Value, E> {
        Ok(Value::Null)
    }

    fn visit_unit<E>(self) -> Result<Self::Value, E> {
        Ok(Value::Null)
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::SeqAccess<'de>,
    {
        let mut vec = Vec::with_capacity(seq.size_hint().unwrap_or(0));
        while let Some(elem) = seq.next_element::<Value>()? {
            vec.push(elem);
        }
        Ok(Value::Array(vec, CompletionState::Complete))
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::MapAccess<'de>,
    {
        let mut object = Vec::with_capacity(map.size_hint().unwrap_or(0));
        while let Some((key, value)) = map.next_entry::<String, Value>()? {
            object.push((key, value));
        }
        Ok(Value::Object(object, CompletionState::Complete))
    }
}

impl<'de> serde::Deserialize<'de> for Value {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_any(ValueVisitor)
    }
}
