//! Load-time linker (B-693 §3): folds symbolic [`CompilationUnit`]s into a
//! runnable [`Program`].
//!
//! The linker assigns final absolute `ObjectIndex`/`GlobalIndex` values,
//! resolves each import against the merged export table, and patches operands
//! by calling the *existing* [`relink::visit_object_operands`](crate::relink)
//! walker with a per-unit relocation closure — the walker is the link-time
//! patcher, not the input to a bespoke splice heuristic. The output `Program`
//! is unchanged in shape; borsh of `link(all units)` must equal borsh of a full
//! compile (the byte-identity oracle, design §5).
//!
//! # Layout the linker must reproduce
//!
//! A full compile emits in two file **groups** — builtin stubs first, then user
//! files — and within a group runs pass-major (all classes, then all enums,
//! then all interfaces, then all code, then the package `$init` tail). So the
//! flat pool is **group-major, pass-major within a group** (design §9 R3):
//!
//! ```text
//! [B classes][B enums][B interfaces][B code][B $init]
//! [U classes][U enums][U interfaces][U code][U $init]
//! ```
//!
//! The linker interleaves each unit's buckets bucket-by-bucket across the units
//! of its group, in file order, reproducing that exact pool order. Units are
//! partitioned into groups by their [`CompilationUnit::source_file`]: builtin
//! files carry a `<builtin>/…` path.

use std::collections::{HashMap, HashSet};

use baml_base::Name;

use crate::{
    ConstValue, GlobalIndex, HeapPtr, Object, ObjectIndex, Program,
    relink::{IndexOperand, visit_object_operands},
    types::{ProgramImplRule, ProgramMethodImpl},
    unit::{CompilationUnit, ExportTable, LocalRef, ProgramPackageFrag, Symbol, SymbolKind},
};

/// A program linked for grafting into a live VM, plus the symbolic entries in
/// its synthetic external prefix. The runtime replaces those prefix slots with
/// pointers/values from the live image and allocates every other slot anew.
#[derive(Clone, Debug)]
pub struct DynamicLinkPlan {
    pub program: Program,
    pub external_objects: Vec<(ObjectIndex, Symbol)>,
    pub external_globals: Vec<(GlobalIndex, Symbol)>,
}

fn import_key(symbol: &Symbol) -> String {
    match &symbol.generic {
        Some(key) => format!("generic:{}:{:?}", key.base_fn, key.type_args),
        None => format!("{:?}:{}", symbol.kind, symbol.fq_name),
    }
}

