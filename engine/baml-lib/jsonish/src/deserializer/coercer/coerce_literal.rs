use std::vec;

use anyhow::Result;
use baml_types::{BamlMediaType, LiteralValue};
use internal_baml_core::ir::{FieldType, TypeValue};

use crate::{
    deserializer::{
        coercer::{
            coerce_primitive::coerce_bool,
            ir_ref::coerce_enum::{enum_match_strategy, strip_punctuation},
            TypeCoercer,
        },
        deserialize_flags::{DeserializerConditions, Flag},
        types::{BamlValueWithFlags, ValueWithFlags},
    },
    jsonish,
};

use super::{
    array_helper::coerce_array_to_singular, coerce_primitive::coerce_int, ParsingContext,
    ParsingError,
};

impl TypeCoercer for LiteralValue {
    fn coerce(
        &self,
        ctx: &ParsingContext,
        target: &FieldType,
        value: Option<&jsonish::Value>,
    ) -> Result<BamlValueWithFlags, ParsingError> {
        log::debug!(
            "scope: {scope} :: coercing to: {name:?} (current: {current})",
            name = self,
            scope = ctx.display_scope(),
            current = value.map(|v| v.r#type()).unwrap_or("<null>".into())
        );

        let literal = match target {
            FieldType::Literal(literal) if literal == self => literal,
            // Received non-literal type or literal value doesn't match expected value.
            _ => {
                return Err(ctx.error_unexpected_type(&FieldType::Literal(self.clone()), target));
            }
        };

        // Get rid of nulls.
        let value = match value {
            None | Some(jsonish::Value::Null) => {
                return Err(ctx.error_unexpected_null(target));
            }
            Some(v) => v,
        };

        match literal {
            LiteralValue::Int(literal_int) => {
                let BamlValueWithFlags::Int(coerced_int) = coerce_int(ctx, target, Some(value))?
                else {
                    unreachable!("coerce_int returned a non-integer value");
                };

                if coerced_int.value() == literal_int {
                    Ok(BamlValueWithFlags::Int(coerced_int))
                } else {
                    Err(ctx.error_unexpected_type(target, value))
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
                    Err(ctx.error_unexpected_type(target, value))
                }
            }

            LiteralValue::String(literal_str) => {
                let mut flags = DeserializerConditions::new();

                let context = match value {
                    crate::jsonish::Value::String(s) => s.clone(),
                    crate::jsonish::Value::AnyOf(_, s) => {
                        flags.add_flag(Flag::ObjectToString(value.clone()));
                        s.clone()
                    }
                    v => {
                        flags.add_flag(Flag::ObjectToString(v.clone()));
                        format!("{}", v)
                    }
                };

                // TODO: Description or alias.
                let candidates = vec![(literal_str.as_str(), vec![literal_str.clone()])];

                if let Some(variant) = enum_match_strategy(&context, &candidates, &mut flags) {
                    if let Some(mismatch) = flags.flags.iter().find_map(|f| match f {
                        Flag::EnumOneFromMany(options) => Some(options),
                        _ => None,
                    }) {
                        return Err(ctx.error_too_many_matches(
                            target,
                            mismatch
                                .iter()
                                .map(|(count, e)| format!("{} ({} times)", e, count)),
                        ));
                    }

                    return Ok(BamlValueWithFlags::String(
                        (variant.to_string(), flags).into(),
                    ));
                }

                // Try to strip punctuation and try again.
                let context = strip_punctuation(&context);
                let candidates = candidates
                    .iter()
                    .map(|(variant, valid_values)| {
                        (
                            *variant,
                            valid_values.iter().map(|v| strip_punctuation(v)).collect(),
                        )
                    })
                    .collect::<Vec<_>>();

                if let Some(variant) = enum_match_strategy(&context, &candidates, &mut flags) {
                    if let Some(mismatch) = flags.flags.iter().find_map(|f| match f {
                        Flag::EnumOneFromMany(options) => Some(options),
                        _ => None,
                    }) {
                        return Err(
                            ctx.error_too_many_matches(target, mismatch.iter().map(|(_, e)| e))
                        );
                    }
                    return Ok(BamlValueWithFlags::String(
                        (variant.to_string(), flags).into(),
                    ));
                }

                Err(ctx.error_unexpected_type(target, &value))
            }
        }
    }
}
