use std::{
    borrow::Cow,
    collections::{HashMap, hash_map},
};

use indexmap::IndexMap;

use super::{ParsingContext, VisitedType};
use crate::{
    baml_value::{BamlClass, BamlValue},
    deserializer::{
        coercer::{
            ParsingError, TypeCoercer, array_helper, match_string::matches_string_to_string,
        },
        deserialize_flags::{DeserializerConditions, Flag},
        types::{BamlValueWithFlags, DeserializerMeta, ValueWithFlags},
    },
    jsonish::{self, CompletionState},
    sap_model::{ClassTy, DefaultValue, TyResolvedRef, TypeIdent},
};

impl<'s, 'v, 't, N: TypeIdent> TypeCoercer<'s, 'v, 't, N> for ClassTy<'t, N>
where
    't: 's,
    's: 'v,
{
    fn try_cast(
        ctx: &ParsingContext<'s, 'v, 't, N>,
        class_ty: &'t Self,
        value: &'v crate::jsonish::Value<'s>,
    ) -> Option<ValueWithFlags<'s, 'v, 't, BamlClass<'s, 'v, 't, N>, N>> {
        let name = &class_ty.name;

        // Only handle object values for class types
        let crate::jsonish::Value::Object(obj, completion_state) = value else {
            return None;
        };

        let flags = match completion_state {
            CompletionState::Incomplete if class_ty.stream_done => return None,
            CompletionState::Incomplete => {
                DeserializerConditions::new().with_flag(Flag::Incomplete)
            }
            CompletionState::Complete => DeserializerConditions::new(),
        };

        let ctx = {
            let visited = VisitedType::Class(name.to_string());

            // If this combination has been visited bail out.
            if ctx.is_visiting(&visited, value, false) {
                return None;
            }

            // Mark this class as visited for the duration of this function
            // call. Further recursion from within this function will see that
            // the class has already been visited and stop recursing. Different
            // calls to this function for other fields pointing to the same
            // recursive class should start from scratch with an empty visited
            // set so they will not fail because this class has already been
            // coerced for a different field.
            &ctx.visit(visited, value, false)
        };

        // add entries as fields
        let mut obj: HashMap<&str, &jsonish::Value<'s>> =
            obj.iter().map(|(k, v)| (k.as_ref(), v)).collect();

        // Iterate fields in definition order for stable output ordering,
        // using alias-aware matching to find the corresponding input key.
        let mut field_data = IndexMap::new();
        for field in &class_ty.fields {
            let ty = ctx.db.resolve(&field.ty).ok()?;

            // Use key_matches for alias-aware lookup (when aliases exist,
            // only aliases match — not the original field name).
            let matched_key = obj.keys().find(|k| field.key_matches(k)).copied();
            let Some(key) = matched_key else {
                return None; // `try_cast` is strict and rejects with missing keys
            };

            let value = obj.remove(key).unwrap();
            if field.stream_done && value.completion_state() == &CompletionState::Incomplete {
                return None; // the field holds its default until the value completes
            }
            let value = TyResolvedRef::try_cast(ctx, ty, value)?;
            field_data.insert(&*field.name, value);
        }
        if !obj.is_empty() {
            return None; // `try_cast` is strict and rejects with extra keys
        }

        Some(ValueWithFlags::new(
            BamlClass {
                name: &class_ty.name,
                value: field_data,
            },
            DeserializerMeta {
                flags,
                ty: TyResolvedRef::Class(class_ty),
            },
        ))
    }

    fn coerce(
        ctx: &ParsingContext<'s, 'v, 't, N>,
        class_ty: &'t Self,
        value: &'v crate::jsonish::Value<'s>,
    ) -> Result<Option<ValueWithFlags<'s, 'v, 't, BamlClass<'s, 'v, 't, N>, N>>, ParsingError> {
        // If value is not None then we'll update the context to store the
        // current class in the visited set and we'll use that to stop recursion
        // when dealing with recursive classes.
        // TODO: is this necessary? we should be recusing over the finite input data, not the potentially infinite type structure

        let visited = VisitedType::Class(class_ty.name.to_string());

        // If this combination has been visited bail out.
        if ctx.is_visiting(&visited, value, true) {
            return Err(ctx.error_circular_reference(&class_ty.name.to_string(), value));
        }

        // Mark this class as visited for the duration of this function
        // call. Further recursion from within this function will see that
        // the class has already been visited and stop recursing. Different
        // calls to this function for other fields pointing to the same
        // recursive class should start from scratch with an empty visited
        // set so they will not fail because this class has already been
        // coerced for a different field.
        let nested_ctx = Some(ctx.visit(visited, value, true));

        // Now just maintain the previous context or get the new one and proceed
        // normally.
        let ctx = nested_ctx.as_ref().unwrap_or(ctx);

        // There are a few possible approaches here:
        match value {
            // `@@stream.done`: nothing to show until the object closes.
            jsonish::Value::Object(_, CompletionState::Incomplete) if class_ty.stream_done => {
                Ok(None)
            }
            jsonish::Value::Object(obj, c) => {
                let is_incomplete = c == &CompletionState::Incomplete;
                let mut flags = DeserializerConditions::new();
                if is_incomplete {
                    flags.add_flag(Flag::Incomplete);
                }
                let mut extra_keys = IndexMap::new();
                let mut entries = HashMap::new();
                for (key, v) in obj {
                    let Some(field) = class_ty.fields.iter().find(|f| {
                        if f.aliases.is_empty() {
                            matches_string_to_string(ctx, key, &f.name)
                        } else {
                            f.aliases
                                .iter()
                                .any(|a| matches_string_to_string(ctx, key, a))
                        }
                    }) else {
                        extra_keys.insert(key.clone(), v);
                        continue;
                    };

                    let entry = if field.stream_done
                        && v.completion_state() == &CompletionState::Incomplete
                    {
                        FieldEntry::Held(v)
                    } else {
                        let scope = ctx.enter_scope(&field.name);
                        let resolved = scope
                            .db
                            .resolve(&field.ty)
                            .map_err(|ident| scope.error_type_resolution(ident));
                        match resolved
                            .and_then(|resolved| TyResolvedRef::coerce(&scope, resolved, v))
                        {
                            Ok(Some(parsed)) => FieldEntry::Parsed(parsed),
                            Ok(None) => FieldEntry::Held(v),
                            Err(e) => FieldEntry::Failed(e),
                        }
                    };

                    match entries.entry(field.name.clone()) {
                        hash_map::Entry::Occupied(_) => {}
                        hash_map::Entry::Vacant(slot) => {
                            slot.insert(entry);
                        }
                    }
                }

                if entries.is_empty()
                    && !extra_keys.is_empty()
                    && let [field] = class_ty.fields.as_slice()
                {
                    // Try to coerce the object into the single field
                    let scope = ctx.enter_scope(&format!("<implied:{}>", field.name));
                    let resolved = scope
                        .db
                        .resolve(&field.ty)
                        .map_err(|ident| scope.error_type_resolution(ident));
                    let parsed = resolved
                        .and_then(|resolved| TyResolvedRef::coerce(&scope, resolved, value))
                        .map(|v| v.map(|v| v.with_flag(Flag::ImpliedKey(field.name.clone()))));

                    if let Ok(Some(parsed_value)) = parsed {
                        entries.insert(field.name.clone(), FieldEntry::Parsed(parsed_value));
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
                class_from_entries(ctx, class_ty, is_incomplete, entries, flags)
            }
            jsonish::Value::Array(items, c) => {
                let mut completed = Vec::new();
                if let [field] = class_ty.fields.as_slice()
                    && let scope = ctx.enter_scope(&format!("<implied:{}>", field.name))
                    && let Ok(Some(mut parsed)) = scope
                        .db
                        .resolve(&field.ty)
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
                    let cls_meta = DeserializerMeta::new(TyResolvedRef::Class(class_ty));
                    completed.push(Ok(ValueWithFlags::new(
                        BamlValue::Class(cls_value),
                        cls_meta,
                    )
                    .with_flags(flags.flags)));
                }

                let singular = array_helper::coerce_array_to_singular(
                    ctx,
                    TyResolvedRef::Class(class_ty),
                    items.iter(),
                    &|value| {
                        Self::coerce(ctx, class_ty, value)
                            .map(|v| v.map(|v| v.map_value(BamlValue::Class)))
                    },
                );
                match singular {
                    Ok(Some(v)) => completed.push(Ok(v)),
                    Ok(None) => {} // every candidate was incomplete without a partial parse
                    Err(e) => completed.push(Err(e)),
                }

                if completed.is_empty() {
                    Err(ctx.error_unexpected_type(class_ty, value))
                } else {
                    array_helper::pick_best(
                        ctx,
                        TyResolvedRef::Class(class_ty),
                        completed.into_iter().map(|r| r.map(Some)).collect(),
                    )
                    .map_err(|e| ctx.error_unexpected_type(class_ty, value).with_cause(e))
                    .map(|v| {
                        v.map(|v| {
                            v.map_value(|v| match v {
                                BamlValue::Class(cls) => cls,
                                _ => unreachable!("We just wrapped it in a BamlValue::Class"),
                            })
                        })
                    })
                }
            }
            x if class_ty.fields.len() == 1 => {
                // If the class has a single field, then we can try to coerce it directly
                let mut flags = DeserializerConditions::new();
                if x.completion_state() == &CompletionState::Incomplete {
                    flags.add_flag(Flag::Incomplete);
                }
                let field = &class_ty.fields[0];
                let scope = ctx.enter_scope(&format!("<implied:{}>", field.name));
                let field_ty = scope
                    .db
                    .resolve(&field.ty)
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
                        let cls_meta = DeserializerMeta::new(TyResolvedRef::Class(class_ty));
                        Ok(Some(
                            ValueWithFlags::new(cls_value, cls_meta).with_flags(flags.flags),
                        ))
                    }
                    Ok(None) => Err(ctx.error_unexpected_type(class_ty, x)),
                    Err(e) => Err(e),
                }
            }
            _ => Err(ctx.error_unexpected_type(class_ty, value)),
        }
    }
}