/// Link runtime-emitted units while leaving references to the already-live
/// image as a symbolic prefix plan.
///
/// The ordinary [`link`] function remains the only operand relocator. This
/// function supplies a synthetic builtin unit that exports inert placeholders
/// for otherwise-unresolved imports, invokes `link`, then reports which linked
/// prefix indices the VM must graft from its live static/dependency images.
pub fn link_dynamic(units: &[CompilationUnit]) -> Result<DynamicLinkPlan, LinkError> {
    let object_exports: HashSet<&str> = units
        .iter()
        .flat_map(|unit| {
            unit.exports
                .objects
                .iter()
                .map(|(name, _)| name.as_str())
                .chain(
                    unit.init_tail
                        .iter()
                        .flat_map(|tail| tail.named.iter().map(|(name, _)| name.as_str())),
                )
        })
        .collect();
    let global_exports: HashSet<&str> = units
        .iter()
        .flat_map(|unit| {
            unit.exports
                .globals
                .iter()
                .map(|(name, _)| name.as_str())
                .chain(
                    unit.init_tail
                        .iter()
                        .flat_map(|tail| tail.named.iter().map(|(name, _)| name.as_str())),
                )
        })
        .collect();

    let mut object_imports = Vec::<Symbol>::new();
    let mut seen_objects = HashSet::<String>::new();
    let mut global_imports = Vec::<Symbol>::new();
    let mut seen_globals = HashSet::<String>::new();

    let mut consider_object = |symbol: &Symbol| {
        let is_local = !matches!(symbol.kind, SymbolKind::GenericFn)
            && object_exports.contains(symbol.fq_name.as_str());
        let key = import_key(symbol);
        if !is_local && seen_objects.insert(key) {
            object_imports.push(symbol.clone());
        }
    };
    let mut consider_global = |symbol: &Symbol| {
        if !global_exports.contains(symbol.fq_name.as_str())
            && seen_globals.insert(import_key(symbol))
        {
            global_imports.push(symbol.clone());
        }
    };

    for unit in units {
        for symbol in &unit.object_imports {
            consider_object(symbol);
        }
        for symbol in &unit.global_imports {
            consider_global(symbol);
        }
        if let Some(tail) = &unit.init_tail {
            for symbol in &tail.object_imports {
                consider_object(symbol);
            }
            for symbol in &tail.global_imports {
                consider_global(symbol);
            }
        }

        // Package fragments also carry symbolic object references (not bytecode
        // operands): most point at the unit's own exports, but an impl of a
        // mounted interface or an inherited mounted default points directly at
        // the live dependency. Feed those names into the same synthetic-prefix
        // plan so fragment resolution and code relocation see one alias table.
        let fragment = &unit.package_fragment;
        for (_, fq_name) in &fragment.classes {
            consider_object(&Symbol {
                kind: SymbolKind::Class,
                fq_name: fq_name.clone(),
                generic: None,
            });
        }
        for (_, fq_name) in &fragment.enums {
            consider_object(&Symbol {
                kind: SymbolKind::Enum,
                fq_name: fq_name.clone(),
                generic: None,
            });
        }
        for (_, fq_name) in &fragment.interfaces {
            consider_object(&Symbol {
                kind: SymbolKind::Interface,
                fq_name: fq_name.clone(),
                generic: None,
            });
        }
        for (_, fq_name) in &fragment.functions {
            consider_object(&Symbol {
                kind: SymbolKind::Function,
                fq_name: fq_name.clone(),
                generic: None,
            });
        }
        for (interface, rules) in &fragment.impl_rules {
            consider_object(&Symbol {
                kind: SymbolKind::Interface,
                fq_name: interface.clone(),
                generic: None,
            });
            for rule in rules {
                consider_object(&Symbol {
                    kind: SymbolKind::Interface,
                    fq_name: rule.interface_head.clone(),
                    generic: None,
                });
                for (_, method) in &rule.methods {
                    consider_object(&Symbol {
                        kind: SymbolKind::Function,
                        fq_name: method.fqn.clone(),
                        generic: None,
                    });
                }
            }
        }
        if let Some(test_init) = &fragment.test_init {
            consider_object(&Symbol {
                kind: SymbolKind::Function,
                fq_name: test_init.clone(),
                generic: None,
            });
        }
    }

    // A function object prefix also needs the function's global slot, and a
    // generic value needs the base function global used by GenericFunction.
    let mut required_functions = Vec::<String>::new();
    let mut seen_functions = HashSet::<String>::new();
    for symbol in object_imports.iter().chain(&global_imports) {
        let base = match (&symbol.kind, &symbol.generic) {
            (SymbolKind::Function, _) => Some(symbol.fq_name.as_str()),
            (SymbolKind::GenericFn, Some(key)) => Some(key.base_fn.as_str()),
            _ => None,
        };
        if let Some(base) = base
            && !global_exports.contains(base)
            && seen_functions.insert(base.to_string())
        {
            required_functions.push(base.to_string());
        }
    }

    let mut classes = Vec::new();
    let mut enums = Vec::new();
    let mut interfaces = Vec::new();
    let mut code = Vec::new();
    let mut exports = ExportTable::default();
    let placeholder = || Object::String("<runtime-import>".into());

    for symbol in &object_imports {
        match symbol.kind {
            SymbolKind::Class => {
                let idx = u32::try_from(classes.len()).expect("runtime class imports fit u32");
                classes.push(placeholder());
                exports
                    .objects
                    .push((symbol.fq_name.clone(), LocalRef::Class(idx)));
            }
            SymbolKind::Enum => {
                let idx = u32::try_from(enums.len()).expect("runtime enum imports fit u32");
                enums.push(placeholder());
                exports
                    .objects
                    .push((symbol.fq_name.clone(), LocalRef::Enum(idx)));
            }
            SymbolKind::Interface => {
                let idx =
                    u32::try_from(interfaces.len()).expect("runtime interface imports fit u32");
                interfaces.push(placeholder());
                exports
                    .objects
                    .push((symbol.fq_name.clone(), LocalRef::Interface(idx)));
            }
            SymbolKind::Function | SymbolKind::Let | SymbolKind::GenericFn => {}
        }
    }

    let mut function_code = HashMap::<String, u32>::new();
    for name in &required_functions {
        let idx = u32::try_from(code.len()).expect("runtime function imports fit u32");
        code.push(placeholder());
        function_code.insert(name.clone(), idx);
        exports.objects.push((name.clone(), LocalRef::Code(idx)));
    }

    let mut let_names = Vec::<String>::new();
    for symbol in &global_imports {
        if matches!(symbol.kind, SymbolKind::Let) && !let_names.contains(&symbol.fq_name) {
            let_names.push(symbol.fq_name.clone());
        }
    }
    for (slot, name) in required_functions.iter().chain(&let_names).enumerate() {
        exports.globals.push((
            name.clone(),
            u32::try_from(slot).expect("runtime global imports fit u32"),
        ));
    }

    // Candidate-owned generic bases are imports of the synthetic unit; live
    // bases are local function globals in its prefix.
    let mut stub_global_imports = Vec::<Symbol>::new();
    let mut generic_code = Vec::<(Symbol, u32)>::new();
    for symbol in &object_imports {
        let SymbolKind::GenericFn = symbol.kind else {
            continue;
        };
        let key = symbol.generic.as_ref().ok_or_else(|| {
            LinkError::InvalidUnit(format!("generic import `{}` has no key", symbol.fq_name))
        })?;
        let function = if let Some(pos) = required_functions
            .iter()
            .position(|name| name == &key.base_fn)
        {
            GlobalIndex::from_raw(pos)
        } else {
            let import_pos = stub_global_imports.len();
            stub_global_imports.push(Symbol {
                kind: SymbolKind::Function,
                fq_name: key.base_fn.clone(),
                generic: None,
            });
            GlobalIndex::from_raw(exports.globals.len() + import_pos)
        };
        let idx = u32::try_from(code.len()).expect("runtime generic imports fit u32");
        code.push(Object::GenericFunction(crate::GenericFunction {
            function,
            type_args: key.type_args.clone().into_boxed_slice(),
            runtime_package: HeapPtr::null(),
        }));
        generic_code.push((symbol.clone(), idx));
    }

    let stub = CompilationUnit {
        source_file: "<builtin>/$runtime_imports.baml".to_string(),
        package: Name::new("$runtime_imports"),
        classes,
        enums,
        interfaces,
        // The synthetic import stub declares nothing of its own, so it pools no
        // `Object::TypeAlias`.
        type_alias_objects: Vec::new(),
        code,
        object_imports: Vec::new(),
        global_imports: stub_global_imports,
        exports,
        package_fragment: ProgramPackageFrag::default(),
        test_cases: Vec::new(),
        callable_throws_fragment: Vec::new(),
        init_tail: None,
    };

    let class_count = stub.classes.len();
    let enum_count = stub.enums.len();
    let interface_count = stub.interfaces.len();
    let code_base = class_count + enum_count + interface_count;
    let mut external_objects = Vec::new();
    let mut class_idx = 0usize;
    let mut enum_idx = 0usize;
    let mut interface_idx = 0usize;
    for symbol in &object_imports {
        let idx = match symbol.kind {
            SymbolKind::Class => {
                let idx = class_idx;
                class_idx += 1;
                idx
            }
            SymbolKind::Enum => {
                let idx = class_count + enum_idx;
                enum_idx += 1;
                idx
            }
            SymbolKind::Interface => {
                let idx = class_count + enum_count + interface_idx;
                interface_idx += 1;
                idx
            }
            SymbolKind::Function => code_base + function_code[&symbol.fq_name] as usize,
            SymbolKind::GenericFn => {
                let (_, local) = generic_code
                    .iter()
                    .find(|(candidate, _)| import_key(candidate) == import_key(symbol))
                    .expect("generic prefix entry was just constructed");
                code_base + *local as usize
            }
            SymbolKind::Let => continue,
        };
        external_objects.push((ObjectIndex::from_raw(idx), symbol.clone()));
    }
    for name in &required_functions {
        let idx = ObjectIndex::from_raw(code_base + function_code[name] as usize);
        if !external_objects
            .iter()
            .any(|(candidate, _)| *candidate == idx)
        {
            external_objects.push((
                idx,
                Symbol {
                    kind: SymbolKind::Function,
                    fq_name: name.clone(),
                    generic: None,
                },
            ));
        }
    }
    // Function prefix slots precede let prefix slots by the ordinary linker contract.
    let mut external_globals = Vec::new();
    for (idx, name) in required_functions.iter().enumerate() {
        external_globals.push((
            GlobalIndex::from_raw(idx),
            Symbol {
                kind: SymbolKind::Function,
                fq_name: name.clone(),
                generic: None,
            },
        ));
    }
    for (idx, name) in let_names.iter().enumerate() {
        external_globals.push((
            GlobalIndex::from_raw(required_functions.len() + idx),
            Symbol {
                kind: SymbolKind::Let,
                fq_name: name.clone(),
                generic: None,
            },
        ));
    }

    let mut all_units = Vec::with_capacity(units.len() + 1);
    all_units.push(stub);
    all_units.extend_from_slice(units);
    let program = link(&all_units)?;
    Ok(DynamicLinkPlan {
        program,
        external_objects,
        external_globals,
    })
}

/// An error raised while linking symbolic units into a [`Program`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LinkError {
    /// An import named a fully-qualified name that no unit exports.
    UnresolvedImport(String),
    /// Two units export the same fully-qualified name.
    DuplicateExport(String),
    /// A decoded unit contains an index outside its declared local/import space.
    InvalidUnit(String),
}

impl std::fmt::Display for LinkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnresolvedImport(name) => {
                write!(f, "unresolved import: no unit exports `{name}`")
            }
            Self::DuplicateExport(name) => {
                write!(
                    f,
                    "duplicate export: `{name}` is exported by more than one unit"
                )
            }
            Self::InvalidUnit(message) => write!(f, "invalid compilation unit: {message}"),
        }
    }
}

