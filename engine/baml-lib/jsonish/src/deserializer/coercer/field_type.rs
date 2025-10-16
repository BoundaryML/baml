use anyhow::Result;
use baml_types::{BamlMap, CompletionState, Constraint, ConstraintLevel, LiteralValue};
use internal_baml_core::ir::{TypeIR, TypeValue};

use super::{
    array_helper,
    coerce_array::coerce_array,
    coerce_map::coerce_map,
    coerce_union::coerce_union,
    ir_ref::{coerce_alias::coerce_alias, IrRef},
    ParsingContext, ParsingError,
};
use crate::deserializer::{
    coercer::{
        coerce_array::try_cast_array, coerce_map::try_cast_map, coerce_union::try_cast_union,
        ir_ref::coerce_alias::try_cast_alias, run_user_checks, DefaultValue, TypeCoercer,
    },
    deserialize_flags::{DeserializerConditions, Flag},
    types::BamlValueWithFlags,
};

impl TypeCoercer for TypeIR {
    fn try_cast(
        &self,
        ctx: &ParsingContext,
        target: &TypeIR,
        value: Option<&crate::jsonish::Value>,
    ) -> Option<BamlValueWithFlags> {
        let result = match self {
            TypeIR::Primitive(p, _) => p.try_cast(ctx, target, value),
            TypeIR::Enum { name, .. } => IrRef::Enum(name).try_cast(ctx, target, value),
            TypeIR::Literal(l, _) => l.try_cast(ctx, target, value),
            TypeIR::Class { name, mode, .. } => {
                IrRef::Class(name, mode).try_cast(ctx, target, value)
            }
            TypeIR::RecursiveTypeAlias { .. } => {
                try_cast_alias(ctx, self, value).map(|v| v.with_target(target))
            }
            TypeIR::List(_, _) => try_cast_array(ctx, self, value).map(|v| v.with_target(target)),
            TypeIR::Union(_, _) => try_cast_union(ctx, self, value).map(|v| v.with_target(target)),
            TypeIR::Map(..) => try_cast_map(ctx, self, value).map(|v| v.with_target(target)),
            TypeIR::Tuple(_, _) => None,
            TypeIR::Arrow(_, _) => None,
            TypeIR::Top(_) => panic!(
                "TypeIR::Top should have been resolved by the compiler before code generation. \
                 This indicates a bug in the type resolution phase."
            ),
        };

        match result {
            Some(mut v) => {
                // run user checks
                let Ok(constrained_results) =
                    run_user_checks(&v.clone().into(), self).map_err(|e| ParsingError {
                        reason: format!("Failed to evaluate constraints: {e:?}"),
                        scope: ctx.scope.clone(),
                        causes: Vec::new(),
                    })
                else {
                    return None;
                };

                // Don't return None if only checks fail (not asserts)
                let check_results = constrained_results
                    .iter()
                    .filter_map(|(maybe_check, result)| {
                        maybe_check
                            .clone()
                            .as_check()
                            .map(|(label, expr)| (label, expr, *result))
                    })
                    .collect::<Vec<_>>();

                if !check_results.is_empty() {
                    v.add_flag(Flag::ConstraintResults(check_results));
                }

                // Only validate asserts - if they fail, then return None
                if validate_asserts(&constrained_results).is_err() {
                    return None;
                }

                Some(v)
            }
            None => None,
        }
    }

