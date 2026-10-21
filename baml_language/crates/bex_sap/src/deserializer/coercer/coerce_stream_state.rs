use crate::{
    baml_value::BamlStreamState,
    deserializer::{
        coercer::{ParsingContext, ParsingError, TypeCoercer},
        deserialize_flags::DeserializerConditions,
        types::{DeserializerMeta, ValueWithFlags},
    },
    jsonish::CompletionState,
    sap_model::{StreamStateTy, TyResolvedRef, TypeIdent},
};

impl<'s, 'v, 't, N: TypeIdent> TypeCoercer<'s, 'v, 't, N> for StreamStateTy<'t, N>
where
    't: 's,
    's: 'v,
{
    fn coerce(
        ctx: &ParsingContext<'s, 'v, 't, N>,
        target: &'t Self,
        value: &'v crate::jsonish::Value<'s>,
    ) -> Result<Option<ValueWithFlags<'s, 'v, 't, Self::Value, N>>, ParsingError> {
        let inner_ty = ctx
            .db
            .resolve(&target.value)
            .map_err(|ident| ctx.error_type_resolution(ident))?;
        let Some(inner) = TyResolvedRef::coerce(ctx, inner_ty, value)? else {
            return Ok(None);
        };
        let value = match value.completion_state() {
            CompletionState::Complete => BamlStreamState::Complete(Box::new(inner)),
            CompletionState::Incomplete => BamlStreamState::Incomplete(Box::new(inner)),
        };
        Ok(Some(ValueWithFlags::new(
            value,
            DeserializerMeta {
                flags: DeserializerConditions::default(),
                ty: TyResolvedRef::StreamState(target),
            },
        )))
    }
    fn try_cast(
        ctx: &ParsingContext<'s, 'v, 't, N>,
        target: &'t Self,
        value: &'v crate::jsonish::Value<'s>,
    ) -> Option<ValueWithFlags<'s, 'v, 't, Self::Value, N>> {
        let inner_ty = ctx
            .db
            .resolve(&target.value)
            .map_err(|ident| ctx.error_type_resolution(ident))
            .ok()?;
        let inner = TyResolvedRef::try_cast(ctx, inner_ty, value)?;
        let value = match value.completion_state() {
            CompletionState::Complete => BamlStreamState::Complete(Box::new(inner)),
            CompletionState::Incomplete => BamlStreamState::Incomplete(Box::new(inner)),
        };
        Some(ValueWithFlags::new(
            value,
            DeserializerMeta {
                flags: DeserializerConditions::default(),
                ty: TyResolvedRef::StreamState(target),
            },
        ))
    }
}
