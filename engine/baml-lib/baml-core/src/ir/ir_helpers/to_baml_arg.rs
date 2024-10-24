use baml_types::{
    BamlMap, BamlValue, BamlValueWithMeta, Constraint, ConstraintLevel, FieldType, LiteralValue, TypeValue
};
use core::result::Result;
use std::path::PathBuf;

use crate::ir::IntermediateRepr;

use super::{scope_diagnostics::ScopeStack, IRHelper};
use crate::ir::jinja_helpers::evaluate_predicate;

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

pub struct ArgCoercer {
    pub span_path: Option<PathBuf>,
    pub allow_implicit_cast_to_string: bool,
}

impl ArgCoercer {
    pub fn coerce_arg(
        &self,
        ir: &IntermediateRepr,
        field_type: &FieldType,
        value: &BamlValue, // original value passed in by user
        scope: &mut ScopeStack,
    ) -> Result<BamlValue, ()> {
        let value = match field_type.distribute_constraints() {
            (FieldType::Primitive(t), _) => match t {
                TypeValue::String if matches!(value, BamlValue::String(_)) => Ok(value.clone()),
                TypeValue::String if self.allow_implicit_cast_to_string => match value {
                    BamlValue::Int(i) => Ok(BamlValue::String(i.to_string())),
                    BamlValue::Float(f) => Ok(BamlValue::String(f.to_string())),
                    BamlValue::Bool(true) => Ok(BamlValue::String("true".to_string())),
                    BamlValue::Bool(false) => Ok(BamlValue::String("false".to_string())),
                    BamlValue::Null => Ok(BamlValue::String("null".to_string())),
                    _ => {
                        scope.push_error(format!("Expected type {:?}, got `{}`", t, value));
                        Err(())
                    }
                },
                TypeValue::Int if matches!(value, BamlValue::Int(_)) => Ok(value.clone()),
                TypeValue::Float => match value {
                    BamlValue::Int(val) => Ok(BamlValue::Float(*val as f64)),
                    BamlValue::Float(_) => Ok(value.clone()),
                    _ => {
                        scope.push_error(format!("Expected type {:?}, got `{}`", t, value));
                        Err(())
                    }
                },
                TypeValue::Bool if matches!(value, BamlValue::Bool(_)) => Ok(value.clone()),
                TypeValue::Null if matches!(value, BamlValue::Null) => Ok(value.clone()),
                TypeValue::Media(media_type) => match value {
                    BamlValue::Media(v) => Ok(BamlValue::Media(v.clone())),
                    BamlValue::Map(kv) => {
                        if let Some(BamlValue::String(s)) = kv.get("file") {
                            let mime_type = match kv.get("media_type") {
                                Some(t) => match t.as_str() {
                                    Some(s) => Some(s.to_string()),
                                    None => {
                                        scope.push_error(format!("Invalid property `media_type` on file {}: expected string, got {:?}", media_type, t.r#type()));
                                        return Err(());
                                    }
                                },
                                None => None,
                            };

                            for key in kv.keys() {
                                if !vec!["file", "media_type"].contains(&key.as_str()) {
                                    scope.push_error(format!(
                                        "Invalid property `{}` on file {}: `media_type` is the only supported property",
                                        key,
                                        media_type
                                    ));
                                }
                            }
                            match self.span_path.as_ref() {
                                Some(span_path) => {
                                    Ok(BamlValue::Media(baml_types::BamlMedia::file(
                                        media_type.clone(),
                                        span_path.clone(),
                                        s.to_string(),
                                        mime_type,
                                    )))
                                }
                                None => {
                                    scope.push_error("BAML internal error: span is missing, cannot resolve file ref".to_string());
                                    Err(())
                                }
                            }
                        } else if let Some(BamlValue::String(s)) = kv.get("url") {
                            let mime_type = match kv.get("media_type") {
                                Some(t) => match t.as_str() {
                                    Some(s) => Some(s.to_string()),
                                    None => {
                                        scope.push_error(format!("Invalid property `media_type` on file {}: expected string, got {:?}", media_type, t.r#type()));
                                        return Err(());
                                    }
                                },
                                None => None,
                            };
                            for key in kv.keys() {
                                if !vec!["url", "media_type"].contains(&key.as_str()) {
                                    scope.push_error(format!(
                                        "Invalid property `{}` on url {}: `media_type` is the only supported property",
                                        key,
                                        media_type
                                    ));
                                }
                            }
                            Ok(BamlValue::Media(baml_types::BamlMedia::url(
                                media_type.clone(),
                                s.to_string(),
                                mime_type,
                            )))
                        } else if let Some(BamlValue::String(s)) = kv.get("base64") {
                            let mime_type = match kv.get("media_type") {
                                Some(t) => match t.as_str() {
                                    Some(s) => Some(s.to_string()),
                                    None => {
                                        scope.push_error(format!("Invalid property `media_type` on file {}: expected string, got {:?}", media_type, t.r#type()));
                                        return Err(());
                                    }
                                },
                                None => None,
                            };
                            for key in kv.keys() {
                                if !vec!["base64", "media_type"].contains(&key.as_str()) {
                                    scope.push_error(format!(
                                        "Invalid property `{}` on base64 {}: `media_type` is the only supported property",
                                        key,
                                        media_type
                                    ));
                                }
                            }
                            Ok(BamlValue::Media(baml_types::BamlMedia::base64(
                                media_type.clone(),
                                s.to_string(),
                                mime_type,
                            )))
                        } else {
                            scope.push_error(format!(
                                "Invalid image: expected `file`, `url`, or `base64`, got `{}`",
                                value
                            ));
                            Err(())
                        }
                    }
                    _ => {
                        scope.push_error(format!("Expected type {:?}, got `{}`", t, value));
                        Err(())
                    }
                },
                _ => {
                    scope.push_error(format!("Expected type {:?}, got `{}`", t, value));
                    Err(())
                }
            },
            (FieldType::Enum(name), _) => match value {
                BamlValue::String(s) => {
                    if let Ok(e) = ir.find_enum(name) {
                        if e.walk_values().find(|v| v.item.elem.0 == *s).is_some() {
                            Ok(BamlValue::Enum(name.to_string(), s.to_string()))
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
                            Err(())
                        }
                    } else {
                        scope.push_error(format!("Enum {} not found", name));
                        Err(())
                    }
                }
                BamlValue::Enum(n, _) if n == name => Ok(value.clone()),
                _ => {
                    scope.push_error(format!("Invalid enum {}: Got `{}`", name, value));
                    Err(())
                }
            },
            (FieldType::Literal(literal), _) => Ok(match (literal, value) {
                (LiteralValue::Int(lit), BamlValue::Int(baml)) if lit == baml => value.clone(),
                (LiteralValue::String(lit), BamlValue::String(baml)) if lit == baml => {
                    value.clone()
                }
                (LiteralValue::Bool(lit), BamlValue::Bool(baml)) if lit == baml => value.clone(),
                _ => {
                    scope.push_error(format!("Expected literal {:?}, got `{}`", literal, value));
                    return Err(());
                }
            }),
            (FieldType::Class(name), _) => match value {
                BamlValue::Class(n, _) if n == name => Ok(value.clone()),
                BamlValue::Class(_, obj) | BamlValue::Map(obj) => match ir.find_class(name) {
                    Ok(c) => {
                        let mut fields = BamlMap::new();

                        for f in c.walk_fields() {
                            if let Some(v) = obj.get(f.name()) {
                                if let Ok(v) = self.coerce_arg(ir, f.r#type(), v, scope) {
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

                        Ok(BamlValue::Class(name.to_string(), fields))
                    }
                    Err(_) => {
                        scope.push_error(format!("Class {} not found", name));
                        Err(())
                    }
                },
                _ => {
                    scope.push_error(format!("Expected class {}, got `{}`", name, value));
                    Err(())
                }
            },
            (FieldType::List(item), _) => match value {
                BamlValue::List(arr) => {
                    let mut items = Vec::new();
                    for v in arr {
                        if let Ok(v) = self.coerce_arg(ir, item, v, scope) {
                            items.push(v);
                        }
                    }
                    Ok(BamlValue::List(items))
                }
                _ => {
                    scope.push_error(format!("Expected array, got `{}`", value));
                    Err(())
                }
            },
            (FieldType::Tuple(_), _) => {
                scope.push_error(format!("Tuples are not yet supported"));
                Err(())
            }
            (FieldType::Map(k, v), _) => {
                if let BamlValue::Map(kv) = value {
                    let mut map = BamlMap::new();
                    for (key, value) in kv {
                        scope.push("<key>".to_string());
                        let k = self.coerce_arg(ir, k, &BamlValue::String(key.clone()), scope);
                        scope.pop(false);

                        if k.is_ok() {
                            scope.push(key.to_string());
                            if let Ok(v) = self.coerce_arg(ir, v, value, scope) {
                                map.insert(key.clone(), v);
                            }
                            scope.pop(false);
                        }
                    }
                    Ok(BamlValue::Map(map))
                } else {
                    scope.push_error(format!("Expected map, got `{}`", value));
                    Err(())
                }
            }
            (FieldType::Union(options), _) => {
                let mut first_good_result = Err(());
                for option in options {
                    let mut scope = ScopeStack::new();
                    if first_good_result.is_err() {
                        let result = self.coerce_arg(ir, option, value, &mut scope);
                        if !scope.has_errors() {
                            if first_good_result.is_err() {
                            first_good_result = result
                            }
                        }
                    }
                }
                if first_good_result.is_err(){
                    scope.push_error(format!("Expected one of {:?}, got `{}`", options, value));
                    Err(())
                } else {
                    first_good_result
                }
            }
            (FieldType::Optional(inner), _) => {
                if matches!(value, BamlValue::Null) {
                    Ok(value.clone())
                } else {
                    let mut inner_scope = ScopeStack::new();
                    let baml_arg = self.coerce_arg(ir, inner, value, &mut inner_scope);
                    if inner_scope.has_errors() {
                        scope.push_error(format!("Expected optional {}, got `{}`", inner, value));
                        Err(())
                    } else {
                        baml_arg
                    }
                }
            }
            (FieldType::Constrained { .. }, _) => {
                unreachable!("The return value of distribute_constraints can never be FieldType::Constrainted");
            }
        }?;


        let search_for_failures_result = first_failing_assert_nested(ir, &value, field_type).map_err(|e| {
            scope.push_error(format!("Failed to evaluate assert: {:?}", e));
            ()
        })?;
        match search_for_failures_result {
            Some(Constraint {label, expression, ..}) => {
                let msg = label.as_ref().unwrap_or(&expression.0);
                scope.push_error(format!("Failed assert: {msg}"));
                Ok(value)
            }
            None => Ok(value)
        }
    }
}

/// Search a potentially deeply-nested `BamlValue` for any failing asserts,
/// returning the first one encountered.
fn first_failing_assert_nested<'a>(
    ir: &'a IntermediateRepr,
    baml_value: &BamlValue,
    field_type: &'a FieldType
) -> anyhow::Result<Option<Constraint>> {
    let value_with_types = ir.distribute_type(baml_value.clone(), field_type.clone()).expect("distribute type works");
    let first_failure = value_with_types
        .iter()
        .map(|value_node| {
            let (_, constraints) = value_node.meta().distribute_constraints();
            constraints.into_iter().filter_map(|c| {
                let constraint = c.clone();
                let baml_value: BamlValue = value_node.into();
                let result = evaluate_predicate(&&baml_value, &c.expression).map_err(|e| {
                    anyhow::anyhow!(format!("Error evaluating constraint: {:?}", e))
                });
                match result {
                    Ok(false) => if c.level == ConstraintLevel::Assert {Some(Ok(constraint))} else { None },
                    Ok(true) => None,
                    Err(e) => Some(Err(e))

                }
            })
            .collect::<Vec<_>>()
        })
        .map(|x| x.into_iter())
        .flatten()
        .next();
    first_failure.transpose()

}
