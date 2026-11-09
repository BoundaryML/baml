//! Closed-world ToString witnesses for nested structural rendering.
use super::*;

pub(super) fn register<'db>(
    db: &'db dyn Db,
    plan: &mut Plan<'_>,
    all: &mut BTreeMap<usize, MirFunction<'db>>,
    instances: &mut Vec<(usize, Vec<RuntimeTy>, usize)>,
    queue: &mut VecDeque<usize>,
) -> Gen<()> {
    if !plan.structural_strings {
        return Ok(());
    }
    let iface = all
        .values()
        .find_map(|mir| {
            let MirFunctionKind::Bytecode(body) = &mir.kind else {
                return None;
            };
            body.blocks
                .iter()
                .find_map(|block| match &block.terminator {
                    Some(Terminator::VirtualCall { iface, method, .. })
                        if method == "to_string"
                            && iface.name.name().as_str() == "ToString"
                            && Some(iface.name.root())
                                == plan.lang.get(baml_base::LangPackage::Baml) =>
                    {
                        Some(iface.clone())
                    }
                    _ => None,
                })
        })
        .ok_or_else(|| no("ToString interface declaration"))?;
    for ty in plan.concrete_types.clone() {
        if matches!(
            ty,
            RuntimeTy::Int
                | RuntimeTy::Float
                | RuntimeTy::Bool
                | RuntimeTy::Null
                | RuntimeTy::String
                | RuntimeTy::Unknown
                | RuntimeTy::TypeAlias(_)
                | RuntimeTy::Interface(..)
                | RuntimeTy::Function { .. }
        ) {
            continue;
        }
        let repr = plan.repr(&ty)?;
        if plan.string_overrides.contains_key(&repr) {
            continue;
        }
        if let Some((callee, frame)) =
            super::dispatch::resolve_string_override(db, plan, &ty, &iface)?
        {
            let target = super::generics::instance(db, plan, all, instances, callee, frame)?;
            plan.string_overrides.insert(repr, target);
            queue.push_back(target);
        }
    }
    Ok(())
}
pub(super) struct Feature;
impl registry::Feature for Feature {
    fn call(
        &self,
        e: &Emitter<'_, '_>,
        name: &str,
        _types: &[Repr],
        values: &[(String, Repr)],
    ) -> Option<Gen<(String, Repr)>> {
        if name != "baml._to_string_default"
            || values.len() != 1
            || matches!(
                values[0].1,
                Repr::Int | Repr::Float | Repr::Bool | Repr::Null | Repr::Str
            )
        {
            return None;
        }
        Some((|| {
            let value = e.capture_value(values[0].0.clone(), values[0].1)?;
            let mut tags = Vec::new();
            let mut invoke = String::from("|value:rt::Dyn|{match value.tag{");
            for (&repr, &callee) in &e.plan.string_overrides {
                let tag = match repr {
                    Repr::Enum(i) => format!("CAPTURE_ENUM_{i}_TAG"),
                    Repr::Naming => "CAPTURE_NAMING_TAG".into(),
                    _ => e.tag(repr)?.to_string(),
                };
                tags.push(tag.clone());
                let raw = match repr {
                    Repr::Bool => "value.payload!=0".into(),
                    Repr::Float => "f64::from_bits(value.payload)".into(),
                    _ => format!("value.payload as {}", repr.rust()),
                };
                write!(invoke,"{tag}=>f{callee}::<OBSERVE>(cx,{}, {raw}).map(|p|rt::strings::Str::from_raw(p)),",e.telemetry_site(callee)).unwrap();
            }
            invoke.push_str("_=>unreachable!(\"checked ToString witness\")}} ");
            let code = format!(
                "rt::format::render(&mut rt::strings::Cx::current(),{value},CAPTURE_SHAPES,&[{}],{invoke}).map(|s|s.as_raw())",
                tags.join(",")
            );
            Ok((e.fallible(&code), Repr::Str))
        })())
    }
}

pub(super) fn is_to_string(
    plan: &Plan<'_>,
    iface: &baml_compiler2_mir::TyTemplateInterface,
    method: &str,
) -> bool {
    method == "to_string"
        && iface.name.name().as_str() == "ToString"
        && iface.name.namespace().is_empty()
        && Some(iface.name.root()) == plan.lang.get(baml_base::LangPackage::Baml)
}
pub(super) fn primitive_override(
    plan: &Plan<'_>,
    concrete: &RuntimeTy,
    iface: &baml_compiler2_mir::TyTemplateInterface,
) -> Gen<Option<(usize, Vec<RuntimeTy>)>> {
    if !is_to_string(plan, iface, "to_string") || plan.repr(concrete)? != Repr::Bytes {
        return Ok(None);
    }
    let class = super::strings::builtin_class(plan, "baml.Uint8Array")?;
    let interface = plan
        .index
        .interface_at(&iface.name)
        .ok_or_else(|| no("ToString interface"))?;
    for package in &plan.program.packages {
        let Some(rules) = package
            .impl_rules
            .get(&bex_vm_types::ObjectIndex::from_raw(interface))
        else {
            continue;
        };
        for rule in rules {
            let mut symbolic = false;
            rule.for_ty_pattern
                .for_each_type_arg_ref(&mut |_| symbolic = true);
            if symbolic {
                continue;
            }
            let concrete = rule.for_ty_pattern.substitute_symbolic(&[]);
            let matched = matches!(concrete, bex_vm_types::RuntimeTy::Uint8Array)
                || matches!(concrete,bex_vm_types::RuntimeTy::Class(ref h,ref args) if args.is_empty() && h.tag()==plan.class(class).type_tag);
            if matched && let Some(method) = rule.methods.get("to_string") {
                let frame = method
                    .frame
                    .iter()
                    .map(|t| {
                        t.substitute_symbolic(&[]).try_map_heads(&mut |h| {
                            plan.heads
                                .get(&h.tag())
                                .cloned()
                                .ok_or_else(|| no("primitive ToString frame"))
                        })
                    })
                    .collect::<Gen<Vec<_>>>()?;
                return Ok(Some((method.fqn.raw(), frame)));
            }
        }
    }
    Ok(None)
}
pub(super) fn structural_fallback(
    plan: &Plan<'_>,
    concrete: &RuntimeTy,
    iface: &baml_compiler2_mir::TyTemplateInterface,
    method: &str,
) -> Gen<Option<(usize, Vec<RuntimeTy>)>> {
    if !is_to_string(plan, iface, method) {
        return Ok(None);
    }
    if let Some(explicit) = primitive_override(plan, concrete, iface)? {
        return Ok(Some(explicit));
    }
    let interface = plan
        .index
        .interface_at(&iface.name)
        .ok_or_else(|| no("ToString interface"))?;
    let Object::Interface(def) =
        &plan.program.objects[bex_vm_types::ObjectIndex::from_raw(interface)]
    else {
        unreachable!()
    };
    let body = def
        .methods
        .iter()
        .find(|m| m.name == "to_string")
        .and_then(|m| m.default)
        .ok_or_else(|| no("structural ToString default body"))?;
    Ok(Some((body.raw(), vec![concrete.clone()])))
}
