//! Scalar boundaries reached by the list JSON and n-body workloads.
use super::*;
pub(super) fn handles(name: &str) -> bool {
    matches!(
        name,
        "baml.Float.itrunc"
            | "baml.Float.floor"
            | "baml.Float.sqrt"
            | "baml.String.is_ascii"
            | "baml.String.from_code_points"
    )
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
            ..
        }) = term
        else {
            return None;
        };
        let (callee, _) = plan.index.function(db, *f)?;
        handles(plan.index.display(callee)).then(|| Ok(registry::Reach::default()))
    }
    fn call(
        &self,
        e: &Emitter<'_, '_>,
        name: &str,
        _: &[Repr],
        values: &[(String, Repr)],
    ) -> Option<Gen<(String, Repr)>> {
        if !handles(name) {
            return None;
        }
        Some((|| {
            let [(value, repr)] = values else {
                return Err(no("list scalar builtin arguments"));
            };
            Ok(match (name,*repr) {
                ("baml.Float.floor",Repr::Float)=>(format!("({value}).floor()"),Repr::Float),
                ("baml.Float.sqrt",Repr::Float)=>(format!("({value}).sqrt()"),Repr::Float),
                ("baml.Float.itrunc",Repr::Float)=>(e.fallible(&format!("rt::list_scalars::itrunc(&mut rt::strings::Cx::current(),{value})")),Repr::Int),
                ("baml.String.is_ascii",Repr::Str)=>(format!("rt::strings::Str::from_raw({value}).as_str(&rt::strings::Cx::current()).is_ascii()"),Repr::Bool),
                ("baml.String.from_code_points",Repr::ListInt)=>(e.fallible(&format!("rt::list_scalars::from_code_points(&mut rt::strings::Cx::current(),rt::lists::List::<i64>::from_raw({value})).map(|s|s.as_raw())")),Repr::Str),
                _=>return Err(no("list scalar builtin representation")),
            })
        })())
    }
}
