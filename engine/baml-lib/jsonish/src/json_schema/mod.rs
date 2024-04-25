mod value_to_bool;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(untagged)]
pub enum SchemaOrBool {
    Schema(Box<JSONSchema7>),
    Bool(bool),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "lowercase")]
pub enum Type {
    String,
    Number,
    Integer,
    Boolean,
    Object,
    Array,
    Null,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct JSONSchema7 {
    #[serde(skip_serializing_if = "Option::is_none", rename = "type")]
    pub type_: Option<Type>,

    #[serde(skip_serializing_if = "Option::is_none", rename = "enum")]
    pub enum_: Option<Vec<Value>>,

    // #[serde(skip_serializing_if = "Option::is_none", rename = "const")]
    // pub const_: Option<Value>,

    // Array specific fields
    #[serde(skip_serializing_if = "Option::is_none")]
    pub items: Option<Box<JSONSchema7>>,

    // Object specific fields
    #[serde(skip_serializing_if = "Option::is_none")]
    pub properties: Option<HashMap<String, JSONSchema7>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub additional_properties: Option<SchemaOrBool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub pattern_properties: Option<HashMap<String, JSONSchema7>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<Vec<String>>,

    // Combinators
    #[serde(skip_serializing_if = "Option::is_none")]
    pub any_of: Option<Vec<JSONSchema7>>,
    // #[serde(skip_serializing_if = "Option::is_none")]
    // pub all_of: Option<Vec<JSONSchema7>>,
    // #[serde(skip_serializing_if = "Option::is_none")]
    // pub one_of: Option<Vec<JSONSchema7>>,

    // Adding support for $ref
    #[serde(rename = "$ref", skip_serializing_if = "Option::is_none")]
    pub ref_: Option<String>,

    // Support for definitions
    // Alternatively, using $defs as recommended in newer drafts
    #[serde(rename = "definitions", skip_serializing_if = "Option::is_none")]
    pub definitions: Option<HashMap<String, JSONSchema7>>,

    #[serde(rename = "$defs", skip_serializing_if = "Option::is_none")]
    pub defs: Option<HashMap<String, JSONSchema7>>,
}

impl JSONSchema7 {
    pub fn coerce(&self, value: &Value) -> Result<Value> {
        match self.type_ {
            Some(Type::String) => self.coerce_string(value).map(Value::String),
            Some(Type::Number) => self.coerce_float(value).map(|n| json!(n)),
            Some(Type::Integer) => self.coerce_int(value).map(|n| json!(n)),
            Some(Type::Boolean) => self.coerce_boolean(value).map(Value::Bool),
            Some(Type::Object) => self.coerce_object(value),
            Some(Type::Array) => self.coerce_array(value),
            Some(Type::Null) => self.coerce_null(value),
            None => {
                if self.enum_.is_some() {
                    return self.coerce_enum(value);
                }
                if self.any_of.is_some() {
                    return self.coerce_union(value);
                }
                if let Some(ref ref_) = self.ref_ {
                    return self.coerce_ref(value, ref_);
                }

                anyhow::bail!("Could not coerce value")
            }
        }
    }

    fn coerce_ref(&self, value: &Value, ref_: &str) -> Result<Value> {
        match self.definitions.as_ref().and_then(|d| d.get(ref_)) {
            Some(schema) => schema.coerce(value),
            None => anyhow::bail!("Could not find schema for ref: {}", ref_),
        }
    }

    fn coerce_enum(&self, value: &Value) -> Result<Value> {
        if self.enum_.as_ref().unwrap().contains(value) {
            Ok(value.clone())
        } else {
            anyhow::bail!("Value does not match enum")
        }
    }

    fn coerce_array(&self, value: &Value) -> Result<Value> {
        match self.items {
            Some(ref schema) => {
                let mut coerced = Vec::new();
                match value {
                    Value::Array(arr) => {
                        for v in arr {
                            coerced.push(schema.coerce(v)?);
                        }
                    }
                    _ => {
                        coerced.push(schema.coerce(value)?);
                    }
                }
                Ok(Value::Array(coerced))
            }
            None => anyhow::bail!("Array schema must have items"),
        }
    }

    fn coerce_object(&self, value: &Value) -> Result<Value> {
        let mut coerced = HashMap::new();
        for (k, v) in value.as_object().unwrap() {
            if let Some(schema) = self.properties.as_ref().and_then(|p| p.get(k)) {
                coerced.insert(k.clone(), schema.coerce(v)?);
            } else {
                coerced.insert(k.clone(), v.clone());
            }
        }
        Ok(Value::Object(serde_json::Map::from_iter(
            coerced.into_iter(),
        )))
    }

    fn coerce_string(&self, value: &Value) -> Result<String> {
        match value {
            Value::String(v) => Ok(v.clone()),
            _ => Ok(value.to_string()),
        }
    }

    fn coerce_float(&self, value: &Value) -> Result<f64> {
        match value {
            Value::Number(v) => {
                if let Some(n) = v.as_i64() {
                    return Ok(n as f64);
                }
                if let Some(n) = v.as_f64() {
                    return Ok(n);
                }
                if let Some(n) = v.as_u64() {
                    return Ok(n as f64);
                }
                anyhow::bail!("Value is not an float")
            }
            Value::Array(arr) => {
                if arr.len() == 1 {
                    return self.coerce_float(&arr[0]);
                }
                anyhow::bail!("Value is not a float");
            }
            Value::String(v) => {
                if let Ok(n) = v.parse::<f64>() {
                    return Ok(n);
                }
                anyhow::bail!("Value is not a float");
            }
            Value::Object(m) => {
                if m.len() == 1 {
                    let (_, v) = m.iter().next().unwrap();
                    return self.coerce_float(v);
                }
                anyhow::bail!("Value is not a float");
            }
            _ => anyhow::bail!("Value is not a float"),
        }
    }

    fn coerce_int(&self, value: &Value) -> Result<i64> {
        match value {
            Value::Number(v) => {
                if let Some(n) = v.as_i64() {
                    return Ok(n);
                }
                if let Some(n) = v.as_f64() {
                    return Ok(n as i64);
                }
                if let Some(n) = v.as_u64() {
                    return Ok(n as i64);
                }
                anyhow::bail!("Value is not an integer")
            }
            Value::String(v) => {
                if let Ok(n) = v.parse::<i64>() {
                    return Ok(n);
                }
                if let Ok(n) = v.parse::<f64>() {
                    return Ok(n as i64);
                }
                anyhow::bail!("Value is not a integer");
            }
            _ => anyhow::bail!("Value is not a integer"),
        }
    }

    fn coerce_boolean(&self, value: &Value) -> Result<bool> {
        match value {
            Value::Bool(v) => Ok(*v),
            Value::String(v) => {
                if v.trim().eq_ignore_ascii_case("true") {
                    Ok(true)
                } else if v.trim().eq_ignore_ascii_case("false") {
                    Ok(false)
                } else {
                    anyhow::bail!("Value is not a boolean")
                }
            }
            _ => anyhow::bail!("Value is not a boolean"),
        }
    }

    fn coerce_null(&self, value: &Value) -> Result<Value> {
        Ok(value.clone())
    }

    fn coerce_union(&self, value: &Value) -> Result<Value> {
        for schema in self.any_of.as_ref().unwrap() {
            if schema.coerce(value).is_ok() {
                return schema.coerce(value);
            }
        }
        anyhow::bail!("Value does not match any schema in union");
    }
}