impl std::error::Error for LinkError {}

/// Is this unit part of the builtin (stdlib) group? Builtin source files carry a
/// `<builtin>/…` project-relative path; user files never do.
fn is_builtin_unit(unit: &CompilationUnit) -> bool {
    unit.source_file.starts_with("<builtin>/")
}

/// Per-unit placement layout: the absolute base of each object bucket and the
/// global-slot bases for the unit's functions and `let`s.
#[derive(Clone, Copy, Default)]
struct UnitLayout {
    class_base: usize,
    enum_base: usize,
    iface_base: usize,
    alias_base: usize,
    code_base: usize,
    /// Number of the unit's globals owned by functions (the rest are `let`s).
    func_count: usize,
    /// Absolute slot of the unit's first function global.
    func_gbase: usize,
    /// Absolute slot of the unit's first `let` global.
    let_gbase: usize,
}

struct ResolvedTailImports {
    objects: Vec<usize>,
    globals: Vec<usize>,
}

impl UnitLayout {
    /// Decode a per-unit-local flat global index (§2a) to an absolute slot. The
    /// local global space is `[functions 0..func_count][lets func_count..]`.
    fn local_global(&self, raw: usize) -> usize {
        if raw < self.func_count {
            self.func_gbase + raw
        } else {
            self.let_gbase + (raw - self.func_count)
        }
    }
}

/// Absolute object index of an exported [`LocalRef`] given the unit's layout.
///
/// Only valid for non-`Code` buckets (classes/enums/interfaces), which never
/// contain a shadowed generic value. `Code` exports are resolved through the
/// shadow-aware `code_abs` map instead.
fn export_object_abs(layout: &UnitLayout, local_ref: LocalRef) -> usize {
    match local_ref {
        LocalRef::Class(k) => layout.class_base + k as usize,
        LocalRef::Enum(k) => layout.enum_base + k as usize,
        LocalRef::Interface(k) => layout.iface_base + k as usize,
        LocalRef::TypeAlias(k) => layout.alias_base + k as usize,
        LocalRef::Code(k) => layout.code_base + k as usize,
    }
}

fn local_ref_in_bounds(unit: &CompilationUnit, local_ref: LocalRef) -> bool {
    match local_ref {
        LocalRef::Class(k) => (k as usize) < unit.classes.len(),
        LocalRef::Enum(k) => (k as usize) < unit.enums.len(),
        LocalRef::Interface(k) => (k as usize) < unit.interfaces.len(),
        LocalRef::TypeAlias(k) => (k as usize) < unit.type_alias_objects.len(),
        LocalRef::Code(k) => (k as usize) < unit.code.len(),
    }
}

fn invalid_index(unit: usize, space: &str, raw: usize) -> LinkError {
    LinkError::InvalidUnit(format!("unit {unit} has out-of-range {space} index {raw}"))
}

/// Rewrite every index operand of a pooled `object` from unit-local space into
/// absolute program space. `resolve_obj` / `resolve_glob` map a raw local (or
/// import) index to its absolute slot, returning `None` when it is out of range;
/// the first such failure is reported through `describe(space, raw)`. This is
/// the single relocation dance used for both regular code placement and the
/// `$init` tail — only the local-index arithmetic and error framing differ.
fn relocate_object_operands(
    object: &mut Object,
    mut resolve_obj: impl FnMut(usize) -> Option<usize>,
    mut resolve_glob: impl FnMut(usize) -> Option<usize>,
    describe: impl Fn(&str, usize) -> LinkError,
) -> Result<(), LinkError> {
    let mut link_error = None;
    visit_object_operands(object, |operand| match operand {
        IndexOperand::Object(idx) => {
            if link_error.is_some() {
                return;
            }
            let raw = idx.raw();
            match resolve_obj(raw) {
                Some(abs) => *idx = ObjectIndex::from_raw(abs),
                None => link_error = Some(describe("object operand", raw)),
            }
        }
        IndexOperand::Global(slot) => {
            if link_error.is_some() {
                return;
            }
            let raw = slot.raw();
            match resolve_glob(raw) {
                Some(abs) => *slot = GlobalIndex::from_raw(abs),
                None => link_error = Some(describe("global operand", raw)),
            }
        }
    });
    match link_error {
        Some(error) => Err(error),
        None => Ok(()),
    }
}

/// The fully-qualified name of the base function of a generic value
/// (`Object::GenericFunction`) in a unit's per-unit encoding (design §9 R1). The
/// base's `GlobalIndex` is unit-local (a function this unit defines, resolved via
/// its export table) or an import (resolved via [`CompilationUnit::global_imports`]).
/// Together with the value's `type_args` this is the whole-program intern key.
fn generic_base_name(
    unit: &CompilationUnit,
    base_raw: usize,
    n_local_globals: usize,
) -> Result<String, LinkError> {
    if base_raw < n_local_globals {
        unit.exports
            .globals
            .iter()
            .find(|(_, flat)| *flat as usize == base_raw)
            .map(|(name, _)| name.clone())
            .ok_or_else(|| {
                LinkError::UnresolvedImport(format!("generic base local global {base_raw}"))
            })
    } else {
        unit.global_imports
            .get(base_raw - n_local_globals)
            .map(|sym| sym.fq_name.clone())
            .ok_or_else(|| LinkError::UnresolvedImport(format!("generic base import {base_raw}")))
    }
}

/// Resolve one object import to an absolute pool index: a non-generic reference
/// through the merged export map, a generic-function value through the
/// whole-program intern map keyed by `(base fn name, type_args)` (design §9 R1).
fn resolve_object_import(
    sym: &Symbol,
    obj_by_name: &HashMap<String, usize>,
    canonical_pos: &HashMap<String, HashMap<Vec<crate::RealizedTy>, usize>>,
) -> Result<usize, LinkError> {
    match sym.kind {
        SymbolKind::GenericFn => {
            let key = sym
                .generic
                .as_ref()
                .ok_or_else(|| LinkError::UnresolvedImport(sym.fq_name.clone()))?;
            canonical_pos
                .get(key.base_fn.as_str())
                .and_then(|by_args| by_args.get(&key.type_args))
                .copied()
                .ok_or_else(|| LinkError::UnresolvedImport(format!("{}<generic>", key.base_fn)))
        }
        _ => obj_by_name
            .get(sym.fq_name.as_str())
            .copied()
            .ok_or_else(|| LinkError::UnresolvedImport(sym.fq_name.clone())),
    }
}

/// The single unit in `group` that carries the group's `$init`/`$init_test`
/// tail, or `None` if the group has none.
///
/// # Errors
///
/// [`LinkError::DuplicateExport`] (named `$init tail`) if more than one unit in
/// the group carries an `init_tail`. A group's tail is placed after the group's
/// regular code and must be unique; silently keeping the first and dropping the
/// rest would lose `$init`/`$init_test` bytecode if an emitter bug ever produced
/// two, so this is surfaced rather than swallowed.
fn sole_init_tail(units: &[CompilationUnit], group: &[usize]) -> Result<Option<usize>, LinkError> {
    let mut tail: Option<usize> = None;
    for &u in group {
        if units[u].init_tail.is_some() {
            if tail.is_some() {
                return Err(LinkError::DuplicateExport("$init tail".to_string()));
            }
            tail = Some(u);
        }
    }
    Ok(tail)
}

