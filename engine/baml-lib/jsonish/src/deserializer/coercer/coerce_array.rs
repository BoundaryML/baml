use anyhow::Result;
use baml_types::CompletionState;
use internal_baml_core::ir::TypeIR;

use super::{ParsingContext, ParsingError, TypeCoercer};
use crate::deserializer::{
    deserialize_flags::{DeserializerConditions, Flag},
    types::BamlValueWithFlags,
};

/// Extract the winning union variant index from a coerced value's flags.
/// Returns None if the value wasn't from a union coercion.
///
/// IMPORTANT: We iterate in REVERSE to get the LAST (outermost) UnionMatch flag.
/// When coercing nested unions like `(A | B)[]` where `B = (C | D)`, the inner
/// union's flag is added first, then the outer union's flag. We want the outer
/// union's index for the array hint, not the inner one.
fn extract_union_winner_index(value: &BamlValueWithFlags) -> Option<usize> {
    value
        .conditions()
        .flags()
        .iter()
        .rev()
        .find_map(|flag| match flag {
            Flag::UnionMatch(idx, _) => Some(*idx),
            _ => None,
        })
}

pub(super) fn try_cast_array(
    ctx: &ParsingContext,
    array_target: &TypeIR,
    value: Option<&crate::jsonish::Value>,
) -> Option<BamlValueWithFlags> {
    let TypeIR::List(element_type, _) = array_target else {
        unreachable!("try_cast_array");
    };

    // Only handle array values
    let Some(crate::jsonish::Value::Array(arr, _)) = value else {
        return None;
    };

    // For empty arrays, we can return immediately
    if arr.is_empty() {
        let mut result = BamlValueWithFlags::List(
            DeserializerConditions::new(),
            array_target.clone(),
            Vec::new(),
        );

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

    // Try to cast all elements, tracking union hints for optimization
    let mut items = Vec::with_capacity(arr.len());
    let mut last_union_hint: Option<usize> = None;
    for (i, item) in arr.iter().enumerate() {
        let child_ctx = ctx.enter_scope_with_hint(&format!("{i}"), last_union_hint);
        match element_type.try_cast(&child_ctx, element_type, Some(item)) {
            Some(v) => {
                // Extract winning variant index for the next iteration's hint
                last_union_hint = extract_union_winner_index(&v);
                items.push(v);
            }
            None => return None, // Fail fast on first error
        }
    }

    let mut result =
        BamlValueWithFlags::List(DeserializerConditions::new(), array_target.clone(), items);

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

pub(super) fn coerce_array(
    ctx: &ParsingContext,
    list_target: &TypeIR,
    value: Option<&crate::jsonish::Value>,
) -> Result<BamlValueWithFlags, ParsingError> {
    assert!(matches!(list_target, TypeIR::List(_, _)));

    log::debug!(
        "scope: {scope} :: coercing to: {name} (current: {current})",
        name = list_target,
        scope = ctx.display_scope(),
        current = value.map(|v| v.r#type()).unwrap_or("<null>".into())
    );

    let inner = match list_target {
        TypeIR::List(inner, _) => inner,
        _ => unreachable!("coerce_array"),
    };

    let mut items = vec![];
    let mut flags = DeserializerConditions::new();

    match &value {
        Some(crate::jsonish::Value::Array(arr, completion_state)) => {
            if *completion_state == CompletionState::Incomplete {
                flags.add_flag(Flag::Incomplete);
            }
            // Track the winning union variant from the previous element to hint the next
            let mut last_union_hint: Option<usize> = None;
            for (i, item) in arr.iter().enumerate() {
                let child_ctx = ctx.enter_scope_with_hint(&format!("{i}"), last_union_hint);
                match inner.coerce(&child_ctx, inner, Some(item)) {
                    Ok(v) => {
                        // Extract winning variant index for the next iteration's hint
                        last_union_hint = extract_union_winner_index(&v);
                        items.push(v);
                    }
                    // TODO(vbv): document why we penalize in proportion to how deep into an array a parse error is
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

    Ok(BamlValueWithFlags::List(flags, list_target.clone(), items))
}
