//! Static reflected type tables and the managed carrier used at data boundaries.
use super::*;

const KINDS: &[(&str, &str)] = &[
    ("Class", "class"),
    ("Enum", "enum"),
    ("Union", "union"),
    ("Literal", "literal"),
    ("Array", "array"),
    ("Map", "map"),
    ("Interface", "interface"),
    ("Primitive", "primitive"),
    ("Function", "function"),
];

fn as_kind(name: &str) -> Option<&'static str> {
    let suffix = name.strip_prefix("reflect.Type.as_")?;
    KINDS
        .iter()
        .find_map(|(kind, method)| (*method == suffix).then_some(*kind))
}
fn view_kind(name: &str) -> Option<&str> {
    name.strip_prefix("reflect.reflect.TypeView$for$")?
        .strip_suffix(".as_type")
}
use super::equality::STRUCTURAL_EQUALS;

pub(super) fn native(name: &str) -> bool {
    baml_builtins2_codegen::supports_rt_reflection(name)
}

fn kind_class(plan: &Plan<'_>, kind: &str) -> Gen<usize> {
    plan.program
        .packages
        .iter()
        .filter(|p| p.name == "reflect")
        .flat_map(|p| &p.classes)
        .find_map(|(path, id)| (path.to_string() == kind).then_some(id.raw()))
        .ok_or_else(|| no(format!("reflection kind {kind}")))
}

pub(super) fn prepare_native(plan: &mut Plan<'_>, name: &str) -> Gen<()> {
    plan.reflected = true;
    let kinds: Vec<_> = if name == "reflect.Type.kind" {
        KINDS.iter().map(|(kind, _)| *kind).collect()
    } else if let Some(kind) = as_kind(name).or_else(|| view_kind(name)) {
        vec![kind]
    } else if name != "reflect.Type._to_string_impl" {
        vec![name.split('.').nth(1).unwrap()]
    } else {
        Vec::new()
    };
    for kind in kinds {
        plan.add(kind_class(plan, kind)?)?;
    }
    Ok(())
}

fn reflected(repr: Repr) -> bool {
    matches!(repr, Repr::Reflected | Repr::Type(_))
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
        native(plan.index.builtin_path(id)).then(|| {
            let name = plan.index.builtin_path(id).to_owned();
            prepare_native(plan, &name)?;
            Ok(registry::Reach::default())
        })
    }

    fn call(
        &self,
        e: &Emitter<'_, '_>,
        name: &str,
        types: &[Repr],
        values: &[(String, Repr)],
    ) -> Option<Gen<(String, Repr)>> {
        if matches!(name, "baml.ops.equals_equals" | STRUCTURAL_EQUALS)
            && values.iter().any(|value| reflected(value.1))
        {
            return Some(equal(e, values));
        }
        if !native(name) {
            return None;
        }
        Some((|| {
            if !types.is_empty() || values.len() != 1 {
                return Err(no("reflected type display arguments"));
            }
            if let Some(kind) = view_kind(name) {
                return Ok((unview(e, values[0].clone(), kind)?, Repr::Reflected));
            }
            if matches!(
                name,
                "reflect.Array.element_type"
                    | "reflect.Map.key_type"
                    | "reflect.Map.value_type"
                    | "reflect.Function.return_type"
            ) {
                let kind = name.split('.').nth(1).unwrap();
                let value = unview(e, values[0].clone(), kind)?;
                let child = usize::from(name == "reflect.Map.value_type");
                return Ok((
                    e.fallible(&format!(
                        "rt::reflection::child(REFLECT_TYPES,{}, {value}, {child})",
                        e.plan.reflection_descriptor().unwrap()
                    )),
                    Repr::Reflected,
                ));
            }
            let value = e.coerce(values[0].clone(), Repr::Reflected)?;
            if name == "reflect.Type.kind" {
                let mut code =
                    format!("{{let ty={value};match rt::reflection::kind(REFLECT_TYPES,ty){{");
                for (kind, _) in KINDS {
                    let class = kind_class(e.plan, kind)?;
                    let view = view(e, class, "ty")?;
                    let tagged = e.coerce((view, Repr::Ptr(class)), Repr::Tagged)?;
                    write!(code, "rt::reflection::TypeKind::{kind}=>{tagged},").unwrap();
                }
                code.push_str("}}");
                return Ok((code, Repr::Tagged));
            }
            if let Some(kind) = as_kind(name) {
                let class = kind_class(e.plan, kind)?;
                let view = view(e, class, "ty")?;
                return Ok((
                    format!(
                        "{{let ty={value};if rt::reflection::kind(REFLECT_TYPES,ty)==rt::reflection::TypeKind::{kind}{{{view}}}else{{0usize}}}}"
                    ),
                    Repr::Ptr(class),
                ));
            }
            Ok((
                e.fallible(&format!(
                    "rt::reflection::text(REFLECT_TYPES,{value}).map(|s|s.as_raw())"
                )),
                Repr::Str,
            ))
        })())
    }

    fn value(
        &self,
        e: &Emitter<'_, '_>,
        value: &Rvalue<'_>,
        _expected: Repr,
    ) -> Option<Gen<(String, Repr)>> {
        if !e.plan.reflected {
            return None;
        }
        if let Rvalue::LoadType(ty) = value {
            return Some((|| {
                let runtime = ty.substitute_symbolic(&[]);
                let index = e
                    .plan
                    .reflection_types
                    .iter()
                    .position(|t| *t == runtime)
                    .ok_or_else(|| no("unregistered reflected type"))?;
                Ok((
                    e.fallible(&format!(
                        "rt::reflection::new({}, {index})",
                        e.plan.reflection_descriptor().unwrap()
                    )),
                    Repr::Type(e.plan.type_code(ty)?),
                ))
            })());
        }
        if let Rvalue::BinaryOp {
            op: BinOp::Eq | BinOp::Ne,
            left,
            right,
        } = value
        {
            if matches!(left, Operand::Constant(Constant::OmittedArg))
                || matches!(right, Operand::Constant(Constant::OmittedArg))
            {
                return None;
            }
            let left = e.operand(left);
            let right = e.operand(right);
            if left.as_ref().is_ok_and(|v| reflected(v.1))
                || right.as_ref().is_ok_and(|v| reflected(v.1))
            {
                return Some((|| {
                    let (result, repr) = equal(e, &[left?, right?])?;
                    let result = if matches!(value, Rvalue::BinaryOp { op: BinOp::Ne, .. }) {
                        format!("!({result})")
                    } else {
                        result
                    };
                    Ok((result, repr))
                })());
            }
        }
        None
    }
}

