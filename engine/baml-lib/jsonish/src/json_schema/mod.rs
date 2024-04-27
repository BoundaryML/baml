mod deserialize_flags;
mod value_to_bool;

use anyhow::Result;
use internal_baml_core::{
    ast::TypeValue,
    ir::{
        repr::{FieldType, IntermediateRepr},
        EnumValueWalker, EnumWalker, IRHelper,
    },
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{collections::HashMap, fmt::format};

pub use self::deserialize_flags::DeserializerConditions;
use self::deserialize_flags::{Flag, SerializationContext};

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
        scope: Vec<String>,
        ir: &IntermediateRepr,
        env: &HashMap<String, String>,
        value: Option<&serde_json::Value>,
    ) -> Result<(serde_json::Value, DeserializerConditions), SerializationContext>;
}

impl ValueCoerce for FieldType {
    fn coerce(
        &self,
        scope: Vec<String>,
        ir: &IntermediateRepr,
        env: &HashMap<String, String>,
        value: Option<&serde_json::Value>,
    ) -> Result<(serde_json::Value, DeserializerConditions), SerializationContext> {
        match self {
            FieldType::Primitive(primitive) => match parse_primitive(primitive, ir, env, value) {
                Ok(v) => Ok(v),
                Err(e) => Err(SerializationContext::from_error(
                    scope,
                    format!("Could not parse {}:\n{}", self, e),
                    value.cloned(),
                )),
            },
            FieldType::Enum(name) => match parse_enum(ir, env, name, value) {
                Ok(v) => Ok(v),
                Err(e) => Err(SerializationContext::from_error(
                    scope,
                    format!("Could not parse enum: {}\n{}", self, e),
                    value.cloned(),
                )),
            },
            FieldType::Class(_) => todo!(),
            FieldType::List(item) => match value {
                Some(Value::Array(items)) => {
                    let res = items
                        .iter()
                        .enumerate()
                        .map(|(idx, v)| {
                            let mut scope = scope.clone();
                            scope.push(format!("{}", idx));
                            item.coerce(scope, ir, env, Some(v))
                        })
                        .filter_map(|r| r.ok())
                        .collect::<Vec<_>>();

                    let parsed = res.iter().map(|v| v.0.clone()).collect::<Vec<_>>();

                    // TODO: @hellovai determine how to send up flags for each field.

                    Ok((
                        serde_json::Value::Array(parsed),
                        DeserializerConditions::new(),
                    ))
                }
                Some(inner) => {
                    let res = item.coerce(scope.clone(), ir, env, Some(inner));
                    match res {
                        Ok((v, flags)) => Ok((json!([v]), flags)),
                        Err(e) => Err(SerializationContext::from_error(
                            scope,
                            format!("Could not parse list: {}\n{}", self, e),
                            value.cloned(),
                        )),
                    }
                }
                None => Ok((json!([]), DeserializerConditions::new())),
            },
            FieldType::Union(options) => match parse_union(&scope, ir, env, options, value) {
                Ok(v) => Ok(v),
                Err(e) => Err(SerializationContext::from_error(
                    scope,
                    format!("Could not parse union: {}\n{}", self, e),
                    value.cloned(),
                )),
            },
            FieldType::Optional(inner) => match value {
                Some(value) => {
                    if value.is_null() {
                        Ok((serde_json::Value::Null, DeserializerConditions::new()))
                    } else {
                        match inner.coerce(scope, ir, env, Some(value)) {
                            Ok(r) => Ok(r),
                            Err(e) => {
                                // TODO: Add a rule to allow this flag.
                                Ok((
                                    serde_json::Value::Null,
                                    DeserializerConditions::new().with_flag(
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

fn pick_best_match_array<T>(
    res: &Vec<Result<(Value, DeserializerConditions), T>>,
) -> Result<(Value, DeserializerConditions)>
where
    T: std::fmt::Display,
{
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
    // Since we already checked for at least one result, this is safe.
    let idx = *res_index.first().unwrap();

    // Get the value and flags of the first result (could have failed as well).
    match res.get(idx) {
        Some(Ok((v, flags))) => {
            // Get all the other possible values.
            let others = res
                .iter()
                .enumerate()
                .filter_map(|(i, r)| match r {
                    Ok((value, _)) if i != idx => Some(value.clone()),
                    _ => None,
                })
                .collect::<Vec<_>>();

            if others.is_empty() {
                Ok((v.to_owned(), flags.clone()))
            } else {
                Ok((
                    v.to_owned(),
                    flags.clone().with_flag(Flag::FirstMatch(others)),
                ))
            }
        }
        Some(Err(_)) | None => {
            // If there are multiple errors, we can't really do anything.
            // Return all the errors.

            let errs = res
                .iter()
                .filter_map(|r| match r {
                    Ok(_) => None,
                    Err(e) => Some(e.to_string()),
                })
                .collect::<Vec<_>>()
                .join("\n");

            anyhow::bail!("{}", errs);
        }
    }
}

fn parse_union(
    scope: &Vec<String>,
    ir: &IntermediateRepr,
    env: &HashMap<String, String>,
    options: &Vec<FieldType>,
    value: Option<&Value>,
) -> Result<(Value, DeserializerConditions)> {
    if options.is_empty() {
        anyhow::bail!("Union has no options");
    }

    let res = options
        .iter()
        .map(|f| f.coerce(scope.clone(), ir, env, value))
        .collect::<Vec<_>>();

    pick_best_match_array(&res)
}

fn parse_primitive(
    primitive: &TypeValue,
    ir: &IntermediateRepr,
    env: &HashMap<String, String>,
    value: Option<&Value>,
) -> Result<(Value, DeserializerConditions)> {
    let value = match value {
        Some(value) => value,
        None => {
            // If the value is None, we can't parse it.
            anyhow::bail!("No value to parse");
        }
    };

    // If the value is a collection, we may need to parse each element and get the one with the best match.
    match value {
        Value::Array(items) => {
            let parsed = items
                .iter()
                .map(|v| parse_primitive(primitive, ir, env, Some(v)))
                .collect::<Vec<_>>();
            pick_best_match_array(&parsed)
        }
        Value::Object(kv) => {
            if kv.len() == 1 {
                let (k, v) = kv.iter().next().unwrap();
                let res = parse_primitive(primitive, ir, env, Some(v))?;
                Ok((res.0, res.1.with_flag(Flag::ObjectToField(value.clone()))))
            } else {
                anyhow::bail!("Object has more than one key")
            }
        }
        Value::String(s) => {
            // First do some primitive type checking.
            match s.as_str() {
                "null" => parse_primitive(primitive, ir, env, Some(&Value::Null)),
                "true" => parse_primitive(primitive, ir, env, Some(&Value::Bool(true))),
                "false" => parse_primitive(primitive, ir, env, Some(&Value::Bool(false))),
                // Special case for numbers.
                _ => {
                    if let Ok(n) = s.parse::<i64>() {
                        return parse_primitive(primitive, ir, env, Some(&json!(n)));
                    } else if let Ok(n) = s.parse::<f32>() {
                        return parse_primitive(primitive, ir, env, Some(&json!(n)));
                    }

                    // If the value is a string, we need to parse it.
                    let mut flags = DeserializerConditions::new();

                    let res = match primitive {
                        TypeValue::Char => match s.len() {
                            0 => anyhow::bail!("String is not a char"),
                            1 => json!(s.chars().next().unwrap()),
                            _ => {
                                flags.add_flag(Flag::StringToChar(s.clone()));
                                json!(s.chars().next().unwrap())
                            }
                        },
                        TypeValue::Int => anyhow::bail!("String is not an int"),
                        TypeValue::Float => anyhow::bail!("String is not a float"),
                        TypeValue::Bool => match s.to_ascii_lowercase().trim() {
                            "true" => {
                                flags.add_flag(Flag::StringToBool(s.clone()));
                                json!(true)
                            }
                            "false" => {
                                flags.add_flag(Flag::StringToBool(s.clone()));
                                json!(false)
                            }
                            _ => anyhow::bail!("String is not a bool"),
                        },
                        TypeValue::Null => match s.to_ascii_lowercase().trim() {
                            "null" => {
                                flags.add_flag(Flag::StringToNull(s.clone()));
                                json!(null)
                            }
                            _ => {
                                flags.add_flag(Flag::NullButHadValue(value.clone()));
                                json!(null)
                            }
                        },
                        TypeValue::String => json!(s),
                        // TODO: double check?
                        TypeValue::Image => json!(s),
                    };

                    Ok((res, flags))
                }
            }
        }
        Value::Null => match primitive {
            TypeValue::Null => Ok((json!(null), DeserializerConditions::new())),
            _ => anyhow::bail!("Value is not null"),
        },
        Value::Bool(b) => match primitive {
            TypeValue::String => Ok((json!(b.to_string()), DeserializerConditions::new())),
            TypeValue::Int => anyhow::bail!("Value is not an int"),
            TypeValue::Float => anyhow::bail!("Value is not an int"),
            TypeValue::Bool => Ok((json!(*b), DeserializerConditions::new())),
            TypeValue::Char => anyhow::bail!("Value is not a char"),
            TypeValue::Null => Ok((
                json!(null),
                DeserializerConditions::new().with_flag(Flag::NullButHadValue(value.clone())),
            )),
            TypeValue::Image => anyhow::bail!("Value is not an image"),
        },
        Value::Number(n) => match primitive {
            TypeValue::String => Ok((json!(n.to_string()), DeserializerConditions::new())),
            TypeValue::Int => {
                if let Some(n) = n.as_i64() {
                    Ok((json!(n), DeserializerConditions::new()))
                } else if let Some(n) = n.as_u64() {
                    Ok((json!(n), DeserializerConditions::new()))
                } else if let Some(n) = n.as_f64() {
                    Ok((
                        json!(n.round() as i64),
                        DeserializerConditions::new().with_flag(Flag::FloatToInt(n)),
                    ))
                } else {
                    anyhow::bail!("Value is not an int")
                }
            }
            TypeValue::Float => {
                if let Some(n) = n.as_f64() {
                    Ok((json!(n), DeserializerConditions::new()))
                } else if let Some(n) = n.as_i64() {
                    Ok((json!(n as f64), DeserializerConditions::new()))
                } else if let Some(n) = n.as_u64() {
                    Ok((json!(n as f64), DeserializerConditions::new()))
                } else {
                    anyhow::bail!("Value is not a float")
                }
            }
            TypeValue::Bool => anyhow::bail!("Value is not a bool"),
            TypeValue::Char => anyhow::bail!("Value is not a char"),
            TypeValue::Null => Ok((
                json!(null),
                DeserializerConditions::new().with_flag(Flag::NullButHadValue(value.clone())),
            )),
            TypeValue::Image => anyhow::bail!("Value is not an image"),
        },
    }
}

fn parse_enum(
    ir: &IntermediateRepr,
    env: &HashMap<String, String>,
    name: &str,
    value: Option<&Value>,
) -> Result<(Value, DeserializerConditions)> {
    let enm = ir.find_enum(name)?;

    // For optimization, we could do this once.
    let candidates = enm
        .walk_values()
        .filter_map(|v| match v.skip(env) {
            Ok(true) => return None,
            Ok(false) => match v.valid_values(env) {
                Ok(valid_values) => Some(Ok((v, valid_values))),
                Err(e) => return Some(Err(e)),
            },
            Err(e) => return Some(Err(e)),
        })
        .collect::<Result<Vec<_>>>()?;

    let value = match value {
        Some(value) => value,
        None => {
            // If the value is None, we can't parse it.
            anyhow::bail!("No value to parse");
        }
    };

    let mut flags = DeserializerConditions::new();
    let value_str = match value {
        serde_json::Value::String(s) => s.to_ascii_lowercase(),
        _ => {
            flags.add_flag(Flag::ObjectToString(value.clone()));
            serde_json::to_string(value)?.to_ascii_lowercase()
        }
    };
    let value_str = value_str.trim();

    let remove_punctuation = |s: &str| {
        s.chars()
            .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
            .collect::<String>()
    };

    match enum_match_strategy(&value_str, &candidates, flags.clone()) {
        Some(res) => Ok(res),
        None => {
            let only_w_str = remove_punctuation(&value_str);
            let no_punc_candidates = candidates
                .iter()
                .map(|(e, valid_values)| {
                    (
                        *e,
                        valid_values
                            .iter()
                            .map(|v| remove_punctuation(v))
                            .collect::<Vec<_>>(),
                    )
                })
                .collect::<Vec<_>>();
            match enum_match_strategy(&only_w_str, &no_punc_candidates, flags.clone()) {
                Some((val, flags)) => Ok((
                    val,
                    flags.with_flag(Flag::StrippedNonAlphaNumeric(value_str.into())),
                )),
                None => {
                    // If we still can't find a match, we can't parse the value.
                    let values = candidates
                        .iter()
                        .map(|(e, values)| {
                            // Format the enum values for the error message.
                            // "{name} - ({values|map|truncate(50 chars)})"

                            let name = e.name();
                            let values = values
                                .iter()
                                // Find all non-exact matches.
                                .filter(|v| !v.as_str().eq_ignore_ascii_case(name))
                                .map(|v| {
                                    if v.len() > 17 {
                                        format!("'{}...'", v[..17].to_string())
                                    } else {
                                        format!("'{}'", v)
                                    }
                                })
                                .collect::<Vec<_>>()
                                .join(", ");

                            if values.is_empty() {
                                format!("{}", name)
                            } else {
                                format!("{} (also matches: {})", name, values)
                            }
                        })
                        .collect::<Vec<_>>()
                        .join("\n");
                    anyhow::bail!("{}", values)
                }
            }
        }
    }
}

fn enum_match_strategy(
    value_str: &str,
    candidates: &Vec<(EnumValueWalker<'_>, Vec<String>)>,
    mut flags: DeserializerConditions,
) -> Option<(Value, DeserializerConditions)> {
    // Try and look for a value that matches the value.
    // First search for exact matches
    for (e, valid_values) in candidates {
        // Consider adding a flag for case insensitive match.
        if valid_values
            .iter()
            .any(|v| v.eq_ignore_ascii_case(value_str))
        {
            // We did nothing fancy, so no extra flags.
            return Some((json!(e.name()), flags));
        }
    }

    // Now find all the enums which occur in the value, by frequency.
    let mut result = candidates
        .iter()
        .filter_map(|(e, valid_names)| {
            // Check how many counts of the enum are in the value.
            let match_count_pos = valid_names
                .iter()
                .filter_map(|v| {
                    let matches = value_str.match_indices(v);
                    // Return (count, first_idx)
                    matches.fold(None, |acc, (idx, _)| match acc {
                        Some((count, prev_idx)) => Some((count + 1, prev_idx)),
                        None => Some((1, idx)),
                    })
                })
                .reduce(|a, b| match a.0.cmp(&b.0) {
                    // Return the one with more matches.
                    std::cmp::Ordering::Less => b,
                    std::cmp::Ordering::Greater => a,
                    // Return the one that matches earlier
                    std::cmp::Ordering::Equal => match a.1.cmp(&b.1) {
                        std::cmp::Ordering::Less => a,
                        _ => b,
                    },
                });
            match_count_pos.map(|(count, pos)| (count, pos, e))
        })
        .collect::<Vec<_>>();

    // Sort by max count, then min pos.
    result.sort_by(|a, b| match a.0.cmp(&b.0) {
        std::cmp::Ordering::Less => std::cmp::Ordering::Greater,
        std::cmp::Ordering::Greater => std::cmp::Ordering::Less,
        std::cmp::Ordering::Equal => a.1.cmp(&b.1),
    });

    // Filter for max count.
    let max_count = result.first().map(|r| r.0).unwrap_or(0);
    result.retain(|r| r.0 == max_count);

    // Return the best match if there is one.
    if let Some((_, _, e)) = result.first() {
        flags.add_flag(Flag::SubstringMatch(value_str.into()));

        if result.len() > 1 {
            flags.add_flag(Flag::FirstMatch(
                result.iter().map(|(_, _, e)| json!(e.name())).collect(),
            ));
        }

        return Some((json!(e.name()), flags));
    }

    None
}
