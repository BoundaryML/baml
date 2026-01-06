//! Type coercion for JSON-ish values.
//!
//! Converts parsed JSON-ish values to BamlValue based on a target TypeIR.

use crate::value::Value;
use crate::parser::ParseError;
use baml_types::{BamlMap, BamlMedia, BamlMediaType, BamlValue, TypeIR, TypeValue};
use thiserror::Error;

/// Error during coercion.
#[derive(Debug, Error)]
pub enum CoercionError {
    #[error("Parse error: {0}")]
    Parse(#[from] ParseError),

    #[error("Type mismatch: expected {expected}, got {actual}")]
    TypeMismatch { expected: String, actual: String },

    #[error("Missing required field: {field}")]
    MissingField { field: String },

    #[error("Unknown enum variant: {variant} for enum {enum_name}")]
    UnknownEnumVariant { enum_name: String, variant: String },

    #[error("Coercion failed: {0}")]
    Other(String),
}

/// Coercer for converting JSON-ish values to BamlValue.
pub struct Coercer {
    /// Whether the input is complete (vs streaming).
    is_done: bool,
}

impl Coercer {
    /// Create a new coercer.
    pub fn new(is_done: bool) -> Self {
        Self { is_done }
    }

    /// Coerce a JSON-ish value to a BamlValue based on the target type.
    pub fn coerce(&self, value: &Value, target: &TypeIR) -> Result<BamlValue, CoercionError> {
        // Unwrap Markdown and FixedJson wrappers
        let value = match value {
            Value::Markdown(_, inner, _) => inner.as_ref(),
            Value::FixedJson(inner, _) => inner.as_ref(),
            Value::AnyOf(choices, _) if !choices.is_empty() => &choices[0],
            other => other,
        };

        match target {
            TypeIR::Primitive(prim, _) => self.coerce_primitive(value, prim),
            TypeIR::Literal(lit, _) => self.coerce_literal(value, lit),
            TypeIR::Optional(inner, _) => self.coerce_optional(value, inner),
            TypeIR::List(elem, _) => self.coerce_list(value, elem),
            TypeIR::Map(key, val, _) => self.coerce_map(value, key, val),
            TypeIR::Union(variants, _) => self.coerce_union(value, variants),
            TypeIR::Class(name, _) => self.coerce_class(value, name),
            TypeIR::Enum(name, _) => self.coerce_enum(value, name),
            TypeIR::Alias(_, inner, _) => self.coerce(value, inner),
            TypeIR::Constrained { base, .. } => self.coerce(value, base),
        }
    }

    fn coerce_primitive(&self, value: &Value, prim: &TypeValue) -> Result<BamlValue, CoercionError> {
        match (prim, value) {
            // String coercion
            (TypeValue::String, Value::String(s, _)) => Ok(BamlValue::String(s.clone())),
            (TypeValue::String, Value::Number(n, _)) => Ok(BamlValue::String(n.to_string())),
            (TypeValue::String, Value::Boolean(b)) => Ok(BamlValue::String(b.to_string())),

            // Int coercion
            (TypeValue::Int, Value::Number(n, _)) => {
                if let Some(i) = n.as_i64() {
                    Ok(BamlValue::Int(i))
                } else if let Some(f) = n.as_f64() {
                    Ok(BamlValue::Int(f as i64))
                } else {
                    Err(CoercionError::TypeMismatch {
                        expected: "int".into(),
                        actual: "number".into(),
                    })
                }
            }
            (TypeValue::Int, Value::String(s, _)) => {
                s.parse::<i64>()
                    .map(BamlValue::Int)
                    .map_err(|_| CoercionError::TypeMismatch {
                        expected: "int".into(),
                        actual: format!("string \"{}\"", s),
                    })
            }

            // Float coercion
            (TypeValue::Float, Value::Number(n, _)) => {
                if let Some(f) = n.as_f64() {
                    Ok(BamlValue::Float(f))
                } else {
                    Err(CoercionError::TypeMismatch {
                        expected: "float".into(),
                        actual: "number".into(),
                    })
                }
            }
            (TypeValue::Float, Value::String(s, _)) => {
                s.parse::<f64>()
                    .map(BamlValue::Float)
                    .map_err(|_| CoercionError::TypeMismatch {
                        expected: "float".into(),
                        actual: format!("string \"{}\"", s),
                    })
            }

            // Bool coercion
            (TypeValue::Bool, Value::Boolean(b)) => Ok(BamlValue::Bool(*b)),
            (TypeValue::Bool, Value::String(s, _)) => {
                match s.to_lowercase().as_str() {
                    "true" | "yes" | "1" => Ok(BamlValue::Bool(true)),
                    "false" | "no" | "0" => Ok(BamlValue::Bool(false)),
                    _ => Err(CoercionError::TypeMismatch {
                        expected: "bool".into(),
                        actual: format!("string \"{}\"", s),
                    })
                }
            }

            // Null coercion
            (TypeValue::Null, Value::Null) => Ok(BamlValue::Null),

            // Media coercion (stub for now)
            (TypeValue::Media(_), _) => {
                // TODO: Proper media coercion
                Ok(BamlValue::Media(BamlMedia::url(BamlMediaType::Image, "")))
            }

            // Type mismatch
            _ => Err(CoercionError::TypeMismatch {
                expected: format!("{:?}", prim),
                actual: value.type_name(),
            })
        }
    }