/// Link symbolic units into a runnable [`Program`].
///
/// `units` must already be in the deterministic file-discovery order (builtins
/// first, then user files) — the linker's positional appends follow that order.
///
/// # Errors
///
/// [`LinkError::UnresolvedImport`] if any unit imports a fully-qualified name no
/// unit exports, and [`LinkError::DuplicateExport`] if two units export the same
/// name.
#[allow(clippy::too_many_lines)]
pub fn link(units: &[CompilationUnit]) -> Result<Program, LinkError> {
    // ---- Group ordering (design §9 R3) --------------------------------------
    // Process builtin units first, then user units; within each group the passes
    // interleave bucket-by-bucket across units in file order.
    let group_order: Vec<usize> = (0..units.len())
        .filter(|&u| is_builtin_unit(&units[u]))
        .chain((0..units.len()).filter(|&u| !is_builtin_unit(&units[u])))
        .collect();
    // The two group slices, as index ranges into `group_order`.
    let builtin_len = units.iter().filter(|u| is_builtin_unit(u)).count();
    let groups: [&[usize]; 2] = [&group_order[..builtin_len], &group_order[builtin_len..]];

    // The `$init`/`$init_test` tail of each group is carried on one of its units
    // (at most one). It is placed after the group's regular code (design §9 R2).
    // Two tails in one group would drop bytecode, so it is an error rather than a
    // silent first-wins.
    let group_tail: [Option<usize>; 2] = [
        sole_init_tail(units, groups[0])?,
        sole_init_tail(units, groups[1])?,
    ];

    // ---- Per-unit func/let counts -------------------------------------------
    let code_names: Vec<std::collections::HashSet<&str>> = units
        .iter()
        .map(|unit| {
            unit.exports
                .objects
                .iter()
                .filter(|(_, reference)| matches!(reference, LocalRef::Code(_)))
                .map(|(name, _)| name.as_str())
                .collect()
        })
        .collect();
    let mut func_count = vec![0usize; units.len()];
    let mut let_count = vec![0usize; units.len()];
    for (u, unit) in units.iter().enumerate() {
        for (name, _) in &unit.exports.globals {
            if code_names[u].contains(name.as_str()) {
                func_count[u] += 1;
            } else {
                let_count[u] += 1;
            }
        }
    }

    // ---- Generic-value dedup layout (design §9 R1) --------------------------
    // A generic-function VALUE (`foo<int>` used as data → `Object::GenericFunction`)
    // is interned once per program; under per-unit reuse two units can each carry a
    // local copy (e.g. a dirty file emits `foo<int>` while a clean unit already
    // owns it). Keep the earliest in link order (the canonical owner) at the pool
    // position a full compile gives it; every later copy is a "shadow" the linker
    // skips, redirecting all references to the canonical index.
    //
    // `canonical_pos` maps a generic value's `(base fn name, type_args)` key to its
    // absolute pool index; `code_abs[u][k]` is the absolute pool index of unit u's
    // k-th `code` object (a shadow maps to its canonical's index and is itself not
    // appended). For a full compile each key has exactly one owner, so `shadow` is
    // empty and this reduces to the flat pass-major layout.
    let mut shadow: Vec<Vec<bool>> = units
        .iter()
        .map(|unit| vec![false; unit.code.len()])
        .collect();
    let mut code_abs: Vec<Vec<usize>> = units.iter().map(|u| vec![0usize; u.code.len()]).collect();
    let mut canonical_pos: HashMap<String, HashMap<Vec<crate::RealizedTy>, usize>> = HashMap::new();

    // ---- Object bucket bases (pass-major, group-major) ----------------------
    let mut layout = vec![UnitLayout::default(); units.len()];
    let mut obj_cursor = 0usize;
    for (g, group) in groups.iter().enumerate() {
        for &u in *group {
            layout[u].class_base = obj_cursor;
            obj_cursor += units[u].classes.len();
        }
        for &u in *group {
            layout[u].enum_base = obj_cursor;
            obj_cursor += units[u].enums.len();
        }
        for &u in *group {
            layout[u].iface_base = obj_cursor;
            obj_cursor += units[u].interfaces.len();
        }
        for &u in *group {
            layout[u].alias_base = obj_cursor;
            obj_cursor += units[u].type_alias_objects.len();
        }
        for &u in *group {
            layout[u].code_base = obj_cursor;
            let unit = &units[u];
            let n_local_globals = func_count[u] + let_count[u];
            let mut placed = 0usize;
            for (k, object) in unit.code.iter().enumerate() {
                if let Object::GenericFunction(gf) = object {
                    let base_name = generic_base_name(unit, gf.function.raw(), n_local_globals)?;
                    if let Some(&canon) = canonical_pos
                        .get(base_name.as_str())
                        .and_then(|by_args| by_args.get(gf.type_args.as_ref()))
                    {
                        // Duplicate of an earlier unit's generic value: shadow it.
                        shadow[u][k] = true;
                        code_abs[u][k] = canon;
                        continue;
                    }
                    let abs = obj_cursor + placed;
                    canonical_pos
                        .entry(base_name)
                        .or_default()
                        .insert(gf.type_args.to_vec(), abs);
                    code_abs[u][k] = abs;
                    placed += 1;
                } else {
                    code_abs[u][k] = obj_cursor + placed;
                    placed += 1;
                }
            }
            obj_cursor += placed;
        }
        // A group's `$init` tail objects append after its code, before the next
        // group's classes (design §9 R2).
        if let Some(tu) = group_tail[g] {
            obj_cursor += units[tu].init_tail.as_ref().map_or(0, |t| t.objects.len());
        }
    }

    // ---- Global-slot bases (functions then lets, then $init tail) -----------
    let mut slot_cursor = 0usize;
    // Absolute slot where each group's `$init` tail slots begin.
    let mut tail_slot_base = [0usize; 2];
    for (g, group) in groups.iter().enumerate() {
        for &u in *group {
            layout[u].func_count = func_count[u];
            layout[u].func_gbase = slot_cursor;
            slot_cursor += func_count[u];
        }
        for &u in *group {
            layout[u].let_gbase = slot_cursor;
            slot_cursor += let_count[u];
        }
        tail_slot_base[g] = slot_cursor;
        if let Some(tu) = group_tail[g] {
            slot_cursor += units[tu]
                .init_tail
                .as_ref()
                .map_or(0, |t| t.slot_objects.len());
        }
    }

    let mut program = Program::new();
    program.globals = vec![ConstValue::Null; slot_cursor];

    // ---- Name-resolution maps (from export tables) --------------------------
    // fq name -> absolute object index (classes/enums/interfaces/functions).
    let mut obj_by_name: HashMap<String, usize> = HashMap::new();
    for (u, unit) in units.iter().enumerate() {
        for (name, local_ref) in &unit.exports.objects {
            if !local_ref_in_bounds(unit, *local_ref) {
                return Err(LinkError::InvalidUnit(format!(
                    "unit {u} export `{name}` points outside its {local_ref:?} bucket"
                )));
            }
            // A `Code` export (named function) may sit after a shadowed generic in
            // its bucket, so its absolute index comes from `code_abs`, not the flat
            // base. Generic *values* are never exported, so a `Code` export is never
            // a shadow. A malformed export could name an out-of-range code slot;
            // `link` is fallible and runs on cache/relink paths, so index safely
            // instead of panicking. The sibling arms resolve through
            // `export_object_abs`, which is pure arithmetic and cannot panic.
            let abs = match local_ref {
                LocalRef::Code(k) => *code_abs[u].get(*k as usize).ok_or_else(|| {
                    LinkError::UnresolvedImport(format!(
                        "{name}: code export slot {k} out of range for unit {u}"
                    ))
                })?,
                other => export_object_abs(&layout[u], *other),
            };
            if obj_by_name.insert(name.clone(), abs).is_some() {
                return Err(LinkError::DuplicateExport(name.clone()));
            }
        }
    }

    // ---- Global-slot assignment (design §3b step 1) -------------------------
    for (u, unit) in units.iter().enumerate() {
        let n_local_globals = func_count[u] + let_count[u];
        let mut seen_local_globals = vec![false; n_local_globals];
        for (name, flat) in &unit.exports.globals {
            let flat = *flat as usize;
            let Some(seen) = seen_local_globals.get_mut(flat) else {
                return Err(invalid_index(u, "exported global", flat));
            };
            if std::mem::replace(seen, true) {
                return Err(LinkError::InvalidUnit(format!(
                    "unit {u} exports more than one global at local slot {flat}"
                )));
            }
            let slot = layout[u].local_global(flat);
            if code_names[u].contains(name.as_str()) {
                // A function: slot holds `Object(function object)`.
                let abs = *obj_by_name
                    .get(name.as_str())
                    .ok_or_else(|| LinkError::UnresolvedImport(name.clone()))?;
                program.globals[slot] = ConstValue::Object(ObjectIndex::from_raw(abs));
                if program.function_indices.insert(name.clone(), abs).is_some() {
                    return Err(LinkError::DuplicateExport(name.clone()));
                }
                program.function_global_indices.insert(name.clone(), slot);
            } else if program
                .let_global_indices
                .insert(name.clone(), slot)
                .is_some()
            {
                return Err(LinkError::DuplicateExport(name.clone()));
            }
        }
    }

    // ---- Resolve every unit's import tables ---------------------------------
    // Precompute, per unit, import-index -> absolute index so the operand patch
    // is a pure array lookup. Non-generic imports resolve against the maps above;
    // generic-function imports resolve through an intern map filled as
    // code is placed.
    // Global imports (functions / lets) resolve fully against the step-1 name
    // maps, up front — this also validates them (keeping the unresolved-import
    // contract for unused imports too).
    let resolve_global_import = |sym: &Symbol| -> Result<usize, LinkError> {
        match sym.kind {
            SymbolKind::Let => program
                .let_global_indices
                .get(sym.fq_name.as_str())
                .copied()
                .ok_or_else(|| LinkError::UnresolvedImport(sym.fq_name.clone())),
            _ => program
                .function_global_indices
                .get(sym.fq_name.as_str())
                .copied()
                .ok_or_else(|| LinkError::UnresolvedImport(sym.fq_name.clone())),
        }
    };
    let mut resolved_glob_imports: Vec<Vec<usize>> = Vec::with_capacity(units.len());
    for unit in units {
        let mut globs = Vec::with_capacity(unit.global_imports.len());
        for sym in &unit.global_imports {
            globs.push(resolve_global_import(sym)?);
        }
        resolved_glob_imports.push(globs);
    }

    let mut resolved_tail_imports: Vec<Option<ResolvedTailImports>> =
        Vec::with_capacity(group_tail.len());
    for tail_unit in group_tail {
        let Some(tail) = tail_unit.and_then(|u| units[u].init_tail.as_ref()) else {
            resolved_tail_imports.push(None);
            continue;
        };
        let objects = tail
            .object_imports
            .iter()
            .map(|symbol| resolve_object_import(symbol, &obj_by_name, &canonical_pos))
            .collect::<Result<_, _>>()?;
        let globals = tail
            .global_imports
            .iter()
            .map(&resolve_global_import)
            .collect::<Result<_, _>>()?;
        resolved_tail_imports.push(Some(ResolvedTailImports { objects, globals }));
    }

    // ---- Definition placement (design §3b step 2) ---------------------------
    // Classes, then enums, then interfaces — pass-major across the units of each
    // group. Definitions are inert to the operand walker, so they are cloned in
    // place. The push order here MUST match the base computation above.
    //
    // Generic-function values (design §9 R1) are interned across the whole program
    // at one canonical pool position (computed above into `canonical_pos`); a unit
    // owning the canonical copy keeps it in its `code` bucket, every other copy is
    // a shadow that is skipped here and whose references resolve to the canonical
    // via `code_abs` / `canonical_pos`.
    for (g, group) in groups.iter().enumerate() {
        for &u in *group {
            for object in &units[u].classes {
                program.objects.push(object.clone());
            }
        }
        for &u in *group {
            for object in &units[u].enums {
                program.objects.push(object.clone());
            }
        }
        for &u in *group {
            let unit = &units[u];
            // An interface's only operands are its default methods' pooled
            // bodies: code-bucket locals or object imports. Relocate them
            // through the same maps code objects use.
            let n_local_objects = unit.classes.len()
                + unit.enums.len()
                + unit.interfaces.len()
                + unit.type_alias_objects.len()
                + unit.code.len();
            let lay = layout[u];
            let c = unit.classes.len();
            let e = unit.enums.len();
            let i = unit.interfaces.len();
            let a = unit.type_alias_objects.len();
            let mut obj_imports = Vec::with_capacity(unit.object_imports.len());
            for sym in &unit.object_imports {
                obj_imports.push(resolve_object_import(sym, &obj_by_name, &canonical_pos)?);
            }
            for object in &unit.interfaces {
                let mut object = object.clone();
                relocate_object_operands(
                    &mut object,
                    |raw| {
                        if raw < n_local_objects {
                            if raw < c {
                                Some(lay.class_base + raw)
                            } else if raw < c + e {
                                Some(lay.enum_base + (raw - c))
                            } else if raw < c + e + i {
                                Some(lay.iface_base + (raw - c - e))
                            } else if raw < c + e + i + a {
                                Some(lay.alias_base + (raw - c - e - i))
                            } else {
                                code_abs[u].get(raw - c - e - i - a).copied()
                            }
                        } else {
                            obj_imports.get(raw - n_local_objects).copied()
                        }
                    },
                    // An interface holds no global-slot operands.
                    |_| None,
                    |space, raw| invalid_index(u, space, raw),
                )?;
                program.objects.push(object);
            }
        }
        for &u in *group {
            for object in &units[u].type_alias_objects {
                program.objects.push(object.clone());
            }
        }
        // ---- Code placement (design §3b step 3) -----------------------------
        for &u in *group {
            let unit = &units[u];
            let n_local_objects = unit.classes.len()
                + unit.enums.len()
                + unit.interfaces.len()
                + unit.type_alias_objects.len()
                + unit.code.len();
            let n_local_globals = func_count[u] + let_count[u];
            let lay = layout[u];
            let glob_imports = &resolved_glob_imports[u];
            let c = unit.classes.len();
            let e = unit.enums.len();
            let i = unit.interfaces.len();
            let a = unit.type_alias_objects.len();

            // Resolve this unit's object imports: non-generic against the name map,
            // generic against the whole-program intern map (`canonical_pos`).
            let mut obj_imports = Vec::with_capacity(unit.object_imports.len());
            for sym in &unit.object_imports {
                obj_imports.push(resolve_object_import(sym, &obj_by_name, &canonical_pos)?);
            }

            for (k, object) in unit.code.iter().enumerate() {
                // A shadowed generic value is a duplicate of the canonical copy —
                // do not append it; its references already resolve to the canonical.
                if shadow[u][k] {
                    continue;
                }
                let mut object = object.clone();
                relocate_object_operands(
                    &mut object,
                    |raw| {
                        if raw < n_local_objects {
                            if raw < c {
                                Some(lay.class_base + raw)
                            } else if raw < c + e {
                                Some(lay.enum_base + (raw - c))
                            } else if raw < c + e + i {
                                Some(lay.iface_base + (raw - c - e))
                            } else if raw < c + e + i + a {
                                Some(lay.alias_base + (raw - c - e - i))
                            } else {
                                // A code-bucket ref: use the shadow-aware map so a
                                // reference to a deduped generic value hits the
                                // canonical pool position.
                                code_abs[u].get(raw - c - e - i - a).copied()
                            }
                        } else {
                            obj_imports.get(raw - n_local_objects).copied()
                        }
                    },
                    |raw| {
                        if raw < n_local_globals {
                            Some(lay.local_global(raw))
                        } else {
                            glob_imports.get(raw - n_local_globals).copied()
                        }
                    },
                    |space, raw| invalid_index(u, space, raw),
                )?;
                program.objects.push(object);
            }
        }

        // ---- $init / $init_test tail placement (design §3b step 4 / §9 R2) --
        // Placed after all of this group's regular code, before the next group's
        // classes. Objects use the tail-local/import convention of `InitTail`.
        if let Some(tu) = group_tail[g]
            && let Some(tail) = &units[tu].init_tail
        {
            let tail_object_base = program.objects.len();
            let n_tail_objects = tail.objects.len();
            let n_tail_slots = tail.slot_objects.len();
            let slot_base = tail_slot_base[g];

            let imports = resolved_tail_imports[g]
                .as_ref()
                .expect("a present group tail has resolved imports");
            let obj_imports = &imports.objects;
            let glob_imports = &imports.globals;

            for object in &tail.objects {
                let mut object = object.clone();
                relocate_object_operands(
                    &mut object,
                    |raw| {
                        if raw < n_tail_objects {
                            Some(tail_object_base + raw)
                        } else {
                            obj_imports.get(raw - n_tail_objects).copied()
                        }
                    },
                    |raw| {
                        if raw < n_tail_slots {
                            Some(slot_base + raw)
                        } else {
                            glob_imports.get(raw - n_tail_slots).copied()
                        }
                    },
                    |space, raw| {
                        LinkError::InvalidUnit(format!("init tail has out-of-range {space} {raw}"))
                    },
                )?;
                program.objects.push(object);
            }

            // Every tail slot holds `Object(its owning tail object)`.
            for (ord, &tobj) in tail.slot_objects.iter().enumerate() {
                if tobj as usize >= n_tail_objects {
                    return Err(LinkError::InvalidUnit(format!(
                        "init tail slot {ord} points to out-of-range object {tobj}"
                    )));
                }
                program.globals[slot_base + ord] =
                    ConstValue::Object(ObjectIndex::from_raw(tail_object_base + tobj as usize));
            }
            // Register the named tail functions ($init / $init_test chainers).
            for (name, tobj) in &tail.named {
                let abs = tail_object_base + *tobj as usize;
                if obj_by_name.insert(name.clone(), abs).is_some() {
                    return Err(LinkError::DuplicateExport(name.clone()));
                }
                program.function_indices.insert(name.clone(), abs);
                let ord = tail
                    .slot_objects
                    .iter()
                    .position(|&x| x == *tobj)
                    .ok_or_else(|| LinkError::UnresolvedImport(name.clone()))?;
                program
                    .function_global_indices
                    .insert(name.clone(), slot_base + ord);
            }
            program
                .package_init_order
                .extend(tail.package_init_order.iter().cloned());
        }
    }

    // ---- Package merge (design §3b step 5) ----------------------------------
    // Each package's fragment is carried by exactly one unit (its first unit).
    // Merge every fragment into the image's `packages`, resolving each symbolic
    // fully-qualified name to an absolute object index, then re-sort exactly as
    // `build_packages` does so the serialized order is content-determined.
    for unit in units {
        merge_package_fragment(
            &mut program,
            &unit.package,
            &unit.package_fragment,
            &obj_by_name,
        )?;
    }
    sort_packages(&mut program);

    // ---- Whole-program tails (design §3b step 5) ----------------------------
    for unit in units {
        program.test_cases.extend(unit.test_cases.iter().cloned());
    }

    Ok(program)
}

