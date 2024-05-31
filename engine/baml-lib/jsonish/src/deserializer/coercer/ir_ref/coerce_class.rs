use std::{collections::HashMap, iter};

use anyhow::Result;
use baml_types::BamlMap;
use internal_baml_core::{
    ast::Field,
    ir::{ClassFieldWalker, ClassWalker, FieldType},
};
use internal_baml_jinja::types::{Class, Name};

use crate::deserializer::{
    coercer::{array_helper, DefaultValue, ParsingError, TypeCoercer},
    deserialize_flags::{DeserializerConditions, Flag},
    types::BamlValueWithFlags,
};

use super::ParsingContext;

// Name, type, description
type FieldValue = (Name, FieldType, Option<String>);

impl TypeCoercer for Class {
    fn coerce(
        &self,
        ctx: &ParsingContext,
        target: &FieldType,
        value: Option<&crate::jsonish::Value>,
    ) -> Result<BamlValueWithFlags, ParsingError> {
        let (optional, required): (Vec<_>, Vec<_>) =
            self.fields.iter().partition(|f| f.1.is_optional());
        let mut optional_values = optional
            .iter()
            .map(|(f, ..)| (f.real_name().to_string(), None))
            .collect::<HashMap<_, _>>();
        let mut required_values = required
            .iter()
            .map(|(f, ..)| (f.real_name().to_string(), None))
            .collect::<HashMap<_, _>>();
        let mut flags = DeserializerConditions::new();

        let mut completed_cls = Vec::new();

        match self.fields.len() {
            0 => {}
            1 => {
                // Special case for single fields (we may want to consider creating the kv manually)
                let field = &self.fields[0];
                let parsed_field =
                    parse_field((self, target), field, ctx, value, &mut completed_cls, false);

                update_map(
                    &mut required_values,
                    &mut optional_values,
                    field,
                    parsed_field,
                );
            }
            _ => {
                match value {
                    None | Some(crate::jsonish::Value::Null) => {
                        // We have multiple fields, but no value to parse
                    }
                    Some(crate::jsonish::Value::Array(items)) => {
                        // Coerce the each item into the class
                        if let Ok(option1) = array_helper::coerce_array_to_singular(
                            ctx,
                            target,
                            &items.iter().collect::<Vec<_>>(),
                            &|value| self.coerce(ctx, target, Some(value)),
                        ) {
                            completed_cls.push(Ok(option1));
                        }
                    }
                    Some(crate::jsonish::Value::Object(obj)) => {
                        obj.iter().for_each(|(key, v)| {
                            if let Some(field) = self
                                .fields
                                .iter()
                                .find(|(name, ..)| name.rendered_name().trim() == key)
                            {
                                let parsed_field = parse_field(
                                    (self, target),
                                    field,
                                    ctx,
                                    Some(v),
                                    &mut completed_cls,
                                    true,
                                );
                                update_map(
                                    &mut required_values,
                                    &mut optional_values,
                                    field,
                                    parsed_field,
                                );
                            } else {
                                flags.add_flag(Flag::ExtraKey(key.clone(), v.clone()));
                            }
                        });
                    }
                    _ => {}
                }
            }
        }

        // Now try and assemble the class.

        // Check what we have / what we need
        {
            self.fields.iter().for_each(|(field_name, t, ..)| {
                if t.is_optional() {
                    if let Some(v) = optional_values.get(field_name.real_name()) {
                        let next = match v {
                            Some(Ok(_)) => None,
                            Some(Err(e)) => {
                                log::info!(
                                    "Error in optional field {}: {}",
                                    field_name.real_name(),
                                    e
                                );
                                t.default_value(Some(e))
                            }
                            // If we're missing a field, thats ok!
                            None => Some(BamlValueWithFlags::Null(
                                DeserializerConditions::new().with_flag(Flag::DefaultFromNoValue),
                            )),
                        };

                        if let Some(next) = next {
                            optional_values
                                .insert(field_name.real_name().to_string(), Some(Ok(next)));
                        }
                    }
                } else {
                    if let Some(v) = required_values.get(field_name.real_name()) {
                        let next = match v {
                            Some(Ok(_)) => None,
                            Some(Err(e)) => t.default_value(Some(e)).or_else(|| {
                                if ctx.allow_partials {
                                    Some(BamlValueWithFlags::Null(
                                        DeserializerConditions::new()
                                            .with_flag(Flag::OptionalDefaultFromNoValue),
                                    ))
                                } else {
                                    None
                                }
                            }),
                            None => t.default_value(None).or_else(|| {
                                if ctx.allow_partials {
                                    Some(BamlValueWithFlags::Null(
                                        DeserializerConditions::new()
                                            .with_flag(Flag::OptionalDefaultFromNoValue),
                                    ))
                                } else {
                                    None
                                }
                            }),
                        };

                        if let Some(next) = next {
                            required_values
                                .insert(field_name.real_name().to_string(), Some(Ok(next)));
                        }
                    }
                }
            });

            log::info!("---");
            for (k, v) in optional_values.iter() {
                log::info!("  Optional field: {} = {:?}", k, v.is_none());
            }
            for (k, v) in required_values.iter() {
                log::info!("  Required field: {} = {:?}", k, v.is_none());
            }
            log::info!("----");

            let missing_required_fields = required_values
                .iter()
                .filter(|(_, v)| v.is_none())
                .map(|(k, _)| k.clone())
                .collect::<Vec<_>>();

            if !missing_required_fields.is_empty() {
                log::info!(
                    "Missing required fields: {:?} in  {:?}",
                    missing_required_fields,
                    value
                );
                if completed_cls.is_empty() {
                    return Err(ctx.error_missing_required_field(&missing_required_fields, value));
                }
            } else {
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
                            None => (k.to_string(), BamlValueWithFlags::Null(Default::default())),
                            Some(Err(e)) => (
                                k.to_string(),
                                BamlValueWithFlags::Null(
                                    DeserializerConditions::new()
                                        .with_flag(Flag::DefaultButHadUnparseableValue(e)),
                                ),
                            ),
                        }
                    }))
                    .collect::<BamlMap<String, _>>();

                completed_cls.insert(
                    0,
                    Ok(BamlValueWithFlags::Class(
                        self.name.real_name().into(),
                        flags,
                        valid_fields,
                    )),
                );
            }
        }

        log::debug!("Completed class: {:#?}", completed_cls);

        array_helper::pick_best(ctx, target, &completed_cls)
    }
}

