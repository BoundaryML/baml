use super::{Emitter, Gen, Repr, Rvalue, registry};

pub(super) struct Feature;
impl registry::Feature for Feature {
    fn value(
        &self,
        emitter: &Emitter<'_, '_>,
        value: &Rvalue<'_>,
        expected: Repr,
    ) -> Option<Gen<(String, Repr)>> {
        (matches!(value, Rvalue::ConcatStrings(_))).then(|| emitter.strings_value(value, expected))
    }
}
impl Emitter<'_, '_> {
    fn strings_value(&self, r: &Rvalue<'_>, _expected: Repr) -> Gen<(String, Repr)> {
        match r {
            Rvalue::ConcatStrings(parts) => {
                let values = parts
                    .iter()
                    .map(|part| {
                        let value = self.coerce(self.operand(part)?, Repr::Str)?;
                        Ok(format!("rt::strings::Str::from_raw({value})"))
                    })
                    .collect::<Gen<Vec<_>>>()?
                    .join(",");
                Ok((self.fallible(&format!("rt::strings::concat(&mut rt::strings::Cx::current(),&[{values}]).map(|s|s.as_raw())")), Repr::Str))
            }
            _ => unreachable!("feature value guard"),
        }
    }
}
