mod coerce_class;
pub mod coerce_enum;

use anyhow::Result;
use internal_baml_core::ir::FieldType;

use crate::deserializer::{coercer::TypeCoercer, types::BamlValueWithFlags};

use super::{ParsingContext, ParsingError};

pub(super) enum IrRef<'a> {
    Enum(&'a String),
    Class(&'a String),
}

impl TypeCoercer for IrRef<'_> {
    fn coerce(
        &self,
        ctx: &ParsingContext,
        target: &FieldType,
        value: Option<&crate::jsonish::Value>,
    ) -> Result<BamlValueWithFlags, ParsingError> {
        match self {
            IrRef::Enum(e) => match ctx.of.find_enum(e.as_str()) {
                Ok(e) => e.coerce(ctx, target, value),
                Err(e) => Err(ctx.error_internal(e.to_string())),
            },
            IrRef::Class(c) => match ctx.of.find_class(c.as_str()) {
                Ok(c) => c.coerce(ctx, target, value),
                Err(e) => Err(ctx.error_internal(e.to_string())),
            },
        }
    }
}
