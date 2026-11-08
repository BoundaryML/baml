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
        || name == "baml._cleanup_begin"
        || name.starts_with("baml.spawn.CancelToken.")
        || name.starts_with("baml.spawn.Limit.")
}

pub(super) fn resource_concat(
    db: &dyn Db,
    plan: &Plan<'_>,
    body: &MirFunctionBody<'_>,
    term: &Terminator<'_>,
) -> bool {
    let Terminator::Call {
        callee: Operand::Constant(Constant::Function(f)),
        args,
        ntypeargs,
        ..
    } = term
    else {
        return false;
    };
    if !plan
        .index
        .function(db, *f)
        .is_some_and(|(id, _)| plan.index.display(id) == "baml.Array.concat")
    {
        return false;
    }
    let Some(Operand::Copy(Place::Local(l)) | Operand::Move(Place::Local(l))) =
        args.get(*ntypeargs)
    else {
        return false;
    };
    matches!(plan.local_repr(body,l.0,0),Ok(Repr::ListPtr(c)) if class_is(plan,c,"spawn.Limit") || class_is(plan,c,"spawn.CancelToken"))
}
// A fresh stdlib Plan.new consumed by the immediately following spawn cannot
// escape. With observation off its plan and two empty modifier arrays have no
// observable identity; keep the launch arguments in ordinary stack locals.
pub(super) fn ephemeral_plan(e: &Emitter<'_, '_>, term: &Terminator<'_>) -> Option<usize> {
    let Terminator::Call {
        callee: Operand::Constant(Constant::Function(f)),
        destination: Place::Local(local),
        target,
        ..
    } = term
    else {
        return None;
    };
    if !e
        .plan
        .index
        .function(e.db, *f)
        .is_some_and(|(id, _)| e.plan.index.display(id) == "baml.spawn.Plan.new")
    {
        return None;
    }
    let next = &e.body.blocks[target.0];
    if !next.statements.is_empty() {
        return None;
    }
    match &next.terminator {
        Some(Terminator::Spawn {
            plan: Operand::Copy(Place::Local(p)) | Operand::Move(Place::Local(p)),
            ..
        }) if p == local => Some(local.0),
        _ => None,
    }
}
pub(super) fn ephemeral_spawn(e: &Emitter<'_, '_>, plan: &Operand<'_>) -> Option<usize> {
    let (Operand::Copy(Place::Local(local)) | Operand::Move(Place::Local(local))) = plan else {
        return None;
    };
    e.body
        .blocks
        .iter()
        .filter_map(|b| b.terminator.as_ref())
        .find_map(|term| ephemeral_plan(e, term).filter(|l| *l == local.0))
}
pub(super) fn fast_plan(
    e: &Emitter<'_, '_>,
    term: &Terminator<'_>,
    values: &[(String, Repr)],
) -> Gen<Option<String>> {
    let Some(local) = ephemeral_plan(e, term) else {
        return Ok(None);
    };
    let body = e.coerce(values[0].clone(), Repr::Closure)?;
    let name = match values[1].1 {
        Repr::Null => "0usize".into(),
        Repr::Str => values[1].0.clone(),
        Repr::Tagged => format!(
            "{{let v={};if v.tag==u64::MAX || v.tag==2{{0usize}}else{{({}) as usize}}}}",
            values[1].0,
            e.fallible("rt::untag(v,3,true)")
        ),
        _ => return Err(no("spawn name storage")),
    };
    Ok(Some(format!(
        "fast_plan_body{local}={body};fast_plan_name{local}={name};l{local}=0usize;"
    )))
}
pub(super) struct Feature;
impl registry::Feature for Feature {
    fn admit(&self, event: &registry::Admission<'_, '_>) -> Gen<()> {
        match event {
            _ => Ok(()),
        }
    }
    fn call(
        &self,
        e: &Emitter<'_, '_>,
        name: &str,
        _: &[Repr],
        values: &[(String, Repr)],
    ) -> Option<Gen<(String, Repr)>> {
        if name!="baml.Array.concat" || values.first().is_none_or(|v|!matches!(v.1,Repr::ListPtr(c) if class_is(e.plan,c,"spawn.Limit") || class_is(e.plan,c,"spawn.CancelToken"))) {return None;}
        Some((|| {
            if values.len() != 2 || values[0].1 != values[1].1 {
                return Err(no("task modifier list concat"));
            }
            let repr = values[0].1;
            let desc = e.lists[&repr];
            Ok((
                e.fallible(&format!(
                    "rt::tasks::concat_pointers({desc},{},{})",
                    values[0].0, values[1].0
                )),
                repr,
            ))
        })())
    }
    fn value(
        &self,
        e: &Emitter<'_, '_>,
        value: &Rvalue<'_>,
        _: Repr,
    ) -> Option<Gen<(String, Repr)>> {
        let Rvalue::BinaryOp { op, left, right } = value else {
            return None;
        };
        let left = e.operand(left);
        let right = e.operand(right);
        if left.as_ref().is_ok_and(|v| v.1 == Repr::Bigint)
            || right.as_ref().is_ok_and(|v| v.1 == Repr::Bigint)
        {
            Some((|| {
                let operand = |(text, repr): (String, Repr)| -> Gen<String> {
                    match repr {
                        Repr::Bigint => Ok(format!("rt::tasks::bigint_value({text})")),
                        Repr::Int => Ok(format!("rt::tasks::integer({text})")),
                        _ => Err(no("duration bigint operand")),
                    }
                };
                let l = operand(left?)?;
                let r = operand(right?)?;
                let desc = e.plan.bigint_descriptor().unwrap();
                match op {
                    BinOp::Add | BinOp::Sub | BinOp::Mul => Ok((
                        e.fallible(&format!("rt::tasks::bigint({desc},&({l}{op}{r}))")),
                        Repr::Bigint,
                    )),
                    BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => {
                        Ok((format!("({l}{op}{r})"), Repr::Bool))
                    }
                    _ => Err(no("duration bigint operation")),
                }
            })())
        } else {
            None
        }
    }
    fn reach(
        &self,
        db: &dyn Db,
        plan: &mut Plan<'_>,
        term: Option<&Terminator<'_>>,
    ) -> Option<Gen<registry::Reach>> {
        match term {
            Some(Terminator::SysOp {
                callee: Operand::Constant(Constant::Function(f)),
                ..
            }) if plan.index.function(db, *f).is_some_and(|(id, _)| {
                matches!(
                    plan.index.display(id),
                    "baml.sys.collect_garbage" | "baml.sys._sleep"
                )
            }) =>
            {
                Some(Ok(registry::Reach::default()))
            }
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
            Terminator::SysOp {
                callee: Operand::Constant(Constant::Function(f)),
                args,
                destination,
                ..
            } if e
                .plan
                .index
                .function(e.db, *f)
                .is_some_and(|(id, _)| e.plan.index.display(id) == "baml.sys._sleep") =>
            {
                Some((|| {
                    let (nanos, Repr::Bigint) = e.operand(&args[0])? else {
                        return Err(no("sleep nanoseconds"));
                    };
                    let value = format!(
                        "{{{};0usize}}",
                        e.fallible(&format!("rt::tasks::sleep({nanos})"))
                    );
                    e.assign(destination, (value, Repr::Null))
                })())
            }
            Terminator::SysOp {
                callee: Operand::Constant(Constant::Function(f)),
                destination,
                ..
            } if e
                .plan
                .index
                .function(e.db, *f)
                .is_some_and(|(id, _)| e.plan.index.display(id) == "baml.sys.collect_garbage") =>
            {
                Some(e.assign(
                    destination,
                    (
                        format!("{{{};0usize}}", e.fallible("rt::tasks::collect()")),
                        Repr::Null,
                    ),
                ))
            }
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
                let name = field("name")?;
                let limits = field("limits")?;
                let tokens = field("cancel_tokens")?;
                let mut cases = String::new();
                let mut entries = String::new();
                for &callee in &e.plan.callables {
                    cases.push_str(&format!("{callee}=>{},", e.telemetry_site(callee)));
                    let site = e.telemetry_sites.borrow().len();
                    e.telemetry_sites.borrow_mut().push((None, 0, callee));
                    entries.push_str(&format!("{callee}=>{site},"));
                }
                let id = e.fallible("rt::closures::object(body)");
                let launch = |body: &str, name: &str, options: &str| {
                    format!(
                        "{{let body={body};let object={id};let site=match object{{{cases}_=>unreachable!()}};let entry=match object{{{entries}_=>unreachable!()}};let name=if {name}==0{{None}}else{{Some(rt::strings::Str::from_raw({name}).as_str(&rt::strings::Cx::current()).to_owned())}};rt::tasks::spawn({},body,site,entry,{options},name)}}",
                        e.plan.future_descriptor().unwrap()
                    )
                };
                let fast = ephemeral_spawn(e, plan).map(|local| {
                    super::scalar_tasks::launch(
                        e,
                        local,
                        launch(
                            &format!("fast_plan_body{local}"),
                            &format!("fast_plan_name{local}"),
                            "rt::tasks::SpawnOptions::default()",
                        ),
                    )
                });
                let call = format!(
                    "{{let plan={value};let body={body};let object={id};let site=match object{{{cases}_=>unreachable!()}};let entry=match object{{{entries}_=>unreachable!()}};let name=if {name}==0{{None}}else{{Some(rt::strings::Str::from_raw({name}).as_str(&rt::strings::Cx::current()).to_owned())}};rt::tasks::spawn({},body,site,entry,rt::tasks::spawn_options({detached}!=0,rt::tasks::pointer_list({limits}),rt::tasks::pointer_list({tokens})),name)}}",
                    e.plan.future_descriptor().unwrap()
                );
                let call = fast.map_or(call.clone(), |fast| {
                    format!("if OBSERVE{{{call}}}else{{{fast}}}")
                });
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
                        e.fallible(
                            &super::scalar_tasks::await_value(e, future)?
                                .unwrap_or_else(|| format!("rt::tasks::await_future({value})")),
                        ),
                        Repr::Tagged,
                    ),
                )
            })()),
            Terminator::AwaitAny {
                futures,
                destination,
                ..
            } => Some((|| {
                let (list, Repr::ListFuture(_)) = e.operand(futures)? else {
                    return Err(no("await_any future list"));
                };
                let call = format!(
                    "{{let list=rt::lists::List::<rt::tasks::FutureSlot>::from_raw({list});let cx=rt::strings::Cx::current();let pointers=(0..list.len(&cx)).map(|i|list.get(&cx,i).map(|f|f.as_raw())).collect::<R<Vec<_>>>()?;rt::tasks::await_any(&pointers)}}"
                );
                e.assign(destination, (e.fallible(&call), Repr::Int))
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
                    if name.starts_with("baml.spawn.CancelToken.")
                        || name.starts_with("baml.spawn.Limit.")
                    {
                        let operation = name.rsplit('.').next().unwrap();
                        let values = args[*ntypeargs..]
                            .iter()
                            .map(|a| e.operand(a))
                            .collect::<Gen<Vec<_>>>()?;
                        let result = if operation == "new" || operation == "any" {
                            let Repr::Ptr(class) = e.place(destination)?.1 else {
                                return Err(no("task resource storage"));
                            };
                            let desc = e.descriptors[&class];
                            if name.starts_with("baml.spawn.Limit.") {
                                (
                                    e.fallible(&format!(
                                        "rt::tasks::limit_new({desc},{})",
                                        values[0].0
                                    )),
                                    Repr::Ptr(class),
                                )
                            } else {
                                let inputs = if operation == "any" {
                                    format!("rt::tasks::pointer_list({})", values[0].0)
                                } else {
                                    "&[]".into()
                                };
                                (
                                    e.fallible(&format!("rt::tasks::token_new({desc},{inputs})")),
                                    Repr::Ptr(class),
                                )
                            }
                        } else {
                            let receiver = &values[0].0;
                            match operation {
                                "cancel" => {
                                    (format!("rt::tasks::token_cancel({receiver})"), Repr::Int)
                                }
                                "is_cancelled" => (
                                    format!("rt::tasks::token_is_cancelled({receiver})"),
                                    Repr::Bool,
                                ),
                                "active_count" => {
                                    (format!("rt::tasks::limit_counts({receiver}).0"), Repr::Int)
                                }
                                "queued_count" => {
                                    (format!("rt::tasks::limit_counts({receiver}).1"), Repr::Int)
                                }
                                _ => return Err(no("task resource operation")),
                            }
                        };
                        return e.assign(destination, result);
                    }
                    if name == "baml._cleanup_begin" {
                        let receiver = e.operand(&args[*ntypeargs])?;
                        let receiver = if matches!(receiver.1, Repr::Tagged | Repr::Dyn(_)) {
                            format!("({}).payload as usize", receiver.0)
                        } else {
                            receiver.0
                        };
                        return e.assign(
                            destination,
                            (format!("rt::tasks::cleanup_begin({receiver})"), Repr::Bool),
                        );
                    }
                    let receiver = e.operand(&args[*ntypeargs])?.0;
                    let operation = name.rsplit('.').next().unwrap();
                    let expression = match operation {
                        "cancel" => (format!("rt::tasks::cancel({receiver})"), Repr::Bool),
                        "is_settled" => (format!("rt::tasks::state({receiver})!=0"), Repr::Bool),
                        "is_result" => (format!("rt::tasks::state({receiver})==1"), Repr::Bool),
                        "is_error" => (format!("rt::tasks::state({receiver})==2"), Repr::Bool),
                        "is_cancelled" => (format!("rt::tasks::state({receiver})==3"), Repr::Bool),
                        "state" => (
                            format!("rt::tasks::state({receiver}) as usize"),
                            e.place(destination)?.1,
                        ),
                        _ => return Err(no(format!("future operation {operation}"))),
                    };
                    e.assign(destination, expression)
                })())
            }
            _ => None,
        }
    }
}
