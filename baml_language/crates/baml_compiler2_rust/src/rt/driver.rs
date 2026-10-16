use super::{
    BTreeMap, BTreeSet, Db, Emitter, Gen, MirFunctionKind, Object, OptLevel, Plan, Program,
    ProgramEmission, ProgramIndex, SourceRoot, admission, builtins, host, json, layout, no,
    recursive, telemetry,
};
use std::fmt::Write as _;

/// Emit a single parameterless integer entry, including a compatible benchmark host.
/// Layouts and direct call bodies come exclusively from MIR and linked metadata.
pub fn compile_rt(
    db: &dyn Db,
    root: SourceRoot,
    program: &Program,
    opt: OptLevel,
    entry: usize,
) -> Gen<String> {
    compile_entry(db, root, program, opt, entry, false, None)
}

/// Compile one zero-argument test body and its reachable closure, without a VM.
/// Unrelated test registrations and top-level initializers are not roots.
pub fn compile_rt_test(
    db: &dyn Db,
    root: SourceRoot,
    program: &Program,
    opt: OptLevel,
    entry: usize,
) -> Gen<String> {
    RtTestCompiler::new(db, root, program, opt).compile(entry)
}

fn compile_entry(
    db: &dyn Db,
    root: SourceRoot,
    program: &Program,
    opt: OptLevel,
    entry: usize,
    test_entry: bool,
    prepared: Option<&RtTestCompiler<'_, '_>>,
) -> Gen<String> {
    let mut plan = Plan {
        lang: baml_compiler2_hir::package::lang_roots(db),
        index: prepared.map_or_else(
            || std::rc::Rc::new(ProgramIndex::new(db, root, program)),
            |p| p.index.clone(),
        ),
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
    } = admission::collect(
        db,
        root,
        opt,
        entry,
        test_entry,
        &mut plan,
        prepared
            .map(|p| p.all.as_ref().map_err(Clone::clone))
            .transpose()?,
    )?;
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
    if test_entry {
        write!(&mut out, "fn entry()->R<i64>{{unsafe{{f{entry}(0)?;Ok(0)}}}}\npub fn run(name:&str)->std::process::ExitCode{{{}rt::test_host::run(DESCRIPTORS,entry,TELEMETRY_FUNCTIONS,TELEMETRY_SITES,name)}}\n", if builtins.is_empty() { "" } else { "init_builtin_observation();" }).unwrap();
    } else {
        host::emit(&mut out, entry, &builtins, format_site);
    }
    Ok(out)
}

/// Shared immutable lookup/lowering work; each leaf still has its own admission
/// walk, layouts and emitted program. No admission result is shared across roots.
pub struct RtTestCompiler<'db, 'program> {
    db: &'db dyn Db,
    root: SourceRoot,
    program: &'program Program,
    opt: OptLevel,
    index: std::rc::Rc<ProgramIndex<'program>>,
    all: admission::Prepared<'db>,
}
impl<'db, 'program> RtTestCompiler<'db, 'program> {
    pub fn new(
        db: &'db dyn Db,
        root: SourceRoot,
        program: &'program Program,
        opt: OptLevel,
    ) -> Self {
        let index = std::rc::Rc::new(ProgramIndex::new(db, root, program));
        let all = admission::prepare_tests(db, root, opt, &index);
        Self {
            db,
            root,
            program,
            opt,
            index,
            all,
        }
    }
    pub fn compile(&self, entry: usize) -> Gen<String> {
        compile_entry(
            self.db,
            self.root,
            self.program,
            self.opt,
            entry,
            true,
            Some(self),
        )
    }
}
