//! Admission and reachability for Rust emission.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use baml_base::SourceRoot;
use baml_compiler2_hir::{
    file_package::file_package,
    item_data::{file_functions, file_lets, is_required_interface_method},
    package::world_files,
};
use baml_compiler2_mir::{
    MirFunction, MirFunctionBody, MirFunctionKind, OptLevel, lower_function, lower_let_body,
};
use bex_vm_types::{FunctionKind, Program};

use crate::{
    analysis::{Analysis, Repr, Signature, capture_reprs, value_reprs},
    codegen::{Generator, ProgramContext},
    index::ProgramIndex,
    reach::{reachable, references},
    render::render_module,
};

/// Why a function stays bytecode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Rejection {
    /// Outside the supported subset; execution stays in bytecode.
    Unsupported(String),
    /// A compiler invariant failed: a backend bug. It falls back to bytecode
    /// in release builds and panics in debug builds, so tests catch it.
    Invalid(String),
    /// Unreachable from the requested entry points.
    Unreached,
}

/// Record that `name` stays bytecode.
fn reject(rejected: &mut Vec<(String, Rejection)>, name: String, rejection: Rejection) {
    debug_assert!(
        !matches!(rejection, Rejection::Invalid(_)),
        "Rust backend bug in {name}: {rejection}"
    );
    rejected.push((name, rejection));
}

impl std::fmt::Display for Rejection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unsupported(reason) => write!(f, "unsupported: {reason}"),
            Self::Invalid(reason) => write!(f, "invalid MIR: {reason}"),
            Self::Unreached => write!(f, "unreached from the entry points"),
        }
    }
}

/// One admitted function in the generated module.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledFunction {
    /// Loader name, checked when binding the body to its program.
    pub name: String,
    /// Package-qualified name for reports, including closure bodies.
    pub display: String,
    /// The Rust symbol of the body (`bex_vm::compiled_rt::InlineBody`).
    pub symbol: String,
    /// Object index of that bytecode function in the program.
    pub object: usize,
    /// Its arity.
    pub arity: usize,
    /// Number of frame slots.
    pub slots: u32,
    /// Frame slot holding the active catch cause.
    pub cause_slot: Option<u32>,
    /// Whether the body must run with a VM frame rather than a frame record.
    pub framed: bool,
}

/// The generated Rust module for one program.
#[derive(Debug, Clone)]
pub struct RustModule {
    /// Rust source of the module.
    pub source: String,
    /// The functions it compiles, in emission order.
    pub functions: Vec<CompiledFunction>,
    /// Functions of any package it does not compile, with the reason.
    pub rejected: Vec<(String, Rejection)>,
}

/// Compile the reachable supported functions in the linked program.
///
/// `artifact_path` embeds that program, and `fingerprint` is its authenticated
/// digest from `baml_artifact::encode_with_digest`. Names and object indices
/// must come from the same build.
///
/// `entries` selects roots for reachability, along with top-level initializers
/// and non-stdlib methods that may be dispatched dynamically. `None` attempts
/// every function.
pub fn compile_program(
    db: &dyn baml_compiler2_mir::Db,
    root: SourceRoot,
    program: &Program,
    opt: OptLevel,
    artifact_path: &str,
    fingerprint: [u8; 32],
    entries: Option<&[usize]>,
) -> Result<RustModule, String> {
    crate::analysis::validate_builtin_metadata()?;
    let index = ProgramIndex::new(db, root, program);
    let mut lowered = lower_program(db, root, &index, opt, entries);
    if entries.is_some() {
        retain_reachable(&index, &mut lowered);
    }
    let (bodies, functions, builtins) =
        admission_fixpoint(db, &index, &mut lowered.units, &mut lowered.rejected);

    let source = render_module(&bodies, &functions, &builtins, artifact_path, fingerprint);
    Ok(RustModule {
        source,
        functions,
        rejected: lowered.rejected,
    })
}

struct LoweredProgram<'db> {
    units: BTreeMap<usize, Unit<'db>>,
    rejected: Vec<(String, Rejection)>,
    refs: HashMap<usize, BTreeSet<usize>>,
    seeds: BTreeSet<usize>,
}

