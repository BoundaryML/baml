//! Scalar operations reached by typed JSON validation.
use super::*;
fn handles(name: &str) -> bool {
    matches!(name, "baml.Float.floor" | "baml.Float.itrunc")
}
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
        let (id, _) = plan.index.function(db, *f)?;
        handles(plan.index.display(id)).then(|| Ok(registry::Reach::default()))
    }
    fn call(
        &self,
        e: &Emitter<'_, '_>,
        name: &str,
        types: &[Repr],
        values: &[(String, Repr)],
    ) -> Option<Gen<(String, Repr)>> {
        handles(name).then(|| {
            if !types.is_empty() || values.len() != 1 {
                return Err(no("JSON scalar arity"));
            }
            let (value, repr) = &values[0];
            match (name, *repr) {
                ("baml.Float.floor", Repr::Float) => {
                    Ok((format!("({value}).floor()"), Repr::Float))
                }
                ("baml.Float.itrunc", Repr::Float) => Ok((
                    e.fallible(&format!(
                        "rt::json_scalars::itrunc(&mut rt::strings::Cx::current(),{value})"
                    )),
                    Repr::Int,
                )),
                _ => Err(no("JSON scalar receiver")),
            }
        })
    }
}
