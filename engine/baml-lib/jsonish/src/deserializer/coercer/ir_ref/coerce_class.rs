use anyhow::Result;
use baml_types::BamlMap;
use internal_baml_core::ir::FieldType;
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
        log::debug!(
            "scope: {scope} :: coercing to: {name} (current: {current})",
            name = self.name.real_name(),
            scope = ctx.display_scope(),
            current = value.map(|v| v.r#type()).unwrap_or("<null>".into())
        );
        let (optional, required): (Vec<_>, Vec<_>) =
            self.fields.iter().partition(|f| f.1.is_optional());

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
            Some(crate::jsonish::Value::Object(obj)) => {
                // match keys, if that fails, then do something fancy later.
                let mut extra_keys = vec![];
                let mut found_keys = false;
                obj.iter().for_each(|(key, v)| {
                    if let Some(field) = self
                        .fields
                        .iter()
                        .find(|(name, ..)| name.rendered_name().trim() == key)
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
                            Some(&crate::jsonish::Value::Object(obj.clone())),
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
            Some(crate::jsonish::Value::Array(items)) => {
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
                if let Ok(option1) = array_helper::coerce_array_to_singular(
                    ctx,
                    target,
                    &items.iter().collect::<Vec<_>>(),
                    &|value| self.coerce(ctx, target, Some(value)),
                ) {
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
                            flags.add_flag(Flag::InferredObject(x.clone()));
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
                                DeserializerConditions::new()
                                    .with_flag(Flag::OptionalDefaultFromNoValue),
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
                            None => (k.to_string(), BamlValueWithFlags::Null(Default::default())),
                            Some(Err(e)) => (
                                k.to_string(),
                                BamlValueWithFlags::Null(
                                    DeserializerConditions::new()
                                        .with_flag(Flag::DefaultButHadUnparsableValue(e)),
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

                completed_cls.insert(
                    0,
                    Ok(BamlValueWithFlags::Class(
                        self.name.real_name().into(),
                        flags,
                        ordered_valid_fields,
                    )),
                );
            }
        }

        log::trace!("Completed class: {:#?}", completed_cls);

        array_helper::pick_best(ctx, target, &completed_cls)
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
            log::trace!("Duplicate field: {}", key);
        }
        Some(None) => {
            map.insert(key.into(), Some(value));
        }
        None => {
            log::trace!("Field not found: {}", key);
        }
    }
}
