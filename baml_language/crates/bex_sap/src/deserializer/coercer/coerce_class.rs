use std::borrow::Cow;
use std::collections::{HashMap, hash_map};

use crate::baml_value::{BamlClass, BamlNull, BamlValue};
use crate::jsonish::{self, CompletionState};
use crate::sap_model::{
    AnnotatedField, ClassTy, FromLiteral as _, Literal, TyResolvedRef, TyWithMeta, TypeAnnotations,
    TypeIdent,
};
use anyhow::Result;
use indexmap::IndexMap;

use super::ParsingContext;
use crate::deserializer::{
    coercer::{ParsingError, TypeCoercer, array_helper, match_string::matches_string_to_string},
    deserialize_flags::{DeserializerConditions, Flag},
    types::{BamlValueWithFlags, DeserializerMeta, ValueWithFlags},
};

impl<'s, 'v, 't, N: TypeIdent> TypeCoercer<'s, 'v, 't, N> for ClassTy<'t, N>
where
    't: 's,
    's: 'v,
{
    fn try_cast(
        ctx: &ParsingContext<'s, 'v, 't, N>,
        target: TyWithMeta<&'t Self, &'t TypeAnnotations<'t, N>>,
        value: &'v crate::jsonish::Value<'s>,
    ) -> Option<ValueWithFlags<'s, 'v, 't, BamlClass<'s, 'v, 't, N>, N>> {
        let class_ty = target.ty;
        let meta = target.meta;
        let name = &class_ty.name;

        // Only handle object values for class types
        let crate::jsonish::Value::Object(obj, completion_state) = value else {
            return None;
        };

        let ctx = {
            let cls_value_pair = (name.to_string(), value);

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

        let mut flags = DeserializerConditions::new();

        // add entries as fields
        let mut field_data = IndexMap::new();
        for (k, v) in obj {
            let field = class_ty.fields.iter().find(|f| f.key_matches(&*k));
            let Some(field) = field else {
                // it is an extra entry. try_cast is strict and rejects when it finds extra keys.
                return None;
            };
            let field_ty = ctx
                .db
                .resolve_with_meta(field.ty.as_ref())
                .map_err(|ident| ctx.error_type_resolution(ident))
                .ok()?;
            let value = TyResolvedRef::try_cast(ctx, field_ty.clone(), v)?;
            field_data.insert(&*field.name, value);
        }

        // check all fields
        for field in &class_ty.fields {
            let AnnotatedField {
                name,
                ty,
                before_started,
                missing,
                ..
            } = field;
            if field_data.contains_key(name.as_ref()) {
                continue; // happy path: we already have this field
            }

            let replacement = match completion_state {
                CompletionState::Incomplete => before_started,
                CompletionState::Complete => missing,
            };
            if matches!(replacement, Literal::Never) && ty.ty.is_optional(ctx.db) {
                continue; // happy path: we don't need the field, it's optional
            }

            let field_ty = ctx
                .db
                .resolve_with_meta(ty.as_ref())
                .map_err(|ident| ctx.error_type_resolution(ident))
                .ok()?;
            let value = field_ty.ty.from_literal(replacement, ctx).ok()?;
            let value = BamlValueWithFlags::new(
                value,
                DeserializerMeta {
                    flags: DeserializerConditions::new().with_flag(Flag::DefaultFromNoValue),
                    ty: field_ty,
                },
            );
            field_data.insert(&**name, value);
        }

        if *completion_state == CompletionState::Incomplete {
            flags.add_flag(Flag::Incomplete);
        }

        Some(ValueWithFlags::new(
            BamlClass {
                name: &class_ty.name,
                value: field_data,
            },
            DeserializerMeta {
                flags,
                ty: TyWithMeta::new(TyResolvedRef::Class(class_ty), meta),
            },
        ))
    }

    fn coerce(
        ctx: &ParsingContext<'s, 'v, 't, N>,
        target: TyWithMeta<&'t Self, &'t TypeAnnotations<'t, N>>,
        value: &'v crate::jsonish::Value<'s>,
    ) -> Result<Option<ValueWithFlags<'s, 'v, 't, BamlClass<'s, 'v, 't, N>, N>>, ParsingError> {
        let class_ty = target.ty;
        let meta = target.meta;

        log::debug!(
            "scope: {scope} :: coercing to: {name} (current: {current})",
            name = class_ty.name,
            scope = ctx.display_scope(),
            current = value.r#type()
        );

        // If value is not None then we'll update the context to store the
        // current class in the visited set and we'll use that to stop recursion
        // when dealing with recursive classes.
        // TODO: is this necessary? we should be recusing over the finite input data, not the potentially infinite type structure

        let cls_value_pair = (class_ty.name.to_string(), value);

        // If this combination has been visited bail out.
        if ctx.visited_during_coerce.contains(&cls_value_pair) {
            return Err(ctx.error_circular_reference(&class_ty.name.to_string(), value));
        }

        // Mark this class as visited for the duration of this function
        // call. Further recursion from within this function will see that
        // the class has already been visited and stop recursing. Different
        // calls to this function for other fields pointing to the same
        // recursive class should start from scratch with an empty visited
        // set so they will not fail because this class has already been
        // coerced for a different field.
        let nested_ctx = Some(ctx.visit_class_value_pair(cls_value_pair, true));

        // Now just maintain the previous context or get the new one and proceed
        // normally.
        let ctx = nested_ctx.as_ref().unwrap_or(ctx);

        // There are a few possible approaches here:
        let ret = match (value, target.meta.in_progress.as_ref()) {
            (jsonish::Value::Object(_, CompletionState::Incomplete), Some(Literal::Never)) => {
                return Ok(None);
            }
            (jsonish::Value::Object(_, CompletionState::Incomplete), Some(lit)) => {
                return target
                    .ty
                    .from_literal(lit, ctx)
                    .map(|v| {
                        ValueWithFlags::new(v, DeserializerMeta::new(target))
                            .with_flag(Flag::DefaultFromInProgress(Cow::Borrowed(value)))
                    })
                    .map(Some);
            }
            (jsonish::Value::Object(obj, c), None) => {
                let mut flags = DeserializerConditions::new();
                if c == &CompletionState::Incomplete {
                    flags.add_flag(Flag::Incomplete);
                }
                let mut extra_keys = IndexMap::new();
                let mut entries = HashMap::new();
                for (key, v) in obj {
                    let Some(field) = class_ty
                        .fields
                        .iter()
                        .find(|f| matches_string_to_string(ctx, key, &f.name))
                    else {
                        extra_keys.insert(key.clone(), v);
                        continue;
                    };
                    // TODO: aliases

                    let scope = ctx.enter_scope(&field.name);
                    let resolved = scope
                        .db
                        .resolve_with_meta(field.ty.as_ref())
                        .map_err(|ident| scope.error_type_resolution(ident));
                    let Some(parsed) = resolved
                        .and_then(|resolved| TyResolvedRef::coerce(&scope, resolved, v))
                        .transpose()
                    else {
                        continue;
                    };

                    match entries.entry(key.clone()) {
                        hash_map::Entry::Occupied(_) => {
                            log::trace!("Duplicate field: {key}");
                        }
                        hash_map::Entry::Vacant(entry) => {
                            entry.insert(parsed);
                        }
                    };
                }

                if entries.is_empty()
                    && !extra_keys.is_empty()
                    && let [field] = class_ty.fields.as_slice()
                {
                    // Try to coerce the object into the single field
                    let scope = ctx.enter_scope(&format!("<implied:{}>", field.name));
                    let resolved = scope
                        .db
                        .resolve_with_meta(field.ty.as_ref())
                        .map_err(|ident| scope.error_type_resolution(ident));
                    let parsed = resolved
                        .and_then(|resolved| TyResolvedRef::coerce(&scope, resolved, value))
                        .map(|v| v.map(|v| v.with_flag(Flag::ImpliedKey(field.name.clone()))));

                    if let Ok(Some(parsed_value)) = parsed {
                        entries.insert(field.name.clone(), Ok(parsed_value));
                    } else {
                        for (key, v) in extra_keys {
                            flags.add_flag(Flag::ExtraKey(key, Cow::Borrowed(v)));
                        }
                    }
                } else {
                    for (key, v) in extra_keys {
                        flags.add_flag(Flag::ExtraKey(key, Cow::Borrowed(v)));
                    }
                }
                class_from_entries(
                    ctx,
                    target.clone(),
                    c == &CompletionState::Incomplete,
                    entries,
                    flags,
                )
            }
            (jsonish::Value::Array(_, CompletionState::Incomplete), Some(Literal::Never)) => {
                return Ok(None);
            }
            (jsonish::Value::Array(_, CompletionState::Incomplete), Some(lit)) => {
                target.ty.from_literal(lit, ctx).map(|v| {
                    ValueWithFlags::new(v, DeserializerMeta::new(target.clone()))
                        .with_flag(Flag::DefaultFromInProgress(Cow::Borrowed(value)))
                })
            }
            (jsonish::Value::Array(items, c), None) => {
                let mut completed = Vec::new();
                if let [field] = class_ty.fields.as_slice()
                    && let scope = ctx.enter_scope(&format!("<implied:{}>", field.name))
                    && let Ok(Some(mut parsed)) = scope
                        .db
                        .resolve_with_meta(field.ty.as_ref())
                        .map_err(|ident| scope.error_type_resolution(ident))
                        .and_then(|resolved| TyResolvedRef::coerce(&scope, resolved, value))
                {
                    // The class has only one field, and this seems to be the inner type
                    let mut flags = DeserializerConditions::new();
                    if c == &CompletionState::Incomplete {
                        flags.add_flag(Flag::Incomplete);
                    }
                    parsed.add_flag(Flag::ImpliedKey(field.name.clone()));
                    flags.add_flag(Flag::InferedObject(Cow::Borrowed(value)));
                    let mut entries = IndexMap::new();
                    entries.insert(&*field.name, parsed);

                    let cls_value = BamlClass {
                        name: &class_ty.name,
                        value: entries,
                    };
                    let cls_meta =
                        DeserializerMeta::new(target.clone().map_ty(TyResolvedRef::Class));
                    completed.push(Ok(ValueWithFlags::new(
                        BamlValue::Class(cls_value),
                        cls_meta,
                    )
                    .with_flags(flags.flags)));
                }

                let singular = array_helper::coerce_array_to_singular(
                    ctx,
                    TyWithMeta::new(TyResolvedRef::Class(class_ty), meta),
                    items.iter(),
                    &|value| {
                        Self::coerce(ctx, TyWithMeta::new(class_ty, meta), value)
                            .map(|v| v.map(|v| v.map_value(BamlValue::Class)))
                    },
                );
                completed.push(singular);

                if completed.is_empty() {
                    Err(ctx.error_unexpected_type(&target, value))
                } else {
                    array_helper::pick_best(
                        ctx,
                        TyWithMeta::new(TyResolvedRef::Class(class_ty), meta),
                        completed,
                    )
                    .map_err(|e| ctx.error_unexpected_type(&target, value).with_cause(e))
                    .map(|v| {
                        v.map_value(|v| match v {
                            BamlValue::Class(cls) => cls,
                            _ => unreachable!("We just wrapped it in a BamlValue::Class"),
                        })
                    })
                }
            }
            (x, _) if class_ty.fields.len() == 1 => {
                // If the class has a single field, then we can try to coerce it directly
                let mut flags = DeserializerConditions::new();
                if x.completion_state() == &CompletionState::Incomplete {
                    flags.add_flag(Flag::Incomplete);
                }
                let field = &class_ty.fields[0];
                let scope = ctx.enter_scope(&format!("<implied:{}>", field.name));
                let field_ty = scope
                    .db
                    .resolve_with_meta(field.ty.as_ref())
                    .map_err(|ident| scope.error_type_resolution(ident))?;
                match TyResolvedRef::coerce(&scope, field_ty, x) {
                    Ok(Some(mut field_value)) => {
                        field_value
                            .meta
                            .flags
                            .add_flag(Flag::ImpliedKey(field.name.clone()));
                        flags.add_flag(Flag::InferedObject(Cow::Borrowed(x)));

                        let mut entries = IndexMap::new();
                        entries.insert(&*field.name, field_value);
                        let cls_value = BamlClass {
                            name: &class_ty.name,
                            value: entries,
                        };
                        let cls_meta =
                            DeserializerMeta::new(target.clone().map_ty(TyResolvedRef::Class));
                        Ok(ValueWithFlags::new(cls_value, cls_meta).with_flags(flags.flags))
                    }
                    Ok(None) => Err(ctx.error_unexpected_type(&target, x)),
                    Err(e) => Err(e),
                }
            }
            _ => Err(ctx.error_unexpected_type(&target, value)),
        };

        match ret {
            Ok(ret) => Ok(Some(ret)),
            Err(e) if matches!(meta.on_error, Literal::Never) => Err(e),
            Err(e) => match target.ty.from_literal(&meta.on_error, ctx) {
                Ok(ret) => {
                    let meta = DeserializerMeta {
                        flags: DeserializerConditions::new()
                            .with_flag(Flag::DefaultButHadUnparseableValue(e)),
                        ty: target.map_ty(TyResolvedRef::Class),
                    };
                    Ok(Some(ValueWithFlags::new(ret, meta)))
                }
                Err(lit_err) => Err(lit_err.with_cause(e)),
            },
        }
    }
}

fn class_from_entries<'s, 'v, 't, N: TypeIdent>(
    ctx: &ParsingContext<'s, 'v, 't, N>,
    target: TyWithMeta<&'t ClassTy<'t, N>, &'t TypeAnnotations<'t, N>>,
    is_incomplete: bool,
    mut entries: HashMap<Cow<'s, str>, Result<BamlValueWithFlags<'s, 'v, 't, N>, ParsingError>>,
    flags: DeserializerConditions<'s, 'v, 't, N>,
) -> Result<ValueWithFlags<'s, 'v, 't, BamlClass<'s, 'v, 't, N>, N>, ParsingError>
where
    't: 's,
    's: 'v,
{
    let mut field_data = IndexMap::new();
    let mut err_unparsed = Vec::new();
    let mut err_missing = Vec::new();
    for field in &target.ty.fields {
        let AnnotatedField {
            name,
            ty,
            before_started,
            missing,
            ..
        } = field;
        let ty = ctx
            .db
            .resolve_with_meta(ty.as_ref())
            .map_err(|ident| ctx.error_type_resolution(ident))?;
        let is_optional = ty.ty.is_optional(ctx.db);
        let field_entry = match entries.remove(name.as_ref()) {
            // Happy path: we have this field
            Some(Ok(some)) => some,
            // Skip optional fields with errors
            Some(Err(e)) if is_optional => {
                let field_value = BamlValue::Null(BamlNull);
                let field_meta = DeserializerMeta::new(ty);
                ValueWithFlags::new(field_value, field_meta)
                    .with_flag(Flag::OptionalFieldError(name.clone(), e))
            }
            // Required field with error
            Some(Err(e)) => {
                err_unparsed.push((name, e));
                continue;
            }
            // Missing entry falls back to `before_started` when object is incomplete
            None if is_incomplete && !matches!(before_started, Literal::Never) => {
                let field_value = ty.ty.from_literal(before_started, ctx)?;
                let field_meta = DeserializerMeta::new(ty);
                ValueWithFlags::new(field_value, field_meta)
            }
            // Missing entry falls back to `missing` when object is complete
            None if !is_incomplete && !matches!(missing, Literal::Never) => {
                let field_value = ty.ty.from_literal(missing, ctx)?;
                let field_meta = DeserializerMeta::new(ty);
                ValueWithFlags::new(field_value, field_meta)
            }
            // Missing optional entries are `null`
            None if is_optional => {
                let field_value = BamlValue::Null(BamlNull);
                let field_meta = DeserializerMeta::new(ty);
                ValueWithFlags::new(field_value, field_meta)
            }
            // Missing required entries are errors
            None => {
                err_missing.push(name.clone());
                continue;
            }
        };
        field_data.insert(&**name, field_entry);
    }
    if !err_unparsed.is_empty() || !err_missing.is_empty() {
        return Err(ctx.error_missing_required_field(err_unparsed, err_missing, None));
    }

    Ok(ValueWithFlags::new(
        BamlClass {
            name: &target.ty.name,
            value: field_data,
        },
        DeserializerMeta {
            flags,
            ty: target.map_ty(TyResolvedRef::Class),
        },
    ))
}
