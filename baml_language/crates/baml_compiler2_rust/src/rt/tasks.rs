use super::*;

pub(super) fn class_is(plan: &Plan<'_>, class: usize, path: &str) -> bool {
    let origin = plan.class_origin(class);
    plan.class(class).path.to_string() == path
        && plan
            .program
            .packages
            .iter()
            .any(|p| p.name == "baml" && p.objects.contains(&(origin as u32)))
}
pub(super) fn native(name: &str) -> bool {
    name.starts_with("baml.future.Future.")
}

pub(super) struct Feature;
impl registry::Feature for Feature {
    fn admit(&self, event: &registry::Admission<'_, '_>) -> Gen<()> {
        match event {
            registry::Admission::ShieldedBlock(true) => Err(no("defer/shielding")),
            registry::Admission::Class { cleanup: true, .. } => Err(no("class cleanup")),
            _ => Ok(()),
        }
    }
    fn reach(
        &self,
        db: &dyn Db,
        plan: &mut Plan<'_>,
        term: Option<&Terminator<'_>>,
    ) -> Option<Gen<registry::Reach>> {
        match term {
            Some(
                Terminator::Spawn { .. } | Terminator::Await { .. } | Terminator::AwaitAny { .. },
            ) => Some(Ok(registry::Reach::default())),
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
        match term {
            Terminator::Spawn { plan, future, .. } => Some((|| {
                let (value, Repr::Ptr(class)) = e.operand(plan)? else {
                    return Err(no("spawn plan storage"));
                };
                if !class_is(e.plan, class, "spawn.Plan") {
                    return Err(no("spawn requires Plan"));
                }
                let field = |name: &str| -> Gen<String> {
                    let i = e
                        .plan
                        .class(class)
                        .fields
                        .iter()
                        .position(|f| f.name == name)
                        .ok_or_else(|| no("spawn plan field"))?;
                    Ok(format!("(*(plan as *const C{class})).f{i}"))
                };
                let body = field("body")?;
                let detached = field("root")?;
                let mut cases = String::new();
                for &callee in &e.plan.callables {
                    cases.push_str(&format!("{callee}=>{},", e.telemetry_site(callee)));
                }
                let id = e.fallible("rt::closures::object(body)");
                let call = format!(
                    "{{let plan={value};let body={body};let site=match {id}{{{cases}_=>unreachable!()}};rt::tasks::spawn({},body,site,rt::tasks::SpawnOptions{{detached:{detached}!=0,..Default::default()}})}}",
                    e.plan.future_descriptor().unwrap()
                );
                e.assign(future, (e.fallible(&call), Repr::Future))
            })()),
            Terminator::Await {
                future,
                destination,
                ..
            } => Some((|| {
                let value = e.place(future)?.0;
                e.assign(
                    destination,
                    (
                        e.fallible(&format!("rt::tasks::await_future({value})")),
                        Repr::Tagged,
                    ),
                )
            })()),
            Terminator::Call {
                callee: Operand::Constant(Constant::Function(f)),
                args,
                ntypeargs,
                destination,
                ..
            } if e
                .plan
                .index
                .function(e.db, *f)
                .is_some_and(|(id, _)| native(e.plan.index.display(id))) =>
            {
                Some((|| {
                    let (id, _) = e.plan.index.function(e.db, *f).unwrap();
                    let name = e.plan.index.display(id);
                    let receiver = e.operand(&args[*ntypeargs])?.0;
                    let operation = name.rsplit('.').next().unwrap();
                    if operation == "cancel" {
                        e.assign(
                            destination,
                            (format!("rt::tasks::cancel({receiver})"), Repr::Bool),
                        )
                    } else {
                        Err(no(format!("future operation {operation}")))
                    }
                })())
            }
            _ => None,
        }
    }
}
