use super::{
    BTreeMap, BTreeSet, Db, Gen, MirFunction, MirFunctionKind, OptLevel, Plan, Repr, RuntimeTy,
    Rvalue, SourceRoot, StatementKind, Terminator, VecDeque, file_functions, file_lets,
    is_required_interface_method, lower_function, no, registry, world_files,
};

pub(super) struct Admitted<'db> {
    pub all: BTreeMap<usize, std::borrow::Cow<'db, MirFunction<'db>>>,
    pub reached: BTreeSet<usize>,
    pub calls: BTreeMap<usize, BTreeSet<usize>>,
    pub builtins: BTreeSet<String>,
    pub json_decode: Option<usize>,
}

// These adapters need the iterator/list job's closed-world specialization.
// Reject them before expanding their recursive interface candidates; otherwise
// the corpus watchdog sees a compiler stall instead of an admission blocker.
pub(super) fn before_specialization(plan: &Plan<'_>, body: &super::MirFunctionBody<'_>) -> Gen<()> {
    for local in &body.locals {
        let mut adapter = false;
        local.ty.visit_heads(&mut |head| {
            adapter |= Some(head.root()) == plan.lang.get(baml_base::LangPackage::Baml)
                && head.namespace().as_slice() == ["iter"]
                && matches!(
                    head.name().as_str(),
                    "Map"
                        | "Filter"
                        | "FilterMap"
                        | "FlatMap"
                        | "Flatten"
                        | "Peekable"
                        | "StepBy"
                        | "Chain"
                        | "Take"
                        | "Skip"
                        | "TakeWhile"
                        | "SkipWhile"
                );
        });
        if adapter {
            return Err(no("lazy iterator adapters"));
        }
    }
    Ok(())
}
pub(super) fn collect<'db>(
    db: &'db dyn Db,
    root: SourceRoot,
    opt: OptLevel,
    entry: usize,
    benchmark: Option<(usize, usize)>,
    test_entry: bool,
    plan: &mut Plan<'_>,
    prepared: Option<&BTreeMap<usize, std::borrow::Cow<'db, MirFunction<'db>>>>,
) -> Gen<Admitted<'db>> {
    let mut all = prepared.cloned().unwrap_or_default();
    if prepared.is_none() {
        for &file in world_files(db, root) {
            if !test_entry {
                registry::admit(registry::Admission::Initializers(
                    file.source_root(db).kind(db) != baml_base::SourceRootKind::Stdlib
                        && !file_lets(db, file).is_empty(),
                ))?;
            }
            for &f in file_functions(db, file) {
                if is_required_interface_method(db, f) {
                    continue;
                }
                let mir = lower_function(db, f, opt)
                    .as_ref()
                    .map_err(|e| no(format!("MIR: {e:?}")))?;
                if let Some(object) = plan.index.declared(db, f) {
                    all.insert(object, std::borrow::Cow::Borrowed(mir));
                }
            }
        }
    }
    let mut queue = VecDeque::from([entry]);
    if let Some((setup, run)) = benchmark {
        queue.extend([setup, run]);
    }
    queue.extend(registry::prepare(db, root, plan, &mut all)?);
    super::generics::expand(db, plan, &mut all, queue.iter().copied().collect())?;
    queue.extend(plan.string_overrides.values().copied());
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
        super::exceptions::prepare_contexts(db, plan, body)?;
        if body.locals.len() > 1024 {
            return Err(no("more than 1024 locals in one body"));
        }
        for statement in body.blocks.iter().flat_map(|b| &b.statements) {
            registry::admit(registry::Admission::Statement(&statement.kind))?;
            if let StatementKind::Assign {
                value: Rvalue::IsType { test, .. },
                ..
            } = &statement.kind
            {
                super::exceptions::record_test(db, plan, test)?;
            }

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
        // A generic type argument is not a JSON schema request. In particular
        // generic class heads may occur as type values without field bindings.
        // Plan schemas only for the concrete argument to decode itself.
        for block in &body.blocks {
            if let Some(Terminator::Call {
                callee: super::Operand::Constant(super::Constant::Function(f)),
                args,
                ..
            }) = &block.terminator
            {
                if plan
                    .index
                    .function(db, *f)
                    .is_some_and(|(id, _)| plan.index.display(id) == "baml.json.decode")
                {
                    let ty = super::generics::type_operand(body, &args[0])?;
                    let template = baml_compiler2_mir::TyTemplate::from(
                        baml_compiler2_mir::RealizedTy::try_from(ty)
                            .map_err(|_| no("non-concrete JSON argument"))?,
                    );
                    let classes = plan.json_class_types(&template)?;
                    for &c in &classes {
                        plan.add(c)?;
                    }
                    if !plan.json_types.contains(&classes) {
                        plan.json_types.push(classes);
                    }
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
            if let Some(Terminator::NarrowBind { test, .. }) = &block.terminator {
                super::exceptions::record_test(db, plan, test)?;
            }
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
                    let reach = result.map_err(|error| {
                        format!(
                            "{error} (reaching {} block {})",
                            plan.index.display(object),
                            block.id.0
                        )
                    })?;
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
    let mir = &all[&entry];
    let MirFunctionKind::Bytecode(body) = &mir.kind else {
        unreachable!()
    };
    if mir.arity != 0
        || plan.repr(&body.locals[0].ty)? != if test_entry { Repr::Null } else { Repr::Int }
    {
        return Err(no(if test_entry {
            "test entry must have no parameters and return null"
        } else {
            "entry must have no parameters and return int"
        }));
    }
    Ok(Admitted {
        all,
        reached,
        calls,
        builtins,
        json_decode,
    })
}

fn insert_lambdas<'db>(
    db: &'db dyn Db,
    index: &crate::index::ProgramIndex<'_>,
    mir: &'db MirFunction<'db>,
    all: &mut BTreeMap<usize, std::borrow::Cow<'db, MirFunction<'db>>>,
) {
    for lambda in &mir.lambdas {
        if let Some(object) = index.lambda(db, &lambda.identity) {
            all.insert(object, std::borrow::Cow::Borrowed(lambda));
        }
        insert_lambdas(db, index, lambda, all);
    }
}

pub(super) type Prepared<'db> = Gen<BTreeMap<usize, std::borrow::Cow<'db, MirFunction<'db>>>>;

pub(super) fn prepare_tests<'db>(
    db: &'db dyn Db,
    root: SourceRoot,
    opt: OptLevel,
    index: &crate::index::ProgramIndex<'_>,
) -> Prepared<'db> {
    let mut all = BTreeMap::new();
    for &file in world_files(db, root) {
        for &function in file_functions(db, file) {
            if is_required_interface_method(db, function) {
                continue;
            }
            let mir = lower_function(db, function, opt)
                .as_ref()
                .map_err(|e| no(format!("MIR: {e:?}")))?;
            if let Some(object) = index.declared(db, function) {
                all.insert(object, std::borrow::Cow::Borrowed(mir));
                insert_lambdas(db, index, mir, &mut all);
            }
        }
    }
    Ok(all)
}