fn view(e: &Emitter<'_, '_>, class: usize, ty: &str) -> Gen<String> {
    let layout = &e.plan.layouts[&class];
    if layout.fields != [Repr::Reflected] {
        return Err(no("reflection kind layout"));
    }
    let allocation = e.fallible(&format!(
        "rt::try_alloc({}, {})",
        e.descriptors[&class], layout.bytes
    ));
    Ok(format!(
        "{{let ty={ty};let p={allocation} as *mut C{class};baml_rt_gc::write_ptr(p.cast(),(&raw mut (*p).f0).cast(),ty);p as usize}}"
    ))
}
fn unview(e: &Emitter<'_, '_>, value: (String, Repr), kind: &str) -> Gen<String> {
    let class = kind_class(e.plan, kind)?;
    let value = e.coerce(value, Repr::Ptr(class))?;
    let checked = e.fallible(&format!("rt::nonnull({value})"));
    Ok(format!("(*({checked} as *const C{class})).f0"))
}
fn children(ty: &RuntimeTy) -> Vec<RuntimeTy> {
    match ty {
        RuntimeTy::List(element) => vec![*element.clone()],
        RuntimeTy::Map { key, value } => vec![*key.clone(), *value.clone()],
        RuntimeTy::Union(members) => members.to_vec(),
        RuntimeTy::Function { ret, .. } => vec![*ret.clone()],
        _ => Vec::new(),
    }
}

fn equal(e: &Emitter<'_, '_>, values: &[(String, Repr)]) -> Gen<(String, Repr)> {
    if values.len() != 2 {
        return Err(no("reflected type equality arguments"));
    }
    let left = e.coerce(values[0].clone(), Repr::Tagged)?;
    let right = e.coerce(values[1].clone(), Repr::Tagged)?;
    let tag = e
        .plan
        .reflection_descriptor()
        .ok_or_else(|| no("reflected type descriptor"))?
        + 3;
    Ok((
        format!("rt::reflection::equal(REFLECT_TYPES,{tag},{left},{right})"),
        Repr::Bool,
    ))
}

