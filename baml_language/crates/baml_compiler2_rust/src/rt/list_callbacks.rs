//! Typed callback adapters. Element storage never passes through a boxed list.
use super::*;

pub(super) fn handles(name: &str) -> bool {
    matches!(
        name,
        "baml.Array.map"
            | "baml.Array.filter"
            | "baml.Array.some"
            | "baml.Array.every"
            | "baml.Array.find"
            | "baml.Array.find_index"
            | "baml.Array.find_last"
            | "baml.Array.find_last_index"
            | "baml.Array.reduce"
            | "baml.Array.flat_map"
            | "baml.Array.sort_by"
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
        let Some(Terminator::Call {
            callee: Operand::Constant(Constant::Function(f)),
            ..
        }) = term
        else {
            return None;
        };
        let (callee, _) = plan.index.function(db, *f)?;
        handles(plan.index.display(callee)).then(|| Ok(registry::Reach::default()))
    }
    fn terminator(&self, e: &mut Emitter<'_, '_>, term: &Terminator<'_>) -> Option<Gen<String>> {
        let Terminator::Call {
            callee: Operand::Constant(Constant::Function(f)),
            args,
            ntypeargs,
            destination,
            argument_layout,
            ..
        } = term
        else {
            return None;
        };
        let (callee, info) = e.plan.index.function(e.db, *f)?;
        let name = e.plan.index.display(callee);
        if !handles(name) {
            return None;
        }
        Some((|| {
            let mut values = args[*ntypeargs..]
                .iter()
                .map(|a| e.operand(a))
                .collect::<Gen<Vec<_>>>()?;
            if let Some(layout) = argument_layout {
                values = layout
                    .map_to(&info.argument_layout())
                    .map_err(no)?
                    .into_iter()
                    .map(|i| {
                        i.map(|i| values[i].clone())
                            .ok_or_else(|| no("omitted list callback argument"))
                    })
                    .collect::<Gen<_>>()?;
            }
            let receiver = values.first().ok_or_else(|| no("list callback receiver"))?;
            let element = e.plan.element(receiver.1)?;
            let slot = element.slot()?;
            let callback = e.coerce(
                values
                    .get(1)
                    .ok_or_else(|| no("missing list callback"))?
                    .clone(),
                Repr::Closure,
            )?;
            let method = name.strip_prefix("baml.Array.").unwrap();
            let output = e.place(destination)?.1;
            let item = e.coerce((element.from_slot("item"), element), Repr::Tagged)?;
            let invoke = if matches!(method, "reduce" | "sort_by") {
                String::new()
            } else {
                e.list_callback("callback", &[item])?
            };
            let cx = "&mut rt::strings::Cx::current()";
            let list = format!("rt::lists::List::<{slot}>::from_raw(receiver)");
            let (body, repr) = match method {
                "map" | "flat_map" => {
                    let mapped = e.plan.element(output)?;
                    let out_slot = mapped.slot()?;
                    let returned = if method == "flat_map" { output } else { mapped };
                    let value = e.callback_result("value", returned)?;
                    let result = if method == "flat_map" {
                        format!("rt::lists::List::<{out_slot}>::from_raw({value})")
                    } else {
                        mapped.to_slot(&value)?
                    };
                    (
                        format!(
                            "{list}.{method}::<{out_slot}>({cx},{},|item|{{let value={invoke}?;Ok({result})}}).map(|v|v.as_raw())",
                            e.lists[&output]
                        ),
                        output,
                    )
                }
                "filter" | "some" | "every" => {
                    let predicate = e.callback_result("value", Repr::Bool)?;
                    let expr = format!(
                        "{list}.{method}({cx},|item|{{let value={invoke}?;Ok({predicate})}})"
                    );
                    if method == "filter" {
                        (format!("{expr}.map(|v|v.as_raw())"), receiver.1)
                    } else {
                        (expr, Repr::Bool)
                    }
                }
                "find" | "find_last" | "find_index" | "find_last_index" => {
                    let predicate = e.callback_result("value", Repr::Bool)?;
                    let index = method.ends_with("index");
                    let scan = if index { "find_index" } else { "find" };
                    let found = if index {
                        e.coerce(("found".into(), Repr::Int), Repr::Tagged)?
                    } else {
                        e.coerce((element.from_slot("found"), element), Repr::Tagged)?
                    };
                    (
                        format!(
                            "{list}.{scan}({cx},{},|item|{{let value={invoke}?;Ok({predicate})}}).map(|result|match result{{Some(found)=>{found},None=>rt::Dyn{{tag:2,payload:0}}}})",
                            method.contains("last")
                        ),
                        Repr::Tagged,
                    )
                }
                "reduce" => {
                    let initial = e.coerce(
                        values
                            .get(2)
                            .ok_or_else(|| no("missing list reduction initial value"))?
                            .clone(),
                        output,
                    )?;
                    let accumulator = e.coerce(("accumulator".into(), output), Repr::Tagged)?;
                    let item = e.coerce((element.from_slot("item"), element), Repr::Tagged)?;
                    let invoke = e.list_callback("callback", &[accumulator, item])?;
                    let result = e.callback_result("value", output)?;
                    (
                        format!(
                            "{list}.reduce({cx},{initial},|accumulator,item|{{let value={invoke}?;Ok({result})}})"
                        ),
                        output,
                    )
                }
                "sort_by" => {
                    let a = e.coerce((element.from_slot("a"), element), Repr::Tagged)?;
                    let b = e.coerce((element.from_slot("b"), element), Repr::Tagged)?;
                    let invoke = e.list_callback("callback", &[a, b])?;
                    (
                        format!(
                            "{list}.sort_by({cx},|a,b|{{let ordering={invoke}?;Ok(match ordering.payload{{0=>std::cmp::Ordering::Less,1=>std::cmp::Ordering::Equal,2=>std::cmp::Ordering::Greater,_=>unreachable!(\"typed Ordering\")}})}}).map(|v|v.as_raw())"
                        ),
                        receiver.1,
                    )
                }
                _ => unreachable!(),
            };
            let expr = format!(
                "{{static SOURCE:rt::Source=rt::Source{{file:{:?},function:{name:?},lines:&[0],pcs:&[0],kinds:&[rt::telemetry::RaiseKind::NativeBoundary]}};let receiver={};let callback={callback};({body}).map_err(|error|error.at(&SOURCE,0))}}",
                e.plan.index.info(callee).source_file,
                receiver.0
            );
            e.assign(destination, (e.fallible(&expr), repr))
        })())
    }
}
impl Emitter<'_, '_> {
    // Nested Rust callbacks return their language error to the enclosing native
    // operation. They cannot jump into the caller's catch labels.
    fn callback_result(&self, value: &str, repr: Repr) -> Gen<String> {
        if matches!(repr, Repr::Tagged | Repr::Dyn(_)) {
            return Ok(value.into());
        }
        Ok(repr.decode_payload(&format!(
            "rt::untag({value},{},{})?",
            self.tag(repr)?,
            repr.pointer()
        )))
    }
    fn list_callback(&self, callback: &str, args: &[String]) -> Gen<String> {
        let mut cases = String::new();
        let args = args.join(",");
        for &object in &self.plan.callables {
            write!(
                cases,
                "{object}=>rt::closures::call({callback},{},&[{args}]),",
                self.telemetry_site(object)
            )
            .unwrap();
        }
        Ok(format!(
            "match rt::closures::object({callback})?{{{cases}_=>Err(rt::Error::internal(\"unlinked list callback\".into()))}}"
        ))
    }
}
