//! Admission: which functions of a program compile, and the module's
//! index of the program they run in.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use baml_compiler2_hir::item_data::{file_functions, is_required_interface_method};
use baml_compiler2_mir::{MirFunction, MirFunctionBody, MirFunctionKind, OptLevel, lower_function};
use bex_vm_types::{FunctionKind, Object, ObjectIndex, Program};

use crate::{analysis::Analysis, codegen::Generator, render::render_module};

/// Why a function stays bytecode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Rejection {
    /// Outside the subset this backend compiles; the bytecode is authoritative.
    Unsupported(String),
    /// The MIR violated an invariant a checked program must hold: a compiler
    /// bug, reported rather than compiled around.
    Invalid(String),
}

impl std::fmt::Display for Rejection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unsupported(reason) => write!(f, "unsupported: {reason}"),
            Self::Invalid(reason) => write!(f, "invalid MIR: {reason}"),
        }
    }
}

/// One admitted function in the generated module.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledFunction {
    /// `Function::name` of the bytecode function this body replaces.
    pub name: String,
    /// The Rust symbol of the body (`bex_vm::compiled_rt::CompiledBody`).
    pub symbol: String,
    /// Object index of that bytecode function in the program.
    pub object: usize,
    /// Its arity.
    pub arity: usize,
    /// Whether it can suspend (it then runs as a resumable state machine;
    /// see `bex_vm::compiled_rt::ResumableBody`).
    pub resumable: bool,
    /// The stack slots its body keeps in its frame
    /// (`bex_vm_types::types::NativeBody::slots`).
    pub slots: u32,
    /// Its cause slot, when it has a `catch`
    /// (`bex_vm_types::types::NativeBody::cause_slot`).
    pub cause_slot: Option<u32>,
}

/// The generated Rust module for one program.
#[derive(Debug, Clone)]
pub struct NativeModule {
    /// Rust source of the module.
    pub source: String,
    /// The functions it compiles, in emission order.
    pub functions: Vec<CompiledFunction>,
    /// Root-package functions it does not compile, with the reason.
    pub rejected: Vec<(String, Rejection)>,
}

/// Compile every admitted function of `root`'s package.
///
/// `program` is the linked program the bytecode of these same functions lives
/// in; names and object indices are read from it so the generated module
/// refers to exactly this build (the generated crate embeds it as
/// `PROGRAM_ARTIFACT`, read from `artifact_path`).
pub fn compile_program<'db>(
    db: &'db dyn baml_compiler2_mir::Db,
    root: baml_base::SourceRoot,
    program: &Program,
    opt: OptLevel,
    artifact_path: &str,
) -> NativeModule {
    let index = ProgramIndex::new(program);
    let mut units: BTreeMap<String, Unit<'db>> = BTreeMap::new();
    let mut rejected: Vec<(String, Rejection)> = Vec::new();

    for &file in root.files(db) {
        let lines: std::sync::Arc<[u32]> =
            baml_compiler2_emit::build_line_starts(file.text(db)).into();
        for &function in file_functions(db, file) {
            if is_required_interface_method(db, function) {
                continue;
            }
            let mir = match lower_function(db, function, opt) {
                Ok(mir) => mir,
                Err(error) => {
                    rejected.push((
                        format!("{function:?}"),
                        Rejection::Invalid(format!("lowering failed: {error:?}")),
                    ));
                    continue;
                }
            };
            let name = mir.identity.display_name(db);
            let MirFunctionKind::Bytecode(body) = &mir.kind else {
                continue;
            };
            let Some(info) = index.functions.get(&name) else {
                rejected.push((
                    name,
                    Rejection::Invalid("no function of this name in the program".into()),
                ));
                continue;
            };
            if info.generic {
                rejected.push((name, Rejection::Unsupported("generic function".into())));
                continue;
            }
            if !matches!(info.kind, FunctionKind::Bytecode) {
                continue;
            }
            match Analysis::new(db, &index, mir, body) {
                Ok(analysis) => {
                    units.insert(
                        name,
                        Unit {
                            mir,
                            analysis,
                            lines: lines.clone(),
                        },
                    );
                }
                Err(rejection) => rejected.push((name, rejection)),
            }
        }
    }

    // Admission is one fixpoint. A function is admitted when its body
    // generates (every construct in it is supported); generation is the only
    // judge, so nothing half-supported is emitted, and each round removes at
    // least one function, so it terminates. Whether an admitted function can
    // suspend follows from its body and its callees: it suspends when it
    // awaits, spawns, runs a sys op, or calls anything compiled code does not
    // call directly (bytecode, a function value, a builtin that runs
    // callbacks, or a function that itself suspends).
    let (bodies, functions, natives) = loop {
        let mut resumable: BTreeSet<String> = units
            .iter()
            .filter(|(_, unit)| unit.analysis.suspends)
            .map(|(name, _)| name.clone())
            .collect();
        loop {
            let grown: Vec<String> =
                units
                    .iter()
                    .filter(|(name, unit)| {
                        !resumable.contains(*name)
                            && unit.analysis.user_callees.iter().any(|callee| {
                                !units.contains_key(callee) || resumable.contains(callee)
                            })
                    })
                    .map(|(name, _)| name.clone())
                    .collect();
            if grown.is_empty() {
                break;
            }
            resumable.extend(grown);
        }
        let symbols: HashMap<String, String> = units
            .keys()
            .enumerate()
            .map(|(ordinal, name)| (name.clone(), symbol_for(name, ordinal)))
            .collect();
        let mut bodies = String::new();
        let mut functions = Vec::new();
        let mut natives: BTreeMap<String, usize> = BTreeMap::new();
        let mut failed = Vec::new();
        for (name, unit) in &units {
            let mut generator = Generator {
                db,
                index: &index,
                symbols: &symbols,
                resumable: &resumable,
                suspends: resumable.contains(name),
                in_loop: false,
                natives: &mut natives,
                body: body_of(unit.mir),
                analysis: &unit.analysis,
                lines: &unit.lines,
                declared_at: unit.mir.span,
                current_line: None,
                out: String::new(),
                pending: Vec::new(),
                temps: 0,
                live_in: Vec::new(),
                slots: 0,
                cause_slot: None,
                point_slot: 0,
                preds: Vec::new(),
                loop_headers: BTreeSet::new(),
                write_through: BTreeSet::new(),
                call_types: Vec::new(),
                structured: None,
            };
            match generator.function(&symbols[name]) {
                Ok(code) => {
                    bodies.push_str(&code);
                    let info = &index.functions[name];
                    functions.push(CompiledFunction {
                        name: name.clone(),
                        symbol: symbols[name].clone(),
                        object: index.objects[name],
                        arity: info.arity,
                        resumable: resumable.contains(name),
                        slots: u32::try_from(generator.slots).unwrap_or(u32::MAX),
                        cause_slot: generator
                            .cause_slot
                            .map(|slot| u32::try_from(slot).unwrap_or(u32::MAX)),
                    });
                }
                Err(rejection) => failed.push((name.clone(), rejection)),
            }
        }
        if failed.is_empty() {
            break (bodies, functions, natives);
        }
        for (name, rejection) in failed {
            units.remove(&name);
            rejected.push((name, rejection));
        }
    };

    let source = render_module(&bodies, &functions, &natives, artifact_path);
    NativeModule {
        source,
        functions,
        rejected,
    }
}

