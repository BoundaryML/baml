use std::collections::HashSet;

use anyhow::Result;
use baml_types::{BamlMap, Constraint};
use internal_baml_core::ir::TypeIR;
use internal_baml_jinja::types::{Class, Name};

use super::ParsingContext;
use crate::deserializer::{
    coercer::{
        array_helper,
        field_type::validate_asserts,
        match_string::{match_string, matches_string_to_string},
        run_user_checks, DefaultValue, ParsingError, TypeCoercer,
    },
    deserialize_flags::{DeserializerConditions, Flag},
    types::BamlValueWithFlags,
};

// Name, type, description, streaming_needed.
type FieldValue = (Name, TypeIR, Option<String>, bool);

impl TypeCoercer for Class {
    fn try_cast(
        &self,
        ctx: &ParsingContext,
        target: &TypeIR,
        value: Option<&crate::jsonish::Value>,
    ) -> Option<BamlValueWithFlags> {
        // Only handle object values for class types
        let Some(crate::jsonish::Value::Object(obj, completion_state)) = value else {
            return None;
        };

        // If this class has @@stream.done and we're in streaming mode,
        // reject incomplete objects
        if self.streaming_behavior.done
            && ctx.do_not_use_mode == baml_types::StreamingMode::Streaming
            && completion_state == &baml_types::CompletionState::Incomplete
        {
            log::debug!(
                "Class {} has @@stream.done but object is incomplete",
                self.name.real_name()
            );
            return None;
        }

        let ctx = {
            let cls_value_pair = (self.name.real_name().to_string(), value.unwrap().to_owned());

            // If this combination has been visited bail out.
            if ctx.visited_during_try_cast.contains(&cls_value_pair) {
                return None;
            }

            // Mark this class as visited for the duration of this function
            // call. Further recursion from within this function will see that
            // the class has already been visited and stop recursing. Different
            // calls to this function for other fields pointing to the same
            // recursive class should start from scratch with an empty visited
            // set so they will not fail because this class has already been
            // coerced for a different field.
            &ctx.visit_class_value_pair(cls_value_pair, false)
        };

        #[derive(Debug)]
        enum Triple {
            Pending,
            NotPresent,
            Present(Box<BamlValueWithFlags>),
        }

        let mut fill_result = self
            .fields
            .iter()
            .map(|(name, field_type, _, streaming_needed)| {
                (
                    name.rendered_name(),
                    (name, field_type, *streaming_needed, Triple::Pending),
                )
            })
            .collect::<BamlMap<_, _>>();

        let flags = DeserializerConditions::new();
        for (k, v) in obj.iter() {
            if let Some((_, field_type, streaming_needed, val)) = fill_result.get_mut(k.as_str()) {
                if matches!(val, Triple::Present(_)) {
                    continue;
                }
                if let Some(cast_value) = field_type.try_cast(ctx, field_type, Some(v)) {
                    *val = Triple::Present(Box::new(cast_value));
                } else {
                    return None;
                }
            } else {
                // In try_cast mode, reject objects with extra keys for stricter matching
                return None;
            }
        }

        let mut result = BamlMap::new();
        for (_, (name, field_type, streaming_needed, val)) in fill_result.into_iter() {
            if let Triple::Present(ref val_ref) = val {
                // Check if field is required (non-optional) and is incomplete in streaming mode
                if !field_type.is_optional()
                    && ctx.do_not_use_mode == baml_types::StreamingMode::Streaming
                    && val_ref
                        .conditions()
                        .flags
                        .iter()
                        .any(|f| matches!(f, Flag::Incomplete))
                {
                    return None;
                }
            }

            if let Triple::Present(val) = val {
                result.insert(name.real_name().to_string(), *val);
            } else if field_type.is_optional() {
                let mut null_value =
                    BamlValueWithFlags::Null(field_type.clone(), Default::default());
                null_value.add_flag(Flag::OptionalDefaultFromNoValue);
                null_value.add_flag(Flag::Pending);
                result.insert(name.real_name().to_string(), null_value);
            } else {
                return None;
            }
        }

        let result = BamlValueWithFlags::Class(
            self.name.real_name().into(),
            flags.clone(),
            target.clone(),
            result,
        );

        Some(result)
    }
    fn coerce(
        &self,
        ctx: &ParsingContext,
        target: &TypeIR,
        value: Option<&crate::jsonish::Value>,
    ) -> Result<BamlValueWithFlags, ParsingError> {
        log::debug!(
            "scope: {scope} :: coercing to: {name} (current: {current})",
            name = self.name.real_name(),
            scope = ctx.display_scope(),
            current = value.map(|v| v.r#type()).unwrap_or("<null>".into())
        );

        // If value is not None then we'll update the context to store the
        // current class in the visited set and we'll use that to stop recursion
        // when dealing with recursive classes.
        let mut nested_ctx = None;

        if let Some(v) = value {
            let cls_value_pair = (self.name.real_name().to_string(), v.to_owned());

            // If this combination has been visited bail out.
            if ctx.visited_during_coerce.contains(&cls_value_pair) {
                return Err(ctx.error_circular_reference(self.name.real_name(), v));
            }

            // Mark this class as visited for the duration of this function
            // call. Further recursion from within this function will see that
            // the class has already been visited and stop recursing. Different
            // calls to this function for other fields pointing to the same
            // recursive class should start from scratch with an empty visited
            // set so they will not fail because this class has already been
            // coerced for a different field.
            nested_ctx = Some(ctx.visit_class_value_pair(cls_value_pair, true));
        }

        // Now just maintain the previous context or get the new one and proceed
        // normally.
        let ctx = nested_ctx.as_ref().unwrap_or(ctx);

        let (optional, required): (Vec<_>, Vec<_>) =
            self.fields.iter().partition(|f| f.1.is_optional());
        let (constraints, streaming_behavior) = ctx
            .of
            .find_class(&self.namespace, self.name.real_name())
            .map_or((vec![], Default::default()), |class| {
                (class.constraints.clone(), class.streaming_behavior.clone())
            });

        let mut optional_values = optional
            .iter()
            .map(|(f, ..)| (f.real_name().to_string(), None))
            .collect::<BamlMap<_, _>>();
        let mut required_values = required
            .iter()
            .map(|(f, ..)| (f.real_name().to_string(), None))
            .collect::<BamlMap<_, _>>();
        let mut flags = DeserializerConditions::new();

        let mut completed_cls = Vec::new();

        // There are a few possible approaches here:
        match value {
            None => {
                // Do nothing
            }
            Some(crate::jsonish::Value::Object(obj, completion)) => {
                // match keys, if that fails, then do something fancy later.
                let mut extra_keys = vec![];
                let mut found_keys = false;
                obj.iter().for_each(|(key, v)| {
                    if let Some(field) = self
                        .fields
                        .iter()
                        .find(|(name, ..)| matches_string_to_string(ctx, key, name.rendered_name()))
                    {
                        let scope = ctx.enter_scope(field.0.real_name());
                        let parsed = field.1.coerce(&scope, &field.1, Some(v));
                        update_map(&mut required_values, &mut optional_values, field, parsed);
                        found_keys = true;
                    } else {
                        extra_keys.push((key, v));
                    }
                });

                if !found_keys && !extra_keys.is_empty() && self.fields.len() == 1 {
                    // Try to coerce the object into the single field
                    let field = &self.fields[0];
                    let scope = ctx.enter_scope(&format!("<implied:{}>", field.0.real_name()));
                    let parsed = field
                        .1
                        .coerce(
                            &scope,
                            &field.1,
                            Some(&crate::jsonish::Value::Object(
                                obj.clone(),
                                completion.clone(),
                            )),
                        )
                        .map(|mut v| {
                            v.add_flag(Flag::ImpliedKey(field.0.real_name().into()));
                            v
                        });

                    if let Ok(parsed_value) = parsed {
                        update_map(
                            &mut required_values,
                            &mut optional_values,
                            field,
                            Ok(parsed_value),
                        );
                    } else {
                        extra_keys.into_iter().for_each(|(key, v)| {
                            flags.add_flag(Flag::ExtraKey(key.to_string(), v.clone()));
                        });
                    }
                } else {
                    extra_keys.into_iter().for_each(|(key, v)| {
                        flags.add_flag(Flag::ExtraKey(key.to_string(), v.clone()));
                    });
                }
            }
            Some(crate::jsonish::Value::Array(items, completion)) => {
                if self.fields.len() == 1 {
                    let field = &self.fields[0];
                    let scope = ctx.enter_scope(&format!("<implied:{}>", field.0.real_name()));
                    let parsed = match field.1.coerce(&scope, &field.1, value) {
                        Ok(mut v) => {
                            v.add_flag(Flag::ImpliedKey(field.0.real_name().into()));
                            Ok(v)
                        }
                        Err(e) => Err(e),
                    };
                    update_map(&mut required_values, &mut optional_values, field, parsed);
                }

                // Coerce the each item into the class if possible
                let option1_result = array_helper::coerce_array_to_singular(
                    ctx,
                    target,
                    &items.iter().collect::<Vec<_>>(),
                    &|value| self.coerce(ctx, target, Some(value)),
                )
                .and_then(|value| {
                    apply_constraints(
                        target,
                        vec![],
                        value,
                        constraints.clone(),
                        streaming_behavior.clone(),
                    )
                });
                if let Ok(option1) = option1_result {
                    completed_cls.push(Ok(option1));
                }
            }
            Some(x) => {
                // If the class has a single field, then we can try to coerce it directly
                if self.fields.len() == 1 {
                    let field = &self.fields[0];
                    let scope = ctx.enter_scope(&format!("<implied:{}>", field.0.real_name()));
                    let parsed = match field.1.coerce(&scope, &field.1, Some(x)) {
                        Ok(mut v) => {
                            v.add_flag(Flag::ImpliedKey(field.0.real_name().into()));
                            flags.add_flag(Flag::InferedObject(x.clone()));
                            Ok(v)
                        }
                        Err(e) => Err(e),
                    };
                    update_map(&mut required_values, &mut optional_values, field, parsed);
                }
            }
        }

        // Check what we have / what we need
        {
            self.fields.iter().for_each(|(field_name, t, ..)| {
                if t.is_optional() {
                    if let Some(v) = optional_values.get(field_name.real_name()) {
                        let next = match v {
                            Some(Ok(_)) => None,
                            Some(Err(e)) => {
                                log::trace!(
                                    "Error in optional field {}: {}",
                                    field_name.real_name(),
                                    e
                                );
                                t.default_value(Some(e))
                            }
                            // If we're missing a field, thats ok!
                            None => Some(BamlValueWithFlags::Null(
                                t.clone(),
                                DeserializerConditions::new()
                                    .with_flag(Flag::OptionalDefaultFromNoValue)
                                    .with_flag(Flag::Pending),
                            )),
                        };

                        if let Some(next) = next {
                            optional_values
                                .insert(field_name.real_name().to_string(), Some(Ok(next)));
                        }
                    }
                } else if let Some(v) = required_values.get(field_name.real_name()) {
                    let next = match v {
                        Some(Ok(_)) => None,
                        Some(Err(e)) => t.default_value(Some(e)),
                        None => t.default_value(None).map(|mut v| {
                            v.add_flag(Flag::Pending);
                            v
                        }),
                    };

                    if let Some(next) = next {
                        required_values.insert(field_name.real_name().to_string(), Some(Ok(next)));
                    }
                }
            });

            log::trace!("---");
            for (k, v) in optional_values.iter() {
                log::trace!(
                    "  Optional field: {} = ({} + {})",
                    k,
                    v.is_none(),
                    v.as_ref().map(|v| v.is_ok()).unwrap_or(false)
                );
            }
            for (k, v) in required_values.iter() {
                log::trace!(
                    "  Required field: {} = ({} + {})",
                    k,
                    v.is_none(),
                    v.as_ref().map(|v| v.is_ok()).unwrap_or(false)
                );
            }
            log::trace!("----");

            let unparsed_required_fields = required_values
                .iter()
                .filter_map(|(k, v)| match v {
                    Some(Ok(_)) => None,
                    Some(Err(e)) => Some((k.clone(), e)),
                    None => None,
                })
                .collect::<Vec<_>>();
            let missing_required_fields = required_values
                .iter()
                .filter_map(|(k, v)| match v {
                    Some(Ok(_)) => None,
                    Some(Err(e)) => None,
                    None => Some(k.clone()),
                })
                .collect::<Vec<_>>();

            if !missing_required_fields.is_empty() || !unparsed_required_fields.is_empty() {
                if completed_cls.is_empty() {
                    return Err(ctx.error_missing_required_field(
                        unparsed_required_fields,
                        missing_required_fields,
                        value,
                    ));
                }
            } else {
                // TODO: Figure out how to propagate these errors as flags.
                let merged_errors = required_values
                    .iter()
                    .filter_map(|(_k, v)| v.clone())
                    .filter_map(|v| match v {
                        Ok(_) => None,
                        Err(e) => Some(e.to_string()),
                    })
                    .collect::<Vec<_>>();

                let valid_fields = required_values
                    .iter()
                    .filter_map(|(k, v)| match v.to_owned() {
                        Some(Ok(v)) => Some((k.to_string(), v)),
                        _ => None,
                    })
                    .chain(optional_values.iter().map(|(k, v)| {
                        match v.to_owned() {
                            Some(Ok(v)) => {
                                // Decide if null is a better option.
                                (k.to_string(), v)
                            }
                            None => (
                                k.to_string(),
                                BamlValueWithFlags::Null(
                                    self.fields
                                        .iter()
                                        .find(|(name, ..)| name.real_name() == k)
                                        .map(|f| f.1.clone().as_optional())
                                        .unwrap_or_else(|| {
                                            panic!(
                                                "Field {} not found in class {}",
                                                k,
                                                self.name.real_name()
                                            )
                                        }),
                                    DeserializerConditions::new().with_flag(Flag::Incomplete),
                                ),
                            ),
                            Some(Err(e)) => (
                                k.to_string(),
                                BamlValueWithFlags::Null(
                                    self.fields
                                        .iter()
                                        .find(|(name, ..)| name.real_name() == k)
                                        .map(|f| f.1.clone().as_optional())
                                        .unwrap_or_else(|| {
                                            panic!(
                                                "Field {} not found in class {}",
                                                k,
                                                self.name.real_name()
                                            )
                                        }),
                                    DeserializerConditions::new()
                                        .with_flag(Flag::DefaultButHadUnparseableValue(e))
                                        .with_flag(Flag::Incomplete),
                                ),
                            ),
                        }
                    }))
                    .collect::<BamlMap<String, _>>();

                // Create a BamlMap ordered according to self.fields
                let mut ordered_valid_fields = BamlMap::new();
                for field in self.fields.iter() {
                    let key = field.0.real_name();
                    if let Some(value) = valid_fields.get(key) {
                        ordered_valid_fields.insert(key.to_string(), value.clone());
                    }
                }

                let completed_instance = Ok(BamlValueWithFlags::Class(
                    self.name.real_name().into(),
                    flags,
                    target.clone(),
                    ordered_valid_fields.clone(),
                ))
                .and_then(|value| {
                    apply_constraints(
                        target,
                        vec![],
                        value,
                        constraints.clone(),
                        streaming_behavior,
                    )
                });

                completed_cls.insert(0, completed_instance);
            }
        }

        log::trace!("Completed class: {completed_cls:#?}");

        array_helper::pick_best(ctx, target, &completed_cls)
    }
}

pub fn apply_constraints(
    class_type: &TypeIR,
    scope: Vec<String>,
    mut value: BamlValueWithFlags,
    constraints: Vec<Constraint>,
    streaming_behavior: baml_types::type_meta::base::StreamingBehavior,
) -> Result<BamlValueWithFlags, ParsingError> {
    if constraints.is_empty() {
        Ok(value)
    } else {
        let mut constrained_class = class_type.clone();
        constrained_class.set_meta(baml_types::type_meta::base::TypeMeta {
            constraints,
            streaming_behavior,
        });
        let constraint_results = run_user_checks(&value.clone().into(), &constrained_class)
            .map_err(|e| ParsingError {
                reason: format!("Failed to evaluate constraints: {e:?}"),
                scope,
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
        value.add_flag(Flag::ConstraintResults(check_results));
        Ok(value)
    }
}

fn update_map<'a>(
    required_values: &'a mut BamlMap<String, Option<Result<BamlValueWithFlags, ParsingError>>>,
    optional_values: &'a mut BamlMap<String, Option<Result<BamlValueWithFlags, ParsingError>>>,
    (name, t, ..): &'a FieldValue,
    value: Result<BamlValueWithFlags, ParsingError>,
) {
    let map = if t.is_optional() {
        optional_values
    } else {
        required_values
    };
    let key = name.real_name();
    // TODO: @hellovai plumb this via some flag?
    match map.get(key) {
        Some(Some(_)) => {
            // DO NOTHING (keep first value)
            log::trace!("Duplicate field: {key}");
        }
        Some(None) => {
            map.insert(key.into(), Some(value));
        }
        None => {
            log::trace!("Field not found: {key}");
        }
    }
}
