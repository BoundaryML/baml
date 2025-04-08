use anyhow::Result;
use baml_types::{BamlMap, CompletionState, Constraint, ConstraintLevel};
use internal_baml_core::{ir::FieldType, ir::TypeValue};

use crate::deserializer::{
    coercer::{run_user_checks, DefaultValue, TypeCoercer},
    deserialize_flags::{DeserializerConditions, Flag},
    types::BamlValueWithFlags,
};

use super::{
    array_helper,
    coerce_array::coerce_array,
    coerce_map::coerce_map,
    coerce_optional::coerce_optional,
    coerce_union::coerce_union,
    ir_ref::{coerce_alias::coerce_alias, IrRef},
    ParsingContext, ParsingError,
};

impl TypeCoercer for FieldType {
    fn coerce(
        &self,
        ctx: &ParsingContext,
        target: &FieldType,
        value: Option<&crate::jsonish::Value>,
    ) -> Result<BamlValueWithFlags, ParsingError> {
        let mut result = match value {
            Some(crate::jsonish::Value::AnyOf(candidates, primitive)) => {
                log::debug!(
                    "scope: {scope} :: coercing to: {name} (current: {current})",
                    name = target.to_string(),
                    scope = ctx.display_scope(),
                    current = value.map(|v| v.r#type()).unwrap_or("<null>".into())
                );
                if matches!(target, FieldType::Primitive(TypeValue::String)) {
                    self.coerce(
                        ctx,
                        target,
                        Some(&crate::jsonish::Value::String(primitive.clone(), CompletionState::Incomplete)),
                    )
                } else {
                    array_helper::coerce_array_to_singular(
                        ctx,
                        target,
                        &candidates.iter().collect::<Vec<_>>(),
                        &|val| self.coerce(ctx, target, Some(val)),
                    )
                }
            }
            Some(crate::jsonish::Value::Markdown(_t, v, _completion)) => {
                log::debug!(
                    "scope: {scope} :: coercing to: {name} (current: {current})",
                    name = target.to_string(),
                    scope = ctx.display_scope(),
                    current = value.map(|v| v.r#type()).unwrap_or("<null>".into())
                );
                self.coerce(ctx, target, Some(v)).map(|mut v| {
                    v.add_flag(Flag::ObjectFromMarkdown(
                        if matches!(target, FieldType::Primitive(TypeValue::String)) {
                            1
                        } else {
                            0
                        },
                    ));

                    v
                })
            }
            Some(crate::jsonish::Value::FixedJson(v, fixes)) => {
                log::debug!(
                    "scope: {scope} :: coercing to: {name} (current: {current})",
                    name = target.to_string(),
                    scope = ctx.display_scope(),
                    current = value.map(|v| v.r#type()).unwrap_or("<null>".into())
                );
                let mut v = self.coerce(ctx, target, Some(v))?;
                v.add_flag(Flag::ObjectFromFixedJson(fixes.to_vec()));
                Ok(v)
            }
            _ => match self {
                FieldType::Primitive(p) => p.coerce(ctx, target, value),
                FieldType::Enum(e) => IrRef::Enum(e).coerce(ctx, target, value),
                FieldType::Literal(l) => l.coerce(ctx, target, value),
                FieldType::Class(c) => IrRef::Class(c).coerce(ctx, target, value),
                FieldType::RecursiveTypeAlias(name) => coerce_alias(ctx, self, value),
                FieldType::List(_) => coerce_array(ctx, self, value),
                FieldType::Union(_) => coerce_union(ctx, self, value),
                FieldType::Optional(_) => coerce_optional(ctx, self, value),
                FieldType::Map(_, _) => coerce_map(ctx, self, value),
                FieldType::Tuple(_) => Err(ctx.error_internal("Tuple not supported")),
                FieldType::WithMetadata { base, .. } => {
                    let mut coerced_value = base.coerce(ctx, base, value)?;
                    let constraint_results = run_user_checks(&coerced_value.clone().into(), self)
                        .map_err(|e| ParsingError {
                        reason: format!("Failed to evaluate constraints: {e:?}"),
                        scope: ctx.scope.clone(),
                        causes: Vec::new(),
                    })?;
                    validate_asserts(&constraint_results)?;
                    let check_results = constraint_results
                        .into_iter()
                        .filter_map(|(maybe_check, result)| {
                            maybe_check
                                .as_check()
                                .map(|(label, expr)| (label, expr, result))
                        })
                        .collect();
                    coerced_value.add_flag(Flag::ConstraintResults(check_results));
                    Ok(coerced_value)
                }
            },
        };
        if let Some(CompletionState::Incomplete) = value.map(|v| v.completion_state()) {
            result.iter_mut().for_each(|v| v.add_flag(Flag::Incomplete));
        }
        result
    }
}

pub fn validate_asserts(constraints: &[(Constraint, bool)]) -> Result<(), ParsingError> {
    let failing_asserts = constraints
        .iter()
        .filter_map(
            |(
                Constraint {
                    level,
                    expression,
                    label,
                },
                result,
            )| {
                if !result && ConstraintLevel::Assert == *level {
                    Some((label, expression))
                } else {
                    None
                }
            },
        )
        .collect::<Vec<_>>();
    let causes = failing_asserts
        .into_iter()
        .map(|(label, expr)| ParsingError {
            causes: vec![],
            reason: format!(
                "Failed: {}{}",
                label.as_ref().map_or("".to_string(), |l| format!("{} ", l)),
                expr.0
            ),
            scope: vec![],
        })
        .collect::<Vec<_>>();
    if !causes.is_empty() {
        Err(ParsingError {
            causes: vec![],
            reason: "Assertions failed.".to_string(),
            scope: vec![],
        })
    } else {
        Ok(())
    }
}

impl DefaultValue for FieldType {
    fn default_value(&self, error: Option<&ParsingError>) -> Option<BamlValueWithFlags> {
        let get_flags = || {
            DeserializerConditions::new().with_flag(error.map_or(Flag::DefaultFromNoValue, |e| {
                Flag::DefaultButHadUnparseableValue(e.clone())
            }))
        };

        match self {
            FieldType::Enum(e) => None,
            FieldType::Literal(_) => None,
            FieldType::Class(_) => None,
            FieldType::RecursiveTypeAlias(_) => None,
            FieldType::List(_) => Some(BamlValueWithFlags::List(get_flags(), Vec::new())),
            FieldType::Union(items) => items.iter().find_map(|i| i.default_value(error)),
            FieldType::Primitive(TypeValue::Null) | FieldType::Optional(_) => {
                Some(BamlValueWithFlags::Null(get_flags()))
            }
            FieldType::Map(_, _) => Some(BamlValueWithFlags::Map(get_flags(), BamlMap::new())),
            FieldType::Tuple(v) => {
                let default_values: Vec<_> = v.iter().map(|f| f.default_value(error)).collect();
                if default_values.iter().all(Option::is_some) {
                    Some(BamlValueWithFlags::List(
                        get_flags(),
                        default_values.into_iter().map(Option::unwrap).collect(),
                    ))
                } else {
                    None
                }
            }
            FieldType::Primitive(_) => None,
            // If it has constraints, we can't assume our defaults meet them.
            FieldType::WithMetadata { .. } => None,
        }
    }
}
