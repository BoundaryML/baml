use std::{any::Any, fs::File};

use anyhow::Result;
use clap::error;
use internal_baml_core::ir::repr::{FieldType, IntermediateRepr};

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

pub fn validate_value(
    ir: &IntermediateRepr,
    field_type: &FieldType,
    value: &serde_json::Value,
    scope: &mut ScopeStack,
) {
    match field_type {
        FieldType::Primitive(t) => {
            if !match t {
                internal_baml_core::ast::TypeValue::String => value.is_string(),
                internal_baml_core::ast::TypeValue::Int => value.is_i64() || value.is_u64(),
                internal_baml_core::ast::TypeValue::Float => {
                    value.is_f64() || value.is_i64() || value.is_u64()
                }
                internal_baml_core::ast::TypeValue::Bool => value.is_boolean(),
                internal_baml_core::ast::TypeValue::Char => {
                    value.is_string() && value.as_str().unwrap().chars().count() == 1
                }
                internal_baml_core::ast::TypeValue::Null => value.is_null(),
            } {
                scope.push_error(format!("Expected type {:?}, got `{}`", t, value));
            }
        }
        FieldType::Enum(name) => {
            if let Ok(e) = ir.find_enum(name) {
                match value.as_str() {
                    Some(s) => {
                        if !e.walk_values().find(|v| v.0 == s).is_some() {
                            scope.push_error(format!(
                                "Invalid enum value for {}: expected one of ({}), got `{}`",
                                name,
                                e.walk_values()
                                    .map(|v| v.0.as_str())
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
                            if let Some(v) = obj.get(&f.name) {
                                validate_value(ir, &f.r#type.elem, v, scope);
                            } else if !f.r#type.elem.is_optional() {
                                scope.push_error(format!("Missing required field `{}`", f.name));
                            }
                        }

                        for f in obj.keys() {
                            if !c.walk_fields().any(|f2| f2.name.as_str() == f) {
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
                    validate_value(ir, item, v, scope);
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
                validate_value(ir, option, value, &mut scope);
                if !scope.has_errors() {
                    return;
                }
            }
            scope.push_error(format!("Expected one of ({}), got `{}`", field_type, value));
        }
        FieldType::Optional(inner) => {
            if !value.is_null() {
                let mut inner_scope = ScopeStack::new();
                validate_value(ir, inner, value, &mut inner_scope);
                if inner_scope.has_errors() {
                    scope.push_error(format!("Expected optional {}, got `{}`", inner, value));
                }
            }
        }
    }
}
