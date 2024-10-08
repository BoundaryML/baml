use std::vec;

use anyhow::Result;
use baml_types::LiteralValue;
use internal_baml_core::ir::FieldType;

use crate::{
    deserializer::{
        coercer::{coerce_primitive::coerce_bool, match_string::match_string, TypeCoercer},
        types::BamlValueWithFlags,
    },
    jsonish,
};

use super::{coerce_primitive::coerce_int, ParsingContext, ParsingError};

impl TypeCoercer for LiteralValue {
    fn coerce(
        &self,
        ctx: &ParsingContext,
        target: &FieldType,
        value: Option<&jsonish::Value>,
    ) -> Result<BamlValueWithFlags, ParsingError> {
        assert!(matches!(target, FieldType::Literal(_)));

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
                let candidates = vec![(literal_str.as_str(), vec![literal_str.clone()])];

                let literal_match = match_string(ctx, target, Some(value), &candidates)?;

                Ok(BamlValueWithFlags::String(literal_match))
            }
        }
    }
}