fn lower_program<'db>(
    db: &'db dyn baml_compiler2_mir::Db,
    root: SourceRoot,
    index: &ProgramIndex,
    opt: OptLevel,
    entries: Option<&[usize]>,
) -> LoweredProgram<'db> {
    // Rendered names can collide across and within packages.
    let mut units: BTreeMap<usize, Unit<'db>> = BTreeMap::new();
    let mut rejected: Vec<(String, Rejection)> = Vec::new();
    // Rejected bodies still contribute edges to reachability.
    let mut refs: HashMap<usize, BTreeSet<usize>> = HashMap::new();
    let mut seeds: BTreeSet<usize> = entries.into_iter().flatten().copied().collect();

    for &file in world_files(db, root) {
        let package = index.package_of(file_package(db, file).root);
        for &function in file_functions(db, file) {
            if is_required_interface_method(db, function) {
                continue;
            }
            let mir = match lower_function(db, function, opt) {
                Ok(mir) => mir,
                Err(error) => {
                    reject(
                        &mut rejected,
                        format!("{function:?}"),
                        Rejection::Invalid(format!("lowering failed: {error:?}")),
                    );
                    continue;
                }
            };
            let MirFunctionKind::Bytecode(body) = &mir.kind else {
                continue;
            };
            // Lambdas are admitted independently of their parent.
            admit_lambdas(db, index, &mut units, &mut rejected, mir, &[], package);
            let Some(object) = index.declared(db, function) else {
                reject(
                    &mut rejected,
                    mir.identity.display_name(db),
                    Rejection::Invalid("no function of this identity in the program".into()),
                );
                continue;
            };
            if entries.is_some() {
                refs.insert(object, references(db, index, body, &mir.lambdas));
            }
            admit_unit(
                db,
                index,
                &mut units,
                &mut rejected,
                object,
                mir,
                body,
                Vec::new(),
                package,
            );
        }
        if entries.is_some() {
            for &binding in file_lets(db, file) {
                if let Ok(Some((body, lambdas))) = lower_let_body(db, binding, opt) {
                    seeds.extend(references(db, index, &body, &lambdas));
                }
            }
        }
    }

    LoweredProgram {
        units,
        rejected,
        refs,
        seeds,
    }
}

fn retain_reachable(index: &ProgramIndex, lowered: &mut LoweredProgram<'_>) {
    let LoweredProgram {
        units,
        rejected,
        refs,
        seeds,
    } = lowered;
    seeds.extend(index.user_methods.iter().copied());
    let reached = reachable(std::mem::take(seeds), refs);
    units.retain(|object, _| {
        let keep = reached.contains(object);
        if !keep {
            reject(
                rejected,
                index.name(*object).to_owned(),
                Rejection::Unreached,
            );
        }
        keep
    });
}

