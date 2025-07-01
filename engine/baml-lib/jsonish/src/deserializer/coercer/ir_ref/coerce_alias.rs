use anyhow::Result;
use internal_baml_core::ir::FieldType;

use super::{ParsingContext, ParsingError, TypeCoercer};
use crate::deserializer::types::BamlValueWithFlags;

pub fn coerce_alias(
    ctx: &ParsingContext,
    target: &FieldType,
    value: Option<&crate::jsonish::Value>,
) -> Result<BamlValueWithFlags, ParsingError> {
    assert!(matches!(target, FieldType::RecursiveTypeAlias { .. }));
    log::debug!(
        "scope: {scope} :: coercing to: {name} (current: {current})",
        name = target,
        scope = ctx.display_scope(),
        current = value.map(|v| v.r#type()).unwrap_or("<null>".into())
    );

    let FieldType::RecursiveTypeAlias { name: alias, .. } = target else {
        unreachable!("coerce_alias");
    };

    // See coerce_class.rs
    let mut nested_ctx = None;
    if let Some(v) = value {
        let cls_value_pair = (alias.to_string(), v.to_owned());
        if ctx.visited.contains(&cls_value_pair) {
            return Err(ctx.error_circular_reference(alias, v));
        }
        nested_ctx = Some(ctx.visit_class_value_pair(cls_value_pair));
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
