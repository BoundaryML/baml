use anyhow::Result;
use internal_baml_core::ir::FieldType;

use crate::deserializer::{
    deserialize_flags::{DeserializerConditions, Flag},
    types::BamlValueWithFlags,
};

use super::{ParsingContext, ParsingError, TypeCoercer};

pub(super) fn coerce_optional(
    ctx: &ParsingContext,
    optional_target: &FieldType,
    value: Option<&crate::jsonish::Value>,
) -> Result<BamlValueWithFlags, ParsingError> {
    assert!(matches!(optional_target, FieldType::Optional(_)));
    log::debug!(
        "scope: {scope} :: coercing to: {name} (current: {current})",
        name = optional_target.to_string(),
        scope = ctx.display_scope(),
        current = value.map(|v| v.r#type()).unwrap_or("<null>".into())
    );

    let inner = match optional_target {
        FieldType::Optional(inner) => inner,
        _ => unreachable!(),
    };

    let mut flags = DeserializerConditions::new();
    match value {
        None | Some(crate::jsonish::Value::Null) => Ok(BamlValueWithFlags::Null(flags)),
        Some(v) => match inner.coerce(ctx, optional_target, Some(v)) {
            Ok(v) => Ok(v),
            Err(e) => {
                flags.add_flag(Flag::DefaultButHadUnparseableValue(e));
                Ok(BamlValueWithFlags::Null(flags))
            }
        },
    }
}
