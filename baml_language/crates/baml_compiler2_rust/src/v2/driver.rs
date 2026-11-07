use super::{
    BTreeMap, BTreeSet, Db, Emitter, Gen, MirFunctionKind, Object, OptLevel, Plan, Program,
    ProgramEmission, ProgramIndex, SourceRoot, admission, builtins, host, json, layout, no,
    recursive, telemetry,
};

/// Emit a single parameterless integer entry, including a compatible benchmark host.
/// Layouts and direct call bodies come exclusively from MIR and linked metadata.
pub fn compile_v2(
    db: &dyn Db,
    root: SourceRoot,
    program: &Program,
    opt: OptLevel,
    entry: usize,
) -> Gen<String> {
    let mut plan = Plan {
        lang: baml_compiler2_hir::package::lang_roots(db),
        index: ProgramIndex::new(db, root, program),
        program,
        layouts: BTreeMap::new(),
        lists: BTreeSet::new(),
        json_types: Vec::new(),
    };
    let admission::Admitted {
        all,
        reached,
        calls,
        builtins,
        json_decode,
    } = admission::collect(db, root, opt, entry, &mut plan)?;
    let mut out = String::from(
        "#![allow(unused_mut,unused_variables,unused_assignments,unused_labels,dead_code,non_snake_case)]\nuse bex_v2_runtime::{self as rt, R};\nuse bex_v2_heap::{Descriptor,Kind,Field,SlotKind};\n",
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
    json::emit(&mut out, &cx, json_decode, alias_function)?;
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
    for &object in &reached {
        let mir = all[&object];
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
            unwind_jumps: Vec::new(),
            sites: Vec::new(),
            compact_sites: Vec::new(),
            raise_kinds: Vec::new(),
            telemetry_sites: &telemetry_sites,
            site: 0,
        };
        out.push_str(&emitter.function(mir.arity, small_program && !recursive(&calls, object))?);
    }
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
