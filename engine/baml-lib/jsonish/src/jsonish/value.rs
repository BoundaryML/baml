use std::collections::HashSet;


#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fixes {
    GreppedForJSON,
    InferredArray,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    // Primitive Types
    String(String),
    Number(serde_json::Number),
    Boolean(bool),
    Null,

    // Complex Types
    Object(Vec<(String, Value)>),
    Array(Vec<Value>),

    // Fixed types
    Markdown(String, Box<Value>),
    FixedJson(Box<Value>, Vec<Fixes>),
    AnyOf(Vec<Value>, String),
}

impl Value {
    pub fn r#type(&self) -> String {
        match self {
            Value::String(_) => "String".to_string(),
            Value::Number(_) => "Number".to_string(),
            Value::Boolean(_) => "Boolean".to_string(),
            Value::Null => "Null".to_string(),
            Value::Object(k) => {
                let mut s = "Object{".to_string();
                for (key, value) in k.iter() {
                    s.push_str(&format!("{}: {}, ", key, value.r#type()));
                }
                s.push('}');
                s
            }
            Value::Array(i) => {
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
            Value::Markdown(tag, item) => {
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
}

impl std::fmt::Display for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Value::String(s) => write!(f, "{s}"),
            Value::Number(n) => write!(f, "{n}"),
            Value::Boolean(b) => write!(f, "{b}"),
            Value::Null => write!(f, "null"),
            Value::Object(o) => {
                write!(f, "{{")?;
                for (i, (k, v)) in o.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{k}: {v}")?;
                }
                write!(f, "}}")
            }
            Value::Array(a) => {
                write!(f, "[")?;
                for (i, v) in a.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{v}")?;
                }
                write!(f, "]")
            }
            Value::Markdown(s, v) => write!(f, "{s}\n{v}"),
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

impl<'de> serde::Deserialize<'de> for Value {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = serde_json::Value::deserialize(deserializer)?;
        match value {
            serde_json::Value::String(s) => Ok(Value::String(s)),
            serde_json::Value::Number(n) => Ok(Value::Number(n)),
            serde_json::Value::Bool(b) => Ok(Value::Boolean(b)),
            serde_json::Value::Null => Ok(Value::Null),
            serde_json::Value::Object(o) => {
                let mut map = Vec::new();
                for (k, v) in o {
                    let parsed_value =
                        serde_json::from_value(v).map_err(serde::de::Error::custom)?;
                    map.push((k, parsed_value));
                }
                Ok(Value::Object(map))
            }
            serde_json::Value::Array(a) => {
                let mut vec = Vec::new();
                for v in a {
                    let parsed_value =
                        serde_json::from_value(v).map_err(serde::de::Error::custom)?;
                    vec.push(parsed_value);
                }
                Ok(Value::Array(vec))
            }
        }
    }
}
