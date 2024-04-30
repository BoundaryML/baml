use anyhow::Error;
use internal_baml_jinja::{BamlArgType, BamlImage, ImageBase64, ImageUrl};

use crate::ir::{FieldType, IntermediateRepr, TypeValue};

use super::{scope_diagnostics::ScopeStack, IRHelper};

#[derive(Default)]
pub struct ParameterError {
    vec: Vec<String>,
}

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

pub fn to_baml_arg(
    ir: &IntermediateRepr,
    field_type: &FieldType,
    value: &serde_json::Value,
    scope: &mut ScopeStack,
) -> BamlArgType {
    match field_type {
        FieldType::Primitive(t) => {
            let error = || {
                scope.push_error(format!("Expected type {:?}, got `{}`", t, value));
                BamlArgType::Unsupported("Error".to_string())
            };

            match t {
                TypeValue::String if value.is_string() => {
                    BamlArgType::String(value.as_str().unwrap().to_string())
                }
                TypeValue::Int if value.is_i64() => BamlArgType::Int(value.as_i64().unwrap()),
                // TODO: should we use as_u64()?
                TypeValue::Int if value.is_u64() => BamlArgType::Int(value.as_i64().unwrap()),
                TypeValue::Float if value.is_f64() => BamlArgType::Float(value.as_f64().unwrap()),
                TypeValue::Bool if value.is_boolean() => {
                    BamlArgType::Bool(value.as_bool().unwrap())
                }
                TypeValue::Char
                    if value.is_string() && value.as_str().unwrap().chars().count() == 1 =>
                {
                    // TODO: create char type?
                    BamlArgType::String(value.as_str().unwrap().chars().next().unwrap().to_string())
                }
                TypeValue::Null if value.is_null() => BamlArgType::None,
                TypeValue::Image if value.is_object() => {
                    let map = value.as_object().unwrap(); // assuming value is an object
                    if let Some(url) = map.get("url") {
                        if let Some(url_str) = url.as_str() {
                            BamlArgType::Image(BamlImage::Url(ImageUrl::new(url_str.to_string())))
                        } else {
                            error()
                        }
                    } else if let Some(base64) = map.get("base64") {
                        if let Some(base64_str) = base64.as_str() {
                            BamlArgType::Image(BamlImage::Base64(ImageBase64::new(
                                base64_str.to_string(),
                            )))
                        } else {
                            error()
                        }
                    } else {
                        error()
                    }
                }
                _ => error(),
            }
        }
        FieldType::Enum(name) => {
            if let Ok(e) = ir.find_enum(name) {
                match value.as_str() {
                    Some(s) => {
                        if !e.walk_values().find(|v| v.item.elem.0 == s).is_some() {
                            scope.push_error(format!(
                                "Invalid enum value for {}: expected one of ({}), got `{}`",
                                name,
                                e.walk_values()
                                    .map(|v| v.item.elem.0.as_str())
                                    .collect::<Vec<&str>>()
                                    .join(" | "),
                                s
                            ));
                        }
                    }
                    None => {
                        scope.push_error(format!(
                            "Expected enum value for {}, got `{}`",
                            name, value
                        ));
                    }
                }
            } else {
                scope.push_error(format!("Enum {} not found", name));
            }
        }
        FieldType::Class(name) => {
            if let Ok(c) = ir.find_class(name) {
                match value {
                    serde_json::Value::Object(obj) => {
                        for f in c.walk_fields() {
                            if let Some(v) = obj.get(f.name()) {
                                to_baml_arg(ir, f.r#type(), v, scope);
                            } else if !f.r#type().is_optional() {
                                scope.push_error(format!("Missing required field `{}`", f.name()));
                            }
                        }

                        for f in obj.keys() {
                            if !c.walk_fields().any(|f2| f2.name() == f) {
                                scope.push_error(format!(
                                    "Field `{}` not found in class `{}`",
                                    f, name
                                ));
                            }
                        }
                    }
                    _ => {
                        scope.push_error(format!(
                            "Expected object for class {}, got `{}`",
                            name, value
                        ));
                    }
                }
            } else {
                scope.push_error(format!("Class {} not found", name));
            }
        }
        FieldType::List(item) => match value.as_array() {
            Some(arr) => {
                for (idx, v) in arr.iter().enumerate() {
                    scope.push(format!("{}", idx));
                    to_baml_arg(ir, item, v, scope);
                    scope.pop(false);
                }
            }
            None => {
                scope.push_error(format!("Expected a list of {}, got `{}`", item, value));
            }
        },
        FieldType::Tuple(_) => unimplemented!("Tuples are not yet supported"),
        FieldType::Map(_, _) => unimplemented!("Maps are not yet supported"),
        FieldType::Union(options) => {
            for option in options {
                let mut scope = ScopeStack::new();
                to_baml_arg(ir, option, value, &mut scope);
                if !scope.has_errors() {
                    return;
                }
            }
            scope.push_error(format!("Expected one of ({}), got `{}`", field_type, value));
        }
        FieldType::Optional(inner) => {
            if !value.is_null() {
                let mut inner_scope = ScopeStack::new();
                to_baml_arg(ir, inner, value, &mut inner_scope);
                if inner_scope.has_errors() {
                    scope.push_error(format!("Expected optional {}, got `{}`", inner, value));
                }
            }
        }
    }
}
