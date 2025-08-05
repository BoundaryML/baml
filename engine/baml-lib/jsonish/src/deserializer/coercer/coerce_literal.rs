use std::vec;

use anyhow::Result;
use baml_types::LiteralValue;
use internal_baml_core::ir::TypeIR;
use internal_baml_jinja::CompletionOptions;

use super::{coerce_primitive::coerce_int, ParsingContext, ParsingError};
use crate::{
    deserializer::{
        coercer::{coerce_primitive::coerce_bool, match_string::match_string, TypeCoercer},
        deserialize_flags::{DeserializerConditions, Flag},
        types::BamlValueWithFlags,
    },
    jsonish,
};

impl TypeCoercer for LiteralValue {
    fn try_cast(
        &self,
        ctx: &ParsingContext,
        target: &TypeIR,
        value: Option<&jsonish::Value>,
    ) -> Option<BamlValueWithFlags> {
        let mut result = match self {
            LiteralValue::Int(literal_int) => match value {
                Some(crate::jsonish::Value::Number(number, _))
                    if number.as_i64().map(|n| n == *literal_int).unwrap_or(false) =>
                {
                    Some(BamlValueWithFlags::Int(
                        (number.as_i64().unwrap(), target).into(),
                    ))
                }
                _ => None,
            },
            LiteralValue::Bool(literal_bool) => match value {
                Some(crate::jsonish::Value::Boolean(b)) if b == literal_bool => {
                    Some(BamlValueWithFlags::Bool((*b, target).into()))
                }
                _ => None,
            },
            LiteralValue::String(literal_str) => match value {
                Some(crate::jsonish::Value::String(s, _)) if s == literal_str => {
                    Some(BamlValueWithFlags::String((s.to_string(), target).into()))
                }
                _ => None,
            },
        };

        // Check completion state exactly like coerce methods do
        if let Some(v) = value {
            match v.completion_state() {
                baml_types::CompletionState::Complete => {}
                baml_types::CompletionState::Incomplete => {
                    result
                        .iter_mut()
                        .for_each(|baml_value| baml_value.add_flag(Flag::Incomplete));
                }
                baml_types::CompletionState::Pending => {
                    unreachable!("jsonish::Value may never be in a Pending state.")
                }
            }
        }

        result
    }

    fn coerce(
        &self,
        ctx: &ParsingContext,
        target: &TypeIR,
        value: Option<&jsonish::Value>,
    ) -> Result<BamlValueWithFlags, ParsingError> {
        log::debug!(
            "scope: {scope} :: coercing to: {name:?} (current: {current})",
            name = self,
            scope = ctx.display_scope(),
            current = value.map(|v| v.r#type()).unwrap_or("<null>".into())
        );

        // Get rid of nulls.
        let value = match value {
            None | Some(jsonish::Value::Null) => {
                return Err(ctx.error_unexpected_null(target));
            }
            Some(v) => v,
        };

        // If we get an object with a single key-value pair, try to extract the value
        if let jsonish::Value::Object(obj, completion_state) = value {
            if obj.len() == 1 {
                let (key, inner_value) = obj.iter().next().unwrap();
                // only extract value if it's a primitive (not an object or array, hoping to god its fixed)
                match inner_value {
                    jsonish::Value::Number(_, _)
                    | jsonish::Value::Boolean(_)
                    | jsonish::Value::String(_, _) => {
                        let mut result = self.coerce(ctx, target, Some(inner_value))?;
                        result.add_flag(Flag::ObjectToPrimitive(jsonish::Value::Object(
                            obj.clone(),
                            completion_state.clone(),
                        )));
                        return Ok(result);
                    }
                    _ => {}
                }
            }
        }

        match self {
            LiteralValue::Int(literal_int) => {
                let BamlValueWithFlags::Int(coerced_int) = coerce_int(ctx, target, Some(value))?
                else {
                    unreachable!("coerce_int returned a non-integer value");
                };

                if coerced_int.value() == literal_int {
                    Ok(BamlValueWithFlags::Int(coerced_int))
                } else {
                    Err(ctx.error_unexpected_type(target, &value))
                }
            }

            LiteralValue::Bool(literal_bool) => {
                let BamlValueWithFlags::Bool(coerced_bool) = coerce_bool(ctx, target, Some(value))?
                else {
                    unreachable!("coerce_bool returned a non-boolean value");
                };

                if coerced_bool.value() == literal_bool {
                    Ok(BamlValueWithFlags::Bool(coerced_bool))
                } else {
                    Err(ctx.error_unexpected_type(target, &value))
                }
            }

            LiteralValue::String(literal_str) => {
                // second element is the list of aliases.
                let candidates = vec![(literal_str.as_str(), vec![literal_str.clone()])];

                let literal_match = match_string(ctx, target, Some(value), &candidates, true)?;

                Ok(BamlValueWithFlags::String(literal_match))
            }
        }
    }
}
