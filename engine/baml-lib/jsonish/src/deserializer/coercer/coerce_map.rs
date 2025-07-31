use std::collections::VecDeque;

use anyhow::Result;
use baml_types::{BamlMap, CompletionState, LiteralValue, TypeIR, TypeValue};

use super::{ParsingContext, ParsingError, TypeCoercer};
use crate::{
    deserializer::{
        deserialize_flags::{DeserializerConditions, Flag},
        types::BamlValueWithFlags,
    },
    jsonish,
};

pub(super) fn try_cast_map(
    ctx: &ParsingContext,
    map_target: &TypeIR,
    value: Option<&jsonish::Value>,
) -> Option<BamlValueWithFlags> {
    let TypeIR::Map(key_type, value_type, _) = map_target else {
        unreachable!("try_cast_map");
    };

    // Only handle object values
    let Some(crate::jsonish::Value::Object(obj, _)) = value else {
        return None;
    };

    // For empty objects, we can return immediately
    if obj.is_empty() {
        let mut flags = DeserializerConditions::new();
        if let Some(v) = value {
            flags.add_flag(Flag::ObjectToMap(v.clone()));
        }

        let mut result = BamlValueWithFlags::Map(flags, map_target.clone(), BamlMap::new());

        // Check completion state
        if let Some(v) = value {
            match v.completion_state() {
                CompletionState::Complete => {}
                CompletionState::Incomplete => {
                    result.add_flag(Flag::Incomplete);
                }
                CompletionState::Pending => {
                    unreachable!("jsonish::Value may never be in a Pending state.")
                }
            }
        }

        return Some(result);
    }

    // Try to cast all values
    let mut items = BamlMap::new();
    for (key, value) in obj {
        match value_type.try_cast(ctx, value_type, Some(value)) {
            Some(cast_value) => {
                items.insert(key.to_string(), (DeserializerConditions::new(), cast_value));
            }
            None => return None, // Fail fast on first error
        }
    }

    let mut flags = DeserializerConditions::new();
    if let Some(v) = value {
        flags.add_flag(Flag::ObjectToMap(v.clone()));
    }

    let mut result = BamlValueWithFlags::Map(flags, map_target.clone(), items);

    // Check completion state
    if let Some(v) = value {
        match v.completion_state() {
            CompletionState::Complete => {}
            CompletionState::Incomplete => {
                result.add_flag(Flag::Incomplete);
            }
            CompletionState::Pending => {
                unreachable!("jsonish::Value may never be in a Pending state.")
            }
        }
    }

    Some(result)
}

pub(super) fn coerce_map(
    ctx: &ParsingContext,
    map_target: &TypeIR,
    value: Option<&jsonish::Value>,
) -> Result<BamlValueWithFlags, ParsingError> {
    log::debug!(
        "scope: {scope} :: coercing to: {name} (current: {current})",
        name = map_target,
        scope = ctx.display_scope(),
        current = value.map(|v| v.r#type()).unwrap_or("<null>".into())
    );

    let Some(value) = value else {
        return Err(ctx.error_unexpected_null(map_target));
    };

    let TypeIR::Map(key_type, value_type, _) = map_target else {
        return Err(ctx.error_unexpected_type(map_target, value));
    };

    // TODO: Do we actually need to check the key type here in the coercion
    // logic? Can the user pass a "type" here at runtime? Can we pass the wrong
    // type from our own code or is this guaranteed to be a valid map key type?
    // If we can determine that the type is always valid then we can get rid of
    // this logic and skip the loops & allocs in the the union branch.
    match key_type.as_ref() {
        // String, enum or just one literal string, OK.
        TypeIR::Primitive(TypeValue::String, _)
        | TypeIR::Enum { .. }
        | TypeIR::Literal(LiteralValue::String(_), _) => {}

        // For unions we need to check if all the items are literal strings.
        TypeIR::Union(items, _) => {
            let mut queue = VecDeque::from_iter(items.iter_include_null());
            while let Some(item) = queue.pop_front() {
                match item {
                    TypeIR::Literal(LiteralValue::String(_), _) => continue,
                    TypeIR::Union(nested, _) => queue.extend(nested.iter_include_null()),
                    other => return Err(ctx.error_map_must_have_supported_key(other)),
                }
            }
        }

        // Key type not allowed.
        other => return Err(ctx.error_map_must_have_supported_key(other)),
    }

    let mut flags = DeserializerConditions::new();
    flags.add_flag(Flag::ObjectToMap(value.clone()));

    match &value {
        jsonish::Value::Object(obj, completion_state) => {
            let mut items = BamlMap::new();
            for (idx, (key, value)) in obj.iter().enumerate() {
                let coerced_value =
                    match value_type.coerce(&ctx.enter_scope(key), value_type, Some(value)) {
                        Ok(v) => v,
                        Err(e) => {
                            flags.add_flag(Flag::MapValueParseError(key.clone(), e));
                            // Could not coerce value, nothing else to do here.
                            continue;
                        }
                    };

                // Keys are just strings but since we suport enums and literals
                // we have to check that the key we are reading is actually a
                // valid enum member or expected literal value. The coercion
                // logic already does that so we'll just coerce the key.
                //
                // TODO: Is it necessary to check that values match here? This
                // is also checked at `coerce_arg` in
                // baml-lib/baml-core/src/ir/ir_helpers/to_baml_arg.rs
                // TODO: Is it Ok that we assume keys are complete?
                let key_as_jsonish =
                    jsonish::Value::String(key.to_owned(), CompletionState::Complete);
                match key_type.coerce(ctx, key_type, Some(&key_as_jsonish)) {
                    Ok(_) => {
                        // Hack to avoid cloning the key twice.
                        let jsonish::Value::String(owned_key, CompletionState::Complete) =
                            key_as_jsonish
                        else {
                            unreachable!("key_as_jsonish is defined as jsonish::Value::String");
                        };

                        // Both the value and the key were successfully
                        // coerced, add the key to the map.
                        items.insert(owned_key, (DeserializerConditions::new(), coerced_value));
                    }
                    // Couldn't coerce key, this is either not a valid enum
                    // variant or it doesn't match any of the literal values
                    // expected.
                    Err(e) => flags.add_flag(Flag::MapKeyParseError(idx, e)),
                }
            }
            if *completion_state == CompletionState::Incomplete {
                flags.add_flag(Flag::Incomplete);
            }
            Ok(BamlValueWithFlags::Map(flags, map_target.clone(), items))
        }
        // TODO: first map in an array that matches
        _ => Err(ctx.error_unexpected_type(map_target, value)),
    }
}
