use std::collections::HashMap;

use anyhow::Result;
use serde::{Deserialize, Deserializer};
use serde_json::{json, Map, Value};

// use self::raw_value::{JsonishValue, N};

mod iterative_parser;
// mod parse_state;
// mod raw_value;

macro_rules! define_structs_and_conversion_to_non_optional {
    ($struct_name:ident, $optional_struct_name:ident, $($field_name:ident: $type:ty => $default:expr),* ) => {
        #[derive(Debug, Clone)]
        pub struct $struct_name {
            $(pub $field_name: $type),*
        }

        #[derive(Debug, Clone, Default, Deserialize)]
        pub struct $optional_struct_name {
            $(pub $field_name: Option<$type>),*
        }

        impl $optional_struct_name {
            pub fn to_rules(&self) -> Rules {
                self.into()
            }

            pub fn to_rules_with_defaults(&self, defaults: &Rules) -> Rules {
                $struct_name {
                    $($field_name: self.$field_name.as_ref().map(|x| x.clone()).unwrap_or(defaults.$field_name.clone())),*
                }
            }
        }

        impl From<&$optional_struct_name> for $struct_name {
            fn from(item: &$optional_struct_name) -> Self {
                $struct_name {
                    $($field_name: item.$field_name.as_ref().map(|x| x.clone()).unwrap_or($default)),*
                }
            }
        }
    };
}

// Define the structs and the conversion logic with default values
define_structs_and_conversion_to_non_optional!(
    Rules,
    OptionalRules,
    strict: bool => false
);

#[derive(Deserialize)]
#[serde(untagged)]
pub(crate) enum Type {
    NoOptions(BaseType),
    TypeWithRules(TypeWithRules),
}

pub(crate) struct TypeWithRules {
    r#type: BaseType,
    rules: OptionalRules,
}

impl<'de> Deserialize<'de> for TypeWithRules {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let mut value: Map<String, Value> = Map::deserialize(deserializer)?;

        // Pop out `rules` and deserialize it separately
        let rules_value = value
            .remove("rules")
            .ok_or_else(|| serde::de::Error::missing_field("rules"))?;
        let rules: OptionalRules =
            serde_json::from_value(rules_value).map_err(serde::de::Error::custom)?;

        // What remains in `value` is now used to deserialize into `BaseType`
        // We reconstruct a Value from the modified map for this purpose
        let type_value = Value::Object(value);
        let r#type: BaseType =
            serde_json::from_value(type_value).map_err(serde::de::Error::custom)?;

        Ok(TypeWithRules { r#type, rules })
    }
}

#[derive(Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "lowercase")]
pub(crate) enum BaseType {
    Null,
    Int,
    Float,
    String,
    Bool,
    Array(Box<Type>),
    Union(Vec<Type>),
    Optional(Box<Type>),
    Enum(Vec<EnumValue>),
    Object(Vec<ObjectField>),
}

#[derive(Deserialize)]
pub(crate) struct ObjectField {
    name: String,
    alias: Option<String>,
    value: Type,
    required: bool,
}

impl ObjectField {
    fn matches(&self, key: &str) -> bool {
        self.name == key || self.alias.as_deref() == Some(key)
    }
}

#[derive(Deserialize)]
pub(crate) struct EnumValue {
    name: String,
    aliases: Vec<String>,
}

impl EnumValue {
    fn matches(&self, value: &str) -> bool {
        value.eq_ignore_ascii_case(&self.name)
            || self
                .aliases
                .iter()
                .any(|alias| alias.eq_ignore_ascii_case(value))
    }
}

pub(crate) struct Target {
    pub(crate) schema: Type,
}

impl Target {
    pub fn parse(&self, raw_string: &str) -> Result<serde_json::Value> {
        let raw_value = JsonishValue::from_str(raw_string);
        let mut state = parse_state::ParseState::default();
        let res = self.schema.parse(&raw_value, None, &mut state);
        match res {
            Ok(value) => {
                if !state.is_empty() {
                    println!("{:?}", state);
                }
                Ok(value)
            }
            Err(()) => Err(anyhow::anyhow!(state.as_error())),
        }
    }
}

impl Type {
    fn rules(&self, parent_rules: Option<&Rules>) -> Rules {
        match (parent_rules, self) {
            (Some(parent_rules), Type::NoOptions(_)) => parent_rules.clone(),
            (Some(parent_rules), Type::TypeWithRules(TypeWithRules { rules, .. })) => {
                rules.to_rules_with_defaults(parent_rules)
            }
            (None, Type::NoOptions(_)) => OptionalRules::default().to_rules(),
            (None, Type::TypeWithRules(TypeWithRules { rules, .. })) => rules.into(),
        }
    }

