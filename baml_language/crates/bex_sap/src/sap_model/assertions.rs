use crate::{
    baml_value::BamlValue,
    deserializer::coercer::{ParsingContext, ParsingError},
    sap_model::TypeIdent,
};

/// TODO
#[derive(Clone)]
pub struct Assertion<'t, N: TypeIdent> {
    _marker: std::marker::PhantomData<&'t N>,
}
impl<'t, N: TypeIdent> Assertion<'t, N> {
    /// Runs the assertion and returns `Ok(true)` if the assertion passes.
    /// Returns `Ok(false)` if the assertion fails.
    ///
    /// ## Errors
    /// Only returns `Err` if the assertion callback could not be evaluated.
    /// Assertion failures will be returned as `Ok(false)`.
    pub fn evaluate(
        &self,
        _value: &BamlValue<'_, '_, 't, N>,
        _ctx: &ParsingContext<'_, '_, 't, N>,
    ) -> Result<bool, ParsingError> {
        todo!()
    }
}