fn admission_fixpoint(
    db: &dyn baml_compiler2_mir::Db,
    index: &ProgramIndex,
    units: &mut BTreeMap<usize, Unit<'_>>,
    rejected: &mut Vec<(String, Rejection)>,
) -> (String, Vec<CompiledFunction>, BTreeMap<String, usize>) {
    // Every failed round removes at least one unit.
    loop {
        let graph: BTreeMap<usize, BTreeSet<usize>> = units
            .iter()
            .map(|(&object, unit)| (object, unit.analysis.user_callees.clone()))
            .collect();
        let recursive = recursive_functions(&graph);
        for (&object, unit) in units.iter_mut() {
            unit.recursive = recursive.contains(&object);
        }
        // A body suspends if it polls at a loop, suspends directly, or calls a
        // body that does; callers root their live values around such calls.
        let mut pausing: BTreeSet<usize> = units
            .iter()
            .filter(|(_, unit)| unit.recursive || unit.analysis.pauses())
            .map(|(&object, _)| object)
            .collect();
        propagate_pausing(units, &mut pausing);
        // Bodies that keep VM-frame state: a trace hook prologue, type
        // bindings, or a pass-through span scope. A model invoke's return
        // charges model usage, which the VM-frame call paths do.
        let framed: BTreeSet<usize> = units
            .iter()
            .filter(|&(&object, unit)| {
                let info = index.info(object);
                unit.analysis.hooked
                    || unit.analysis.binds_types
                    || info.trace_passthrough
                    || info.model_invoke
            })
            .map(|(&object, _)| object)
            .collect();
        let signatures: HashMap<usize, Signature> = units
            .iter()
            .map(|(&object, unit)| (object, unit.analysis.signature(body_of(unit.mir))))
            .collect();
        let symbols: HashMap<usize, String> = units
            .keys()
            .map(|&object| (object, symbol_for(index.display(object), object)))
            .collect();
        let mut bodies = String::new();
        let mut functions = Vec::new();
        let mut builtins: BTreeMap<String, usize> = BTreeMap::new();
        let mut failed = Vec::new();
        for (&object, unit) in units.iter() {
            let program = ProgramContext {
                db,
                index,
                symbols: &symbols,
                pausing: &pausing,
                framed: &framed,
                signatures: &signatures,
            };
            let mut generator = Generator::new(&program, &mut builtins, object, unit);
            match generator.function(&symbols[&object]) {
                Ok(code) => {
                    let Ok(slots) = u32::try_from(generator.slots) else {
                        failed.push((
                            object,
                            Rejection::Unsupported("compiled frame slot count exceeds u32".into()),
                        ));
                        continue;
                    };
                    bodies.push_str(&code);
                    functions.push(CompiledFunction {
                        name: index.name(object).to_owned(),
                        display: index.display(object).to_owned(),
                        symbol: symbols[&object].clone(),
                        object,
                        arity: index.info(object).arity,
                        slots,
                        cause_slot: generator
                            .cause_slot
                            .map(|slot| u32::try_from(slot).expect("bounded MIR slots")),
                        framed: framed.contains(&object),
                    });
                }
                Err(rejection) => failed.push((object, rejection)),
            }
        }
        if failed.is_empty() {
            break (bodies, functions, builtins);
        }
        for (object, rejection) in failed {
            units.remove(&object);
            reject(rejected, index.display(object).to_owned(), rejection);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn admit_unit<'db>(
    db: &'db dyn baml_compiler2_mir::Db,
    index: &ProgramIndex,
    units: &mut BTreeMap<usize, Unit<'db>>,
    rejected: &mut Vec<(String, Rejection)>,
    object: usize,
    mir: &'db MirFunction<'db>,
    body: &'db MirFunctionBody<'db>,
    captures: Vec<Repr>,
    package: Option<u32>,
) {
    let name = index.display(object).to_owned();
    let info = index.info(object);
    if !matches!(info.kind, FunctionKind::Bytecode) {
        return;
    }
    match Analysis::new(db, index, mir, body, captures) {
        Ok(analysis) => {
            units.insert(
                object,
                Unit {
                    mir,
                    analysis,
                    package,
                    recursive: false,
                },
            );
        }
        Err(rejection) => reject(rejected, name, rejection),
    }
}

/// Admit nested lambdas with their enclosing capture representations.
fn admit_lambdas<'db>(
    db: &'db dyn baml_compiler2_mir::Db,
    index: &ProgramIndex,
    units: &mut BTreeMap<usize, Unit<'db>>,
    rejected: &mut Vec<(String, Rejection)>,
    parent: &'db MirFunction<'db>,
    captures: &[Repr],
    package: Option<u32>,
) {
    let MirFunctionKind::Bytecode(parent_body) = &parent.kind else {
        return;
    };
    let values = value_reprs(parent_body);
    for (lambda_idx, lambda) in parent.lambdas.iter().enumerate() {
        let MirFunctionKind::Bytecode(body) = &lambda.kind else {
            continue;
        };
        let captures = capture_reprs(parent_body, lambda_idx, &values, captures);
        let Some(object) = index.lambda(db, &lambda.identity) else {
            reject(
                rejected,
                lambda.identity.display_name(db),
                Rejection::Invalid("no function of this identity in the program".into()),
            );
            continue;
        };
        admit_lambdas(db, index, units, rejected, lambda, &captures, package);
        admit_unit(
            db, index, units, rejected, object, lambda, body, captures, package,
        );
    }
}

pub(crate) fn body_of<'a, 'db>(mir: &'a MirFunction<'db>) -> &'a MirFunctionBody<'db> {
    match &mir.kind {
        MirFunctionKind::Bytecode(body) => body,
        MirFunctionKind::Builtin(_) => unreachable!("units hold bytecode bodies only"),
    }
}

/// Object indices disambiguate functions whose display names collide.
fn symbol_for(name: &str, object: usize) -> String {
    let mut symbol = String::with_capacity(name.len() + 8);
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            symbol.push(ch);
        } else {
            symbol.push('_');
        }
    }
    format!("{symbol}_{object}")
}

