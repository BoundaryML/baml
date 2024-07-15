use baml_types::{BamlMap, BamlMediaType, BamlValue, TypeValue};

use crate::ir::{FieldType, IntermediateRepr};

use super::{scope_diagnostics::ScopeStack, IRHelper};

#[derive(Default)]
pub struct ParameterError {
    vec: Vec<String>,
}

#[allow(dead_code)]
impl ParameterError {
    pub(super) fn required_param_missing(&mut self, param_name: &str) {
        self.vec
            .push(format!("Missing required parameter: {}", param_name));
    }

    pub fn invalid_param_type(&mut self, param_name: &str, expected: &str, got: &str) {
        self.vec.push(format!(
            "Invalid parameter type for {}: expected {}, got {}",
            param_name, expected, got
        ));
    }
}

pub fn validate_arg(
    ir: &IntermediateRepr,
    field_type: &FieldType,
    value: &BamlValue, // original value passed in by user
    scope: &mut ScopeStack,
    allow_implicit_cast_to_string: bool,
) -> Option<BamlValue> {
    match field_type {
        FieldType::Primitive(t) => match t {
            TypeValue::String if matches!(value, BamlValue::String(_)) => Some(value.clone()),
            TypeValue::String if allow_implicit_cast_to_string => match value {
                BamlValue::Int(i) => Some(BamlValue::String(i.to_string())),
                BamlValue::Float(f) => Some(BamlValue::String(f.to_string())),
                BamlValue::Bool(true) => Some(BamlValue::String("true".to_string())),
                BamlValue::Bool(false) => Some(BamlValue::String("false".to_string())),
                BamlValue::Null => Some(BamlValue::String("null".to_string())),
                _ => {
                    scope.push_error(format!("Expected type {:?}, got `{}`", t, value));
                    None
                }
            },
            TypeValue::Int if matches!(value, BamlValue::Int(_)) => Some(value.clone()),
            TypeValue::Float => match value {
                BamlValue::Int(val) => Some(BamlValue::Float(*val as f64)),
                BamlValue::Float(_) => Some(value.clone()),
                _ => {
                    scope.push_error(format!("Expected type {:?}, got `{}`", t, value));
                    None
                }
            },
            TypeValue::Bool if matches!(value, BamlValue::Bool(_)) => Some(value.clone()),
            TypeValue::Null if matches!(value, BamlValue::Null) => Some(value.clone()),
            TypeValue::Image => match value {
                BamlValue::Media(v) => Some(BamlValue::Media(v.clone())),
                BamlValue::Map(kv) => {
                    if let (Some(BamlValue::String(s)), Some(BamlValue::String(media_type_str))) =
                        (kv.get("base64"), kv.get("media_type"))
                    {
                        Some(BamlValue::Media(baml_types::BamlMedia::url(
                            BamlMediaType::Image,
                            s.to_string(),
                            Some(media_type_str.to_string()), // Fix: wrap in Some()
                        )))
                    } else if let (
                        Some(BamlValue::String(s)),
                        Some(BamlValue::String(media_type_str)),
                    ) = (kv.get("base64"), kv.get("media_type"))
                    {
                        Some(BamlValue::Media(baml_types::BamlMedia::base64(
                            BamlMediaType::Image,
                            s.to_string(),
                            media_type_str.to_string(),
                        )))
                    } else {
                        scope.push_error(format!(
                                "Invalid image: expected `url` or (`base64` and `media_type`), got `{}`",
                                value
                            ));
                        None
                    }
                }
                _ => {
                    scope.push_error(format!("Expected type {:?}, got `{}`", t, value));
                    None
                }
            },
            TypeValue::Audio => match value {
                BamlValue::Media(v) => Some(BamlValue::Media(v.clone())),
                BamlValue::Map(kv) => {
                    if let (Some(BamlValue::String(s)), Some(BamlValue::String(media_type_str))) =
                        (kv.get("base64"), kv.get("media_type"))
                    {
                        Some(BamlValue::Media(baml_types::BamlMedia::url(
                            BamlMediaType::Audio,
                            s.to_string(),
                            Some(media_type_str.to_string()), // Fix: wrap in Some()
                        )))
                    } else if let (
                        Some(BamlValue::String(s)),
                        Some(BamlValue::String(media_type_str)),
                    ) = (kv.get("base64"), kv.get("media_type"))
                    {
                        Some(BamlValue::Media(baml_types::BamlMedia::base64(
                            BamlMediaType::Audio,
                            s.to_string(),
                            media_type_str.to_string(),
                        )))
                    } else {
                        scope.push_error(format!(
                                "Invalid audio: expected `url` or (`base64` and `media_type`), got `{}`",
                                value
                            ));
                        None
                    }
                }
                _ => {
                    scope.push_error(format!("Expected type {:?}, got `{}`", t, value));
                    None
                }
            },
            _ => {
                scope.push_error(format!("Expected type {:?}, got `{}`", t, value));
                None
            }
        },
        FieldType::Enum(name) => match value {
            BamlValue::String(s) => {
                if let Ok(e) = ir.find_enum(name) {
                    if e.walk_values().find(|v| v.item.elem.0 == *s).is_some() {
                        Some(BamlValue::Enum(name.to_string(), s.to_string()))
                    } else {
                        scope.push_error(format!(
                            "Invalid enum {}: expected one of ({}), got `{}`",
                            name,
                            e.walk_values()
                                .map(|v| v.item.elem.0.as_str())
                                .collect::<Vec<&str>>()
                                .join(" | "),
                            s
                        ));
                        None
                    }
                } else {
                    scope.push_error(format!("Enum {} not found", name));
                    None
                }
            }
            BamlValue::Enum(n, _) if n == name => Some(value.clone()),
            _ => {
                scope.push_error(format!("Invalid enum {}: Got `{}`", name, value));
                None
            }
        },
        FieldType::Class(name) => match value {
            BamlValue::Class(n, _) if n == name => return Some(value.clone()),
            BamlValue::Class(_, obj) | BamlValue::Map(obj) => match ir.find_class(name) {
                Ok(c) => {
                    let mut fields = BamlMap::new();

                    for f in c.walk_fields() {
                        if let Some(v) = obj.get(f.name()) {
                            if let Some(v) = validate_arg(
                                ir,
                                f.r#type(),
                                v,
                                scope,
                                allow_implicit_cast_to_string,
                            ) {
                                fields.insert(f.name().to_string(), v);
                            }
                        } else if !f.r#type().is_optional() {
                            scope.push_error(format!(
                                "Missing required field `{}` for class {}",
                                f.name(),
                                name
                            ));
                        }
                    }
                    let is_dynamic = c.item.attributes.get("dynamic_type").is_some();
                    if is_dynamic {
                        for (key, value) in obj {
                            if !fields.contains_key(key) {
                                fields.insert(key.clone(), value.clone());
                            }
                        }
                    } else {
                        // We let it slide here... but we should probably emit a warning like this:
                        // for key in obj.keys() {
                        //     if !fields.contains_key(key) {
                        //         scope.push_error(format!(
                        //             "Unexpected field `{}` for class {}. Mark the class as @@dynamic if you want to allow additional fields.",
                        //             key, name
                        //         ));
                        //     }
                        // }
                    }

                    Some(BamlValue::Class(name.to_string(), fields))
                }
                Err(_) => {
                    scope.push_error(format!("Class {} not found", name));
                    None
                }
            },
            _ => {
                scope.push_error(format!("Expected class {}, got `{}`", name, value));
                None
            }
        },
        FieldType::List(item) => match value {
            BamlValue::List(arr) => {
                let mut items = Vec::new();
                for v in arr {
                    if let Some(v) = validate_arg(ir, item, v, scope, allow_implicit_cast_to_string)
                    {
                        items.push(v);
                    }
                }
                Some(BamlValue::List(items))
            }
            _ => {
                scope.push_error(format!("Expected array, got `{}`", value));
                None
            }
        },
        FieldType::Tuple(_) => unimplemented!("Tuples are not yet supported"),
        FieldType::Map(_, _) => unimplemented!("Maps are not yet supported"),
        FieldType::Union(options) => {
            for option in options {
                let mut scope = ScopeStack::new();
                let result =
                    validate_arg(ir, option, value, &mut scope, allow_implicit_cast_to_string);
                if !scope.has_errors() {
                    return result;
                }
            }
            scope.push_error(format!("Expected one of {:?}, got `{}`", options, value));
            None
        }
        FieldType::Optional(inner) => {
            if matches!(value, BamlValue::Null) {
                Some(value.clone())
            } else {
                let mut inner_scope = ScopeStack::new();
                let baml_arg = validate_arg(
                    ir,
                    inner,
                    value,
                    &mut inner_scope,
                    allow_implicit_cast_to_string,
                );
                if inner_scope.has_errors() {
                    scope.push_error(format!("Expected optional {}, got `{}`", inner, value));
                    None
                } else {
                    baml_arg
                }
            }
        }
    }
}
