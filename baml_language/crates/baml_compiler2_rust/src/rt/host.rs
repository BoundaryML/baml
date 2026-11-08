use super::*;

fn host_type(plan: &Plan<'_>, lists: &BTreeMap<Repr, usize>, ty: &RuntimeTy) -> Gen<String> {
    if let RuntimeTy::Literal(literal, _) = ty {
        return literal_type(literal);
    }
    if let RuntimeTy::Union(members) = ty
        && members.len() == 2
        && members.iter().any(|t| matches!(t, RuntimeTy::Null))
    {
        let member = members
            .iter()
            .find(|t| !matches!(t, RuntimeTy::Null))
            .unwrap();
        return Ok(format!(
            "rt::host::Type::Optional(&{})",
            host_type(plan, lists, member)?
        ));
    }
    if let RuntimeTy::List(element) = ty {
        let repr = plan.repr(ty)?;
        return Ok(format!(
            "rt::host::Type::List{{element:&{},desc:{},growing:{}}}",
            host_type(plan, lists, element)?,
            lists.get(&repr).copied().unwrap_or(0),
            repr.growing()
        ));
    }
    repr_type(plan, lists, plan.repr(ty)?)
}
fn literal_type(literal: &baml_type::Literal) -> Gen<String> {
    Ok(match literal {
        baml_type::Literal::Int(v) => format!("rt::host::Type::IntLiteral({v})"),
        baml_type::Literal::Bool(v) => format!("rt::host::Type::BoolLiteral({v})"),
        baml_type::Literal::String(v) => format!("rt::host::Type::StringLiteral({:?})", v.as_str()),
        baml_type::Literal::Float(v) => format!(
            "rt::host::Type::FloatLiteral({}u64)",
            v.as_str()
                .parse::<f64>()
                .map_err(|_| no("invalid float literal"))?
                .to_bits()
        ),
        _ => return Err(no("host literal type")),
    })
}
fn field_type(
    plan: &Plan<'_>,
    lists: &BTreeMap<Repr, usize>,
    ty: &bex_vm_types::RuntimeTy,
) -> Gen<String> {
    use bex_vm_types::RuntimeTy as T;
    match ty {
        T::Literal(literal, _) => literal_type(literal),
        T::Union(m) if m.len() == 2 && m.iter().any(|t| matches!(t, T::Null)) => Ok(format!(
            "rt::host::Type::Optional(&{})",
            field_type(
                plan,
                lists,
                m.iter().find(|t| !matches!(t, T::Null)).unwrap()
            )?
        )),
        T::List(element) => {
            let repr = plan.field_repr(ty)?;
            Ok(format!(
                "rt::host::Type::List{{element:&{},desc:{},growing:{}}}",
                field_type(plan, lists, element)?,
                lists.get(&repr).copied().unwrap_or(0),
                repr.growing()
            ))
        }
        _ => repr_type(plan, lists, plan.field_repr(ty)?),
    }
}
fn repr_type(plan: &Plan<'_>, lists: &BTreeMap<Repr, usize>, repr: Repr) -> Gen<String> {
    let primitive = match repr {
        Repr::Null => "Null",
        Repr::Int => "Int",
        Repr::Float => "Float",
        Repr::Bool => "Bool",
        Repr::Str => "String",
        Repr::Ptr(c) => return Ok(format!("rt::host::Type::Class(&HOST_CLASS_{c})")),
        r @ (Repr::ListInt | Repr::ListStr | Repr::ListPtr(_) | Repr::ListListStr) => {
            let element = repr_type(plan, lists, Emitter::element(r)?)?;
            return Ok(format!(
                "rt::host::Type::List{{element:&{element},desc:{},growing:{}}}",
                lists.get(&r).copied().unwrap_or(0),
                r.growing()
            ));
        }
        _ => return Err(no("host argument/return type adapter")),
    };
    Ok(format!("rt::host::Type::{primitive}"))
}

pub(super) fn admit(plan: &Plan<'_>, mir: &MirFunction<'_>) -> Gen<()> {
    let MirFunctionKind::Bytecode(body) = mir.kind.as_ref() else {
        return Err(no("host entry"));
    };
    for local in &body.locals[..=mir.arity] {
        host_type(plan, &BTreeMap::new(), &local.ty)?;
    }
    Ok(())
}

