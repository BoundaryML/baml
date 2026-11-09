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
            let name = plan.index.display(callee);
            if !baml_builtins2_codegen::supports_rt(name) {
                return Err(no(format!("sysop {name}")));
            }
            Ok(registry::Reach {
                builtin: Some(name.into()),
                ..Default::default()
            })
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
            let name = e.plan.index.display(id);
            let values = args
                .iter()
                .map(|v| {
                    let value = e.coerce(e.operand(v)?, Repr::Str)?;
                    Ok(format!("rt::strings::Str::from_raw({value})"))
                })
                .collect::<Gen<Vec<_>>>()?
                .join(",");
            let function = name.replace('.', "_").to_lowercase();
            let value = e.fallible(&format!("rt_io_{function}({values})"));
            e.assign(destination, (value, Repr::Tagged))
        })())
    }
}
