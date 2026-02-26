use crate::baml_value::{BamlClass, BamlNull, BamlValue};
use crate::sap_model::{
    AnnotatedField, ClassTy, NullTy, PrimitiveTy, TyResolvedRef, TyWithMeta, TypeAnnotations,
    TypeIdent, TypeRefDb,
};
use anyhow::Result;
use indexmap::IndexMap;

use super::ParsingContext;
use crate::deserializer::{
    coercer::{ParsingError, TypeCoercer, array_helper, match_string::matches_string_to_string},
    deserialize_flags::{DeserializerConditions, Flag},
    types::{BamlValueWithFlags, DeserializerMeta, ValueWithFlags},
};

/// Helper to construct a null BamlValueWithFlags with the given flags.
fn null_value_with_flags<'t, N: TypeIdent>(
    flags: DeserializerConditions<'t, N>,
    meta: &'t TypeAnnotations<'t, N>,
) -> BamlValueWithFlags<'t, N> {
    BamlValueWithFlags::new(
        BamlValue::Null(BamlNull),
        DeserializerMeta {
            flags,
            ty: TyWithMeta::new(TyResolvedRef::Primitive(PrimitiveTy::Null(NullTy)), meta),
        },
    )
}

/// Helper to resolve and coerce a field type.
fn resolve_and_coerce<'t, N: TypeIdent>(
    ctx: &ParsingContext<'t, N>,
    field_ty: &'t TyWithMeta<crate::sap_model::Ty<'t, N>, TypeAnnotations<'t, N>>,
    value: Option<&crate::jsonish::Value>,
) -> Result<BamlValueWithFlags<'t, N>, ParsingError> {
    let resolved = ctx
        .db
        .resolve_with_meta(field_ty.as_ref())
        .map_err(|ident| ctx.error_type_resolution(ident))?;
    let rt = TyWithMeta::new(resolved.ty, resolved.meta);
    TyResolvedRef::coerce(ctx, rt, value)
}