/// Merge one unit's symbolic package fragment into the image's `packages` map,
/// resolving each fully-qualified name to an absolute object index.
fn merge_package_fragment(
    program: &mut Program,
    package: &Name,
    frag: &ProgramPackageFrag,
    obj_by_name: &HashMap<String, usize>,
) -> Result<(), LinkError> {
    if frag.exported_names.is_empty()
        && frag.classes.is_empty()
        && frag.enums.is_empty()
        && frag.interfaces.is_empty()
        && frag.functions.is_empty()
        && frag.impl_rules.is_empty()
        && frag.type_aliases.is_empty()
        && frag.interface_blob.is_empty()
        && frag.test_init.is_none()
    {
        // A unit that does not carry its package's fragment: nothing to merge,
        // but ensure an (empty) package entry exists if it will gain classes
        // from another unit's fragment. (No-op: `.entry` in the loops below
        // creates it on demand.)
        return Ok(());
    }
    let resolve = |fq: &str| -> Result<ObjectIndex, LinkError> {
        obj_by_name
            .get(fq)
            .copied()
            .map(ObjectIndex::from_raw)
            .ok_or_else(|| LinkError::UnresolvedImport(fq.to_string()))
    };
    let pkg = program.packages.entry(package.clone()).or_default();
    pkg.exported_names
        .extend(frag.exported_names.iter().cloned());
    for (local, fq) in &frag.classes {
        let abs = resolve(fq)?;
        pkg.classes.insert(local.clone(), abs);
    }
    for (local, fq) in &frag.enums {
        let abs = resolve(fq)?;
        pkg.enums.insert(local.clone(), abs);
    }
    for (local, fq) in &frag.interfaces {
        let abs = resolve(fq)?;
        pkg.interfaces.insert(local.clone(), abs);
    }
    for (local, fq) in &frag.functions {
        let abs = resolve(fq)?;
        pkg.functions.insert(local.clone(), abs);
    }
    for (local, fq) in &frag.type_aliases {
        let abs = resolve(fq)?;
        pkg.type_aliases.insert(local.clone(), abs);
    }
    if !frag.interface_blob.is_empty() {
        pkg.interface_blob.clone_from(&frag.interface_blob);
    }
    if let Some(test_init) = &frag.test_init {
        pkg.test_init = Some(resolve(test_init)?);
    }
    for (iface_fq, rules) in &frag.impl_rules {
        let interface_head = resolve(iface_fq)?;
        let mut built = Vec::with_capacity(rules.len());
        for rule in rules {
            let head = resolve(&rule.interface_head)?;
            let mut methods = indexmap::IndexMap::new();
            for (name, method) in &rule.methods {
                methods.insert(
                    name.clone(),
                    ProgramMethodImpl {
                        fqn: resolve(&method.fqn)?,
                        frame: method.frame.clone(),
                    },
                );
            }
            built.push(ProgramImplRule {
                interface_head: head,
                for_ty_pattern: rule.for_ty_pattern.clone(),
                generic_param_bounds: rule.generic_param_bounds.clone(),
                interface_args: rule.interface_args.clone(),
                interface_assoc: rule.interface_assoc.clone(),
                methods,
                field_links: rule.field_links.clone(),
            });
        }
        pkg.impl_rules
            .entry(interface_head)
            .or_default()
            .extend(built);
    }
    Ok(())
}

