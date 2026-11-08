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
            let input = values[0].1;
            let element = Emitter::element(input)?;
            let callback = e.coerce(values[1].clone(), Repr::Closure)?;
            let desc = e.lists[&input] + usize::from(input.growing());
            let value = e.coerce(
                (
                    format!("*(((snapshot+16) as *const {}).add(i))", element.rust()),
                    element,
                ),
                Repr::Tagged,
            )?;
            let reduce = name == "baml.Array.reduce";
            let arg = if reduce { "acc,value" } else { "value" };
            let mut cases = String::new();
            for &target in &e.plan.callables {
                let site = e.telemetry_site(target);
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
            let result = e.place(destination)?.1;
            if reduce {
                code.push_str(&format!(
                    "let mut acc={};",
                    e.coerce(values[2].clone(), Repr::Tagged)?
                ));
                code.push_str(&format!(
                    "for i in 0..length{{let value={value};acc={call};}}acc}}"
                ));
                return e.assign(destination, (code, Repr::Tagged));
            }
            let predicate = matches!(name, "baml.Array.some" | "baml.Array.every");
            if predicate {
                let every = name == "baml.Array.every";
                let test = if every { "!selected" } else { "selected" };
                let selected = e.coerce((call, Repr::Tagged), Repr::Bool)?;
                code.push_str(&format!("let mut answer={every};for i in 0..length{{let value={value};let selected={selected};if {test}{{answer={};break;}}}}answer}}",!every));
                return e.assign(destination, (code, Repr::Bool));
            }
            let output = Emitter::element(result)?;
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
            code.push_str("let mut count=0usize;for i in 0..length{");
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
                        "out.push(&mut rt::strings::Cx::current(),{}::from_raw({selected}))",
                        output.slot()?
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
            e.assign(destination, (code, result))
        })())
    }
}