    fn coerce_literal(&self, value: &Value, lit: &baml_types::LiteralValue) -> Result<BamlValue, CoercionError> {
        match (lit, value) {
            (baml_types::LiteralValue::String(expected), Value::String(s, _)) if s == expected => {
                Ok(BamlValue::String(s.clone()))
            }
            (baml_types::LiteralValue::Int(expected), Value::Number(n, _)) if n.as_i64() == Some(*expected) => {
                Ok(BamlValue::Int(*expected))
            }
            (baml_types::LiteralValue::Bool(expected), Value::Boolean(b)) if b == expected => {
                Ok(BamlValue::Bool(*expected))
            }
            _ => Err(CoercionError::TypeMismatch {
                expected: format!("literal {:?}", lit),
                actual: value.type_name(),
            })
        }
    }

    fn coerce_optional(&self, value: &Value, inner: &TypeIR) -> Result<BamlValue, CoercionError> {
        match value {
            Value::Null => Ok(BamlValue::Null),
            _ => self.coerce(value, inner),
        }
    }

    fn coerce_list(&self, value: &Value, elem: &TypeIR) -> Result<BamlValue, CoercionError> {
        match value {
            Value::Array(items, _) => {
                let coerced: Result<Vec<_>, _> = items
                    .iter()
                    .map(|v| self.coerce(v, elem))
                    .collect();
                Ok(BamlValue::List(coerced?))
            }
            // Single value can be coerced to a list of one
            other => {
                let coerced = self.coerce(other, elem)?;
                Ok(BamlValue::List(vec![coerced]))
            }
        }
    }

    fn coerce_map(&self, value: &Value, _key: &TypeIR, val: &TypeIR) -> Result<BamlValue, CoercionError> {
        match value {
            Value::Object(pairs, _) => {
                let mut map = BamlMap::new();
                for (k, v) in pairs {
                    let coerced = self.coerce(v, val)?;
                    map.insert(k.clone(), coerced);
                }
                Ok(BamlValue::Map(map))
            }
            _ => Err(CoercionError::TypeMismatch {
                expected: "map".into(),
                actual: value.type_name(),
            })
        }
    }

    fn coerce_union(&self, value: &Value, variants: &[TypeIR]) -> Result<BamlValue, CoercionError> {
        // Try each variant until one succeeds
        for variant in variants {
            if let Ok(coerced) = self.coerce(value, variant) {
                return Ok(coerced);
            }
        }
        Err(CoercionError::TypeMismatch {
            expected: "union variant".into(),
            actual: value.type_name(),
        })
    }

    fn coerce_class(&self, value: &Value, name: &str) -> Result<BamlValue, CoercionError> {
        match value {
            Value::Object(pairs, _) => {
                let mut fields = BamlMap::new();
                for (k, v) in pairs {
                    // For now, coerce all fields as strings (we don't have field type info)
                    // TODO: Get field types from schema
                    let coerced = self.coerce_to_any(v);
                    fields.insert(k.clone(), coerced);
                }
                Ok(BamlValue::Class(name.to_string(), fields))
            }
            _ => Err(CoercionError::TypeMismatch {
                expected: format!("class {}", name),
                actual: value.type_name(),
            })
        }
    }

