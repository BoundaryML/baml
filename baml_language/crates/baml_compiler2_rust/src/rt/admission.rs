use super::{
    BTreeMap, BTreeSet, Db, Gen, MirFunction, MirFunctionKind, OptLevel, Plan, Repr, Rvalue,
    SourceRoot, StatementKind, Terminator, VecDeque, file_functions, file_lets,
    is_required_interface_method, lower_function, no, registry, world_files,
};

pub(super) struct Admitted<'db> {
    pub all: BTreeMap<usize, &'db MirFunction<'db>>,
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
    for &file in world_files(db, root) {
        registry::admit(registry::Admission::Initializers(
            file.source_root(db).kind(db) != baml_base::SourceRootKind::Stdlib
                && !file_lets(db, file).is_empty(),
        ))?;
        for &f in file_functions(db, file) {
            if is_required_interface_method(db, f) {
                continue;
            }
            let mir = lower_function(db, f, opt)
                .as_ref()
                .map_err(|e| no(format!("MIR: {e:?}")))?;
            if let Some(object) = plan.index.declared(db, f) {
                all.insert(object, mir);
            }
        }
    }
    let mut queue = VecDeque::from([entry]);
    queue.extend(registry::prepare(db, root, plan, &mut all)?);
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
        if body.locals.len() > 1024 {
            return Err(no("more than 1024 locals in one body"));
        }
        for statement in body.blocks.iter().flat_map(|b| &b.statements) {
            registry::admit(registry::Admission::Statement(&statement.kind))?;
            super::builtin_classes::record_test(db, plan, &statement.kind)?;

            if let StatementKind::Assign {
                value:
                    Rvalue::LoadType(
                        ty @ (baml_compiler2_mir::TyTemplate::Union(_)
                        | baml_compiler2_mir::TyTemplate::Class(..)),
                    ),
                ..
            } = &statement.kind
            {
                let classes = plan.json_class_types(ty)?;
                for &c in &classes {
                    plan.add(c)?;
                }
                if !plan.json_types.contains(&classes) {
                    plan.json_types.push(classes);
                }
            }
        }
        let iters = crate::analysis::array_iterators(db, body, mir.arity);
        for (i, local) in body.locals.iter().enumerate() {
            registry::admit(registry::Admission::CapturedLocal(local.is_captured))?;
            if iters.contains(&i) {
                continue;
            }
            let repr = plan.local_repr(body, i, 0)?;
            if !matches!(repr, Repr::Type(_)) {
                plan.record_ty(&local.ty)?;
            }
            plan.record(repr)?;
        }
        for block in &body.blocks {
            registry::admit(registry::Admission::ShieldedBlock(block.shielded))?;
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
    let mir = all[&entry];
    super::host::admit(plan, mir)?;
    Ok(Admitted {
        all,
        reached,
        calls,
        builtins,
        json_decode,
    })
}
