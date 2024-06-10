use anyhow::Result;
use internal_baml_core::ir::FieldType;

use crate::deserializer::{
    deserialize_flags::{DeserializerConditions, Flag},
    types::BamlValueWithFlags,
};

use super::{ParsingContext, ParsingError, TypeCoercer};

pub(super) fn coerce_array(
    ctx: &ParsingContext,
    list_target: &FieldType,
    value: Option<&crate::jsonish::Value>,
) -> Result<BamlValueWithFlags, ParsingError> {
    assert!(matches!(list_target, FieldType::List(_)));

    log::debug!(
        "scope: {scope} :: coercing to: {name} (current: {current})",
        name = list_target.to_string(),
        scope = ctx.display_scope(),
        current = value.map(|v| v.r#type()).unwrap_or("<null>".into())
    );

    let inner = match list_target {
        FieldType::List(inner) => inner,
        _ => unreachable!(),
    };

    let mut items = vec![];
    let mut flags = DeserializerConditions::new();

    match &value {
        Some(crate::jsonish::Value::Array(arr)) => {
            for (i, item) in arr.iter().enumerate() {
                match inner.coerce(&ctx.enter_scope(&format!("{i}")), inner, Some(item)) {
                    Ok(v) => items.push(v),
                    Err(e) => flags.add_flag(Flag::ArrayItemParseError(i, e)),
                }
            }
        }
        Some(v) => {
            flags.add_flag(Flag::SingleToArray);
            match inner.coerce(&ctx.enter_scope("<implied>"), inner, Some(v)) {
                Ok(v) => items.push(v),
                Err(e) => flags.add_flag(Flag::ArrayItemParseError(0, e)),
            }
        }
        None => {}
    };

    Ok(BamlValueWithFlags::List(flags, items))
}
