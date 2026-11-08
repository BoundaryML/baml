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
    let mut plan = Plan {
        root,
        lang: baml_compiler2_hir::package::lang_roots(db),
        index: ProgramIndex::new(db, root, program),
        program,
        layouts: BTreeMap::new(),
        lists: BTreeSet::new(),
        list_elements: std::cell::RefCell::new(Vec::new()),
        json_types: Vec::new(),
        list_stringify: false,
        list_render_roots: std::cell::RefCell::new(BTreeSet::new()),
        json_decode_codes: BTreeSet::new(),
        json_list_types: Vec::new(),
        enums: BTreeSet::new(),
        specialized_calls: BTreeMap::new(),
        type_codes: Vec::new(),
        next_object: program.objects.len(),
        class_instances: Vec::new(),
        concrete_types: Vec::new(),
        heads: BTreeMap::new(),
        dispatches: BTreeMap::new(),
        function_frames: BTreeMap::new(),
        alias_tests: std::cell::RefCell::new(BTreeSet::new()),
        environments: false,
        captures: BTreeMap::new(),
        lambda_bodies: BTreeMap::new(),
        callables: BTreeSet::new(),
        closure_lists: BTreeMap::new(),
    };
    let admission::Admitted {
        all,
        reached,
        calls,
        builtins,
        json_decode,
    } = admission::collect(db, root, opt, entry, &mut plan)?;
    let mut out = String::from(
        "#![allow(unused_mut,unused_variables,unused_assignments,unused_labels,dead_code,non_snake_case)]\nuse baml_rt::{self as rt, R};\nuse baml_rt_gc::{Descriptor,Kind,Field,SlotKind};\n",
    );
    builtins::emit(&mut out, &builtins)?;
    let layout::LayoutIds { descriptors, lists } = layout::emit(&mut out, &plan)?;
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
    let telemetry_sites = std::cell::RefCell::new(vec![(None, 0usize, entry)]);
    let mut placements = reached
        .iter()
        .map(|&object| {
            let mir = &all[&object];
            let MirFunctionKind::Bytecode(body) = &mir.kind else {
                unreachable!()
            };
            (
                object,
                super::stack::Placement::new(db, root, body, mir.arity, opt, &plan, &all),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let stack_leaves = placements
        .values()
        .flat_map(|p| p.leaf_calls.iter().copied())
        .collect::<BTreeSet<_>>();
    // Inlined bodies share one physical frame. Keep their combined object
    // storage below the stack guard's 64 KiB reserve, including ordinary locals.
    let small_program =
        small_program && placements.values().map(|p| p.bytes(&plan)).sum::<usize>() <= 16 * 1024;
    for &object in &reached {
        let mir = &all[&object];
        let MirFunctionKind::Bytecode(body) = &mir.kind else {
            unreachable!()
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
        };
        let stack_leaf = stack_leaves.contains(&object);
        out.push_str(&emitter.function(
            mir.arity,
            (small_program || stack_leaf) && !recursive(&calls, object),
            stack_leaf,
        )?);
    }
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
                proven_arithmetic: BTreeSet::new(),
                stack: super::stack::Placement::default(),
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
                unwind_jumps: Vec::new(),
                sites: Vec::new(),
                compact_sites: Vec::new(),
                raise_kinds: Vec::new(),
                telemetry_sites: &telemetry_sites,
                site: 0,
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
    let format_site = telemetry::emit(
        &mut out,
        &cx,
        &reached,
        json_decode,
        (stringify, aliased),
        &telemetry_sites,
    )?;
    host::emit(&mut out, entry, &builtins, format_site);
    Ok(out)
}
