use super::*;
pub(super) struct Feature;
impl registry::Feature for Feature {
    fn reach(
        &self,
        db: &dyn Db,
        plan: &mut Plan<'_>,
        term: Option<&Terminator<'_>>,
    ) -> Option<Gen<registry::Reach>> {
        let Some(Terminator::SysOp {
            callee: Operand::Constant(Constant::Function(f)),
            ..
        }) = term
        else {
            return None;
        };
        Some((|| {
            let (callee, _) = plan
                .index
                .function(db, *f)
                .ok_or_else(|| no("unlinked sysop"))?;
            let name = plan.index.builtin_path(callee);
            if !baml_builtins2_codegen::supports_rt(name) {
                return Err(no(format!("sysop {name}")));
            }
            Ok(registry::Reach {
                builtin: Some(name.into()),
                ..Default::default()
            })
        })())
    }
    fn call(
        &self,
        e: &Emitter<'_, '_>,
        name: &str,
        types: &[Repr],
        values: &[(String, Repr)],
    ) -> Option<Gen<(String, Repr)>> {
        if !baml_builtins2_codegen::supports_rt_io(name) {
            return None;
        }
        Some((|| {
            if !types.is_empty() {
                return Err(no("native IO type arguments"));
            }
            let mut args = values
                .iter()
                .map(|v| {
                    e.coerce(v.clone(), Repr::Str)
                        .map(|v| format!("rt::strings::Str::from_raw({v})"))
                })
                .collect::<Gen<Vec<_>>>()?;
            args.push(
                if matches!(e.all[&e.object].kind, MirFunctionKind::Builtin(_)) {
                    "Some(_site)"
                } else {
                    "None"
                }
                .into(),
            );
            Ok((
                e.fallible(&format!(
                    "rt_io_{}({})",
                    name.replace('.', "_").to_lowercase(),
                    args.join(",")
                )),
                Repr::Tagged,
            ))
        })())
    }
    fn terminator(&self, e: &mut Emitter<'_, '_>, term: &Terminator<'_>) -> Option<Gen<String>> {
        let Terminator::SysOp {
            callee: Operand::Constant(Constant::Function(f)),
            args,
            destination,
            ..
        } = term
        else {
            return None;
        };
        Some((|| {
            let (id, _) = e
                .plan
                .index
                .function(e.db, *f)
                .ok_or_else(|| no("sysop target"))?;
            let name = e.plan.index.builtin_path(id);
            let mut values = args
                .iter()
                .map(|v| {
                    let value = e.coerce(e.operand(v)?, Repr::Str)?;
                    Ok(format!("rt::strings::Str::from_raw({value})"))
                })
                .collect::<Gen<Vec<_>>>()?;
            values.push("None".into());
            let values = values.join(",");
            let function = name.replace('.', "_").to_lowercase();
            let value = e.fallible(&format!("rt_io_{function}({values})"));
            e.assign(destination, (value, Repr::Tagged))
        })())
    }
}
