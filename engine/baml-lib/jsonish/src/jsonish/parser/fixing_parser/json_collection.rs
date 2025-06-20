use baml_types::{BamlMap, CompletionState};
use bstd::dedent;

use crate::jsonish::Value;

#[derive(Debug)]
pub enum JsonCollection {
    // Key, Value
    Object(Vec<String>, Vec<Value>, CompletionState),
    Array(Vec<Value>, CompletionState),
    QuotedString(String, CompletionState),
    TripleQuotedString(String, CompletionState),
    SingleQuotedString(String, CompletionState),
    // edge cases that need handling:
    // - triple backticks in a triple backtick string
    // - will the LLM terminate a triple backtick with a single backtick? probably not
    // - do we give the language specifier out? no
    // - what if the triple backtick block contains both a lang and path specifier? e.g. ```tsx path/to/file.tsx
    //   should we hand back the path?
    // - do we dedent the output?
    // - is it an acceptable heuristic to discard the first line of a triple backtick block?
    TripleBacktickString {
        lang: Option<(String, CompletionState)>,
        path: Option<(String, CompletionState)>,
        content: (String, CompletionState),
    },
    BacktickString(String, CompletionState),
    // Handles numbers, booleans, null, and unquoted strings
    UnquotedString(String, CompletionState),
    // Starting with // or #
    TrailingComment(String, CompletionState),
    // Content between /* and */
    BlockComment(String, CompletionState),
}

impl JsonCollection {
    pub fn name(&self) -> &'static str {
        match self {
            JsonCollection::Object(_, _, _) => "Object",
            JsonCollection::Array(_, _) => "Array",
            JsonCollection::QuotedString(_, _) => "String",
            JsonCollection::SingleQuotedString(_, _) => "String",
            JsonCollection::TripleBacktickString { .. } => "TripleBacktickString",
            JsonCollection::BacktickString(_, _) => "String",
            JsonCollection::TripleQuotedString(_, _) => "TripleQuotedString",
            JsonCollection::UnquotedString(_, _) => "UnquotedString",
            JsonCollection::TrailingComment(_, _) => "Comment",
            JsonCollection::BlockComment(_, _) => "Comment",
        }
    }

    pub fn completion_state(&self) -> &CompletionState {
        match self {
            JsonCollection::Object(_, _, s) => s,
            JsonCollection::Array(_, s) => s,
            JsonCollection::QuotedString(_, s) => s,
            JsonCollection::SingleQuotedString(_, s) => s,
            JsonCollection::TripleBacktickString { content, .. } => &content.1, // TODO: correct?
            JsonCollection::BacktickString(_, s) => s,
            JsonCollection::TripleQuotedString(_, s) => s,
            JsonCollection::UnquotedString(_, s) => s,
            JsonCollection::TrailingComment(_, s) => s,
            JsonCollection::BlockComment(_, s) => s,
        }
    }
}

impl From<JsonCollection> for Option<Value> {
    fn from(collection: JsonCollection) -> Option<Value> {
        Some(match collection {
            JsonCollection::TrailingComment(_, _) | JsonCollection::BlockComment(_, _) => {
                return None
            }
            JsonCollection::Object(keys, values, object_completion) => {
                // log::debug!("keys: {:?}", keys);
                let mut object: Vec<_> = Vec::new();
                for (key, value) in keys.into_iter().zip(values.into_iter()) {
                    object.push((key, value));
                }
                Value::Object(object, object_completion)
            }
            JsonCollection::Array(values, completion_state) => {
                Value::Array(values, completion_state)
            }
            JsonCollection::QuotedString(s, completion_state) => Value::String(s, completion_state),
            JsonCollection::TripleQuotedString(s, completion_state) => {
                Value::String(dedent(s.as_str()).content, completion_state)
            }
            JsonCollection::SingleQuotedString(s, completion_state) => {
                Value::String(s, completion_state)
            }
            JsonCollection::TripleBacktickString { content, .. } => {
                let Some((fenced_codeblock_info, codeblock_contents)) = content.0.split_once("\n")
                else {
                    return Some(Value::String(content.0, content.1));
                };

                Value::String(dedent(codeblock_contents).content, content.1)
            }
            JsonCollection::BacktickString(s, completion_state) => {
                Value::String(s, completion_state)
            }
            JsonCollection::UnquotedString(s, completion_state) => {
                let s = s.trim();
                if s == "true" {
                    Value::Boolean(true)
                } else if s == "false" {
                    Value::Boolean(false)
                } else if s == "null" {
                    Value::Null
                } else if let Ok(n) = s.parse::<i64>() {
                    Value::Number(n.into(), completion_state)
                } else if let Ok(n) = s.parse::<u64>() {
                    Value::Number(n.into(), completion_state)
                } else if let Ok(n) = s.parse::<f64>() {
                    match serde_json::Number::from_f64(n) {
                        Some(n) => Value::Number(n, completion_state),
                        None => Value::String(s.into(), completion_state),
                    }
                } else {
                    Value::String(s.into(), completion_state)
                }
            }
        })
    }
}
