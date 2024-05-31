use anyhow::Result;
use internal_baml_core::ir::{FieldType, TypeValue};

use crate::deserializer::{
    coercer::TypeCoercer,
    deserialize_flags::{DeserializerConditions, Flag},
    types::BamlValueWithFlags,
};

use super::{array_helper::coerce_array_to_singular, ParsingContext, ParsingError};

impl TypeCoercer for TypeValue {
    fn coerce(
        &self,
        ctx: &ParsingContext,
        target: &FieldType,
        // Parsed from JSONish
        value: Option<&crate::jsonish::Value>,
    ) -> Result<BamlValueWithFlags, ParsingError> {
        match self {
            TypeValue::String => coerce_string(ctx, target, value),
            TypeValue::Int => coerce_int(ctx, target, value),
            TypeValue::Float => coerce_float(ctx, target, value),
            TypeValue::Bool => coerce_bool(ctx, target, value),
            TypeValue::Null => coerce_null(ctx, target, value),
            TypeValue::Image => Err(ctx.error_image_not_supported()),
        }
    }
}

fn coerce_null(
    _ctx: &ParsingContext,
    _target: &FieldType,
    value: Option<&crate::jsonish::Value>,
) -> Result<BamlValueWithFlags, ParsingError> {
    match value {
        Some(crate::jsonish::Value::Null) | None => {
            Ok(BamlValueWithFlags::Null(Default::default()))
        }
        Some(v) => Ok(BamlValueWithFlags::Null(
            DeserializerConditions::new().with_flag(Flag::DefaultButHadValue(v.clone())),
        )),
    }
}

fn coerce_string(
    ctx: &ParsingContext,
    target: &FieldType,
    value: Option<&crate::jsonish::Value>,
) -> Result<BamlValueWithFlags, ParsingError> {
    if let Some(value) = value {
        match value {
            crate::jsonish::Value::String(s) => {
                Ok(BamlValueWithFlags::String(s.to_string().into()))
            }
            crate::jsonish::Value::Null => Err(ctx.error_unexpected_null(target)),
            v => Ok(BamlValueWithFlags::String(
                (v.to_string(), Flag::JsonToString(v.clone())).into(),
            )),
        }
    } else {
        Err(ctx.error_unexpected_null(target))
    }
}

fn coerce_int(
    ctx: &ParsingContext,
    target: &FieldType,
    value: Option<&crate::jsonish::Value>,
) -> Result<BamlValueWithFlags, ParsingError> {
    if let Some(value) = value {
        match value {
            crate::jsonish::Value::Number(n) => {
                if let Some(n) = n.as_i64() {
                    Ok(BamlValueWithFlags::Int(n.into()))
                } else if let Some(n) = n.as_u64() {
                    Ok(BamlValueWithFlags::Int((n as i64).into()))
                } else if let Some(n) = n.as_f64() {
                    Ok(BamlValueWithFlags::Int(
                        ((n.round() as i64), Flag::FloatToInt(n)).into(),
                    ))
                } else {
                    Err(ctx.error_unexpected_type(target, value))
                }
            }
            crate::jsonish::Value::String(s) => {
                if let Ok(n) = s.parse::<i64>() {
                    Ok(BamlValueWithFlags::Int(n.into()))
                } else if let Ok(n) = s.parse::<u64>() {
                    Ok(BamlValueWithFlags::Int((n as i64).into()))
                } else if let Ok(n) = s.parse::<f64>() {
                    Ok(BamlValueWithFlags::Int(
                        ((n.round() as i64), Flag::FloatToInt(n)).into(),
                    ))
                } else {
                    Err(ctx.error_unexpected_type(target, value))
                }
            }
            crate::jsonish::Value::Array(items) => {
                coerce_array_to_singular(ctx, target, &items.iter().collect::<Vec<_>>(), &|value| {
                    coerce_int(ctx, target, Some(value))
                })
            }
            _ => Err(ctx.error_unexpected_type(target, value)),
        }
    } else {
        Err(ctx.error_unexpected_null(target))
    }
}

fn coerce_float(
    ctx: &ParsingContext,
    target: &FieldType,
    value: Option<&crate::jsonish::Value>,
) -> Result<BamlValueWithFlags, ParsingError> {
    if let Some(value) = value {
        match value {
            crate::jsonish::Value::Number(n) => {
                if let Some(n) = n.as_f64() {
                    Ok(BamlValueWithFlags::Float(n.into()))
                } else if let Some(n) = n.as_i64() {
                    Ok(BamlValueWithFlags::Float((n as f64).into()))
                } else if let Some(n) = n.as_u64() {
                    Ok(BamlValueWithFlags::Float((n as f64).into()))
                } else {
                    Err(ctx.error_unexpected_type(target, value))
                }
            }
            crate::jsonish::Value::String(s) => {
                if let Ok(n) = s.parse::<f64>() {
                    Ok(BamlValueWithFlags::Float(n.into()))
                } else if let Ok(n) = s.parse::<i64>() {
                    Ok(BamlValueWithFlags::Float((n as f64).into()))
                } else if let Ok(n) = s.parse::<u64>() {
                    Ok(BamlValueWithFlags::Float((n as f64).into()))
                } else {
                    Err(ctx.error_unexpected_type(target, value))
                }
            }
            crate::jsonish::Value::Array(items) => {
                coerce_array_to_singular(ctx, target, &items.iter().collect::<Vec<_>>(), &|value| {
                    coerce_float(ctx, target, Some(value))
                })
            }
            _ => Err(ctx.error_unexpected_type(target, value)),
        }
    } else {
        Err(ctx.error_unexpected_null(target))
    }
}

fn coerce_bool(
    ctx: &ParsingContext,
    target: &FieldType,
    value: Option<&crate::jsonish::Value>,
) -> Result<BamlValueWithFlags, ParsingError> {
    if let Some(value) = value {
        match value {
            crate::jsonish::Value::Boolean(b) => Ok(BamlValueWithFlags::Bool((*b).into())),
            crate::jsonish::Value::String(s) => match s.as_str() {
                "true" => Ok(BamlValueWithFlags::Bool(
                    (true, Flag::StringToBool(s.clone())).into(),
                )),
                "false" => Ok(BamlValueWithFlags::Bool(
                    (true, Flag::StringToBool(s.clone())).into(),
                )),
                _ => match s.to_ascii_lowercase().trim() {
                    "true" => Ok(BamlValueWithFlags::Bool(
                        (true, Flag::StringToBool(s.clone())).into(),
                    )),
                    "false" => Ok(BamlValueWithFlags::Bool(
                        (false, Flag::StringToBool(s.clone())).into(),
                    )),
                    _ => Err(ctx.error_unexpected_type(target, value)),
                },
            },
            crate::jsonish::Value::Array(items) => {
                coerce_array_to_singular(ctx, target, &items.iter().collect::<Vec<_>>(), &|value| {
                    coerce_float(ctx, target, Some(value))
                })
            }
            _ => Err(ctx.error_unexpected_type(target, value)),
        }
    } else {
        Err(ctx.error_unexpected_null(target))
    }
}