/// Re-sort every per-package map and the top-level `packages` map exactly as the
/// full compile's `build_packages` tail does, so the serialized order is
/// content-determined regardless of merge order. Per-package sorting is shared
/// with the full compile through [`ProgramPackage::sort_maps`] so the two paths
/// cannot drift.
fn sort_packages(program: &mut Program) {
    for pkg in program.packages.values_mut() {
        pkg.sort_maps();
    }
    program.packages.sort_keys();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Instruction, Object,
        bytecode::Bytecode,
        types::{Class, Function, FunctionCaptureProps, FunctionKind, FunctionOrigin},
        unit::{ExportTable, InitTail, ProgramPackageFrag},
    };

    fn func(name: &str, instructions: Vec<Instruction>) -> Object {
        let bytecode = Bytecode {
            instructions,
            ..Bytecode::default()
        };
        Object::Function(Box::new(Function {
            name: name.to_string(),
            source_file: "user.baml".to_string(),
            docstring: None,
            declared_name: None,
            arity: 0,
            real_local_count: 0,
            bytecode,
            kind: FunctionKind::Bytecode,
            local_names: Vec::new(),
            debug_locals: Vec::new(),
            span: baml_base::Span::fake(),
            return_type: crate::TyTemplate::BuiltinUnknown {
                attr: baml_type::TyAttr::default(),
            },
            param_names: Vec::new(),
            param_types: Vec::new(),
            param_has_default: Vec::new(),
            display_type_params: Vec::new(),
            generic_param_bounds: Vec::new(),
            display_param_types: Vec::new(),
            display_return_type: String::new(),
            throws_type: crate::TyTemplate::Never {
                attr: baml_type::TyAttr::default(),
            },
            origin: FunctionOrigin::UserDefined,
            body_meta: None,
            capture: FunctionCaptureProps::disabled(),
            function_id: 0,
            runtime_package: HeapPtr::null(),
        }))
    }

    fn class(name: &str, type_tag: i64) -> Object {
        Object::Class(Box::new(Class {
            name: baml_type::TypeName::local(baml_base::Name::new(name)),
            fields: Vec::new(),
            description: None,
            alias: None,
            docstring: None,
            other: indexmap::IndexMap::new(),
            type_tag: baml_type::typetag::TypeTag::from_i64(type_tag),
            ty_attr: baml_type::TyAttr::default(),
            has_cleanup: false,
            generic_param_count: 0,
            runtime_type: None,
        }))
    }

    /// A single local-only unit: one class + two functions that reference the
    /// class and each other through the per-unit convention.
    fn local_only_unit() -> CompilationUnit {
        use Instruction as I;
        CompilationUnit {
            source_file: "user.baml".to_string(),
            package: baml_base::Name::new("user"),
            classes: vec![class("MyClass", 100)],
            enums: Vec::new(),
            interfaces: Vec::new(),
            type_alias_objects: Vec::new(),
            code: vec![
                func(
                    "user.foo",
                    vec![
                        I::AllocInstance {
                            class_obj: ObjectIndex::from_raw(0),
                            ntypeargs: 0,
                        },
                        I::Call {
                            callee: GlobalIndex::from_raw(1),
                            ntypeargs: 0,
                        },
                        I::Return,
                    ],
                ),
                func(
                    "user.bar",
                    vec![I::LoadGlobal(GlobalIndex::from_raw(0)), I::Return],
                ),
            ],
            object_imports: Vec::new(),
            global_imports: Vec::new(),
            exports: ExportTable {
                objects: vec![
                    ("user.MyClass".to_string(), LocalRef::Class(0)),
                    ("user.foo".to_string(), LocalRef::Code(0)),
                    ("user.bar".to_string(), LocalRef::Code(1)),
                ],
                globals: vec![("user.foo".to_string(), 0), ("user.bar".to_string(), 1)],
            },
            package_fragment: ProgramPackageFrag::default(),
            test_cases: Vec::new(),
            callable_throws_fragment: Vec::new(),
            init_tail: None,
        }
    }

    fn operands_of(object: &Object) -> (Vec<usize>, Vec<usize>) {
        let Object::Function(function) = object else {
            panic!("expected a function object");
        };
        let mut globals = Vec::new();
        let mut objects = Vec::new();
        crate::relink::visit_index_operands_ref(function, |operand| match operand {
            crate::relink::IndexOperandRef::Global(slot) => globals.push(slot.raw()),
            crate::relink::IndexOperandRef::Object(obj) => objects.push(obj.raw()),
        });
        (globals, objects)
    }

    #[test]
    fn link_single_local_only_unit() {
        let unit = local_only_unit();
        let program = link(std::slice::from_ref(&unit)).expect("link");

        assert_eq!(program.objects.len(), 3);
        assert!(matches!(
            &program.objects[ObjectIndex::from_raw(0)],
            Object::Class(_)
        ));
        let Object::Function(foo) = &program.objects[ObjectIndex::from_raw(1)] else {
            panic!("object 1 should be user.foo");
        };
        assert_eq!(foo.name, "user.foo");

        let (foo_globals, foo_objects) = operands_of(&program.objects[ObjectIndex::from_raw(1)]);
        assert_eq!(foo_objects, vec![0], "AllocInstance class_obj rebased");
        assert_eq!(foo_globals, vec![1], "Call callee rebased");

        let (bar_globals, bar_objects) = operands_of(&program.objects[ObjectIndex::from_raw(2)]);
        assert_eq!(bar_globals, vec![0], "LoadGlobal rebased");
        assert!(bar_objects.is_empty());

        assert_eq!(program.globals.len(), 2);
        assert_eq!(
            program.globals[0],
            ConstValue::Object(ObjectIndex::from_raw(1)),
            "slot 0 = user.foo object"
        );
        assert_eq!(
            program.globals[1],
            ConstValue::Object(ObjectIndex::from_raw(2)),
            "slot 1 = user.bar object"
        );

        assert_eq!(program.function_indices.get("user.foo").copied(), Some(1));
        assert_eq!(program.function_indices.get("user.bar").copied(), Some(2));
        assert_eq!(
            program.function_global_indices.get("user.foo").copied(),
            Some(0)
        );
        assert_eq!(
            program.function_global_indices.get("user.bar").copied(),
            Some(1)
        );
        assert!(program.let_global_indices.is_empty());
    }

    #[test]
    fn link_two_units_with_cross_imports() {
        use Instruction as I;
        // Unit A: defines class A.C and function a.f (calls b.g, which is an
        // import).
        let unit_a = CompilationUnit {
            source_file: "a.baml".to_string(),
            package: baml_base::Name::new("user"),
            classes: vec![class("a.C", 100)],
            enums: Vec::new(),
            interfaces: Vec::new(),
            type_alias_objects: Vec::new(),
            // code[0] = a.f. n_local_objects = 1 class + 1 code = 2.
            // Object import 0 -> b.D at raw 2. Global import 0 -> b.g at raw 1.
            code: vec![func(
                "a.f",
                vec![
                    I::AllocInstance {
                        class_obj: ObjectIndex::from_raw(0), // local class A.C
                        ntypeargs: 0,
                    },
                    I::AllocInstance {
                        class_obj: ObjectIndex::from_raw(2), // import 0 -> b.D
                        ntypeargs: 0,
                    },
                    I::Call {
                        callee: GlobalIndex::from_raw(1), // import 0 -> b.g
                        ntypeargs: 0,
                    },
                    I::Return,
                ],
            )],
            object_imports: vec![Symbol {
                kind: SymbolKind::Class,
                fq_name: "b.D".to_string(),
                generic: None,
            }],
            global_imports: vec![Symbol {
                kind: SymbolKind::Function,
                fq_name: "b.g".to_string(),
                generic: None,
            }],
            exports: ExportTable {
                objects: vec![
                    ("a.C".to_string(), LocalRef::Class(0)),
                    ("a.f".to_string(), LocalRef::Code(0)),
                ],
                globals: vec![("a.f".to_string(), 0)],
            },
            package_fragment: ProgramPackageFrag::default(),
            test_cases: Vec::new(),
            callable_throws_fragment: Vec::new(),
            init_tail: None,
        };
        // Unit B: defines class b.D and function b.g.
        let unit_b = CompilationUnit {
            source_file: "b.baml".to_string(),
            package: baml_base::Name::new("user"),
            classes: vec![class("b.D", 101)],
            enums: Vec::new(),
            interfaces: Vec::new(),
            type_alias_objects: Vec::new(),
            code: vec![func("b.g", vec![I::Return])],
            object_imports: Vec::new(),
            global_imports: Vec::new(),
            exports: ExportTable {
                objects: vec![
                    ("b.D".to_string(), LocalRef::Class(0)),
                    ("b.g".to_string(), LocalRef::Code(0)),
                ],
                globals: vec![("b.g".to_string(), 0)],
            },
            package_fragment: ProgramPackageFrag::default(),
            test_cases: Vec::new(),
            callable_throws_fragment: Vec::new(),
            init_tail: None,
        };

        let program = link(&[unit_a, unit_b]).expect("link");
        // Pool order (pass-major): [a.C, b.D, a.f, b.g] = indices 0,1,2,3.
        assert_eq!(program.objects.len(), 4);
        assert_eq!(program.function_indices.get("a.f").copied(), Some(2));
        assert_eq!(program.function_indices.get("b.g").copied(), Some(3));

        let (f_globals, f_objects) = operands_of(&program.objects[ObjectIndex::from_raw(2)]);
        // local class A.C -> 0; import b.D -> 1; import global b.g -> slot 1.
        assert_eq!(f_objects, vec![0, 1], "a.f object operands");
        assert_eq!(f_globals, vec![1], "a.f global operand (b.g slot)");
    }

    #[test]
    fn link_rejects_unresolved_import() {
        let mut unit = local_only_unit();
        unit.object_imports.push(Symbol {
            kind: SymbolKind::Class,
            fq_name: "other.External".to_string(),
            generic: None,
        });
        match link(std::slice::from_ref(&unit)) {
            Err(LinkError::UnresolvedImport(name)) => assert_eq!(name, "other.External"),
            Err(e) => panic!("expected UnresolvedImport, got a different error: {e:?}"),
            Ok(_) => panic!("expected UnresolvedImport, got Ok"),
        }
    }

    /// Two units in the same (user) group each carrying an `init_tail` must be
    /// rejected — silently keeping the first would drop the second's
    /// `$init`/`$init_test` bytecode.
    #[test]
    fn link_rejects_two_init_tails_in_one_group() {
        let mut a = local_only_unit();
        a.source_file = "a.baml".to_string();
        a.init_tail = Some(InitTail::default());
        // A second user unit with disjoint names so name-dedup can't fire first.
        let mut b = local_only_unit();
        b.source_file = "b.baml".to_string();
        b.classes = vec![class("b.Other", 200)];
        b.code = vec![func("b.foo", vec![Instruction::Return])];
        b.exports = ExportTable {
            objects: vec![
                ("b.Other".to_string(), LocalRef::Class(0)),
                ("b.foo".to_string(), LocalRef::Code(0)),
            ],
            globals: vec![("b.foo".to_string(), 0)],
        };
        b.init_tail = Some(InitTail::default());

        match link(&[a, b]) {
            Err(LinkError::DuplicateExport(name)) => assert_eq!(name, "$init tail"),
            other => panic!("expected DuplicateExport for duplicate init tail, got {other:?}"),
        }
    }

    /// A malformed export naming a code slot past the end of the unit's `code`
    /// bucket must return an error, not panic — `link` runs on cache/relink paths
    /// where it must stay panic-free.
    #[test]
    fn link_rejects_out_of_range_code_export() {
        let mut unit = local_only_unit();
        unit.exports
            .objects
            .push(("user.bogus".to_string(), LocalRef::Code(99)));
        match link(std::slice::from_ref(&unit)) {
            Err(LinkError::InvalidUnit(_)) => {}
            other => {
                panic!("expected InvalidUnit for out-of-range code export, got {other:?}")
            }
        }
    }

    #[test]
    fn link_rejects_out_of_range_operand() {
        let mut unit = local_only_unit();
        let Object::Function(function) = &mut unit.code[0] else {
            panic!("fixture contains a function");
        };
        function
            .bytecode
            .instructions
            .insert(0, Instruction::AllocVariant(ObjectIndex::from_raw(99)));
        assert!(matches!(
            link(std::slice::from_ref(&unit)),
            Err(LinkError::InvalidUnit(_))
        ));
    }

    #[test]
    fn link_rejects_out_of_range_global_export() {
        let mut unit = local_only_unit();
        unit.exports.globals[0].1 = 99;
        assert!(matches!(
            link(std::slice::from_ref(&unit)),
            Err(LinkError::InvalidUnit(_))
        ));
    }
}