impl<'t, N: TypeIdent> TypeCoercer<'t, N> for ClassTy<'t, N> {
    fn try_cast(
        ctx: &ParsingContext<'t, N>,
        target: TyWithMeta<&'t Self, &'t TypeAnnotations<'t, N>>,
        value: Option<&crate::jsonish::Value>,
    ) -> Option<ValueWithFlags<'t, BamlClass<'t, N>, N>> {
        let class_ty = target.ty;
        let meta = target.meta;
        let name = &class_ty.name;
        let fields = &class_ty.fields;

        // Only handle object values for class types
        let Some(crate::jsonish::Value::Object(obj, _completion_state)) = value else {
            return None;
        };

        let ctx = {
            let cls_value_pair = (name.to_string(), value.unwrap().to_owned());

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
        enum Triple<'t, N: TypeIdent> {
            Pending,
            NotPresent,
            Present(Box<BamlValueWithFlags<'t, N>>),
        }

        let mut fill_result = fields
            .iter()
            .map(|f| (f.name.as_ref(), (&f.ty, Triple::Pending)))
            .collect::<IndexMap<_, _>>();

        let flags = DeserializerConditions::new();
        for (k, v) in obj.iter() {
            if let Some((field_type, val)) = fill_result.get_mut(k.as_str()) {
                if matches!(val, Triple::Present(_)) {
                    continue;
                }
                // Resolve the field type and try_cast
                let resolved = ctx.db.resolve_with_meta(field_type.as_ref()).ok()?;
                let rt = TyWithMeta::new(resolved.ty, resolved.meta);
                if let Some(cast_value) = TyResolvedRef::try_cast(ctx, rt, Some(v)) {
                    *val = Triple::Present(Box::new(cast_value));
                } else {
                    return None;
                }
            } else {
                // In try_cast mode, reject objects with extra keys for stricter matching
                return None;
            }
        }

        let mut result: IndexMap<String, BamlValueWithFlags<'t, N>> = IndexMap::new();
        for (field_name, (field_type, val)) in fill_result.into_iter() {
            if let Triple::Present(ref val_ref) = val {
                // Check if field is required (non-optional) and is incomplete in streaming mode
                // TODO: Add streaming mode check when ParsingContext supports it.
                // Previously checked ctx.do_not_use_mode == StreamingMode::Streaming
                if !field_type.ty.is_optional(ctx.db)
                    && val_ref
                        .conditions()
                        .flags
                        .iter()
                        .any(|f| matches!(f, Flag::Incomplete))
                {
                    // In non-streaming mode, incomplete required fields are accepted.
                    // In streaming mode (when supported), this would return None.
                }
            }

            if let Triple::Present(val) = val {
                result.insert(field_name.to_string(), *val);
            } else if field_type.ty.is_optional(ctx.db) {
                result.insert(
                    field_name.to_string(),
                    null_value_with_flags(DeserializerConditions::new(), meta),
                );
            } else {
                return None;
            }
        }

        Some(ValueWithFlags::new(
            BamlClass {
                name: &class_ty.name,
                value: result,
            },
            DeserializerMeta {
                flags,
                ty: TyWithMeta::new(TyResolvedRef::Class(class_ty), meta),
            },
        ))
    }

    fn coerce(
        ctx: &ParsingContext<'t, N>,
        target: TyWithMeta<&'t Self, &'t TypeAnnotations<'t, N>>,
        value: Option<&crate::jsonish::Value>,
    ) -> Result<ValueWithFlags<'t, BamlClass<'t, N>, N>, ParsingError> {
        let class_ty = target.ty;
        let meta = target.meta;

        log::debug!(
            "scope: {scope} :: coercing to: {name} (current: {current})",
            name = class_ty.name,
            scope = ctx.display_scope(),
            current = value.map(|v| v.r#type()).unwrap_or("<null>".into())
        );

        // If value is not None then we'll update the context to store the
        // current class in the visited set and we'll use that to stop recursion
        // when dealing with recursive classes.
        let mut nested_ctx = None;

        if let Some(v) = value {
            let cls_value_pair = (class_ty.name.to_string(), v.to_owned());

            // If this combination has been visited bail out.
            if ctx.visited_during_coerce.contains(&cls_value_pair) {
                return Err(ctx.error_circular_reference(&class_ty.name.to_string(), v));
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

        let (optional, required): (Vec<_>, Vec<_>) = class_ty
            .fields
            .iter()
            .partition(|f| f.ty.ty.is_optional(ctx.db));

        let mut optional_values = optional
            .iter()
            .map(|f| (f.name.to_string(), None))
            .collect::<IndexMap<String, Option<Result<BamlValueWithFlags<'t, N>, ParsingError>>>>();
        let mut required_values = required
            .iter()
            .map(|f| (f.name.to_string(), None))
            .collect::<IndexMap<String, Option<Result<BamlValueWithFlags<'t, N>, ParsingError>>>>();
        let mut flags = DeserializerConditions::new();

        let mut completed_cls: Vec<Result<BamlValueWithFlags<'t, N>, ParsingError>> = Vec::new();

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
                    if let Some(field) = class_ty
                        .fields
                        .iter()
                        .find(|f| matches_string_to_string(ctx, key, &f.name))
                    {
                        let scope = ctx.enter_scope(&field.name);
                        let parsed = resolve_and_coerce(&scope, &field.ty, Some(v));
                        update_map(
                            ctx.db,
                            &mut required_values,
                            &mut optional_values,
                            field,
                            parsed,
                        );
                        found_keys = true;
                    } else {
                        extra_keys.push((key, v));
                    }
                });

                if !found_keys && !extra_keys.is_empty() && class_ty.fields.len() == 1 {
                    // Try to coerce the object into the single field
                    let field = &class_ty.fields[0];
                    let scope = ctx.enter_scope(&format!("<implied:{}>", field.name));
                    let parsed = resolve_and_coerce(
                        &scope,
                        &field.ty,
                        Some(&crate::jsonish::Value::Object(
                            obj.clone(),
                            completion.clone(),
                        )),
                    )
                    .map(|mut v| {
                        v.add_flag(Flag::ImpliedKey(field.name.to_string()));
                        v
                    });

                    if let Ok(parsed_value) = parsed {
                        update_map(
                            ctx.db,
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
            Some(crate::jsonish::Value::Array(items, _completion)) => {
                if class_ty.fields.len() == 1 {
                    let field = &class_ty.fields[0];
                    let scope = ctx.enter_scope(&format!("<implied:{}>", field.name));
                    let parsed = match resolve_and_coerce(&scope, &field.ty, value) {
                        Ok(mut v) => {
                            v.add_flag(Flag::ImpliedKey(field.name.to_string()));
                            Ok(v)
                        }
                        Err(e) => Err(e),
                    };
                    update_map(
                        ctx.db,
                        &mut required_values,
                        &mut optional_values,
                        field,
                        parsed,
                    );
                }

                // Coerce each item into the class if possible
                let option1_result = array_helper::coerce_array_to_singular(
                    ctx,
                    TyWithMeta::new(TyResolvedRef::Class(class_ty), meta),
                    &items.iter().collect::<Vec<_>>(),
                    &|value| {
                        Self::coerce(ctx, TyWithMeta::new(class_ty, meta), Some(value))
                            .map(|v| v.map_value(BamlValue::Class))
                    },
                )
                .and_then(|value| {
                    apply_constraints(TyWithMeta::new(class_ty, meta), ctx.scope.clone(), value)
                });
                if let Ok(option1) = option1_result {
                    completed_cls.push(Ok(option1));
                }
            }
            Some(x) => {
                // If the class has a single field, then we can try to coerce it directly
                if class_ty.fields.len() == 1 {
                    let field = &class_ty.fields[0];
                    let scope = ctx.enter_scope(&format!("<implied:{}>", field.name));
                    let parsed = match resolve_and_coerce(&scope, &field.ty, Some(x)) {
                        Ok(mut v) => {
                            v.add_flag(Flag::ImpliedKey(field.name.to_string()));
                            flags.add_flag(Flag::InferedObject(x.clone()));
                            Ok(v)
                        }
                        Err(e) => Err(e),
                    };
                    update_map(
                        ctx.db,
                        &mut required_values,
                        &mut optional_values,
                        field,
                        parsed,
                    );
                }
            }
        }

        // Check what we have / what we need
        {
            class_ty.fields.iter().for_each(|field| {
                if field.ty.ty.is_optional(ctx.db) {
                    if let Some(v) = optional_values.get(&*field.name) {
                        let next = match v {
                            Some(Ok(_)) => None,
                            Some(Err(e)) => {
                                log::trace!("Error in optional field {}: {}", field.name, e);
                                // TODO: Implement DefaultValue for AnnotatedTy.
                                // For now, return a null default for optional fields with errors.
                                let mut flags = DeserializerConditions::new();
                                flags.add_flag(Flag::DefaultButHadUnparseableValue(e.clone()));
                                Some(null_value_with_flags(flags, meta))
                            }
                            // If we're missing a field, thats ok!
                            None => {
                                let mut flags = DeserializerConditions::new();
                                flags.add_flag(Flag::OptionalDefaultFromNoValue);
                                flags.add_flag(Flag::Pending);
                                Some(null_value_with_flags(flags, meta))
                            }
                        };

                        if let Some(next) = next {
                            optional_values.insert(field.name.to_string(), Some(Ok(next)));
                        }
                    }
                } else if let Some(v) = required_values.get(&*field.name) {
                    let next = match v {
                        Some(Ok(_)) => None,
                        // TODO: Implement DefaultValue for AnnotatedTy.
                        // For now, required fields with errors have no default.
                        Some(Err(_e)) => None,
                        None => None,
                    };

                    if let Some(next) = next {
                        required_values.insert(field.name.to_string(), Some(Ok(next)));
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
                    Some(Err(_e)) => None,
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
                let valid_fields = required_values
                    .iter()
                    .filter_map(|(k, v)| match v {
                        Some(Ok(v)) => Some((k.to_string(), v.clone())),
                        _ => None,
                    })
                    .chain(optional_values.iter().map(|(k, v)| match v {
                        Some(Ok(v)) => (k.to_string(), v.clone()),
                        None => {
                            let mut flags = DeserializerConditions::new();
                            flags.add_flag(Flag::Incomplete);
                            (k.to_string(), null_value_with_flags(flags, meta))
                        }
                        Some(Err(e)) => {
                            let mut flags = DeserializerConditions::new();
                            flags.add_flag(Flag::DefaultButHadUnparseableValue(e.clone()));
                            flags.add_flag(Flag::Incomplete);
                            (k.to_string(), null_value_with_flags(flags, meta))
                        }
                    }))
                    .collect::<IndexMap<String, _>>();

                // Create an IndexMap ordered according to class_ty.fields
                let mut class_fields: IndexMap<String, BamlValueWithFlags<'t, N>> = IndexMap::new();
                for field in class_ty.fields.iter() {
                    let key = &*field.name;
                    if let Some(value) = valid_fields.get(key) {
                        class_fields.insert(key.to_string(), value.clone());
                    }
                }

                let completed_instance = Ok(ValueWithFlags::new(
                    BamlClass {
                        name: &class_ty.name,
                        value: class_fields,
                    },
                    DeserializerMeta {
                        flags,
                        ty: TyWithMeta::new(TyResolvedRef::Class(class_ty), meta),
                    },
                ))
                .map(|v| v.map_value(BamlValue::Class))
                .and_then(|value| {
                    apply_constraints(TyWithMeta::new(class_ty, meta), ctx.scope.clone(), value)
                });

                completed_cls.insert(0, completed_instance);
            }
        }

        log::trace!("Completed class: {completed_cls:#?}");

        let best = array_helper::pick_best(
            ctx,
            TyWithMeta::new(TyResolvedRef::Class(class_ty), meta),
            completed_cls,
        )?;
        let BamlValue::Class(class_val) = best.value else {
            unreachable!("pick_best should only return Class for ClassTy coercion");
        };
        Ok(ValueWithFlags::new(class_val, best.meta))
    }
}

pub fn apply_constraints<'t, N: TypeIdent>(
    class_target: TyWithMeta<&ClassTy<'t, N>, &'t TypeAnnotations<'t, N>>,
    _scope: Vec<String>,
    value: BamlValueWithFlags<'t, N>,
) -> Result<BamlValueWithFlags<'t, N>, ParsingError> {
    if !class_target.meta.asserts.is_empty() {
        // TODO: run assertion checks using class_target.meta.asserts
        // Need to adapt run_user_checks to work with TypeAnnotations
    }
    Ok(value)
}

fn update_map<'t, N: TypeIdent>(
    db: &TypeRefDb<'t, N>,
    required_values: &mut IndexMap<String, Option<Result<BamlValueWithFlags<'t, N>, ParsingError>>>,
    optional_values: &mut IndexMap<String, Option<Result<BamlValueWithFlags<'t, N>, ParsingError>>>,
    field: &AnnotatedField<'t, N>,
    value: Result<BamlValueWithFlags<'t, N>, ParsingError>,
) {
    let map = if field.ty.ty.is_optional(db) {
        optional_values
    } else {
        required_values
    };
    let key = &*field.name;
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
