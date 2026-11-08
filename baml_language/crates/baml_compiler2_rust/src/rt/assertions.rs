//! The broad equality operation used by the BAML assertion bodies.
use super::*;

pub(super) fn validate(plan: &Plan<'_>, builtins: &BTreeSet<String>) -> Gen<()> {
    let _ = builtins;
    // Structural comparison must never silently bypass a user-defined eq.
    // The general builtin/dispatch job owns those callback adapters.
    let Some(interface) = plan.program.packages.iter().find_map(|p| {
        p.interfaces.iter().find_map(|(path, id)| {
            (path.under(&p.name).to_string() == "baml.ops.Equals").then_some(*id)
        })
    }) else {
        return Ok(());
    };
    for package in &plan.program.packages {
        if let Some(rules) = package.impl_rules.get(&interface) {
            for rule in rules.iter().filter(|r| r.methods.contains_key("eq")) {
                let tag = match &rule.for_ty_pattern {
                    bex_vm_types::TyTemplate::Class(head, _)
                    | bex_vm_types::TyTemplate::Enum(head) => head.tag(),
                    _ => continue,
                };
                if plan.layouts.keys().any(|c|plan.class(*c).type_tag==tag) || plan.enums.iter().any(|e|matches!(&plan.program.objects[bex_vm_types::ObjectIndex::from_raw(*e)],Object::Enum(e) if e.type_tag==tag)) {
                    return Err(no("assertion comparison with custom Equals.eq"));
                }
            }
        }
    }
    Ok(())
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
        let (id, _) = plan.index.function(db, *f)?;
        matches!(
            plan.index.display(id),
            "baml.Float.is_nan" | "baml.Float.abs"
        )
        .then(|| Ok(registry::Reach::default()))
    }
    fn call(
        &self,
        e: &Emitter<'_, '_>,
        name: &str,
        _: &[Repr],
        values: &[(String, Repr)],
    ) -> Option<Gen<(String, Repr)>> {
        if matches!(name, "baml.Float.is_nan" | "baml.Float.abs") {
            return Some(if values.len() == 1 && values[0].1 == Repr::Float {
                let method = if name.ends_with("is_nan") {
                    "is_nan"
                } else {
                    "abs"
                };
                Ok((
                    format!("({}).{method}()", values[0].0),
                    if method == "is_nan" {
                        Repr::Bool
                    } else {
                        Repr::Float
                    },
                ))
            } else {
                Err(no("assertion float helper arguments"))
            });
        }
        if name != "baml.ops.equals_equals" {
            return None;
        }
        Some(compare(e, values, false))
    }
    fn value(
        &self,
        e: &Emitter<'_, '_>,
        value: &Rvalue<'_>,
        _: Repr,
    ) -> Option<Gen<(String, Repr)>> {
        let Rvalue::BinaryOp {
            op: op @ (BinOp::Eq | BinOp::Ne),
            left,
            right,
        } = value
        else {
            return None;
        };
        let left = e.operand(left).ok()?;
        let right = e.operand(right).ok()?;
        if !matches!(left.1, Repr::Tagged | Repr::Dyn(_))
            && !matches!(right.1, Repr::Tagged | Repr::Dyn(_))
        {
            return None;
        }
        Some(compare(e, &[left, right], *op == BinOp::Ne))
    }
}
fn compare(e: &Emitter<'_, '_>, values: &[(String, Repr)], negate: bool) -> Gen<(String, Repr)> {
    if values.len() != 2 {
        return Err(no("equality arity"));
    }
    let a = e.capture_value(values[0].0.clone(), values[0].1)?;
    let b = e.capture_value(values[1].0.clone(), values[1].1)?;
    let code = e.fallible(&format!("rt::assertions::equal({a},{b},CAPTURE_SHAPES)"));
    Ok((if negate { format!("!({code})") } else { code }, Repr::Bool))
}