/// What an object supplied for one field.
enum FieldEntry<'s, 'v, 't, N: TypeIdent>
where
    's: 'v,
{
    Parsed(BamlValueWithFlags<'s, 'v, 't, N>),
    Failed(ParsingError),
    /// The value is incomplete and has no partial parse (its type has none, or the field is
    /// `@stream.done`): the field holds its default until the value completes.
    Held(&'v jsonish::Value<'s>),
}

/// Assembles a class from the fields the object supplied, filling the rest per BEP-075.
///
/// Returns `Ok(None)` when the object is incomplete and a field without a default (never) is
/// pending or held: the class has no partial parse yet.
fn class_from_entries<'s, 'v, 't, N: TypeIdent>(
    ctx: &ParsingContext<'s, 'v, 't, N>,
    class_ty: &'t ClassTy<'t, N>,
    is_incomplete: bool,
    mut entries: HashMap<Cow<'s, str>, FieldEntry<'s, 'v, 't, N>>,
    flags: DeserializerConditions<'s, 'v, 't, N>,
) -> Result<Option<ValueWithFlags<'s, 'v, 't, BamlClass<'s, 'v, 't, N>, N>>, ParsingError>
where
    't: 's,
    's: 'v,
{
    let mut field_data = IndexMap::new();
    let mut err_unparsed = Vec::new();
    let mut err_missing = Vec::new();
    for field in &class_ty.fields {
        let ty = ctx
            .db
            .resolve(&field.ty)
            .map_err(|ident| ctx.error_type_resolution(ident))?;
        let field_entry = match entries.remove(field.name.as_ref()) {
            // Happy path: we have this field
            Some(FieldEntry::Parsed(parsed)) => parsed,
            Some(FieldEntry::Failed(e)) => {
                err_unparsed.push((&field.name, e));
                continue;
            }
            // Incomplete without a partial parse: the default stands in, or the class waits.
            Some(FieldEntry::Held(had)) => {
                debug_assert!(
                    is_incomplete,
                    "a complete object holds only complete values"
                );
                let default = ctx
                    .db
                    .field_default(field)
                    .map_err(|ident| ctx.error_type_resolution(ident))?;
                match &*default {
                    DefaultValue::Never => return Ok(None),
                    default => ValueWithFlags::new(
                        ty.from_literal(default, ctx)?,
                        DeserializerMeta::new(ty),
                    )
                    .with_flag(Flag::DefaultFromInProgress(Cow::Borrowed(had))),
                }
            }
            // Pending: the object has not reached this field yet.
            None if is_incomplete => {
                let default = ctx
                    .db
                    .field_default(field)
                    .map_err(|ident| ctx.error_type_resolution(ident))?;
                match &*default {
                    DefaultValue::Never => return Ok(None),
                    default => ValueWithFlags::new(
                        ty.from_literal(default, ctx)?,
                        DeserializerMeta::new(ty),
                    )
                    .with_flag(Flag::Pending),
                }
            }
            // Missing from a complete object.
            None => match ctx
                .db
                .missing_default(&field.ty)
                .map_err(|ident| ctx.error_type_resolution(ident))?
            {
                DefaultValue::Never => {
                    err_missing.push(field.name.clone());
                    continue;
                }
                default => {
                    let flag = if ty.is_optional(ctx.db) {
                        Flag::OptionalDefaultFromNoValue
                    } else {
                        Flag::DefaultFromNoValue
                    };
                    ValueWithFlags::new(ty.from_literal(&default, ctx)?, DeserializerMeta::new(ty))
                        .with_flag(flag)
                }
            },
        };
        field_data.insert(&*field.name, field_entry);
    }
    if !err_unparsed.is_empty() || !err_missing.is_empty() {
        return Err(ctx.error_missing_required_field(err_unparsed, err_missing, None));
    }

    Ok(Some(ValueWithFlags::new(
        BamlClass {
            name: &class_ty.name,
            value: field_data,
        },
        DeserializerMeta {
            flags,
            ty: TyResolvedRef::Class(class_ty),
        },
    )))
}