    fn coerce_enum(&self, value: &Value, name: &str) -> Result<BamlValue, CoercionError> {
        match value {
            Value::String(s, _) => {
                // TODO: Validate against actual enum variants
                Ok(BamlValue::Enum(name.to_string(), s.clone()))
            }
            _ => Err(CoercionError::TypeMismatch {
                expected: format!("enum {}", name),
                actual: value.type_name(),
            })
        }
    }

    /// Coerce a value to BamlValue without a specific target type.
    fn coerce_to_any(&self, value: &Value) -> BamlValue {
        match value {
            Value::String(s, _) => BamlValue::String(s.clone()),
            Value::Number(n, _) => {
                if let Some(i) = n.as_i64() {
                    BamlValue::Int(i)
                } else if let Some(f) = n.as_f64() {
                    BamlValue::Float(f)
                } else {
                    BamlValue::String(n.to_string())
                }
            }
            Value::Boolean(b) => BamlValue::Bool(*b),
            Value::Null => BamlValue::Null,
            Value::Array(items, _) => {
                BamlValue::List(items.iter().map(|v| self.coerce_to_any(v)).collect())
            }
            Value::Object(pairs, _) => {
                let mut map = BamlMap::new();
                for (k, v) in pairs {
                    map.insert(k.clone(), self.coerce_to_any(v));
                }
                BamlValue::Map(map)
            }
            Value::Markdown(_, inner, _) => self.coerce_to_any(inner),
            Value::FixedJson(inner, _) => self.coerce_to_any(inner),
            Value::AnyOf(choices, raw) => {
                if let Some(first) = choices.first() {
                    self.coerce_to_any(first)
                } else {
                    BamlValue::String(raw.clone())
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use baml_types::CompletionState;

    #[test]
    fn test_coerce_string() {
        let coercer = Coercer::new(true);
        let value = Value::String("hello".into(), CompletionState::Complete);
        let result = coercer.coerce(&value, &TypeIR::string());
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), BamlValue::String("hello".into()));
    }

    #[test]
    fn test_coerce_int() {
        let coercer = Coercer::new(true);
        let value = Value::Number(serde_json::Number::from(42), CompletionState::Complete);
        let result = coercer.coerce(&value, &TypeIR::int());
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), BamlValue::Int(42));
    }

    #[test]
    fn test_coerce_int_from_string() {
        let coercer = Coercer::new(true);
        let value = Value::String("42".into(), CompletionState::Complete);
        let result = coercer.coerce(&value, &TypeIR::int());
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), BamlValue::Int(42));
    }

    #[test]
    fn test_coerce_bool() {
        let coercer = Coercer::new(true);
        let value = Value::Boolean(true);
        let result = coercer.coerce(&value, &TypeIR::bool());
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), BamlValue::Bool(true));
    }

    #[test]
    fn test_coerce_optional_null() {
        let coercer = Coercer::new(true);
        let value = Value::Null;
        let result = coercer.coerce(&value, &TypeIR::optional(TypeIR::string()));
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), BamlValue::Null);
    }

    #[test]
    fn test_coerce_optional_value() {
        let coercer = Coercer::new(true);
        let value = Value::String("hello".into(), CompletionState::Complete);
        let result = coercer.coerce(&value, &TypeIR::optional(TypeIR::string()));
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), BamlValue::String("hello".into()));
    }

    #[test]
    fn test_coerce_list() {
        let coercer = Coercer::new(true);
        let value = Value::Array(
            vec![
                Value::Number(serde_json::Number::from(1), CompletionState::Complete),
                Value::Number(serde_json::Number::from(2), CompletionState::Complete),
            ],
            CompletionState::Complete,
        );
        let result = coercer.coerce(&value, &TypeIR::list(TypeIR::int()));
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), BamlValue::List(vec![BamlValue::Int(1), BamlValue::Int(2)]));
    }

    #[test]
    fn test_coerce_class() {
        let coercer = Coercer::new(true);
        let value = Value::Object(
            vec![
                ("name".into(), Value::String("Alice".into(), CompletionState::Complete)),
                ("age".into(), Value::Number(serde_json::Number::from(30), CompletionState::Complete)),
            ],
            CompletionState::Complete,
        );
        let result = coercer.coerce(&value, &TypeIR::class("Person"));
        assert!(result.is_ok());
        match result.unwrap() {
            BamlValue::Class(name, _) => assert_eq!(name, "Person"),
            _ => panic!("Expected Class"),
        }
    }
}
