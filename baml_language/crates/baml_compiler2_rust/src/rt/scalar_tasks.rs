//! Eliminate an unobservable temporary task only in an otherwise idle loop.
//! The body must return a raw scalar constant, carry no cells, and its future
//! must flow through anonymous copies straight to await. Observation always
//! executes the ordinary plan, scheduler entry and closure adapter.
use super::*;
struct Pair<'db> {
    body: usize,
    plan: usize,
    awaited: usize,
    value: Constant<'db>,
}
// A temporary is eligible only when every read belongs to the one linear
// producer/copy/await chain. Named locals and projected uses never qualify.
fn reads(e: &Emitter<'_, '_>, local: usize) -> usize {
    use baml_compiler2_mir::memory::{place_reads, walk_operand_places, walk_rvalue_places};
    let mut count = 0;
    let mut place = |p: &Place| {
        let mut resources = baml_compiler2_mir::memory::Resources::default();
        place_reads(p, &mut resources);
        count += usize::from(resources.locals.iter().any(|l| l.0 == local));
    };
    for block in &e.body.blocks {
        for statement in &block.statements {
            match &statement.kind {
                StatementKind::Assign { destination, value } => {
                    walk_rvalue_places(value, &mut place);
                    if !matches!(destination, Place::Local(_)) {
                        place(destination);
                    }
                }
                StatementKind::Drop(p) => place(p),
                other => crate::operands::walk_statement_operands(other, &mut |o| {
                    walk_operand_places(o, &mut place)
                }),
            }
        }
        if let Some(term) = &block.terminator {
            crate::operands::walk_terminator_operands(term, &mut |o| {
                walk_operand_places(o, &mut place)
            });
            if let Terminator::Await { future, .. } = term {
                place(future);
            }
        }
    }
    count
}
fn pairs<'db>(e: &Emitter<'_, 'db>) -> Vec<Pair<'db>> {
    let mut pairs = Vec::new();
    for block in &e.body.blocks {
        let Some(
            term @ Terminator::Call {
                args,
                ntypeargs,
                target,
                ..
            },
        ) = &block.terminator
        else {
            continue;
        };
        let Some(plan) = super::tasks::ephemeral_plan(e, term) else {
            continue;
        };
        let Some(Operand::Copy(Place::Local(body)) | Operand::Move(Place::Local(body))) =
            args.get(*ntypeargs)
        else {
            continue;
        };
        let definitions = e
            .body
            .blocks
            .iter()
            .flat_map(|b| &b.statements)
            .filter_map(|s| match &s.kind {
                StatementKind::Assign {
                    destination: Place::Local(l),
                    value,
                } if l == body => Some(value),
                _ => None,
            })
            .collect::<Vec<_>>();
        let [
            Rvalue::MakeClosure {
                lambda_idx,
                captures,
                ..
            },
        ] = definitions.as_slice()
        else {
            continue;
        };
        // No call or yield may occur after the idle-loop decision and before
        // spawn. Only the current block's closure and raw constant stores can
        // separate that decision from Plan.new.
        let Some(position) = block.statements.iter().position(|s| matches!(&s.kind,
            StatementKind::Assign { destination: Place::Local(l), value: Rvalue::MakeClosure { .. } } if l == body)) else { continue; };
        if block.statements[position + 1..].iter().any(|s| {
            !matches!(
                &s.kind,
                StatementKind::Nop
                    | StatementKind::Assign {
                        destination: Place::Local(_),
                        value: Rvalue::LoadType(_)
                    }
                    | StatementKind::Assign {
                        destination: Place::Local(_),
                        value: Rvalue::Use(Operand::Constant(
                            Constant::Int(_)
                                | Constant::Bool(_)
                                | Constant::Float(_)
                                | Constant::Null
                                | Constant::OmittedArg
                        ))
                    }
            )
        }) {
            continue;
        }
        if !captures.is_empty()
            || e.body.locals[body.0].name.is_some()
            || reads(e, body.0) != 1
            || reads(e, plan) != 1
        {
            continue;
        }
        let lambda = &e.all[&e.object].lambdas[*lambda_idx];
        let MirFunctionKind::Bytecode(inner) = &lambda.kind else {
            continue;
        };
        if lambda.arity != 0 {
            continue;
        }
        let mut cursor = inner.entry.0;
        let mut seen = BTreeSet::new();
        let mut value = None;
        let valid = loop {
            if !seen.insert(cursor) {
                break false;
            }
            let leaf = &inner.blocks[cursor];
            let mut valid = true;
            for statement in &leaf.statements {
                match &statement.kind {
                    StatementKind::Nop => {}
                    StatementKind::Assign {
                        destination: Place::Local(result),
                        value: Rvalue::Use(Operand::Constant(constant)),
                    } if result.0 == 0
                        && value.is_none()
                        && matches!(
                            constant,
                            Constant::Int(_)
                                | Constant::Bool(_)
                                | Constant::Float(_)
                                | Constant::Null
                        ) =>
                    {
                        value = Some(constant.clone());
                    }
                    _ => {
                        valid = false;
                        break;
                    }
                }
            }
            if !valid {
                break false;
            }
            match &leaf.terminator {
                Some(Terminator::Return) => break value.is_some(),
                Some(Terminator::Goto { target }) => cursor = target.0,
                _ => break false,
            }
        };
        if !valid {
            continue;
        }
        let value = value.unwrap();
        let Some(Terminator::Spawn {
            future: Place::Local(future),
            resume,
            ..
        }) = &e.body.blocks[target.0].terminator
        else {
            continue;
        };
        if e.body.locals[future.0].name.is_some() || e.body.locals[future.0].is_captured {
            continue;
        }
        let next = &e.body.blocks[resume.0];
        let mut aliases = BTreeSet::from([future.0]);
        let mut valid = true;
        for statement in &next.statements {
            match &statement.kind {
                StatementKind::Assign {
                    destination: Place::Local(dst),
                    value:
                        Rvalue::Use(
                            Operand::Copy(Place::Local(from)) | Operand::Move(Place::Local(from)),
                        ),
                } if aliases.contains(&from.0)
                    && e.body.locals[dst.0].name.is_none()
                    && !e.body.locals[dst.0].is_captured =>
                {
                    aliases.insert(dst.0);
                }
                StatementKind::Nop => {}
                _ => {
                    valid = false;
                    break;
                }
            }
        }
        if !valid {
            continue;
        }
        if let Some(Terminator::Await {
            future: Place::Local(awaited),
            ..
        }) = &next.terminator
        {
            if aliases.contains(&awaited.0) && aliases.iter().all(|l| reads(e, *l) == 1) {
                pairs.push(Pair {
                    body: body.0,
                    plan,
                    awaited: awaited.0,
                    value,
                });
            }
        }
    }
    pairs
}
pub(super) fn closure(e: &Emitter<'_, '_>, ordinary: String) -> String {
    let Some(StatementKind::Assign {
        destination: Place::Local(local),
        ..
    }) = e.body.blocks[e.block]
        .statements
        .get(e.statement)
        .map(|s| &s.kind)
    else {
        return ordinary;
    };
    if pairs(e).iter().any(|p| p.body == local.0) {
        format!("if !OBSERVE && rt::tasks::can_elide_scalar_task(){{0usize}}else{{{ordinary}}}")
    } else {
        ordinary
    }
}
pub(super) fn launch(e: &Emitter<'_, '_>, plan: usize, ordinary: String) -> String {
    if pairs(e).iter().any(|p| p.plan == plan) {
        format!("if fast_plan_body{plan}==0{{Ok(rt::tasks::SCALAR_FUTURE)}}else{{{ordinary}}}")
    } else {
        ordinary
    }
}
pub(super) fn await_value(e: &Emitter<'_, '_>, future: &Place) -> Gen<Option<String>> {
    let Place::Local(local) = future else {
        return Ok(None);
    };
    let Some(pair) = pairs(e).into_iter().find(|p| p.awaited == local.0) else {
        return Ok(None);
    };
    let value = e.coerce(e.operand(&Operand::Constant(pair.value))?, Repr::Tagged)?;
    let pointer = e.place(future)?.0;
    Ok(Some(format!("rt::tasks::await_scalar({pointer},{value})")))
}
