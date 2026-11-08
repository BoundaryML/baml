use super::{
    BTreeMap, BTreeSet, Db, Gen, MirFunction, MirFunctionKind, OptLevel, Plan, Repr, RuntimeTy,
    Rvalue, SourceRoot, StatementKind, Terminator, VecDeque, file_functions,
    is_required_interface_method, lower_function, no, registry, world_files,
};

pub(super) struct Admitted<'db> {
    pub all: BTreeMap<usize, MirFunction<'db>>,
    pub reached: BTreeSet<usize>,
    pub calls: BTreeMap<usize, BTreeSet<usize>>,
    pub builtins: BTreeSet<String>,
    pub json_decode: Option<usize>,
}
pub(super) fn collect<'db>(
    db: &'db dyn Db,
    root: SourceRoot,
    opt: OptLevel,
    entry: usize,
    plan: &mut Plan<'_>,
) -> Gen<Admitted<'db>> {
    let mut all = BTreeMap::new();
    let initializer_roots = super::globals::prepare(db, root, opt, plan, &mut all)?;
    for &file in world_files(db, root) {
        for &f in file_functions(db, file) {
            if is_required_interface_method(db, f) {
                continue;
            }
            let mir = lower_function(db, f, opt)
                .as_ref()
                .map_err(|e| no(format!("MIR: {e:?}")))?;
            if let Some(object) = plan.index.declared(db, f) {
                all.insert(object, MirFunction::from(mir));
            }
        }
    }
    let mut queue = VecDeque::from([entry]);
    queue.extend(initializer_roots);
    queue.extend(registry::prepare(db, root, plan, &mut all)?);
    super::generics::expand(db, plan, &mut all, queue.iter().copied().collect())?;
    plan.lambda_bodies.clear();
    for (&object, mir) in &all {
        if let MirFunctionKind::Bytecode(body) = &mir.kind {
            plan.lambda_bodies.insert(body as *const _ as usize, object);
        }
    }
    let mut reached = BTreeSet::new();
    let mut calls: BTreeMap<usize, BTreeSet<usize>> = BTreeMap::new();
    let mut builtins = BTreeSet::new();
    let mut json_decode = None;
    while let Some(object) = queue.pop_front() {
        if !reached.insert(object) {
            continue;
        }
        let mir = all
            .get(&object)
            .ok_or_else(|| no(format!("missing MIR for {}", plan.index.display(object))))?;
        let MirFunctionKind::Bytecode(body) = &mir.kind else {
            return Err(no(plan.index.display(object)));
        };
        registry::admit(registry::Admission::Function(mir))?;
        for target in super::closures::discover(db, plan, object, mir)? {
            calls.entry(object).or_default().insert(target);
            queue.push_back(target);
        }
        if body.locals.len() > 1024 {
            return Err(no("more than 1024 locals in one body"));
        }
        for statement in body.blocks.iter().flat_map(|b| &b.statements) {
            registry::admit(registry::Admission::Statement(&statement.kind))?;
            if let StatementKind::Assign {
                value:
                    Rvalue::Aggregate {
                        kind:
                            super::AggregateKind::Class {
                                class,
                                type_arg_templates,
                            },
                        ..
                    },
                ..
            } = &statement.kind
            {
                let ty = RuntimeTy::Class(
                    baml_compiler2_hir_ty::layout::class_head(db, *class),
                    type_arg_templates
                        .iter()
                        .map(|t| t.substitute_symbolic(&[]))
                        .collect(),
                );
                plan.record_ty(&ty)
                    .map_err(|e| format!("{e} (constructor in {})", plan.index.display(object)))?;
            }
        }
        super::json::register_decoders(db, plan, body)?;
        let iters = crate::analysis::array_iterators(db, body, mir.arity);
        for (i, local) in body.locals.iter().enumerate() {
            registry::admit(registry::Admission::CapturedLocal(local.is_captured))?;
            plan.environments |= local.is_captured;
            if iters.contains(&i) {
                continue;
            }
            let repr = plan.local_repr(body, i, 0)?;
            if !matches!(repr, Repr::Type(_)) {
                plan.record_ty(&local.ty).map_err(|e| {
                    format!(
                        "{e} ({} local {i}: {:?})",
                        plan.index.display(object),
                        local.ty
                    )
                })?;
            }
            plan.record(repr)?;
        }
        for block in &body.blocks {
            registry::admit(registry::Admission::ShieldedBlock(block.shielded))?;
            if let Some(targets) = plan.dispatches.get(&(object, block.id.0)).cloned() {
                for (repr, target) in targets {
                    plan.record(repr)?;
                    calls.entry(object).or_default().insert(target);
                    queue.push_back(target);
                }
                continue;
            }
            if let Some(&target) = plan.specialized_calls.get(&(object, block.id.0)) {
                calls.entry(object).or_default().insert(target);
                queue.push_back(target);
                continue;
            }
            if let Some(term) = &block.terminator
                && (crate::analysis::array_iter_walk(db, body, mir.arity, term, &iters).is_some()
                    || crate::analysis::array_next_call(db, term, &iters).is_some())
            {
                continue;
            }
            match &block.terminator {
                Some(
                    Terminator::Goto { .. }
                    | Terminator::Branch { .. }
                    | Terminator::ShortCircuit { .. }
                    | Terminator::Return
                    | Terminator::Throw { .. }
                    | Terminator::Rethrow { .. },
                ) => {}
                other => {
                    let Some(result) = registry::reach(db, plan, other.as_ref()) else {
                        return Err(no(format!("terminator {other:?}")));
                    };
                    let reach = result?;
                    if let Some(name) = reach.builtin {
                        builtins.insert(name);
                    }
                    if let Some(callee) = reach.json_decode {
                        json_decode = Some(callee);
                    }
                    for target in reach.targets {
                        calls.entry(object).or_default().insert(target);
                        queue.push_back(target);
                    }
                }
            }
        }
    }
    // Dynamic call cycles need real native frames and the stack guard too.
    // Every materialized callable has already been reached by discover().
    for &object in &reached {
        if let MirFunctionKind::Bytecode(body) = &all[&object].kind
            && body.blocks.iter().any(|b| {
                matches!(
                    b.terminator,
                    Some(Terminator::Call {
                        callee: baml_compiler2_mir::Operand::Copy(_)
                            | baml_compiler2_mir::Operand::Move(_),
                        ..
                    })
                )
            })
        {
            calls.entry(object).or_default().extend(&plan.callables);
        }
    }
    let mir = &all[&entry];
    let MirFunctionKind::Bytecode(body) = &mir.kind else {
        unreachable!()
    };
    if mir.arity != 0 || plan.repr(&body.locals[0].ty)? != Repr::Int {
        return Err(no("entry must have no parameters and return int"));
    }
    Ok(Admitted {
        all,
        reached,
        calls,
        builtins,
        json_decode,
    })
}
