//! MIR census before native admission. Unsupported bodies remain visible.
use crate::{
    index::ProgramIndex,
    reach::{reachable, references},
};
use baml_base::SourceRoot;
use baml_compiler2_hir::{
    item_data::{file_functions, is_required_interface_method},
    package::world_files,
};
use baml_compiler2_mir::{MirFunction, MirFunctionKind, OptLevel, lower_function};
use bex_vm_types::Program;
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    fmt::Write as _,
};

/// Dump the conservative declaration closure of explicit entry points before
/// admission, including builtin leaves and all lowered lambda bodies.
///
/// This is not a monomorphized reachability proof: virtual calls retain every
/// same-named method, as in the native backend. Top-level initializers are not
/// added as roots; callers must supply initializer entries when auditing them.
pub fn inventory_program(
    db: &dyn baml_compiler2_mir::Db,
    root: SourceRoot,
    program: &Program,
    opt: OptLevel,
    entries: &[usize],
) -> Result<String, String> {
    for entry in entries {
        if !matches!(
            program.objects.get(*entry),
            Some(bex_vm_types::Object::Function(_))
        ) {
            return Err(format!(
                "MIR inventory entry {entry} is not a function object"
            ));
        }
    }
    let index = ProgramIndex::new(db, root, program);
    let mut bodies = BTreeMap::new();
    let mut edges = HashMap::new();
    for &file in world_files(db, root) {
        for &function in file_functions(db, file) {
            if is_required_interface_method(db, function) {
                continue;
            }
            let mir = lower_function(db, function, opt)
                .as_ref()
                .map_err(|error| format!("MIR inventory lowering failed: {error:?}"))?;
            if let Some(object) = index.declared(db, function) {
                record(db, &index, object, mir, &mut bodies, &mut edges);
            }
        }
    }
    let reached = reachable(entries.iter().copied(), &edges);
    let mut out = String::from(
        "# Conservative MIR inventory\n\nBefore native admission. Virtual edges are same-name candidates, not proven instantiations.\nOnly the supplied entries are roots; package initializers are not implicit.\n",
    );
    for object in reached {
        writeln!(out, "\n## {}\n", index.display(object)).unwrap();
        if let Some(body) = bodies.get(&object) {
            out.push_str(body);
        } else {
            writeln!(
                out,
                "No source MIR: {:?}; native key {:?}",
                index.info(object).kind,
                index.info(object).native_key
            )
            .unwrap();
        }
        for callee in edges.get(&object).into_iter().flatten() {
            writeln!(out, "edge: {}", index.display(*callee)).unwrap();
        }
    }
    Ok(out)
}

fn record(
    db: &dyn baml_compiler2_mir::Db,
    index: &ProgramIndex<'_>,
    object: usize,
    mir: &MirFunction<'_>,
    bodies: &mut BTreeMap<usize, String>,
    edges: &mut HashMap<usize, BTreeSet<usize>>,
) {
    bodies.insert(
        object,
        baml_compiler2_mir::pretty::display_function(db, mir),
    );
    if let MirFunctionKind::Bytecode(body) = &mir.kind {
        edges.insert(object, references(db, index, body, &mir.lambdas));
    }
    for lambda in &mir.lambdas {
        if let Some(object) = index.lambda(db, &lambda.identity) {
            record(db, index, object, lambda, bodies, edges);
        }
    }
}
