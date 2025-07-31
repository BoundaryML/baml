use anyhow::Result;
use internal_baml_core::ir::TypeIR;

use super::{ParsingContext, ParsingError, TypeCoercer};
use crate::deserializer::types::BamlValueWithFlags;

pub fn try_cast_alias(
    ctx: &ParsingContext,
    alias_target: &TypeIR,
    value: Option<&crate::jsonish::Value>,
) -> Option<BamlValueWithFlags> {
    let TypeIR::RecursiveTypeAlias { name: alias, .. } = alias_target else {
        unreachable!("try_cast_alias");
    };

    // Handle circular references
    let mut nested_ctx = None;
    if let Some(v) = value {
        let cls_value_pair = (alias.to_string(), v.to_owned());
        if ctx.visited_during_try_cast.contains(&cls_value_pair) {
            return None;
        }
        nested_ctx = Some(ctx.visit_class_value_pair(cls_value_pair, false));
    }
    let ctx = nested_ctx.as_ref().unwrap_or(ctx);

    // Try to resolve and cast to the target type
    let mut result = match ctx.of.find_recursive_alias_target(alias) {
        Ok(resolved_type) => resolved_type.try_cast(ctx, alias_target, value),
        Err(_) => None,
    };

    // Check completion state
    if let Some(v) = value {
        if let Some(ref mut res) = result {
            match v.completion_state() {
                baml_types::CompletionState::Complete => {}
                baml_types::CompletionState::Incomplete => {
                    res.add_flag(crate::deserializer::deserialize_flags::Flag::Incomplete);
                }
                baml_types::CompletionState::Pending => {
                    unreachable!("jsonish::Value may never be in a Pending state.")
                }
            }
        }
    }

    result
}

pub fn coerce_alias(
    ctx: &ParsingContext,
    target: &TypeIR,
    value: Option<&crate::jsonish::Value>,
) -> Result<BamlValueWithFlags, ParsingError> {
    assert!(matches!(target, TypeIR::RecursiveTypeAlias { .. }));
    log::debug!(
        "scope: {scope} :: coercing to: {name} (current: {current})",
        name = target,
        scope = ctx.display_scope(),
        current = value.map(|v| v.r#type()).unwrap_or("<null>".into())
    );

    let TypeIR::RecursiveTypeAlias { name: alias, .. } = target else {
        unreachable!("coerce_alias");
    };

    // See coerce_class.rs
    let mut nested_ctx = None;
    if let Some(v) = value {
        let cls_value_pair = (alias.to_string(), v.to_owned());
        if ctx.visited_during_coerce.contains(&cls_value_pair) {
            return Err(ctx.error_circular_reference(alias, v));
        }
        nested_ctx = Some(ctx.visit_class_value_pair(cls_value_pair, true));
    }
    let ctx = nested_ctx.as_ref().unwrap_or(ctx);

    ctx.of
        .find_recursive_alias_target(alias)
        .map_err(|e| ParsingError {
            reason: format!("Failed to find recursive alias target: {e}"),
            scope: ctx.scope.clone(),
            causes: Vec::new(),
        })?
        .coerce(ctx, target, value)
}