/// Generic-only type tokens stay compile-time values. Ordinary parameters,
/// fields, named locals, comparisons and logs require a managed carrier.
pub(super) fn collect(
    plan: &mut Plan<'_>,
    all: &BTreeMap<usize, MirFunction<'_>>,
    reached: &BTreeSet<usize>,
) -> Gen<()> {
    for object in reached {
        let MirFunctionKind::Bytecode(body) = &all[object].kind else {
            continue;
        };
        let type_operand = |operand: &Operand<'_>| match operand {
            Operand::Copy(Place::Local(local)) | Operand::Move(Place::Local(local)) => {
                matches!(body.locals[local.0].ty, RuntimeTy::Type)
            }
            _ => false,
        };
        for block in &body.blocks {
            if let Some(Terminator::Call {
                args, ntypeargs, ..
            }) = &block.terminator
            {
                plan.reflected |= args[*ntypeargs..].iter().any(type_operand);
            }
            for statement in &block.statements {
                match &statement.kind {
                    StatementKind::Assign {
                        value: Rvalue::BinaryOp { left, right, .. },
                        ..
                    } => plan.reflected |= type_operand(left) || type_operand(right),
                    StatementKind::Intrinsic { args, .. } => {
                        plan.reflected |= args.iter().any(type_operand)
                    }
                    _ => {}
                }
            }
        }
    }
    if !plan.reflected {
        return Ok(());
    }
    for object in reached {
        let MirFunctionKind::Bytecode(body) = &all[object].kind else {
            continue;
        };
        for statement in body.blocks.iter().flat_map(|block| &block.statements) {
            if let StatementKind::Assign {
                value: Rvalue::LoadType(ty),
                ..
            } = &statement.kind
            {
                let ty = ty.substitute_symbolic(&[]);
                baml_compiler2_mir::RealizedTy::try_from(ty.clone())
                    .map_err(|_| no("unrealized reflected type"))?;
                if !plan.reflection_types.contains(&ty) {
                    plan.reflection_types.push(ty);
                }
            }
        }
    }
    let mut cursor = 0;
    while cursor < plan.reflection_types.len() {
        for child in children(&plan.reflection_types[cursor]) {
            if !plan.reflection_types.contains(&child) {
                plan.reflection_types.push(child);
            }
        }
        cursor += 1;
    }
    Ok(())
}

pub(super) fn emit(out: &mut String, db: &dyn Db, plan: &Plan<'_>) -> Gen<()> {
    if !plan.reflected {
        return Ok(());
    }
    let facts = baml_compiler2_hir_ty::facts::Facts::new(db);
    let mut identities = Vec::<baml_type::Ty<baml_type::DeclName>>::new();
    let mut anchored_types = Vec::new();
    for ty in &plan.reflection_types {
        let realized = baml_compiler2_mir::RealizedTy::try_from(ty.clone())
            .map_err(|_| no("unrealized reflected type"))?;
        let anchored = plan
            .index
            .anchor(&baml_compiler2_mir::TyTemplate::from(realized))
            .ok_or_else(|| no("reflected type identity"))?;
        anchored_types.push(
            bex_vm_types::RealizedTy::try_from(anchored.substitute_symbolic(&[]))
                .map_err(|_| no("unrealized reflected type"))?,
        );
    }
    // Spelling comes from the interpreter's own renderer over the linked
    // declarations, so displayed names cannot drift from `Type.to_string`.
    let displays = bex_vm::telemetry::frozen::type_displays(plan.program, &anchored_types)
        .map_err(no)?;
    out.push_str("static REFLECT_TYPES:&[rt::reflection::StaticType]=&[\n");
    for (ty, display) in plan.reflection_types.iter().zip(&displays) {
        let comparable = baml_type::Ty::from(ty.clone());
        let identity = identities
            .iter()
            .position(|old| baml_type::normalize::equivalent(old, &comparable, &facts))
            .unwrap_or(identities.len());
        if identity == identities.len() {
            identities.push(comparable);
        }
        let realized = baml_compiler2_mir::RealizedTy::try_from(ty.clone())
            .map_err(|_| no("unrealized reflected type"))?;
        let anchored = plan
            .index
            .anchor(&baml_compiler2_mir::TyTemplate::from(realized))
            .ok_or_else(|| no("reflected type identity"))?;
        let wire = anchored.substitute_symbolic(&[]);
        let capture = capture_codegen::capture_wire_type(plan, &wire)?;
        let kind = baml_type::type_kind::classify_type(
            &baml_compiler2_mir::RealizedTy::try_from(ty.clone())
                .map_err(|_| no("reflected kind"))?,
        );
        let children = children(ty)
            .iter()
            .map(|child| {
                plan.reflection_types
                    .iter()
                    .position(|t| t == child)
                    .unwrap()
            })
            .collect::<Vec<_>>();
        writeln!(out,"rt::reflection::StaticType{{ty:&{capture},display:{display:?},identity:{identity},kind:rt::reflection::TypeKind::{kind:?},children:&{children:?}}},").unwrap();
    }
    out.push_str("];\n");
    Ok(())
}