    fn coerce(
        &self,
        ctx: &ParsingContext,
        target: &TypeIR,
        value: Option<&crate::jsonish::Value>,
    ) -> Result<BamlValueWithFlags, ParsingError> {
        let mut result = match value {
            Some(crate::jsonish::Value::AnyOf(candidates, primitive)) => {
                log::debug!(
                    "scope: {scope} :: coercing to: {name} (current: {current})",
                    name = target,
                    scope = ctx.display_scope(),
                    current = value.map(|v| v.r#type()).unwrap_or("<null>".into())
                );
                if matches!(
                    target,
                    TypeIR::Primitive(TypeValue::String, _)
                        | TypeIR::Enum { .. }
                        | TypeIR::Literal(LiteralValue::String(_), _)
                ) {
                    self.coerce(
                        ctx,
                        target,
                        Some(&crate::jsonish::Value::String(
                            primitive.clone(),
                            CompletionState::Complete,
                        )),
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
                    name = target,
                    scope = ctx.display_scope(),
                    current = value.map(|v| v.r#type()).unwrap_or("<null>".into())
                );
                self.coerce(ctx, target, Some(v)).map(|mut v| {
                    v.add_flag(Flag::ObjectFromMarkdown(
                        if matches!(target, TypeIR::Primitive(TypeValue::String, _)) {
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
                    name = target,
                    scope = ctx.display_scope(),
                    current = value.map(|v| v.r#type()).unwrap_or("<null>".into())
                );
                let mut v = self.coerce(ctx, target, Some(v))?;
                v.add_flag(Flag::ObjectFromFixedJson(fixes.to_vec()));
                Ok(v)
            }
            _ => {
                // try_cast is basically a way to exit early
                if let Some(v) = self.try_cast(ctx, target, value) {
                    Ok(v)
                } else {
                    match self {
                        TypeIR::Primitive(p, _) => p.coerce(ctx, target, value),
                        TypeIR::Enum { name, .. } => IrRef::Enum(name).coerce(ctx, target, value),
                        TypeIR::Literal(l, _) => l.coerce(ctx, target, value),
                        TypeIR::Class { name, mode, .. } => {
                            IrRef::Class(name, mode).coerce(ctx, target, value)
                        }
                        TypeIR::RecursiveTypeAlias { name, .. } => {
                            coerce_alias(ctx, self, value).map(|v| v.with_target(target))
                        }
                        TypeIR::List(_, _) => {
                            coerce_array(ctx, self, value).map(|v| v.with_target(target))
                        }
                        TypeIR::Union(_, _) => {
                            coerce_union(ctx, self, value).map(|v| v.with_target(target))
                        }
                        TypeIR::Map(..) => {
                            coerce_map(ctx, self, value).map(|v| v.with_target(target))
                        }
                        TypeIR::Tuple(_, _) => Err(ctx.error_internal("Tuple not supported")),
                        TypeIR::Arrow(_, _) => Err(ctx.error_internal("Arrow type not supported")),
                        TypeIR::Top(_) => panic!(
                            "TypeIR::Top should have been resolved by the compiler before code generation. \
                             This indicates a bug in the type resolution phase."
                        ),
                    }
                }
            }
        };
        if !target.meta().constraints.is_empty() {
            if let Ok(coerced_value) = result.as_mut() {
                let constrainted_results = run_user_checks(&coerced_value.clone().into(), self)
                    .map_err(|e| ParsingError {
                        reason: format!("Failed to evaluate constraints: {e:?}"),
                        scope: ctx.scope.clone(),
                        causes: Vec::new(),
                    })?;
                validate_asserts(&constrainted_results)?;
                let check_results = constrainted_results
                    .into_iter()
                    .filter_map(|(maybe_check, result)| {
                        maybe_check
                            .as_check()
                            .map(|(label, expr)| (label, expr, result))
                    })
                    .collect();
                coerced_value.add_flag(Flag::ConstraintResults(check_results));
            }
        }
        if let Some(CompletionState::Incomplete) = value.map(|v| v.completion_state()) {
            match result {
                Ok(mut v) => {
                    if self.meta().streaming_behavior.done
                        && ctx.do_not_use_mode == baml_types::StreamingMode::Streaming
                    {
                        return Err(ctx.error_internal("Streaming field is not done"));
                    }
                    v.add_flag(Flag::Incomplete);
                    return Ok(v);
                }
                Err(e) => return Err(e),
            }
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
                label.as_ref().map_or("".to_string(), |l| format!("{l} ")),
                expr.0
            ),
            scope: vec![],
        })
        .collect::<Vec<_>>();
    if !causes.is_empty() {
        Err(ParsingError {
            causes: vec![],
            reason: "Assertions failed.".to_string(), // IMPORTANT: DO NOT CHANGE THIS MESSAGE. TALK TO GREG.
            scope: vec![],
        })
    } else {
        Ok(())
    }
}

impl DefaultValue for TypeIR {
    fn default_value(&self, error: Option<&ParsingError>) -> Option<BamlValueWithFlags> {
        let get_flags = || {
            DeserializerConditions::new().with_flag(error.map_or(Flag::DefaultFromNoValue, |e| {
                Flag::DefaultButHadUnparseableValue(e.clone())
            }))
        };

        let unasserted = match self {
            TypeIR::Enum { .. } => None,
            TypeIR::Literal(_, _) => None,
            TypeIR::Class { .. } => None,
            TypeIR::RecursiveTypeAlias { .. } => None,
            TypeIR::List(_, _) => Some(BamlValueWithFlags::List(
                get_flags(),
                self.clone(),
                Vec::new(),
            )),
            TypeIR::Union(items, _) => items
                .iter_include_null()
                .iter()
                .find_map(|i| i.default_value(error)),
            TypeIR::Primitive(TypeValue::Null, _) => {
                return Some(BamlValueWithFlags::Null(self.clone(), get_flags()))
            }
            TypeIR::Map(..) => Some(BamlValueWithFlags::Map(
                get_flags(),
                self.clone(),
                BamlMap::new(),
            )),
            TypeIR::Tuple(v, _) => {
                let default_values: Vec<_> = v.iter().map(|f| f.default_value(error)).collect();
                if default_values.iter().all(Option::is_some) {
                    Some(BamlValueWithFlags::List(
                        get_flags(),
                        self.clone(),
                        default_values.into_iter().map(Option::unwrap).collect(),
                    ))
                } else {
                    None
                }
            }
            TypeIR::Primitive(_, _) => None,
            TypeIR::Arrow(_, _) => None,
            TypeIR::Top(_) => panic!(
                "TypeIR::Top should have been resolved by the compiler before code generation. \
                 This indicates a bug in the type resolution phase."
            ),
        };

        // TODO (Greg): Get rid of string-matching for this.
        fn has_assert_failure(error: &ParsingError) -> bool {
            error.reason.contains("Assertions failed.")
                || error.causes.iter().any(has_assert_failure)
        }

        // If there are no constraints, we can just return the unasserted value.
        // If there are constraints, we need to check if the unasserted value passes the constraints.
        match (self.meta().constraints.is_empty(), unasserted) {
            (_, None) => None,
            (true, Some(v)) => Some(v),
            (false, Some(v)) => {
                let asserts = self
                    .meta()
                    .constraints
                    .iter()
                    .filter(|c| c.level == ConstraintLevel::Assert)
                    .collect::<Vec<_>>();
                let Ok(results) = run_user_checks(&v.clone().into(), self) else {
                    return None;
                };
                let results_ok = results.iter().all(|(_, ok)| *ok);
                if results_ok {
                    match error {
                        Some(e) => {
                            if has_assert_failure(e) {
                                None
                            } else {
                                Some(v)
                            }
                        }
                        None => Some(v),
                    }
                } else {
                    None
                }
            }
        }
    }
}
