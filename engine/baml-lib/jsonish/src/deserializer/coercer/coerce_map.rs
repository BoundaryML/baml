use anyhow::Result;

use crate::deserializer::{
    deserialize_flags::{DeserializerConditions, Flag},
    types::BamlValueWithFlags,
};
use baml_types::{BamlMap, FieldType, TypeValue};

use super::{ParsingContext, ParsingError, TypeCoercer};

pub(super) fn coerce_map(
    ctx: &ParsingContext,
    map_target: &FieldType,
    value: Option<&crate::jsonish::Value>,
) -> Result<BamlValueWithFlags, ParsingError> {
    log::debug!(
        "scope: {scope} :: coercing to: {name} (current: {current})",
        name = map_target.to_string(),
        scope = ctx.display_scope(),
        current = value.map(|v| v.r#type()).unwrap_or("<null>".into())
    );

    let Some(value) = value else {
        return Err(ctx.error_unexpected_null(map_target));
    };

    let FieldType::Map(key_type, value_type) = map_target else {
        return Err(ctx.error_unexpected_type(map_target, value));
    };

    if !matches!(**key_type, FieldType::Primitive(TypeValue::String)) {
        return Err(ctx.error_map_must_have_string_key(key_type));
    }

    let mut flags = DeserializerConditions::new();
    flags.add_flag(Flag::ObjectToMap(value.clone()));

    match &value {
        crate::jsonish::Value::Object(obj) => {
            let mut items = BamlMap::new();
            for (key, value) in obj.iter() {
                match value_type.coerce(&ctx.enter_scope(key), value_type, Some(value)) {
                    Ok(v) => {
                        items.insert(key.clone(), (DeserializerConditions::new(), v));
                    }
                    Err(e) => flags.add_flag(Flag::MapValueParseError(key.clone(), e)),
                }
            }
            Ok(BamlValueWithFlags::Map(flags, items))
        }
        // TODO: first map in an array that matches
        _ => Err(ctx.error_unexpected_type(map_target, value)),
    }
}
