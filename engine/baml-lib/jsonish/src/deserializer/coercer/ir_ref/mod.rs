pub mod coerce_alias;
mod coerce_class;
pub mod coerce_enum;

use core::panic;

use anyhow::Result;
use internal_baml_core::ir::TypeIR;

use super::{ParsingContext, ParsingError};
use crate::deserializer::{coercer::TypeCoercer, types::BamlValueWithFlags};

pub(super) enum IrRef<'a> {
    Enum(&'a String),
    Class(&'a String, &'a baml_types::StreamingMode),
    RecursiveAlias(&'a String),
}

impl<'a> TypeCoercer for IrRef<'a> {
    fn try_cast(
        &self,
        ctx: &ParsingContext,
        target: &TypeIR,
        value: Option<&crate::jsonish::Value>,
    ) -> Option<BamlValueWithFlags> {
        match self {
            IrRef::Enum(e) => match ctx.of.find_enum(e.as_str()) {
                Ok(e) => e.try_cast(ctx, target, value),
                Err(e) => None,
            },
            IrRef::Class(c, mode) => match ctx.of.find_class(mode, c.as_str()) {
                Ok(c) => c.try_cast(ctx, target, value),
                Err(e) => None,
            },
            IrRef::RecursiveAlias(a) => match ctx.of.find_recursive_alias_target(a.as_str()) {
                Ok(a) => a.try_cast(ctx, target, value),
                Err(e) => None,
            },
        }
    }

    fn coerce(
        &self,
        ctx: &ParsingContext,
        target: &TypeIR,
        value: Option<&crate::jsonish::Value>,
    ) -> Result<BamlValueWithFlags, ParsingError> {
        match self {
            IrRef::Enum(e) => match ctx.of.find_enum(e.as_str()) {
                Ok(e) => e.coerce(ctx, target, value),
                Err(e) => Err(ctx.error_internal(e.to_string())),
            },
            IrRef::Class(c, mode) => match ctx.of.find_class(mode, c.as_str()) {
                Ok(c) => c.coerce(ctx, target, value),
                Err(e) => Err(ctx.error_internal(e.to_string())),
            },
            IrRef::RecursiveAlias(a) => match ctx.of.find_recursive_alias_target(a.as_str()) {
                Ok(a) => a.coerce(ctx, target, value),
                Err(e) => Err(ctx.error_internal(e.to_string())),
            },
        }
    }
}
