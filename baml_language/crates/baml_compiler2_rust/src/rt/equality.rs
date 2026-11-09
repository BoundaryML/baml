//! `==` drivers over erased or structural operands: `baml.ops.equals_equals`
//! and `baml._equals_structural_default` share the interpreter's one walk.
use super::*;

pub(super) const STRUCTURAL_EQUALS: &str = "baml._equals_structural_default";
const EQUALS_EQUALS: &str = "baml.ops.equals_equals";

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
            ..
        }) = term
        else {
            return None;
        };
        let (id, _) = plan.index.function(db, *function)?;
        // Operand representations are only known at emission, which rejects
        // overrides and layouts the shared walk cannot describe.
        (plan.index.builtin_path(id) == STRUCTURAL_EQUALS)
            .then(|| Ok(registry::Reach::default()))
    }

    fn call(
        &self,
        e: &Emitter<'_, '_>,
        name: &str,
        _types: &[Repr],
        values: &[(String, Repr)],
    ) -> Option<Gen<(String, Repr)>> {
        if name != STRUCTURAL_EQUALS && name != EQUALS_EQUALS {
            return None;
        }
        // Null tests and primitive string comparisons keep their direct forms.
        if name == EQUALS_EQUALS
            && values
                .iter()
                .any(|v| matches!(v.1, Repr::Null | Repr::Str))
        {
            return None;
        }
        Some((|| {
            if values.len() != 2 {
                return Err(no("equality arity"));
            }
            if values.iter().all(|v| v.1 == Repr::Null) {
                return Ok(("true".into(), Repr::Bool));
            }
            reject_overrides(e.plan)?;
            let left = e.coerce(values[0].clone(), Repr::Tagged)?;
            let right = e.coerce(values[1].clone(), Repr::Tagged)?;
            Ok((
                e.fallible(&format!(
                    "rt::equality::structural(&rt::strings::Cx::current(),{left},{right},CAPTURE_SHAPES)"
                )),
                Repr::Bool,
            ))
        })())
    }
}

/// The shared walk never calls user code. An explicit `Equals` on a class or
/// enum this program can hold would need pre-order dispatch, so reject it.
fn reject_overrides(plan: &Plan<'_>) -> Gen<()> {
    let Some(interface) = plan.program.objects.iter().position(|object| {
        matches!(object, Object::Interface(def) if def.structural_default.as_ref().is_some_and(
            |d| d.kind == baml_type::StructuralInterface::Equals))
    }) else {
        return Ok(());
    };
    let mut tags = BTreeSet::new();
    for &class in plan.layouts.keys() {
        tags.insert(plan.class(class).type_tag);
    }
    for &enumeration in &plan.enums {
        if let Object::Enum(e) = &plan.program.objects[bex_vm_types::ObjectIndex::from_raw(enumeration)]
        {
            tags.insert(e.type_tag);
        }
    }
    for package in &plan.program.packages {
        let Some(rules) = package
            .impl_rules
            .get(&bex_vm_types::ObjectIndex::from_raw(interface))
        else {
            continue;
        };
        for rule in rules {
            let head = match &rule.for_ty_pattern {
                bex_vm_types::TyTemplate::Class(head, _) => head.tag(),
                bex_vm_types::TyTemplate::Enum(head) => head.tag(),
                _ => continue,
            };
            if tags.contains(&head) {
                return Err(no("structural equality with an explicit Equals implementation"));
            }
        }
    }
    Ok(())
}