pub(super) fn emit(
    mut out: &mut String,
    plan: &Plan<'_>,
    mir: &MirFunction<'_>,
    lists: &BTreeMap<Repr, usize>,
    descriptors: &BTreeMap<usize, usize>,
    entry: usize,
    builtins: &BTreeSet<String>,
    format_site: usize,
) -> Gen<()> {
    let MirFunctionKind::Bytecode(body) = mir.kind.as_ref() else {
        unreachable!()
    };
    let builtin_init = if builtins.is_empty() {
        ""
    } else {
        "init_builtin_observation();"
    };
    let has_globals = !plan.init_chains.is_empty();
    let int_driver = if has_globals {
        "rt::driver_observed_initialized"
    } else {
        "rt::driver_observed"
    };
    let initialize = if has_globals {
        ",initialize_program"
    } else {
        ""
    };
    let host_initialize = if has_globals {
        "Some(initialize_program)"
    } else {
        "None"
    };
    if mir.arity == 0 && plan.repr(&body.locals[0].ty)? == Repr::Int {
        writeln!(&mut out,"fn entry()->R<i64>{{unsafe{{if rt::telemetry::active(){{f{entry}::<true>(rt::calls::Context::current(),0)}}else{{f{entry}::<false>(rt::calls::Context::current(),0)}}}}}}\nfn main()->std::process::ExitCode{{{builtin_init}{int_driver}(DESCRIPTORS,entry,TELEMETRY_FUNCTIONS,TELEMETRY_SITES,Some({format_site}){initialize})}}").unwrap();
        return Ok(());
    }
    let mut classes = BTreeSet::new();
    fn visit(plan: &Plan<'_>, r: Repr, classes: &mut BTreeSet<usize>) {
        if let Repr::Ptr(c) | Repr::ListPtr(c) = r
            && classes.insert(c)
        {
            for r in &plan.layouts[&c].fields {
                visit(plan, *r, classes);
            }
        }
    }
    for local in &body.locals[..=mir.arity] {
        visit(plan, plan.repr(&local.ty)?, &mut classes);
    }
    for c in classes {
        let class = plan.class(c);
        for package in &plan.program.packages {
            for rule in package.impl_rules.values().flatten() {
                if matches!(&rule.for_ty_pattern,bex_vm_types::TyTemplate::Class(head,_) if head.tag()==class.type_tag)
                {
                    return Err(no(
                        "host class conversion with custom interface implementations",
                    ));
                }
            }
        }
        let layout = &plan.layouts[&c];
        write!(&mut out,"static HOST_CLASS_{c}:rt::host::Class=rt::host::Class{{name:{:?},desc:{},bytes:{},fields:&[",class.path.to_string(),descriptors[&c],layout.bytes).unwrap();
        for (i, field) in class.fields.iter().enumerate() {
            let repr = layout.fields[i];
            let schema = field_type(plan, lists, &field.field_type)?;
            write!(&mut out,"rt::host::Field{{name:{:?},offset:{},slot:baml_rt_gc::SlotKind::{},ty:&{schema}}},",field.name,layout.offsets[i],if repr.pointer(){"Pointer"}else if repr==Repr::Tagged {"Tagged"}else {"Scalar"}).unwrap();
        }
        out.push_str("]};\n");
    }
    let info = plan.index.info(entry);
    out.push_str("static HOST_FUNCTION:rt::host::Function=rt::host::Function{name:");
    write!(&mut out, "{:?},arguments:&[", plan.index.display(entry)).unwrap();
    for (i, name) in info.param_names.iter().enumerate() {
        write!(
            &mut out,
            "rt::host::Argument{{name:{:?},ty:&{}}},",
            name,
            host_type(plan, lists, &body.locals[i + 1].ty)?
        )
        .unwrap();
    }
    out.push_str("],invoke:host_entry};\nunsafe fn host_entry(args:&[rt::host::Owned])->R<rt::host::Owned>{unsafe{");
    let mut args = Vec::new();
    for i in 1..=mir.arity {
        write!(
            &mut out,
            "let d{i}=({}).hydrate(&args[{}])?;",
            host_type(plan, lists, &body.locals[i].ty)?,
            i - 1
        )
        .unwrap();
        let repr = plan.repr(&body.locals[i].ty)?;
        args.push(match repr {
            Repr::Float => format!("f64::from_bits(d{i}.payload)"),
            Repr::Bool => format!("d{i}.payload!=0"),
            Repr::Tagged => format!("d{i}"),
            _ => format!("d{i}.payload as {}", repr.rust()),
        });
    }
    write!(&mut out, "let value=if rt::telemetry::active(){{f{entry}::<true>(rt::calls::Context::current(),0,{0})}}else{{f{entry}::<false>(rt::calls::Context::current(),0,{0})}}?;", args.join(",")).unwrap();
    let repr = plan.repr(&body.locals[0].ty)?;
    let tag = match repr {
        Repr::Int => "0",
        Repr::Float => "rt::scalars::FLOAT_TAG",
        Repr::Bool => "1",
        Repr::Null => "2",
        Repr::Str => "3",
        Repr::Tagged => "",
        _ => "",
    };
    let value = if repr == Repr::Tagged {
        "value".into()
    } else if let Repr::Ptr(c) = repr {
        format!(
            "rt::Dyn{{tag:if value==0{{2}}else{{{}}},payload:value as u64}}",
            descriptors[&c] + 3
        )
    } else if let Some(desc) = lists.get(&repr) {
        format!(
            "rt::Dyn{{tag:if value==0{{2}}else{{{}}},payload:value as u64}}",
            desc + 3
        )
    } else {
        let payload = if repr == Repr::Float {
            "value.to_bits()"
        } else {
            "value as u64"
        };
        let tag = if repr.pointer() {
            format!("if value==0{{2}}else{{{tag}}}")
        } else {
            tag.to_owned()
        };
        format!("rt::Dyn{{tag:{tag},payload:{payload}}}")
    };
    let returned = host_type(plan, lists, &body.locals[0].ty)?;
    writeln!(&mut out,"({returned}).copy_out({value},0)}}}}\nfn main()->std::process::ExitCode{{{builtin_init}rt::host::driver_initialized(DESCRIPTORS,&HOST_FUNCTION,TELEMETRY_FUNCTIONS,TELEMETRY_SITES,{host_initialize})}}").unwrap();
    Ok(())
}
