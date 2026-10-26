use crate::{
    baml_value::BamlShaped,
    deserializer::{
        coercer::{ParsingContext, ParsingError, TypeCoercer, shape_key},
        deserialize_flags::DeserializerConditions,
        types::{BamlValueWithFlags, DeserializerMeta, ValueWithFlags},
    },
    sap_model::{ShapedTy, TyResolvedRef, TypeIdent},
};

/// A shaped type reads its shape, wrappers included: a model's answer for
/// `string`-shaped `Instant` may be bare text, quoted, or fenced, as for a
/// `string`. The value read is kept for the host to build from, unless the
/// host refused it.
impl<'s, 'v, 't, N: TypeIdent> TypeCoercer<'s, 'v, 't, N> for ShapedTy<'t, N>
where
    't: 's,
    's: 'v,
{
    fn coerce(
        ctx: &ParsingContext<'s, 'v, 't, N>,
        target: &'t Self,
        value: &'v crate::jsonish::Value<'s>,
    ) -> Result<Option<ValueWithFlags<'s, 'v, 't, Self::Value, N>>, ParsingError> {
        let shape = ctx
            .db
            .resolve(&target.shape)
            .map_err(|ident| ctx.error_type_resolution(ident))?;
        let Some(read) = TyResolvedRef::coerce(ctx, shape, value)? else {
            return Ok(None);
        };
        match shaped(ctx, target, read) {
            Ok(read) => Ok(Some(read)),
            // A whole answer reads as a `string` shape verbatim, fence and
            // prose included. When the host refuses that, the other readings
            // the text stage found (the JSON inside a fence, or after some
            // prose) are tried in turn.
            Err(refused) => {
                if let crate::jsonish::Value::AnyOf(candidates, _) = value {
                    for candidate in candidates {
                        if let Ok(Some(read)) = TyResolvedRef::coerce(ctx, shape, candidate)
                            && let Ok(read) = shaped(ctx, target, read)
                        {
                            return Ok(Some(read));
                        }
                    }
                }
                Err(refused)
            }
        }
    }

    fn try_cast(
        ctx: &ParsingContext<'s, 'v, 't, N>,
        target: &'t Self,
        value: &'v crate::jsonish::Value<'s>,
    ) -> Option<ValueWithFlags<'s, 'v, 't, Self::Value, N>> {
        let shape = ctx.db.resolve(&target.shape).ok()?;
        let read = TyResolvedRef::try_cast(ctx, shape, value)?;
        shaped(ctx, target, read).ok()
    }
}

/// `read`, the shape value, as a value of `target`, or the host's refusal.
fn shaped<'s, 'v, 't, N: TypeIdent>(
    ctx: &ParsingContext<'s, 'v, 't, N>,
    target: &'t ShapedTy<'t, N>,
    read: BamlValueWithFlags<'s, 'v, 't, N>,
) -> Result<ValueWithFlags<'s, 'v, 't, BamlShaped<'s, 'v, 't, N>, N>, ParsingError>
where
    't: 's,
    's: 'v,
{
    if let Some(message) = ctx.rejection(&target.name, &shape_key(&read.value)) {
        return Err(ctx.error_rejected_shape(&target.name, message));
    }
    Ok(ValueWithFlags::new(
        BamlShaped {
            name: &target.name,
            value: Box::new(read),
        },
        DeserializerMeta {
            flags: DeserializerConditions::default(),
            ty: TyResolvedRef::Shaped(target),
        },
    ))
}
