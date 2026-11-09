//! Native higher-order array operations share closure adapters and VM snapshots.
use super::*;
pub(super) fn native(name: &str) -> bool {
    matches!(
        name,
        "baml.Array.map"
            | "baml.Array.filter"
            | "baml.Array.some"
            | "baml.Array.every"
            | "baml.Array.reduce"
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
        match term {
            Some(Terminator::Call {
                callee: Operand::Constant(Constant::Function(f)),
                ..
            }) if plan
                .index
                .function(db, *f)
                .is_some_and(|(id, _)| native(plan.index.display(id))) =>
            {
                Some(Ok(registry::Reach::default()))
            }
            _ => None,
        }
    }
    fn terminator(&self, e: &mut Emitter<'_, '_>, term: &Terminator<'_>) -> Option<Gen<String>> {
        let Terminator::Call {
            callee: Operand::Constant(Constant::Function(f)),
            args,
            ntypeargs,
            destination,
            ..
        } = term
        else {
            return None;
        };
        let (id, _) = e.plan.index.function(e.db, *f)?;
        let name = e.plan.index.display(id);
        if !native(name) {
            return None;
        }
        Some((|| {
            let values = args[*ntypeargs..]
                .iter()
                .map(|a| e.operand(a))
                .collect::<Gen<Vec<_>>>()?;
            let result = e.place(destination)?.1;
            let (value, repr) = emit(e, name, &values, result)?;
            e.assign(destination, (value, repr))
        })())
    }
    fn call(
        &self,
        e: &Emitter<'_, '_>,
        name: &str,
        _: &[Repr],
        values: &[(String, Repr)],
    ) -> Option<Gen<(String, Repr)>> {
        if !native(name) || !matches!(e.all[&e.object].kind, MirFunctionKind::Builtin(_)) {
            return None;
        }
        Some((|| {
            let frame = e.plan.linked_function_frame(e.object)?;
            let result = e.plan.field_repr(
                &e.plan
                    .index
                    .info(e.object)
                    .return_type
                    .substitute_symbolic(&frame),
            )?;
            emit(e, name, values, result)
        })())
    }
}
fn emit(
    e: &Emitter<'_, '_>,
    name: &str,
    values: &[(String, Repr)],
    result: Repr,
) -> Gen<(String, Repr)> {
    let input = values[0].1;
    let element = e.plan.element(input)?;
    let callback = e.coerce(values[1].clone(), Repr::Closure)?;
    let desc = e.lists[&input] + usize::from(input.growing());
    let value = e.coerce(
        (
            format!("*(((snapshot+16) as *const {}).add(i))", element.rust()),
            element,
        ),
        Repr::Tagged,
    )?;
    let poll = format!("if i&15==0{{{};}}", e.fallible("cx.poll()"));
    let reduce = name == "baml.Array.reduce";
    let arg = if reduce { "acc,value" } else { "value" };
    let mut cases = String::new();
    for &target in &e.plan.callables {
        let site = if matches!(e.all[&e.object].kind, MirFunctionKind::Builtin(_)) {
            format!("callback_site{}(_site,{target})", e.object)
        } else {
            e.telemetry_site(target).to_string()
        };
        cases.push_str(&format!(
            "{target}=>rt::closures::call(callback,{site},&[{arg}]),"
        ));
    }
    let call = e.fallible(&format!("match object{{{cases}_=>unreachable!()}}"));
    let object = e.fallible("rt::closures::object(callback)");
    let snapshot = e.fallible(&format!(
        "rt::callbacks::snapshot({}, {},{desc})",
        values[0].0,
        input.growing()
    ));
    let mut code = format!(
        "{{let callback={callback};let object={object};let snapshot={snapshot};let length=*((snapshot+8) as *const usize);"
    );
    if reduce {
        code.push_str(&format!(
            "let mut acc={};",
            e.coerce(values[2].clone(), Repr::Tagged)?
        ));
        code.push_str(&format!(
            "for i in 0..length{{{poll}let value={value};acc={call};}}acc}}"
        ));
        return Ok((code, Repr::Tagged));
    }
    let predicate = matches!(name, "baml.Array.some" | "baml.Array.every");
    if predicate {
        let every = name == "baml.Array.every";
        let test = if every { "!selected" } else { "selected" };
        let selected = e.coerce((call, Repr::Tagged), Repr::Bool)?;
        code.push_str(&format!("let mut answer={every};for i in 0..length{{{poll}let value={value};let selected={selected};if {test}{{answer={};break;}}}}answer}}",!every));
        return Ok((code, Repr::Bool));
    }
    let output = e.plan.element(result)?;
    let out_desc = e.lists[&result];
    if result.growing() {
        let slot = output.slot()?;
        code.push_str(&format!(
            "let out={};",
            e.fallible(&format!(
                "rt::lists::List::<{slot}>::new(&mut rt::strings::Cx::current(),{out_desc})"
            ))
        ));
    } else {
        code.push_str(&format!(
            "let out={};",
            e.fallible(&format!("rt::callbacks::array({out_desc},length)"))
        ));
    }
    code.push_str(&format!("let mut count=0usize;for i in 0..length{{{poll}"));
    code.push_str(&format!("let value={value};let mapped={call};"));
    let filter = name == "baml.Array.filter";
    if filter {
        code.push_str(&format!(
            "if !{}{{continue;}}",
            e.coerce(("mapped".into(), Repr::Tagged), Repr::Bool)?
        ));
    }
    let selected = e.coerce(
        (if filter { "value" } else { "mapped" }.into(), Repr::Tagged),
        output,
    )?;
    if result.growing() {
        code.push_str(&format!(
            "{};",
            e.fallible(&format!(
                "out.push(&mut rt::strings::Cx::current(),{})",
                output.to_slot(&selected)?
            ))
        ));
    } else if output.pointer() {
        code.push_str(&format!(
            "baml_rt_gc::write_ptr(out.cast(),out.add(16+count*8).cast(),{selected});"
        ));
    } else {
        code.push_str(&format!(
            "out.add(16+count*8).cast::<{}>().write({selected});",
            output.rust()
        ));
    }
    code.push_str("count+=1;}");
    if result.growing() {
        code.push_str("out.as_raw()}");
    } else {
        code.push_str("out.add(8).cast::<usize>().write(count);out as usize}");
    }
    Ok((code, result))
}

