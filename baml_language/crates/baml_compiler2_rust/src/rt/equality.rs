//! `==` drivers over erased or structural operands: `baml.ops.equals_equals`
//! and `baml._equals_structural_default` share the interpreter's one walk.
//! Explicit `Equals.eq` implementations on classes and enums are closed-world
//! witnesses, called where the interpreter yields to them.
use super::*;

pub(super) const STRUCTURAL_EQUALS: &str = "baml._equals_structural_default";
const EQUALS_EQUALS: &str = "baml.ops.equals_equals";

fn equals_interface(plan: &Plan<'_>) -> Option<usize> {
    plan.program.objects.iter().position(|object| {
        matches!(object, Object::Interface(def) if def.structural_default.as_ref().is_some_and(
            |d| d.kind == baml_type::StructuralInterface::Equals))
    })
}

fn declaration_tag(plan: &Plan<'_>, repr: Repr) -> Option<baml_type::typetag::TypeTag> {
    match repr {
        Repr::Ptr(class) => Some(plan.class(class).type_tag),
        Repr::Enum(e) => match &plan.program.objects[bex_vm_types::ObjectIndex::from_raw(e)] {
            Object::Enum(e) => Some(e.type_tag),
            _ => None,
        },
        _ => None,
    }
}

/// Explicit `Equals` rules whose implementing head is `tag`.
fn rules<'p>(
    plan: &'p Plan<'_>,
    interface: usize,
    tag: baml_type::typetag::TypeTag,
) -> impl Iterator<Item = &'p bex_vm_types::types::ProgramImplRule> + 'p {
    plan.program.packages.iter().flat_map(move |package| {
        package
            .impl_rules
            .get(&bex_vm_types::ObjectIndex::from_raw(interface))
            .into_iter()
            .flatten()
            .filter(move |rule| match &rule.for_ty_pattern {
                bex_vm_types::TyTemplate::Class(head, _) | bex_vm_types::TyTemplate::Enum(head) => {
                    head.tag() == tag
                }
                _ => false,
            })
    })
}

/// Register the `eq` override of every concrete class or enum the program can
/// compare. Generic implementations need bindings and remain unsupported.
pub(super) fn register<'db>(
    db: &'db dyn Db,
    plan: &mut Plan<'_>,
    all: &mut BTreeMap<usize, super::TestMir<'db>>,
    instances: &mut Vec<(usize, Vec<RuntimeTy>, usize)>,
    queue: &mut VecDeque<usize>,
) -> Gen<()> {
    if !plan.structural_equals {
        return Ok(());
    }
    let Some(interface) = equals_interface(plan) else {
        return Ok(());
    };
    for ty in plan.concrete_types.clone() {
        if !matches!(ty, RuntimeTy::Class(..) | RuntimeTy::Enum(..)) {
            continue;
        }
        let Ok(repr) = plan.repr(&ty) else {
            continue;
        };
        if plan.equals_overrides.contains_key(&repr) {
            continue;
        }
        let Some(tag) = declaration_tag(plan, repr) else {
            continue;
        };
        let iface = baml_compiler2_mir::TyTemplateInterface {
            name: baml_type::DeclName::of_lang_item(plan.lang, bex_vm::lang_items::BAML_OPS_EQUALS)
                .ok_or_else(|| no("Equals interface declaration"))?,
            generics: Box::new([]),
            associated_types: Box::new([]),
        };
        if rules(plan, interface, tag).all(|rule| !rule.methods.contains_key("eq")) {
            continue;
        }
        let found = super::dispatch::resolve_explicit(db, plan, &ty, &iface, "eq")?;
        if let Some((callee, frame)) = found {
            let target = super::generics::instance(db, plan, all, instances, callee, frame)?;
            plan.equals_overrides.insert(repr, target);
            queue.push_back(target);
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
            callee: Operand::Constant(Constant::Function(function)),
            ..
        }) = term
        else {
            return None;
        };
        let (id, _) = plan.index.function(db, *function)?;
        // Operand representations are only known at emission, which rejects
        // layouts the shared walk cannot describe. Overrides are roots.
        (plan.index.builtin_path(id) == STRUCTURAL_EQUALS).then(|| Ok(registry::Reach::default()))
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
        if name == EQUALS_EQUALS && values.iter().any(|v| matches!(v.1, Repr::Null | Repr::Str)) {
            return None;
        }
        Some((|| {
            if values.len() != 2 {
                return Err(no("equality arity"));
            }
            if values.iter().all(|v| v.1 == Repr::Null) {
                return Ok(("true".into(), Repr::Bool));
            }
            reject_unregistered(e.plan)?;
            let left = e.coerce(values[0].clone(), Repr::Tagged)?;
            let right = e.coerce(values[1].clone(), Repr::Tagged)?;
            if e.plan.equals_overrides.is_empty() {
                return Ok((
                    e.fallible(&format!(
                        "rt::equality::structural(&rt::strings::Cx::current(),{left},{right},CAPTURE_SHAPES)"
                    )),
                    Repr::Bool,
                ));
            }
            let mut tags = Vec::new();
            let mut invoke = String::from("|a:rt::Dyn,b:rt::Dyn|{match a.tag{");
            for (&repr, &callee) in &e.plan.equals_overrides {
                let tag = match repr {
                    Repr::Enum(i) => format!("CAPTURE_ENUM_{i}_TAG"),
                    _ => e.tag(repr)?.to_string(),
                };
                tags.push(tag.clone());
                let rust = repr.rust();
                write!(
                    invoke,
                    "{tag}=>f{callee}::<OBSERVE>(cx,{}, a.payload as {rust}, b.payload as {rust}),",
                    e.telemetry_site(callee)
                )
                .unwrap();
            }
            invoke.push_str("_=>unreachable!(\"checked Equals witness\")}}");
            Ok((
                e.fallible(&format!(
                    "rt::equality::with_overrides(&rt::strings::Cx::current(),{left},{right},CAPTURE_SHAPES,&[{}],{invoke})",
                    tags.join(",")
                )),
                Repr::Bool,
            ))
        })())
    }
}

/// The walk only calls registered witnesses; any other explicit `Equals` on a
/// class or enum this program can hold must not be skipped silently.
fn reject_unregistered(plan: &Plan<'_>) -> Gen<()> {
    let Some(interface) = equals_interface(plan) else {
        return Ok(());
    };
    let reprs = plan
        .layouts
        .keys()
        .map(|&c| Repr::Ptr(c))
        .chain(plan.enums.iter().map(|&e| Repr::Enum(e)));
    for repr in reprs {
        if plan.equals_overrides.contains_key(&repr) {
            continue;
        }
        let Some(tag) = declaration_tag(plan, repr) else {
            continue;
        };
        if rules(plan, interface, tag).any(|rule| rule.methods.contains_key("eq")) {
            return Err(no(
                "structural equality with an unregistered Equals implementation",
            ));
        }
    }
    Ok(())
}