fn body_of<'a, 'db>(mir: &'a MirFunction<'db>) -> &'a MirFunctionBody<'db> {
    match &mir.kind {
        MirFunctionKind::Bytecode(body) => body,
        MirFunctionKind::Builtin(_) => unreachable!("units hold bytecode bodies only"),
    }
}

fn symbol_for(name: &str, ordinal: usize) -> String {
    let mut symbol = String::with_capacity(name.len() + 8);
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            symbol.push(ch);
        } else {
            symbol.push('_');
        }
    }
    format!("{symbol}_{ordinal}")
}

struct Unit<'db> {
    mir: &'db MirFunction<'db>,
    analysis: Analysis,
    /// Line starts of the function's source file, for reported lines.
    lines: std::sync::Arc<[u32]>,
}

// ─── The program's objects by name ──────────────────────────────────────────

pub(crate) struct FunctionInfo {
    pub(crate) kind: FunctionKind,
    /// The slots a call lays its arguments over (`Function::argument_layout`).
    pub(crate) layout: baml_type::CallLayout,
    pub(crate) native_key: Option<String>,
    pub(crate) arity: usize,
    pub(crate) generic: bool,
}

pub(crate) struct ProgramIndex {
    pub(crate) functions: HashMap<String, FunctionInfo>,
    /// Function name → object index.
    pub(crate) objects: HashMap<String, usize>,
    /// Class link name → (object index, field count).
    pub(crate) classes: HashMap<String, (usize, usize)>,
    /// Enum link name → object index.
    pub(crate) enums: HashMap<String, usize>,
    /// String literal → the compile-time string object bytecode loads it from.
    pub(crate) strings: HashMap<String, usize>,
}

impl ProgramIndex {
    fn new(program: &Program) -> Self {
        let mut functions = HashMap::new();
        let mut objects = HashMap::new();
        let mut classes = HashMap::new();
        let mut enums = HashMap::new();
        let mut strings = HashMap::new();
        // Names as the loader renders them: under the label of the package
        // that placed the declaration. A pooled declaration's own `name()`
        // has no owner until load, so it would render unqualified.
        for (index, object) in program.objects.iter().enumerate() {
            match object {
                Object::Function(function) => {
                    let Some(name) = program.function_name(ObjectIndex::from_raw(index)) else {
                        continue;
                    };
                    objects.insert(name.clone(), index);
                    functions.insert(
                        name,
                        FunctionInfo {
                            kind: function.kind,
                            layout: function.argument_layout(),
                            native_key: function.native_key.as_deref().map(str::to_owned),
                            arity: function.arity,
                            generic: !function.generic_param_bounds.is_empty()
                                || !function.display_type_params.is_empty(),
                        },
                    );
                }
                Object::String(text) => {
                    strings.entry(text.to_string()).or_insert(index);
                }
                _ => {}
            }
        }
        for package in &program.packages {
            for (path, &object) in &package.classes {
                if let Object::Class(class) = &program.objects[object] {
                    let name = qualified(&package.name, path);
                    classes.insert(name, (object.raw(), class.fields.len()));
                }
            }
            for (path, &object) in &package.enums {
                let name = qualified(&package.name, path);
                enums.insert(name, object.raw());
            }
        }
        Self {
            functions,
            objects,
            classes,
            enums,
            strings,
        }
    }
}

/// A declaration's fully qualified name, `package.ns.Name`: what MIR's
/// `class_display_name` and `enum_display_name` spell. `TypeName`'s display
/// name elides the local package and would miss them.
fn qualified(package: &baml_type::Name, path: &bex_vm_types::types::LocalName) -> String {
    std::iter::once(package.as_str())
        .chain(path.namespace.iter().map(baml_type::Name::as_str))
        .chain(std::iter::once(path.name.as_str()))
        .collect::<Vec<_>>()
        .join(".")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn symbols_are_identifiers() {
        assert_eq!(symbol_for("user.Add", 3), "user_Add_3");
        assert_eq!(symbol_for("pkg.ns.Class.m", 0), "pkg_ns_Class_m_0");
    }
}
