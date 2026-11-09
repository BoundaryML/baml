use super::{
    BTreeMap, BTreeSet, Db, Emitter, Gen, MirFunctionKind, Object, OptLevel, Plan, Program,
    ProgramEmission, ProgramIndex, SourceRoot, admission, builtins, host, json, json_lists, layout,
    no, recursive, telemetry,
};

/// Emit a single parameterless integer entry, including a compatible benchmark host.
/// Layouts and direct call bodies come exclusively from MIR and linked metadata.
pub fn compile_rt(
    db: &dyn Db,
    root: SourceRoot,
    program: &Program,
    opt: OptLevel,
    entry: usize,
) -> Gen<String> {
    compile(db, root, program, opt, entry, None)
}

/// Compile the normal entry plus an explicit setup/run native benchmark host.
pub fn compile_rt_benchmark(
    db: &dyn Db,
    root: SourceRoot,
    program: &Program,
    opt: OptLevel,
    entry: usize,
    setup_run: (usize, usize),
) -> Gen<String> {
    compile(db, root, program, opt, entry, Some(setup_run))
}

fn compile(
    db: &dyn Db,
    root: SourceRoot,
    program: &Program,
    opt: OptLevel,
    entry: usize,
    benchmark: Option<(usize, usize)>,
) -> Gen<String> {
    let mut plan = Plan {
        root,
        lang: baml_compiler2_hir::package::lang_roots(db),
        index: ProgramIndex::new(db, root, program),
        program,
        layouts: BTreeMap::new(),
        bigints: false,
        json_tree: false,
        json_public_to: false,
        json_to_types: Vec::new(),
        lists: BTreeSet::new(),
        list_elements: std::cell::RefCell::new(Vec::new()),
        json_types: Vec::new(),
        environments: false,
        task_support: false,
        cleanup_methods: BTreeMap::new(),
        captures: BTreeMap::new(),
        capture_types: BTreeMap::new(),
        lambda_bodies: BTreeMap::new(),
        lambda_targets: BTreeMap::new(),
        generic_functions: Vec::new(),
        generic_values: BTreeMap::new(),
        virtual_values: BTreeMap::new(),
        callables: BTreeSet::new(),
        closure_lists: BTreeMap::new(),
        enums: BTreeSet::new(),
        specialized_calls: BTreeMap::new(),
        type_codes: Vec::new(),
        next_object: program.objects.len(),
        class_instances: Vec::new(),
        concrete_types: Vec::new(),
        heads: BTreeMap::new(),
        dispatches: BTreeMap::new(),
        function_frames: BTreeMap::new(),
        structural_strings: false,
        structural_string_calls: BTreeSet::new(),
        string_overrides: BTreeMap::new(),
        alias_tests: std::cell::RefCell::new(BTreeSet::new()),
        list_stringify: false,
        list_render_roots: std::cell::RefCell::new(BTreeSet::new()),
        json_decode_codes: BTreeSet::new(),
        json_list_types: Vec::new(),
        map_types: Default::default(),
        json_maps: BTreeMap::new(),
        map_stringify: false,
        globals: BTreeMap::new(),
        literal_ids: std::cell::RefCell::new(BTreeMap::new()),
        init_chains: Vec::new(),
    };
    let admission::Admitted {
        all,
        reached,
        calls,
        builtins,
        json_decode,
    } = admission::collect(db, root, opt, entry, benchmark, &mut plan)?;
    let mut out = String::from(
        "#![allow(unused_mut,unused_variables,unused_assignments,unused_labels,dead_code,non_snake_case)]\nuse baml_rt::{self as rt, R};\nuse baml_rt_gc::{Descriptor,Kind,Field,SlotKind};\n",
    );
    builtins::emit(&mut out, &builtins)?;
    let layout::LayoutIds { descriptors, lists } = layout::emit(&mut out, &plan)?;
    super::regex::emit(&mut out, &plan, &descriptors, &lists, &builtins)?;
    let host_function = |name: &str| {
        program
            .objects
            .iter()
            .enumerate()
            .find_map(|(i, o)| {
                (matches!(o, Object::Function(_)) && plan.index.display(i) == name).then_some(i)
            })
            .ok_or_else(|| no(format!("host function {name}")))
    };
    let stringify = host_function("baml.json.stringify")?;
    let aliased = host_function("baml.json._aliased")?;
    let alias_function = aliased;
    let cx = ProgramEmission {
        db,
        plan: &plan,
        all: &all,
        descriptors: &descriptors,
    };
    json::emit(&mut out, &cx, json_decode, alias_function, &lists)?;
    if plan.list_stringify {
        json_lists::emit_shapes(&mut out, &plan, &descriptors, &lists)?;
    }
    super::json_tree::emit(&mut out, &cx, &lists)?;
    super::map_json::emit(&mut out, &cx, &lists, json_decode, alias_function)?;
    // Bound forced inlining by the same total-local budget as a single body.
    // Recursive SCCs retain real frames and the normal per-entry stack guard.
    let small_program = reached
        .iter()
        .map(|id| match &all[id].kind {
            MirFunctionKind::Bytecode(body) => body.locals.len(),
            MirFunctionKind::Builtin(_) => 0,
        })
        .sum::<usize>()
        <= 1024;
    let MirFunctionKind::Bytecode(entry_body) = &all[&entry].kind else {
        unreachable!()
    };
    let result_repr = plan.repr(&entry_body.locals[0].ty)?;
    let string_result = result_repr == super::Repr::Str;
    let mut host_sites = vec![(None, 0usize, entry)];
    if let Some((setup, run)) = benchmark {
        let MirFunctionKind::Bytecode(setup_body) = &all[&setup].kind else {
            return Err(no("benchmark setup must be a user function"));
        };
        let MirFunctionKind::Bytecode(run_body) = &all[&run].kind else {
            return Err(no("benchmark run must be a user function"));
        };
        if all[&setup].arity != 0
            || all[&run].arity != 1
            || setup_body.locals[0].ty != run_body.locals[1].ty
            || plan.repr(&run_body.locals[0].ty)? != result_repr
        {
            return Err(no(
                "benchmark requires setup() -> S and run(S) matching the entry result",
            ));
        }
        host_sites.extend([(None, 0usize, setup), (None, 0usize, run)]);
    }
    let telemetry_sites = std::cell::RefCell::new(host_sites);
    let mut placements = reached
        .iter()
        .map(|&object| {
            let mir = &all[&object];
            let MirFunctionKind::Bytecode(body) = &mir.kind else {
                return (object, super::stack::Placement::default());
            };
            (
                object,
                super::stack::Placement::new(db, root, object, body, opt, &plan, &all),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let mut stack_leaves = placements
        .values()
        .flat_map(|p| p.leaf_calls.iter().copied())
        .collect::<BTreeSet<_>>();
    // Inlined bodies share one physical frame. Keep their combined object
    // storage below the stack guard's 64 KiB reserve, including ordinary locals.
    let small_program =
        small_program && placements.values().map(|p| p.bytes(&plan)).sum::<usize>() <= 16 * 1024;
    if small_program {
        stack_leaves.extend(super::dispatch::inline_leaves(&plan, &all));
    }
    for &object in &reached {
        let mir = &all[&object];
        let body = match &mir.kind {
            MirFunctionKind::Bytecode(body) => body,
            MirFunctionKind::Builtin(_) => match &all[&entry].kind {
                MirFunctionKind::Bytecode(body) => body,
                _ => unreachable!(),
            },
        };
        let iters = crate::analysis::array_iterators(db, body, mir.arity);
        let iter_arrays = body
            .blocks
            .iter()
            .filter_map(|b| {
                b.terminator
                    .as_ref()
                    .and_then(|t| crate::analysis::array_iter_walk(db, body, mir.arity, t, &iters))
            })
            .collect();
        let mut emitter = Emitter {
            db,
            plan: &plan,
            body,
            object,
            descriptors: &descriptors,
            lists: &lists,
            all: &all,
            iters,
            iter_arrays,
            arity: mir.arity,
            block: 0,
            statement: 0,
            stack: placements.remove(&object).unwrap(),
            proven_arithmetic: BTreeSet::new(),
            unwind_jumps: Vec::new(),
            sites: Vec::new(),
            compact_sites: Vec::new(),
            raise_kinds: Vec::new(),
            telemetry_sites: &telemetry_sites,
            site: 0,
            string_cx: std::cell::Cell::new(false),
            string_literals: std::cell::RefCell::new(BTreeMap::new()),
        };
        if matches!(mir.kind, MirFunctionKind::Builtin(_)) {
            out.push_str(&emitter.native_closure_adapter()?);
            continue;
        }
        let stack_leaf = stack_leaves.contains(&object);
        out.push_str(
            &emitter
                .function(
                    mir.arity,
                    (small_program || stack_leaf) && !recursive(&calls, object),
                    stack_leaf,
                    recursive(&calls, object),
                )
                .map_err(|e| format!("{e} (emitting {})", plan.index.display(object)))?,
        );
    }
    if !plan.cleanup_methods.is_empty() {
        out.push_str("unsafe fn cleanup_dispatch(id:u32,object:usize)->R<()>{unsafe{match id{");
        for (&class, &method) in &plan.cleanup_methods {
            let site = telemetry_sites.borrow().len();
            telemetry_sites.borrow_mut().push((None, 0, method));
            out.push_str(&format!("{class}=>rt::tasks::run_cleanup({site},object,move ||{{let cx=rt::calls::Context::current();if rt::telemetry::active(){{f{method}::<true>(cx,{site},object)?;}}else{{f{method}::<false>(cx,{site},object)?;}}Ok(())}}),"));
        }
        out.push_str("_=>unreachable!(),}}}\n");
    }
    super::callbacks::callback_tables(&mut out, &plan, &telemetry_sites);
    // Membership helpers are mutually recursive for recursive alias groups.
    let mut emitted = BTreeSet::new();
    loop {
        let pending = plan
            .alias_tests
            .borrow()
            .iter()
            .filter(|h| !emitted.contains(*h))
            .cloned()
            .collect::<Vec<_>>();
        if pending.is_empty() {
            break;
        }
        for head in pending {
            let alias = baml_compiler2_hir_ty::interfaces::package_resolved_aliases(db, plan.root)
                .get(&head)
                .ok_or_else(|| no("alias definition missing"))?;
            let runtime = baml_compiler2_mir::RuntimeLowering::of(db, plan.root).convert(alias);
            let template = baml_compiler2_mir::TyTemplate::from(
                baml_compiler2_mir::RealizedTy::try_from(runtime)
                    .map_err(|_| no("alias definition is not concrete"))?,
            );
            let emitter = Emitter {
                db,
                plan: &plan,
                body: match &all[&entry].kind {
                    MirFunctionKind::Bytecode(b) => b,
                    _ => unreachable!(),
                },
                object: entry,
                descriptors: &descriptors,
                lists: &lists,
                all: &all,
                iters: BTreeSet::new(),
                iter_arrays: BTreeMap::new(),
                arity: 0,
                block: 0,
                statement: 0,
                stack: super::stack::Placement::default(),
                proven_arithmetic: BTreeSet::new(),
                unwind_jumps: Vec::new(),
                sites: Vec::new(),
                compact_sites: Vec::new(),
                raise_kinds: Vec::new(),
                telemetry_sites: &telemetry_sites,
                site: 0,
                string_cx: std::cell::Cell::new(false),
                string_literals: std::cell::RefCell::new(BTreeMap::new()),
            };
            let expression =
                emitter.membership(("value".into(), super::Repr::Tagged), &template)?;
            let id = emitter.alias_test_id(&head)?;
            out.push_str(&format!(
                "unsafe fn rt_alias{id}(value:rt::Dyn)->bool{{unsafe{{{expression}}}}}\n"
            ));
            emitted.insert(head);
        }
    }
    super::lists::emit_render_shapes(&mut out, &plan, &descriptors, &lists)?;
    super::globals::emit(&mut out, &plan, &descriptors, &lists, &telemetry_sites)?;
    let format_site = telemetry::emit(
        &mut out,
        &cx,
        &reached,
        json_decode,
        (stringify, aliased),
        &telemetry_sites,
    )?;
    if let Some((setup, run)) = benchmark {
        host::emit_benchmark(
            &mut out,
            entry,
            setup,
            run,
            &builtins,
            format_site,
            !plan.init_chains.is_empty(),
            string_result,
            !plan.cleanup_methods.is_empty(),
        );
    } else {
        host::emit(
            &mut out,
            entry,
            &builtins,
            format_site,
            !plan.init_chains.is_empty(),
            string_result,
            !plan.cleanup_methods.is_empty(),
        );
    }
    Ok(out)
}
