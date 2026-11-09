//! The broad equality operation used by the BAML assertion bodies.
use super::*;

pub(super) fn renderer(plan: &Plan<'_>) -> Gen<usize> {
    plan.program
        .objects
        .iter()
        .enumerate()
        .find_map(|(id, o)| {
            (matches!(o, Object::Function(_))
                && plan.index.display(id) == "baml.<(reflect.Type as baml.ToString)>.to_string")
                .then_some(id)
        })
        .ok_or_else(|| no("assert.is_type renderer metadata"))
}
pub(super) fn emit(out: &mut String, plan: &Plan<'_>) -> Gen<()> {
    if plan.frozen_types.is_empty() {
        return Ok(());
    }
    out.push_str("static ASSERT_FROZEN_TYPES:&[(&str,&rt::telemetry::capture::Type)]=&[");
    for (text, ty) in &plan.frozen_types {
        write!(
            out,
            "({text:?},&{}),",
            super::capture_codegen::capture_type(plan, ty)?
        )
        .unwrap();
    }
    out.push_str("];\n");
    Ok(())
}

fn type_text(plan: &Plan<'_>, ty: &bex_vm_types::RuntimeTy) -> Gen<String> {
    ty.try_map_heads(&mut |head| {
        for p in &plan.program.packages {
            for (path, id) in p
                .classes
                .iter()
                .chain(p.enums.iter())
                .chain(p.interfaces.iter())
                .chain(p.type_aliases.iter())
            {
                let tag = match &plan.program.objects[*id] {
                    Object::Class(c) => c.type_tag,
                    Object::Enum(e) => e.type_tag,
                    Object::Interface(i) => i.type_tag,
                    Object::TypeAlias(a) => a.type_tag,
                    _ => continue,
                };
                if tag == head.tag() {
                    return Ok(path.under(&p.name));
                }
            }
        }
        Err(no("assert.is_type display declaration"))
    })
    .map(|ty| ty.to_string())
}

// assert.is_type only uses its Type.of<T>() value to render the expected type
// in a failure message. Freeze that immutable spelling in the specialization.
pub(super) fn specialize(
    plan: &mut Plan<'_>,
    callee: usize,
    frame: &[RuntimeTy],
    mir: &mut MirFunction<'_>,
) -> Gen<()> {
    if plan.index.display(callee) != "assert.is_type"
        && !(plan.index.display(callee) == "baml.String.from" && frame == [RuntimeTy::Type])
    {
        return Ok(());
    }
    let MirFunctionKind::Bytecode(body) = &mut mir.kind else {
        return Ok(());
    };
    let mut arguments = BTreeSet::new();
    for b in &body.blocks {
        if let Some(
            Terminator::Call {
                args, ntypeargs, ..
            }
            | Terminator::VirtualCall {
                args, ntypeargs, ..
            },
        ) = &b.terminator
        {
            for a in &args[..*ntypeargs] {
                if let Operand::Copy(Place::Local(l)) | Operand::Move(Place::Local(l)) = a {
                    arguments.insert(l.0);
                }
            }
        }
    }
    let mut types = BTreeSet::new();
    for (i, local) in body.locals.iter_mut().enumerate() {
        if arguments.contains(&i) {
            continue;
        }
        if matches!(&local.ty, RuntimeTy::Type)
            || matches!(&local.ty,RuntimeTy::Class(head,args) if args.is_empty() && head.name().as_str()=="Type" && Some(head.root())==plan.lang.get(baml_base::LangPackage::Reflect))
        {
            local.ty = RuntimeTy::String;
            types.insert(i);
        }
    }
    for block in &mut body.blocks {
        for statement in &mut block.statements {
            if let StatementKind::Assign {
                destination: Place::Local(local),
                value: Rvalue::LoadType(ty),
            } = &mut statement.kind
            {
                if types.contains(&local.0) {
                    let anchored = plan
                        .index
                        .anchor(ty)
                        .ok_or_else(|| no("assert.is_type type identity"))?;
                    let text = type_text(plan, &anchored.substitute_symbolic(&[]))?;
                    plan.frozen_types
                        .insert(text.clone(), ty.substitute_symbolic(&[]));
                    statement.kind = StatementKind::Assign {
                        destination: Place::Local(*local),
                        value: Rvalue::Use(Operand::Constant(Constant::String(text))),
                    };
                }
            }
        }
    }
    Ok(())
}

pub(super) fn validate(plan: &Plan<'_>, builtins: &BTreeSet<String>) -> Gen<()> {
    let _ = (plan, builtins);
    Ok(())
}

/// Structural comparison must never silently bypass a user-defined eq. When a
/// laid-out class or enum overrides `Equals.eq`, equality stays with the
/// ordinary dispatch path (or fails closed there).
fn custom_equals(plan: &Plan<'_>) -> Gen<()> {
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
        if name == "baml._to_string_default" && e.plan.frozen_strings.contains(&e.object) {
            return Some((|| {
                if values.len() != 1 || values[0].1 != Repr::Str {
                    return Err(no("frozen Type renderer storage"));
                }
                let renderer = e.plan.type_renderer.expect("frozen Type renderer");
                let site = e.telemetry_site(renderer);
                Ok((e.fallible(&format!("rt::assertions::type_string::<OBSERVE>({}, {site}, &O{renderer}, ASSERT_FROZEN_TYPES)",values[0].0)),Repr::Str))
            })());
        }
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
        if name != "baml.ops.equals_equals" || custom_equals(e.plan).is_err() {
            return None;
        }
        if values.iter().any(|v| matches!(v.1, Repr::Null | Repr::Str)) {
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
        if custom_equals(e.plan).is_err() {
            return None;
        }
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