    fn r#type(&self) -> &BaseType {
        match self {
            Type::NoOptions(base) => base,
            Type::TypeWithRules(TypeWithRules { r#type, .. }) => r#type,
        }
    }

    fn parse<'a, 'b>(
        &'a self,
        raw_value: &'a JsonishValue,
        parent_rules: Option<&Rules>,
        state: &'b mut parse_state::ParseState<'a>,
    ) -> Result<serde_json::Value, ()> {
        self.r#type()
            .parse(raw_value, &self.rules(parent_rules), state)
    }
}

impl BaseType {
    fn parse<'a, 'b>(
        &'a self,
        raw_value: &'a JsonishValue,
        rules: &Rules,
        state: &'b mut parse_state::ParseState<'a>,
    ) -> Result<serde_json::Value, ()> {
        match self {
            BaseType::Null => {
                if let Some(_) = raw_value.as_null(state) {
                    Ok((serde_json::Value::Null))
                } else {
                    state.new_not_parsed(raw_value, "null");
                    Err(())
                }
            }
            BaseType::Int => {
                if let Some(val) = raw_value.as_number(state) {
                    match val {
                        N::PosInt(val) => Ok((json!(*val))),
                        N::NegInt(val) => Ok((json!(*val))),
                        N::Float(val) => {
                            state.new_precision_loss(raw_value, "float -> int");
                            if rules.strict {
                                Err(())
                            } else {
                                Ok((json!((*val).round() as i64)))
                            }
                        }
                        _ => {
                            state.new_not_parsed(raw_value, "int");
                            Err(())
                        }
                    }
                } else {
                    state.new_not_parsed(raw_value, "int");
                    Err(())
                }
            }
            BaseType::Float => {
                if let Some(val) = raw_value.as_number(state) {
                    match val {
                        N::NegInt(val) => Ok((json!(*val as f64))),
                        N::PosInt(val) => Ok((json!(*val as f64))),
                        N::Float(val) => Ok((json!(*val as f64))),
                        _ => {
                            state.new_not_parsed(raw_value, "float");
                            Err(())
                        }
                    }
                } else {
                    state.new_not_parsed(raw_value, "float");
                    Err(())
                }
            }
            BaseType::Bool => {
                if let Some(val) = raw_value.as_bool(state) {
                    Ok((json!(val)))
                } else {
                    state.new_not_parsed(raw_value, "bool");
                    Err(())
                }
            }
            BaseType::String => {
                if let Some(val) = raw_value.as_string(state) {
                    Ok((json!(*val)))
                } else {
                    state.new_not_parsed(raw_value, "string");
                    Err(())
                }
            }
            BaseType::Enum(_allowed_values) => {
                state.new_not_parsed(raw_value, "enum");
                Err(())
            }
            BaseType::Array(inner) => {
                if let Some(arr) = raw_value.as_array(state) {
                    let mut result = Vec::new();
                    for item in arr {
                        match inner.parse(item, Some(rules), state) {
                            Ok((value)) => {
                                result.push(value);
                            }
                            Err(err_state) => {}
                        }
                    }
                    Ok((json!(result)))
                } else {
                    state.new_not_parsed(raw_value, "array");
                    Err(())
                }
            }
            BaseType::Object(fields) => {
                if let Some(obj) = raw_value.as_object(state) {
                    let mut result = serde_json::Map::new();
                    let mut missing_fields = Vec::new();

                    // For every key-value pair in parsed_obj, get all (key (as str), index) pairs.
                    let mut keys = obj
                        .iter()
                        .enumerate()
                        .filter_map(|(i, (key, _))| {
                            // Check if the key can be parsed a string, bool, or number.
                            if let Ok((value)) = BaseType::String.parse(key, rules, state) {
                                Some((value.to_string(), i))
                            } else if let Ok((value)) = BaseType::Int.parse(key, rules, state) {
                                Some((value.to_string(), i))
                            } else if let Ok((value)) = BaseType::Float.parse(key, rules, state) {
                                Some((value.to_string(), i))
                            } else if let Ok((value)) = BaseType::Bool.parse(key, rules, state) {
                                Some((value.to_string(), i))
                            } else {
                                state.new_not_parsed(raw_value, "object key");
                                None
                            }
                        })
                        .collect::<HashMap<_, _>>();

                    for field in fields {
                        if let Some(idx) = keys.remove(&field.name) {
                            let value = &obj[idx].1;
                            match field.value.parse(value, Some(rules), state) {
                                Ok((value)) => {
                                    result.insert(field.name.clone(), value);
                                }
                                Err(err_state) => {}
                            }
                        } else if field.required {
                            missing_fields.push(field.name.clone());
                        }
                    }
                    // Get excess fields
                    let excess_fields = keys.keys().map(|k| k.as_str()).collect::<Vec<_>>();
                    if !excess_fields.is_empty() {
                        state.new_excess_fields(raw_value, excess_fields.join(", ").as_str());
                    }

                    if !missing_fields.is_empty() {
                        state.new_missing_fields(raw_value, &missing_fields.join(", "));
                        Err(())
                    } else {
                        Ok((json!(result)))
                    }
                } else {
                    state.new_not_parsed(raw_value, "object");
                    Err(())
                }
            }
            BaseType::Union(_) => {
                state.new_not_parsed(raw_value, "union");
                Err(())
            }
            BaseType::Optional(inner) => {
                if let Some(_) = raw_value.as_null(state) {
                    Ok((serde_json::Value::Null))
                } else {
                    inner.parse(raw_value, Some(rules), state)
                }
            }
        }
    }
}
