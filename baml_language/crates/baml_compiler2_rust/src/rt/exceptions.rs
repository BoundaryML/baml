//! Language exceptions share layouts and tags with runtime-produced failures.
use super::*;

pub(super) fn dividend<N: Clone + PartialEq>(ty: &baml_type::RuntimeTy<N>) -> bool {
    use baml_type::RuntimeTy as T;
    matches!(ty,T::Union(m) if m.len()==2 && m.contains(&T::Int) && m.contains(&T::Bigint))
}

pub(super) fn record_test(
    db: &dyn Db,
    plan: &mut Plan<'_>,
    test: &baml_compiler2_mir::TypeTest<'_>,
) -> Gen<()> {
    let ty = test.template(db).substitute_symbolic(&[]);
    if matches!(ty, RuntimeTy::Class(..)) {
        plan.record_ty(&ty)?;
    }
    Ok(())
}

pub(super) fn reserved(plan: &Plan<'_>, object: usize) -> Option<usize> {
    let class = plan.class(object);
    if !plan
        .program
        .packages
        .iter()
        .any(|p| p.name == "baml" && p.objects.contains(&(object as u32)))
    {
        return None;
    }
    Some(match class.path.to_string().as_str() {
        "panics.IntegerOverflow" => 1,
        "panics.NegativeBitShift" => 2,
        "panics.Cancelled" => 3,
        "panics.StackOverflow" => 4,
        "panics.AllocFailure" => 5,
        "panics.IndexOutOfBounds" => 6,
        "json.ParseError" => 7,
        "json.DecodeError" => 8,
        "panics.DivisionByZero" => 9,
        _ => return None,
    })
}

pub(super) struct Feature;
impl registry::Feature for Feature {
    fn terminator(&self, e: &mut Emitter<'_, '_>, term: &Terminator<'_>) -> Option<Gen<String>> {
        let Terminator::Throw { value } = term else {
            return None;
        };
        Some((|| {
            let (value, repr) = e.operand(value)?;
            let Repr::Ptr(class) = repr else {
                return Err(no("non-class throw"));
            };
            let name = class_name(e.plan, class)?;
            let value = e.coerce((value, repr), Repr::Tagged)?;
            let error = format!(
                "rt::exceptions::thrown({value},{name:?},CAPTURE_SHAPES).at(&S{},{})",
                e.object, e.site
            );
            Ok(e.raise(&error))
        })())
    }
}

fn class_name(plan: &Plan<'_>, object: usize) -> Gen<String> {
    let origin = plan.class_origin(object);
    let package = plan
        .program
        .packages
        .iter()
        .find(|p| p.objects.contains(&(origin as u32)))
        .ok_or_else(|| no("throw class package"))?;
    Ok(format!("{}.{}", package.name, plan.class(object).path))
}
