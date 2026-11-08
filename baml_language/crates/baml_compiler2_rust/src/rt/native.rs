//! Managed native leaves and typed bigint adapters.
use super::*;
pub(super) struct Feature;
impl registry::Feature for Feature {
    fn call(
        &self,
        e: &Emitter<'_, '_>,
        name: &str,
        types: &[Repr],
        values: &[(String, Repr)],
    ) -> Option<Gen<(String, Repr)>> {
        if name == "baml.ops.equals_equals"
            && values.len() == 2
            && values
                .iter()
                .any(|v| matches!(v.1, Repr::Bigint | Repr::Bytes))
        {
            return Some((|| {
                let l = &values[0];
                let r = &values[1];
                if matches!(l.1, Repr::Int | Repr::Bigint)
                    && matches!(r.1, Repr::Int | Repr::Bigint)
                {
                    let l = e.coerce(l.clone(), Repr::Tagged)?;
                    let r = e.coerce(r.clone(), Repr::Tagged)?;
                    Ok((
                        e.fallible(&format!("rt::native::compare({l},{r},\"==\")")),
                        Repr::Bool,
                    ))
                } else if l.1 == Repr::Bytes && r.1 == Repr::Bytes {
                    Ok((
                        format!(
                            "rt::native::bytes_owned({})==rt::native::bytes_owned({})",
                            l.0, r.0
                        ),
                        Repr::Bool,
                    ))
                } else {
                    Ok(("false".into(), Repr::Bool))
                }
            })());
        }
        if name == "baml._to_string_default" && values.first().is_some_and(|v| v.1 == Repr::Bigint)
        {
            return Some(Ok((e.fallible(&format!("rt::strings::Cx::current().string(&rt::native::bigint_owned({}).to_string()).map(|s|s.as_raw())",values[0].0)), Repr::Str)));
        }
        if !name.starts_with("baml.Bigint.") && !name.starts_with("baml.Uint8Array.") {
            return None;
        }
        Some((|| {
            if !types.is_empty() || !baml_builtins2_codegen::supports_rt(name) {
                return Err(no(name));
            }
            let (declarations, _, _) =
                baml_builtins2_codegen::extract_native_builtins().map_err(no)?;
            let d = declarations
                .iter()
                .find(|d| d.path == name)
                .ok_or_else(|| no(name))?;
            let repr = |t: &baml_builtins2_codegen::BamlType| -> Gen<Repr> {
                use baml_builtins2_codegen::BamlType as T;
                Ok(match t {
                    T::Bigint => Repr::Bigint,
                    T::Uint8Array => Repr::Bytes,
                    T::String => Repr::Str,
                    T::Int => Repr::Int,
                    T::Bool => Repr::Bool,
                    T::Null => Repr::Null,
                    T::Optional(_) => Repr::Tagged,
                    T::List(t) if **t == T::Int => Repr::ListInt,
                    _ => return Err(no("native signature")),
                })
            };
            let mut params = Vec::new();
            if d.receiver
                .as_ref()
                .is_some_and(|r| r.receiver_type != baml_builtins2_codegen::ReceiverType::Static)
            {
                params.push(if name.starts_with("baml.Uint8Array.") {
                    Repr::Bytes
                } else {
                    Repr::Bigint
                });
            }
            params.extend(
                d.params
                    .iter()
                    .map(|p| repr(&p.ty))
                    .collect::<Gen<Vec<_>>>()?,
            );
            if params.len() != values.len() {
                return Err(no("bigint arity"));
            }
            let mut args = values
                .iter()
                .zip(params)
                .map(|(v, t)| {
                    let value = e.coerce(v.clone(), t)?;
                    Ok(if t == Repr::Str {
                        format!("rt::strings::Str::from_raw({value})")
                    } else {
                        value
                    })
                })
                .collect::<Gen<Vec<_>>>()?
                .join(",");
            if name == "baml.Uint8Array.to_array" {
                args.push_str(&format!(",{}u32", e.lists[&Repr::ListInt]));
            }
            Ok((
                e.fallible(&format!("rt_{}({args})", d.fn_name)),
                repr(&d.return_type)?,
            ))
        })())
    }
    fn value(
        &self,
        e: &Emitter<'_, '_>,
        value: &Rvalue<'_>,
        _expected: Repr,
    ) -> Option<Gen<(String, Repr)>> {
        if let Rvalue::Len(place) = value
            && let Ok((v, Repr::Bytes)) = e.place(place)
        {
            return Some(Ok((
                format!("(*(({v}+16) as *const usize)) as i64"),
                Repr::Int,
            )));
        }
        if let Rvalue::Uint8Array(bytes) = value {
            return Some(Ok((
                e.fallible(&format!("rt::native::bytes(vec!{bytes:?})")),
                Repr::Bytes,
            )));
        }
        if let Rvalue::UnaryOp {
            op: UnaryOp::Neg,
            operand,
        } = value
        {
            if let Ok((v, Repr::Bigint)) = e.operand(operand) {
                return Some(Ok((
                    e.fallible(&format!(
                        "rt::native::bigint(-rt::native::bigint_owned({v}))"
                    )),
                    Repr::Bigint,
                )));
            }
        }
        let Rvalue::BinaryOp { op, left, right } = value else {
            return None;
        };
        let l = e.operand(left);
        let r = e.operand(right);
        if !l.as_ref().is_ok_and(|v| v.1 == Repr::Bigint)
            && !r.as_ref().is_ok_and(|v| v.1 == Repr::Bigint)
        {
            return None;
        }
        Some((|| {
            let l = l?;
            let r = r?;
            if !matches!(l.1, Repr::Int | Repr::Bigint) || !matches!(r.1, Repr::Int | Repr::Bigint)
            {
                return Err(no("bigint binary operand"));
            }
            let l = e.coerce(l, Repr::Tagged)?;
            let r = e.coerce(r, Repr::Tagged)?;
            let cmp = matches!(
                op,
                BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge
            );
            Ok((
                e.fallible(&format!(
                    "rt::native::{}({l},{r},{:?})",
                    if cmp { "compare" } else { "binary" },
                    op.to_string()
                )),
                if cmp { Repr::Bool } else { Repr::Bigint },
            ))
        })())
    }
}
