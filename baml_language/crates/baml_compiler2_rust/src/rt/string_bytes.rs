//! The UTF-8 string boundary and its precise byte-buffer representation.
use super::*;
pub(super) struct Feature;
impl registry::Feature for Feature {
    fn reach(
        &self,
        db: &dyn Db,
        plan: &mut Plan<'_>,
        term: Option<&Terminator<'_>>,
    ) -> Option<Gen<registry::Reach>> {
        let Some(Terminator::Call {
            callee: Operand::Constant(Constant::Function(f)),
            has_trace: false,
            ..
        }) = term
        else {
            return None;
        };
        let (callee, _) = plan.index.function(db, *f)?;
        let name = plan.index.display(callee).to_owned();
        if !baml_builtins2_codegen::supports_rt(&name)
            || !matches!(
                name.as_str(),
                "baml.String.to_utf8" | "baml.String.from_utf8" | "baml.Uint8Array._to_string_impl"
            )
        {
            return None;
        }
        Some((|| {
            if name == "baml.String.from_utf8" {
                let error = super::strings::builtin_class(plan, "baml.errors.InvalidArgument")?;
                plan.add(error)?;
            }
            Ok(registry::Reach {
                builtin: Some(name),
                ..Default::default()
            })
        })())
    }
    fn value(
        &self,
        e: &Emitter<'_, '_>,
        value: &Rvalue<'_>,
        _expected: Repr,
    ) -> Option<Gen<(String, Repr)>> {
        if let Rvalue::Len(place) = value {
            let (value, repr) = e.place(place).ok()?;
            if repr == Repr::Bytes {
                return Some(Ok((format!("*((({value})+8) as *const i64)"), Repr::Int)));
            }
        }
        let Rvalue::Uint8Array(bytes) = value else {
            return None;
        };
        Some(Ok((
            e.fallible(&format!(
                "rt::bytes::new(&mut rt::strings::Cx::current(),&{bytes:?})"
            )),
            Repr::Bytes,
        )))
    }
    fn call(
        &self,
        e: &Emitter<'_, '_>,
        name: &str,
        types: &[Repr],
        values: &[(String, Repr)],
    ) -> Option<Gen<(String, Repr)>> {
        let (adapter, input, output) = match name {
            "baml.String.to_utf8" => ("rt_string_to_utf8", Repr::Str, Repr::Bytes),
            "baml.String.from_utf8" => ("rt_string_from_utf8", Repr::Bytes, Repr::Str),
            "baml.Uint8Array._to_string_impl" => ("rt_bytes_to_string", Repr::Bytes, Repr::Str),
            _ => return None,
        };
        Some((|| {
            if !types.is_empty() || values.len() != 1 || values[0].1 != input {
                return Err(no("UTF-8 arguments"));
            }
            let argument = if input == Repr::Str {
                format!("rt::strings::Str::from_raw({})", values[0].0)
            } else {
                values[0].0.clone()
            };
            let error = if name == "baml.String.from_utf8" {
                format!(
                    ",{}",
                    e.descriptors
                        [&super::strings::builtin_class(e.plan, "baml.errors.InvalidArgument")?]
                )
            } else {
                String::new()
            };
            let conversion = if output == Repr::Str {
                ".map(|s|s.as_raw())"
            } else {
                ""
            };
            Ok((
                e.fallible(&format!(
                    "{adapter}(&mut rt::strings::Cx::current(),{argument}{error}){conversion}"
                )),
                output,
            ))
        })())
    }
}