fn parse_field<'a>(
    (cls, cls_target): (&'a Class, &FieldType),
    (field_name, t, ..): &'a FieldValue,
    ctx: &ParsingContext,
    value: Option<&crate::jsonish::Value>,
    completed_cls: &mut Vec<Result<BamlValueWithFlags, ParsingError>>,
    in_key: bool,
) -> Result<BamlValueWithFlags, ParsingError> {
    log::info!("Parsing field: {} from {:?}", field_name.real_name(), value);

    match value {
        Some(crate::jsonish::Value::Array(items)) => {
            // This could be either the case that:
            // - multiple candidates for that class
            // - multiple values for the field
            // - the field itself is mutliple value

            // Coerce the each item into the class
            if let Ok(option1) = array_helper::coerce_array_to_singular(
                ctx,
                cls_target,
                &items.iter().collect::<Vec<_>>(),
                &|value| cls.coerce(ctx, cls_target, Some(value)),
            ) {
                completed_cls.push(Ok(option1));
            }

            let field_scope = ctx.enter_scope(field_name.real_name());
            // Coerce the each item into the field
            let option2 = array_helper::coerce_array_to_singular(
                &field_scope,
                t,
                &items.iter().collect::<Vec<_>>(),
                &|value| t.coerce(&field_scope, t, Some(value)),
            );

            // Coerce the array to the field
            let option3 = t.coerce(&ctx.enter_scope(field_name.real_name()), t, value);

            match array_helper::pick_best(&field_scope, t, &[option2, option3]) {
                Ok(mut v) => {
                    if !in_key {
                        v.add_flag(Flag::ImpliedKey(field_name.real_name().into()));
                    }
                    Ok(v)
                }
                Err(e) => Err(e),
            }
        }
        Some(crate::jsonish::Value::Object(obj)) => {
            let field_scope = ctx.enter_scope(field_name.real_name());
            let valid_keys = [field_name.rendered_name()];

            // Coerce each matching key into the field
            let mut candidates = valid_keys
                .iter()
                .filter_map(|&key| {
                    obj.get(key)
                        .map(|value| t.coerce(&field_scope, t, Some(value)))
                })
                .collect::<Vec<_>>();

            if obj.is_empty() && t.is_optional() {
                // If the object is empty, and the field is optional, then we can just return null
                candidates.push(Ok(BamlValueWithFlags::Null(
                    DeserializerConditions::new().with_flag(Flag::OptionalDefaultFromNoValue),
                )));
            }

            // Also try to implicitly coerce the object into the field
            let option2 = match t.coerce(&field_scope, t, value) {
                Ok(mut v) => {
                    v.add_flag(Flag::ImpliedKey(field_name.real_name().into()));
                    Ok(v)
                }
                Err(e) => Err(e),
            };

            candidates.push(option2);
            array_helper::pick_best(&field_scope, t, &candidates)
        }
        v => match t.coerce(&ctx.enter_scope(field_name.real_name()), t, v) {
            Ok(mut v) => {
                if !in_key {
                    v.add_flag(Flag::ImpliedKey(field_name.real_name().into()));
                }
                Ok(v)
            }
            Err(e) => Err(e),
        },
    }
}

fn update_map<'a>(
    required_values: &'a mut HashMap<String, Option<Result<BamlValueWithFlags, ParsingError>>>,
    optional_values: &'a mut HashMap<String, Option<Result<BamlValueWithFlags, ParsingError>>>,
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
            log::debug!("Duplicate field: {}", key);
        }
        Some(None) => {
            map.insert(key.into(), Some(value));
        }
        None => {
            log::debug!("Field not found: {}", key);
        }
    }
}
