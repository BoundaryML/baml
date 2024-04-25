mod deserialize_flags;
mod value_to_bool;

use anyhow::Result;
use internal_baml_core::ir::{
    repr::{FieldType, IntermediateRepr},
    IRHelper,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;

pub use self::deserialize_flags::DeserializerConditions;
use self::deserialize_flags::Flag;

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

pub trait ValueCoerce {
    fn coerce(
        &self,
        ir: &IntermediateRepr,
        env: &HashMap<String, String>,
        value: Option<&serde_json::Value>,
    ) -> Result<(serde_json::Value, DeserializerConditions)>;
}

impl ValueCoerce for FieldType {
    fn coerce(
        &self,
        ir: &IntermediateRepr,
        env: &HashMap<String, String>,
        value: Option<&serde_json::Value>,
    ) -> Result<(serde_json::Value, DeserializerConditions)> {
        match self {
            FieldType::Primitive(_) => todo!(),
            FieldType::Enum(name) => {
                let enm = ir.find_enum(name)?;

                // For optimization, we could do this once.
                let candidates = enm
                    .walk_values()
                    .map(|v| Ok((v, v.valid_values(env)?)))
                    .collect::<Result<Vec<_>>>()?;

                if let Some(value) = value {
                    let value_str = match value {
                        serde_json::Value::String(s) => s.clone(),
                        _ => value.to_string(),
                    };

                    // Try and look for a value that matches the value.
                    // First search for exact matches
                    for (v, valid_values) in candidates {
                        todo!()
                    }
                }

                todo!()
            }
            FieldType::Class(_) => todo!(),
            FieldType::List(_) => todo!(),
            FieldType::Union(options) => {
                if options.is_empty() {
                    anyhow::bail!("Union type has no options");
                }

                let mut res = options
                    .iter()
                    .map(|f| f.coerce(ir, env, value))
                    .collect::<Vec<_>>();

                // For all the results, sort them by the number of flags.
                // If there are any results with no flags, return that.
                // Otherwise, return the result with the fewest flags.
                // In case of a tie, return the leftmost result.

                let mut res_index = (0..res.len()).collect::<Vec<_>>();

                res_index.sort_by(|&a, &b| {
                    let a_res = &res[a];
                    let b_res = &res[b];

                    match (a_res, b_res) {
                        (Err(_), Err(_)) => a.cmp(&b),
                        (Ok(_), Err(_)) => std::cmp::Ordering::Less,
                        (Err(_), Ok(_)) => std::cmp::Ordering::Greater,
                        (Ok((_, a_flags)), Ok((_, b_flags))) => match a_flags.cmp(&b_flags) {
                            std::cmp::Ordering::Equal => a.cmp(&b),
                            other => other,
                        },
                    }
                });

                // Get the first result that succeeded.

                let idx = res_index.first().unwrap();

                // Remove all elements behind the first successful result.
                res.truncate(*idx + 1);

                // Get the value and flags of the first successful result.
                let (value, flags) = res.pop().unwrap()?;
                Ok((value, flags))
            }
            FieldType::Optional(inner) => match value {
                Some(value) => {
                    if value.is_null() {
                        Ok((serde_json::Value::Null, DeserializerConditions::new()))
                    } else {
                        match inner.coerce(ir, env, Some(value)) {
                            Ok(r) => Ok(r),
                            Err(e) => {
                                // TODO: Add a rule to allow this flag.
                                Ok((
                                    serde_json::Value::Null,
                                    DeserializerConditions::new().add_flag(
                                        Flag::NullButHadUnparseableValue(e, value.clone()),
                                    ),
                                ))
                            }
                        }
                    }
                }
                None => Ok((serde_json::Value::Null, DeserializerConditions::new())),
            },
            FieldType::Tuple(_) => {
                unimplemented!("Tuple coercion not implemented")
            }
            FieldType::Map(_, _) => {
                unimplemented!("Map coercion not implemented")
            }
        }
    }
}
