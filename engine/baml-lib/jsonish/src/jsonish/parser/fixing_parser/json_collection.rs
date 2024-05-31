use baml_types::BamlMap;

use crate::jsonish::Value;

#[derive(Debug)]
pub enum JsonCollection {
    // Key, Value
    Object(Vec<String>, Vec<Value>),
    Array(Vec<Value>),
    QuotedString(String),
    SingleQuotedString(String),
    // Handles numbers, booleans, null, and unquoted strings
    UnquotedString(String),
    // Starting with // or #
    TrailingComment(String),
    // Content between /* and */
    BlockComment(String),
}

impl JsonCollection {
    pub fn name(&self) -> &'static str {
        match self {
            JsonCollection::Object(_, _) => "Object",
            JsonCollection::Array(_) => "Array",
            JsonCollection::QuotedString(_) => "String",
            JsonCollection::SingleQuotedString(_) => "String",
            JsonCollection::UnquotedString(_) => "UnquotedString",
            JsonCollection::TrailingComment(_) => "Comment",
            JsonCollection::BlockComment(_) => "Comment",
        }
    }
}

impl From<JsonCollection> for Option<Value> {
    fn from(collection: JsonCollection) -> Option<Value> {
        Some(match collection {
            JsonCollection::TrailingComment(_) | JsonCollection::BlockComment(_) => return None,
            JsonCollection::Object(keys, values) => {
                log::info!("keys: {:?}", keys);
                let mut object = BamlMap::new();
                for (key, value) in keys.into_iter().zip(values.into_iter()) {
                    object.insert(key, value);
                }
                Value::Object(object)
            }
            JsonCollection::Array(values) => Value::Array(values),
            JsonCollection::QuotedString(s) => Value::String(s),
            JsonCollection::SingleQuotedString(s) => Value::String(s),
            JsonCollection::UnquotedString(s) => {
                let s = s.trim();
                if s == "true" {
                    Value::Boolean(true)
                } else if s == "false" {
                    Value::Boolean(false)
                } else if s == "null" {
                    Value::Null
                } else if let Ok(n) = s.parse::<i64>() {
                    Value::Number(n.into())
                } else if let Ok(n) = s.parse::<u64>() {
                    Value::Number(n.into())
                } else if let Ok(n) = s.parse::<f64>() {
                    Value::Number(serde_json::Number::from_f64(n).unwrap())
                } else {
                    Value::String(s.into())
                }
            }
        })
    }
}