pub(crate) struct Unit<'db> {
    pub(crate) recursive: bool,
    pub(crate) mir: &'db MirFunction<'db>,
    pub(crate) analysis: Analysis,
    /// Ordinal of the package that owns the function.
    pub(crate) package: Option<u32>,
}

/// Functions in a call cycle, including a one-function cycle.
fn recursive_functions(graph: &BTreeMap<usize, BTreeSet<usize>>) -> BTreeSet<usize> {
    let mut reverse: BTreeMap<usize, Vec<usize>> =
        graph.keys().map(|&object| (object, Vec::new())).collect();
    for (&caller, callees) in graph {
        for callee in callees {
            if let Some(callers) = reverse.get_mut(callee) {
                callers.push(caller);
            }
        }
    }
    let mut seen = BTreeSet::new();
    let mut order = Vec::new();
    for &root in graph.keys() {
        if !seen.insert(root) {
            continue;
        }
        let mut stack = vec![(root, graph[&root].iter())];
        while let Some((object, callees)) = stack.last_mut() {
            if let Some(&callee) = callees.next() {
                if graph.contains_key(&callee) && seen.insert(callee) {
                    stack.push((callee, graph[&callee].iter()));
                }
            } else {
                order.push(*object);
                stack.pop();
            }
        }
    }
    seen.clear();
    let mut recursive = BTreeSet::new();
    for &root in order.iter().rev() {
        if !seen.insert(root) {
            continue;
        }
        let mut component = Vec::new();
        let mut stack = vec![root];
        while let Some(object) = stack.pop() {
            component.push(object);
            for &caller in &reverse[&object] {
                if seen.insert(caller) {
                    stack.push(caller);
                }
            }
        }
        if component.len() > 1 || graph[&root].contains(&root) {
            recursive.extend(component);
        }
    }
    recursive
}

/// Propagate a call effect until the set stops growing.
fn propagate_pausing(units: &BTreeMap<usize, Unit<'_>>, set: &mut BTreeSet<usize>) {
    loop {
        let grown: Vec<usize> = units
            .iter()
            .filter(|(object, unit)| {
                !set.contains(*object)
                    && unit
                        .analysis
                        .user_callees
                        .iter()
                        .any(|&callee| !units.contains_key(&callee) || set.contains(&callee))
            })
            .map(|(&object, _)| object)
            .collect();
        if grown.is_empty() {
            return;
        }
        set.extend(grown);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recursive_components_exclude_callers_and_callees() {
        let graph = BTreeMap::from([
            (0, BTreeSet::from([1])),
            (1, BTreeSet::from([2])),
            (2, BTreeSet::from([1, 3])),
            (3, BTreeSet::new()),
            (4, BTreeSet::from([4])),
            (5, BTreeSet::from([6])),
        ]);
        assert_eq!(recursive_functions(&graph), BTreeSet::from([1, 2, 4]));
    }

    #[test]
    fn symbols_are_identifiers() {
        assert_eq!(symbol_for("user.Add", 3), "user_Add_3");
        assert_eq!(
            symbol_for("user.<lambda(f, 0)>", 7),
            "user__lambda_f__0___7"
        );
        assert_eq!(symbol_for("pkg.ns.Class.m", 0), "pkg_ns_Class_m_0");
        assert_eq!(
            symbol_for("user.<(user.A as baml.Show)>.show", 12),
            "user___user_A_as_baml_Show___show_12"
        );
    }
}
