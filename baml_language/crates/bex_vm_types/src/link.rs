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

use std::collections::HashMap;

use baml_base::Name;

use crate::{
    ConstValue, GlobalIndex, Object, ObjectIndex, Program,
    relink::{IndexOperand, visit_object_operands},
    types::{ProgramImplRule, ProgramMethodImpl},
    unit::{CompilationUnit, LocalRef, ProgramPackageFrag, Symbol, SymbolKind},
};

/// An error raised while linking symbolic units into a [`Program`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LinkError {
    /// An import named a fully-qualified name that no unit exports.
    UnresolvedImport(String),
    /// Two units export the same fully-qualified name.
    DuplicateExport(String),
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
    code_base: usize,
    /// Number of the unit's globals owned by functions (the rest are `let`s).
    func_count: usize,
    /// Absolute slot of the unit's first function global.
    func_gbase: usize,
    /// Absolute slot of the unit's first `let` global.
    let_gbase: usize,
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
        LocalRef::Code(k) => layout.code_base + k as usize,
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
    canonical_pos: &HashMap<(String, Vec<u8>), usize>,
) -> Result<usize, LinkError> {
    match sym.kind {
        SymbolKind::GenericFn => {
            let key = sym
                .generic
                .as_ref()
                .ok_or_else(|| LinkError::UnresolvedImport(sym.fq_name.clone()))?;
            let ta = borsh::to_vec(&key.type_args).expect("serialize type_args");
            canonical_pos
                .get(&(key.base_fn.clone(), ta))
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
    let mut func_count = vec![0usize; units.len()];
    let mut let_count = vec![0usize; units.len()];
    for (u, unit) in units.iter().enumerate() {
        // A global is a function iff its name is exported as a `Code` object.
        let code_names: std::collections::HashSet<&str> = unit
            .exports
            .objects
            .iter()
            .filter(|(_, r)| matches!(r, LocalRef::Code(_)))
            .map(|(n, _)| n.as_str())
            .collect();
        for (name, _) in &unit.exports.globals {
            if code_names.contains(name.as_str()) {
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
    let mut shadow: Vec<std::collections::HashSet<usize>> =
        vec![std::collections::HashSet::new(); units.len()];
    let mut code_abs: Vec<Vec<usize>> = units.iter().map(|u| vec![0usize; u.code.len()]).collect();
    let mut canonical_pos: HashMap<(String, Vec<u8>), usize> = HashMap::new();

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
            layout[u].code_base = obj_cursor;
            let unit = &units[u];
            let n_local_globals = func_count[u] + let_count[u];
            let mut placed = 0usize;
            for (k, object) in unit.code.iter().enumerate() {
                if let Object::GenericFunction(gf) = object {
                    let base_name = generic_base_name(unit, gf.function.raw(), n_local_globals)?;
                    let ta = borsh::to_vec(&gf.type_args.to_vec()).expect("serialize type_args");
                    let key = (base_name, ta);
                    if let Some(&canon) = canonical_pos.get(&key) {
                        // Duplicate of an earlier unit's generic value: shadow it.
                        shadow[u].insert(k);
                        code_abs[u][k] = canon;
                        continue;
                    }
                    let abs = obj_cursor + placed;
                    canonical_pos.insert(key, abs);
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
        let code_names: std::collections::HashSet<&str> = unit
            .exports
            .objects
            .iter()
            .filter(|(_, r)| matches!(r, LocalRef::Code(_)))
            .map(|(n, _)| n.as_str())
            .collect();
        for (name, flat) in &unit.exports.globals {
            let slot = layout[u].local_global(*flat as usize);
            if code_names.contains(name.as_str()) {
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
    // generic-function imports (Stage 2) resolve through an intern map filled as
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

    // Snapshot of function global slots for generic-fn base-function resolution
    // (needed while `program.objects` is being mutated during placement).
    let fn_gslot = program.function_global_indices.clone();
    let let_gslot = program.let_global_indices.clone();

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
            for object in &units[u].interfaces {
                program.objects.push(object.clone());
            }
        }
        // ---- Code placement (design §3b step 3) -----------------------------
        for &u in *group {
            let unit = &units[u];
            let n_local_objects =
                unit.classes.len() + unit.enums.len() + unit.interfaces.len() + unit.code.len();
            let n_local_globals = func_count[u] + let_count[u];
            let lay = layout[u];
            let glob_imports = &resolved_glob_imports[u];
            let c = unit.classes.len();
            let e = unit.enums.len();
            let i = unit.interfaces.len();

            // Resolve this unit's object imports: non-generic against the name map,
            // generic against the whole-program intern map (`canonical_pos`).
            let mut obj_imports = Vec::with_capacity(unit.object_imports.len());
            for sym in &unit.object_imports {
                obj_imports.push(resolve_object_import(sym, &obj_by_name, &canonical_pos)?);
            }

            for (k, object) in unit.code.iter().enumerate() {
                // A shadowed generic value is a duplicate of the canonical copy —
                // do not append it; its references already resolve to the canonical.
                if shadow[u].contains(&k) {
                    continue;
                }
                let mut object = object.clone();
                visit_object_operands(&mut object, |operand| match operand {
                    IndexOperand::Object(idx) => {
                        let raw = idx.raw();
                        let abs = if raw < n_local_objects {
                            if raw < c {
                                lay.class_base + raw
                            } else if raw < c + e {
                                lay.enum_base + (raw - c)
                            } else if raw < c + e + i {
                                lay.iface_base + (raw - c - e)
                            } else {
                                // A code-bucket ref: use the shadow-aware map so a
                                // reference to a deduped generic value hits the
                                // canonical pool position.
                                code_abs[u][raw - c - e - i]
                            }
                        } else {
                            obj_imports[raw - n_local_objects]
                        };
                        *idx = ObjectIndex::from_raw(abs);
                    }
                    IndexOperand::Global(slot) => {
                        let raw = slot.raw();
                        let abs = if raw < n_local_globals {
                            lay.local_global(raw)
                        } else {
                            glob_imports[raw - n_local_globals]
                        };
                        *slot = GlobalIndex::from_raw(abs);
                    }
                });
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

            // Resolve the tail's imports.
            let mut obj_imports = Vec::with_capacity(tail.object_imports.len());
            for sym in &tail.object_imports {
                obj_imports.push(resolve_object_import(sym, &obj_by_name, &canonical_pos)?);
            }
            let mut glob_imports = Vec::with_capacity(tail.global_imports.len());
            for sym in &tail.global_imports {
                let abs = match sym.kind {
                    SymbolKind::Let => let_gslot
                        .get(sym.fq_name.as_str())
                        .copied()
                        .ok_or_else(|| LinkError::UnresolvedImport(sym.fq_name.clone()))?,
                    _ => fn_gslot
                        .get(sym.fq_name.as_str())
                        .copied()
                        .ok_or_else(|| LinkError::UnresolvedImport(sym.fq_name.clone()))?,
                };
                glob_imports.push(abs);
            }

            for object in &tail.objects {
                let mut object = object.clone();
                visit_object_operands(&mut object, |operand| match operand {
                    IndexOperand::Object(idx) => {
                        let raw = idx.raw();
                        let abs = if raw < n_tail_objects {
                            tail_object_base + raw
                        } else {
                            obj_imports[raw - n_tail_objects]
                        };
                        *idx = ObjectIndex::from_raw(abs);
                    }
                    IndexOperand::Global(slot) => {
                        let raw = slot.raw();
                        let abs = if raw < n_tail_slots {
                            slot_base + raw
                        } else {
                            glob_imports[raw - n_tail_slots]
                        };
                        *slot = GlobalIndex::from_raw(abs);
                    }
                });
                program.objects.push(object);
            }

            // Every tail slot holds `Object(its owning tail object)`.
            for (ord, &tobj) in tail.slot_objects.iter().enumerate() {
                program.globals[slot_base + ord] =
                    ConstValue::Object(ObjectIndex::from_raw(tail_object_base + tobj as usize));
            }
            // Register the named tail functions ($init / $init_test chainers).
            for (name, tobj) in &tail.named {
                let abs = tail_object_base + *tobj as usize;
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
    // Template macros are joined by newlines, in unit (file) order.
    let mut macros: Vec<&str> = Vec::new();
    for unit in units {
        for m in &unit.template_macros {
            macros.push(m.as_str());
        }
    }
    program.template_strings_macros = macros.join("\n");

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
    if frag.classes.is_empty()
        && frag.enums.is_empty()
        && frag.interfaces.is_empty()
        && frag.impl_rules.is_empty()
        && frag.recursive_type_aliases.is_empty()
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
    for (local, ty) in &frag.recursive_type_aliases {
        pkg.recursive_type_aliases.insert(local.clone(), ty.clone());
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
/// content-determined regardless of merge order.
fn sort_packages(program: &mut Program) {
    for pkg in program.packages.values_mut() {
        pkg.classes.sort_keys();
        pkg.enums.sort_keys();
        pkg.recursive_type_aliases.sort_keys();
        pkg.interfaces.sort_keys();
        pkg.impl_rules.sort_keys();
        for rules in pkg.impl_rules.values_mut() {
            rules.sort_by_cached_key(|rule| {
                (
                    rule.for_ty_pattern.to_string(),
                    format!("{:?}", rule.for_ty_pattern),
                    format!("{:?}", rule.interface_args),
                    format!("{:?}", rule.interface_assoc),
                )
            });
        }
    }
    program.packages.sort_keys();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Instruction, Object,
        bytecode::Bytecode,
        relink::visit_index_operands,
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
            arity: 0,
            real_local_count: 0,
            bytecode,
            kind: FunctionKind::Bytecode,
            local_names: Vec::new(),
            debug_locals: Vec::new(),
            span: baml_base::Span::fake(),
            return_type: baml_type::RuntimeTy::unknown(),
            param_names: Vec::new(),
            param_types: Vec::new(),
            param_has_default: Vec::new(),
            display_type_params: Vec::new(),
            display_param_types: Vec::new(),
            display_return_type: String::new(),
            throws_type: None,
            origin: FunctionOrigin::UserDefined,
            body_meta: None,
            capture: FunctionCaptureProps::disabled(),
            function_id: 0,
        }))
    }

    fn class(name: &str, type_tag: i64) -> Object {
        Object::Class(Box::new(Class {
            name: baml_type::TypeName::local(baml_base::Name::new(name)),
            fields: Vec::new(),
            description: None,
            alias: None,
            type_tag,
            ty_attr: baml_type::TyAttr::default(),
            has_cleanup: false,
            generic_param_count: 0,
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
            lets: Vec::new(),
            package_fragment: ProgramPackageFrag::default(),
            template_macros: Vec::new(),
            test_cases: Vec::new(),
            throw_facts: Vec::new(),
            interface_fragment: Vec::new(),
            init_tail: None,
        }
    }

    fn operands_of(object: &Object) -> (Vec<usize>, Vec<usize>) {
        let Object::Function(function) = object else {
            panic!("expected a function object");
        };
        let mut function = (**function).clone();
        let mut globals = Vec::new();
        let mut objects = Vec::new();
        visit_index_operands(&mut function, |operand| match operand {
            IndexOperand::Global(slot) => globals.push(slot.raw()),
            IndexOperand::Object(obj) => objects.push(obj.raw()),
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
    fn link_is_stable_across_unit_round_trip() {
        let unit = local_only_unit();
        let program = link(std::slice::from_ref(&unit)).expect("link fresh");

        let bytes = borsh::to_vec(&unit).expect("serialize unit");
        let round_tripped: CompilationUnit = borsh::from_slice(&bytes).expect("deserialize unit");
        let program2 = link(std::slice::from_ref(&round_tripped)).expect("link round-tripped");

        assert_eq!(
            borsh::to_vec(&program).expect("serialize program"),
            borsh::to_vec(&program2).expect("serialize program2"),
            "link output must be identical for a round-tripped unit"
        );
    }

    /// Two local-only units that reference each other across the unit boundary
    /// through the import tables. Exercises pass-major placement + import
    /// resolution.
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
            lets: Vec::new(),
            package_fragment: ProgramPackageFrag::default(),
            template_macros: Vec::new(),
            test_cases: Vec::new(),
            throw_facts: Vec::new(),
            interface_fragment: Vec::new(),
            init_tail: None,
        };
        // Unit B: defines class b.D and function b.g.
        let unit_b = CompilationUnit {
            source_file: "b.baml".to_string(),
            package: baml_base::Name::new("user"),
            classes: vec![class("b.D", 101)],
            enums: Vec::new(),
            interfaces: Vec::new(),
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
            lets: Vec::new(),
            package_fragment: ProgramPackageFrag::default(),
            template_macros: Vec::new(),
            test_cases: Vec::new(),
            throw_facts: Vec::new(),
            interface_fragment: Vec::new(),
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
            Err(LinkError::UnresolvedImport(_)) => {}
            other => {
                panic!("expected UnresolvedImport for out-of-range code export, got {other:?}")
            }
        }
    }
}
