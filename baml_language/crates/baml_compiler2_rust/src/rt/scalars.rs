//! Typed scalar calls and raw float operations. No dynamic interpreter calls.
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
            callee: Operand::Constant(Constant::Function(function)),
            has_trace: false,
            ..
        }) = term else { return None; };
        let (id, _) = plan.index.function(db, *function)?;
        let name = plan.index.builtin_path(id);
        // Parse admission also prepares its language error layout.
        if !matches!(name.rsplit_once('.'), Some(("baml.Int" | "baml.Float", method)) if method != "parse")
            || !baml_builtins2_codegen::supports_rt(name)
        {
            return None;
        }
        Some(Ok(registry::Reach { builtin: Some(name.to_owned()), ..Default::default() }))
    }

    fn call(
        &self,
        e: &Emitter<'_, '_>,
        name: &str,
        types: &[Repr],
        values: &[(String, Repr)],
    ) -> Option<Gen<(String, Repr)>> {
        if name == "baml._to_string_primitive" {
            return Some((|| {
                if !types.is_empty() || values.len() != 1 {
                    return Err(no("primitive conversion arguments"));
                }
                let v = e.coerce(values[0].clone(), Repr::Tagged)?;
                Ok((
                    e.fallible(&format!(
                        "rt::scalars::primitive_string({v}).map(|s|s.as_raw())"
                    )),
                    Repr::Str,
                ))
            })());
        }
        let (class, method) = name.rsplit_once('.')?;
        if !matches!(class, "baml.Int" | "baml.Float") {
            return None;
        }
        // Source-defined scalar helpers compile as ordinary BAML bodies.
        if !baml_builtins2_codegen::supports_rt(name) {
            return None;
        }
        Some((|| {
            if !types.is_empty() {
                return Err(no(format!("builtin {name}")));
            }
            let (declarations, _, _) =
                baml_builtins2_codegen::extract_native_builtins().map_err(no)?;
            let d = declarations
                .iter()
                .find(|d| d.path == name)
                .ok_or_else(|| no(format!("builtin {name}")))?;
            let repr = |t: &baml_builtins2_codegen::BamlType| match t {
                baml_builtins2_codegen::BamlType::Int => Ok(Repr::Int),
                baml_builtins2_codegen::BamlType::Float => Ok(Repr::Float),
                baml_builtins2_codegen::BamlType::Bool => Ok(Repr::Bool),
                baml_builtins2_codegen::BamlType::String => Ok(Repr::Str),
                _ => Err(no("scalar signature")),
            };
            let mut expected = Vec::new();
            if d.receiver
                .as_ref()
                .is_some_and(|r| r.receiver_type != baml_builtins2_codegen::ReceiverType::Static)
            {
                expected.push(if class == "baml.Float" {
                    Repr::Float
                } else {
                    Repr::Int
                });
            }
            expected.extend(
                d.params
                    .iter()
                    .map(|p| repr(&p.ty))
                    .collect::<Gen<Vec<_>>>()?,
            );
            if expected.len() != values.len() {
                return Err(no(format!("{method} arity")));
            }
            let args = values
                .iter()
                .zip(expected)
                .map(|(v, t)| {
                    let s = e.coerce(v.clone(), t)?;
                    Ok(if t == Repr::Str {
                        format!("rt::strings::Str::from_raw({s})")
                    } else {
                        s
                    })
                })
                .collect::<Gen<Vec<_>>>()?
                .join(",");
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
        match value {
            Rvalue::BinaryOp { op, left, right } => {
                let l = e.operand(left);
                let r = e.operand(right);
                if !l.as_ref().is_ok_and(|v| v.1 == Repr::Float)
                    && !r.as_ref().is_ok_and(|v| v.1 == Repr::Float)
                {
                    return None;
                }
                Some((|| {
                    let l = e.coerce(l?, Repr::Float)?;
                    let r = e.coerce(r?, Repr::Float)?;
                    let cmp = match op {
                        BinOp::Eq => Some("Eq"),
                        BinOp::Ne => Some("NotEq"),
                        BinOp::Lt => Some("Lt"),
                        BinOp::Le => Some("LtEq"),
                        BinOp::Gt => Some("Gt"),
                        BinOp::Ge => Some("GtEq"),
                        _ => None,
                    };
                    if let Some(op) = cmp {
                        Ok((
                            format!("rt::scalars::compare(rt::scalars::CmpOp::{op},{l},{r})"),
                            Repr::Bool,
                        ))
                    } else if matches!(
                        op,
                        BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Mod
                    ) {
                        Ok((format!("({l} {op} {r})"), Repr::Float))
                    } else {
                        Err(no("float binary operation"))
                    }
                })())
            }
            Rvalue::UnaryOp {
                op: UnaryOp::Neg,
                operand,
            } => {
                let v = e.operand(operand);
                match v.as_ref().map(|v| v.1) {
                    Ok(Repr::Float) => Some(v.map(|v| (format!("-({})", v.0), Repr::Float))),
                    Ok(Repr::Int) => {
                        Some(v.map(|v| {
                            (e.fallible(&format!("rt::scalars::neg({})", v.0)), Repr::Int)
                        }))
                    }
                    _ => None,
                }
            }
            _ => None,
        }
    }
}