// A hidden native callback inherits its invoking call's visible caller and PC.
// Freeze those edges after every ordinary call site has been emitted, including
// native callbacks invoked by another native callback.
pub(super) fn callback_tables(
    out: &mut String,
    plan: &Plan<'_>,
    sites: &std::cell::RefCell<Vec<(Option<usize>, usize, usize)>>,
) {
    let natives = plan
        .callables
        .iter()
        .copied()
        .filter(|id| native(plan.index.display(*id)))
        .collect::<BTreeSet<_>>();
    if natives.is_empty() {
        return;
    }
    let mut sites = sites.borrow_mut();
    let mut indices = sites
        .iter()
        .copied()
        .enumerate()
        .map(|(i, s)| (s, i))
        .collect::<BTreeMap<_, _>>();
    let mut cursor = 0;
    while cursor < sites.len() {
        let (caller, pc, callee) = sites[cursor];
        cursor += 1;
        if !natives.contains(&callee) {
            continue;
        }
        for &target in &plan.callables {
            let key = (caller, pc, target);
            if !indices.contains_key(&key) {
                let i = sites.len();
                indices.insert(key, i);
                sites.push(key);
            }
        }
    }
    for native in natives {
        write!(
            out,
            "fn callback_site{native}(outer:usize,target:usize)->usize{{match (outer,target){{"
        )
        .unwrap();
        for (i, &(caller, pc, callee)) in sites.iter().enumerate() {
            if callee != native {
                continue;
            }
            for &target in &plan.callables {
                write!(out, "({i},{target})=>{},", indices[&(caller, pc, target)]).unwrap();
            }
        }
        out.push_str("_=>unreachable!(),}}\n");
    }
}
