//! Native implementations for the `reflect` package's runtime classes.
//!
//! Currently only `reflect.Package` lives here. `reflect.type_of<T>()` is a
//! compiler intrinsic (lowered to a `LoadType` instruction at emit time) and
//! has no runtime counterpart.

use std::collections::HashMap;

use bex_vm_types::{
    ConstValue, GlobalIndex, HeapPtr, Instruction, Object, Package, PackageGlobals, Program, Value,
    types::Function,
};
use indexmap::IndexMap;

use super::{BamlClassReflectPackage, BamlNamespaceReflect, PackageBamlImpl};
use crate::{
    BexVm,
    errors::{VmBamlError, VmRustFnError},
};

/// Extract the [`ObjectIndex`] operand from an instruction if it has one.
///
/// Used by the `add_compile` lift to find every object-pool reference in a
/// runtime function's bytecode so they can be lifted into the function's
/// per-function `aux_object_ptrs` pool. `IsType` is omitted: its operand
/// indexes into `bytecode.constants` (a `ConstValue`), not the object pool,
/// so it's covered by `resolve_runtime_const` instead.
fn instruction_object_idx(instr: &Instruction) -> Option<bex_vm_types::ObjectIndex> {
    match *instr {
        Instruction::AllocInstance { class_obj, .. } => Some(class_obj),
        Instruction::AllocVariant(idx) => Some(idx),
        Instruction::MakeClosure { obj_idx, .. } => Some(obj_idx),
        _ => None,
    }
}

/// Set the [`ObjectIndex`] operand on an instruction that has one.
///
/// Returns `false` for instructions without an `ObjectIndex` operand — those
/// are no-ops for the aux-pool rewrite pass.
fn set_instruction_object_idx(instr: &mut Instruction, idx: bex_vm_types::ObjectIndex) -> bool {
    match instr {
        Instruction::AllocInstance { class_obj, .. } => {
            *class_obj = idx;
            true
        }
        Instruction::AllocVariant(slot) => {
            *slot = idx;
            true
        }
        Instruction::MakeClosure { obj_idx, .. } => {
            *obj_idx = idx;
            true
        }
        _ => false,
    }
}

/// Extract the [`GlobalIndex`] operand from an instruction if it has one.
///
/// Used by the `add_compile` lift to find every global slot reference in a
/// runtime function's bytecode so they can be remapped from the build-time
/// emit's flat slot space to per-package slot indices.
fn instruction_global_slot(instr: &Instruction) -> Option<GlobalIndex> {
    match *instr {
        Instruction::LoadGlobal(s)
        | Instruction::StoreGlobal(s)
        | Instruction::DispatchFuture(s)
        | Instruction::MakeBoundMethod(s) => Some(s),
        Instruction::Call { callee, .. } => Some(callee),
        _ => None,
    }
}

/// Set the [`GlobalIndex`] operand on an instruction that has one.
///
/// Returns `false` for instructions that don't carry a `GlobalIndex` — those
/// are no-ops for the remap pass.
fn set_instruction_global_slot(instr: &mut Instruction, slot: GlobalIndex) -> bool {
    match instr {
        Instruction::LoadGlobal(s)
        | Instruction::StoreGlobal(s)
        | Instruction::DispatchFuture(s)
        | Instruction::MakeBoundMethod(s) => {
            *s = slot;
            true
        }
        Instruction::Call { callee, .. } => {
            *callee = slot;
            true
        }
        _ => false,
    }
}

/// Convert a `ConstValue` to a `Value` for a runtime-lifted function.
///
/// Handles primitives directly. `ConstValue::Type` and
/// `ConstValue::ClassWithTypeArgs` resolve to `Value::Null`: build-time emit
/// does the same (see `BexHeap::resolve_function_constants`) because those
/// instructions read the raw `ConstValue` from `bytecode.constants` at
/// execution time rather than the resolved slot.
///
/// `ConstValue::Object(idx)` looks up `program.objects[idx]` and routes:
/// - `Object::String` → a fresh gen0 allocation (each lifted function gets
///   its own copy; no string interning across lifts).
/// - `Object::Function(f)` → existing item / external lookup, matching the
///   per-package slot allocator used for `Call` instructions.
/// - Other `Object` kinds (Class, Enum, …) currently throw `Unsupported`;
///   they land alongside class/enum lifting.
fn resolve_runtime_const(
    vm: &mut BexVm,
    cv: &ConstValue,
    program: &Program,
    pkg_dot: &str,
    name_to_ptr: &HashMap<String, HeapPtr>,
) -> Result<Value, VmRustFnError> {
    match cv {
        ConstValue::Null => Ok(Value::Null),
        ConstValue::Bool(b) => Ok(Value::Bool(*b)),
        ConstValue::Int(i) => Ok(Value::Int(*i)),
        ConstValue::Float(f) => Ok(Value::Float(*f)),
        ConstValue::OmittedArg => Ok(Value::OmittedArg),
        // Both are evaluated lazily by `LoadType` / `IsType` reading from
        // `bytecode.constants` directly; the resolved slot is unused.
        ConstValue::Type(_) | ConstValue::ClassWithTypeArgs { .. } => Ok(Value::Null),
        ConstValue::Object(idx) => {
            let raw = idx.into_raw();
            let obj = program
                .objects
                .get(raw)
                .ok_or_else(|| VmBamlError::Unsupported {
                    message: format!(
                        "reflect.Package.add_compile: ObjectIndex {raw} out of range \
                     (program.objects.len() = {})",
                        program.objects.len()
                    ),
                })?;
            match obj {
                Object::String(s) => Ok(vm.alloc_string(s.clone())),
                Object::Function(f) => {
                    let fqn = &f.name;
                    if let Some(local) = fqn.strip_prefix(pkg_dot) {
                        let ptr =
                            *name_to_ptr
                                .get(local)
                                .ok_or_else(|| VmBamlError::Unsupported {
                                    message: format!(
                                        "reflect.Package.add_compile: same-package function \
                                     constant {fqn} not found in package items"
                                    ),
                                })?;
                        Ok(Value::Object(ptr))
                    } else {
                        let ptr = vm.find_function_by_name(fqn).ok_or_else(|| {
                            VmBamlError::Unsupported {
                                message: format!(
                                    "reflect.Package.add_compile: external function \
                                     constant {fqn} not found in engine globals"
                                ),
                            }
                        })?;
                        Ok(Value::Object(ptr))
                    }
                }
                other => Err(VmBamlError::Unsupported {
                    message: format!(
                        "reflect.Package.add_compile: ConstValue::Object refers to \
                         unsupported object kind: {other:?}"
                    ),
                }
                .into()),
            }
        }
    }
}

/// Object kind tag for `ObjectIndex` resolution during the lift. Carried in
/// the index map so `AllocInstance`/`AllocVariant`/`MakeClosure` `ObjectIndex`
/// operands route to the right same-package or external lookup table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LiftedObjectKind {
    Function,
    Class,
    Enum,
}

/// Lift the runtime package's items from a freshly compiled `Program` into
/// `pkg_ptr`'s `Package.items` + `Package.globals.Dynamic`, preserving
/// `HeapPtr` identity for items defined in earlier `add_compile` batches.
///
/// For each new function whose `package_name` matches this package:
/// 1. Walk its bytecode for `GlobalIndex` operands → build per-package slot map.
/// 2. Walk its bytecode for `ObjectIndex` operands (`AllocInstance`,
///    `AllocVariant`, `MakeClosure`) → populate `aux_object_ptrs`. Same-package
///    targets resolve through `pkg.items`; external targets resolve through
///    the engine's resolved-name tables (functions / classes / enums).
/// 3. Rewrite the bytecode's `GlobalIndex` operands from flat to per-package
///    slots and rewrite `ObjectIndex` operands from the program object pool
///    to aux-pool slots. Regenerate the compact form.
/// 4. Resolve the function's bytecode constants (`ConstValue::*`) to
///    runtime `Value::*`, allocating strings into gen0.
/// 5. Allocate `Object::Function` in gen0 and register it in `pkg.items`
///    under its local name.
/// 6. Append the new slot assignments to `pkg.globals.Dynamic` and
///    `pkg.function_slot_map`.
///
/// Classes and enums whose FQN's package prefix matches `pkg_name` are also
/// lifted into gen0 and registered in `pkg.items` under their local names.
fn lift_runtime_package(
    vm: &mut BexVm,
    pkg_ptr: HeapPtr,
    pkg_name: &str,
    program: &Program,
) -> Result<(), VmRustFnError> {
    let pkg_dot = format!("{pkg_name}.");

    // Snapshot the package's existing state. `function_slot_map` and the
    // current length of `Dynamic` globals are what makes batch N+1's slot
    // assignments *append* to batch N's — identity preservation hinges on
    // never reassigning an existing slot.
    let (existing_items, mut slot_map, existing_globals_len) = match vm.get_object(pkg_ptr) {
        Object::Package(pkg) => {
            let g_len = match &pkg.globals {
                PackageGlobals::Dynamic(v) => v.len(),
                PackageGlobals::Static(_) => {
                    return Err(VmBamlError::Unsupported {
                        message: "reflect.Package.add_compile: package has Static globals \
                                  (build-time package); runtime mutation not permitted"
                            .to_string(),
                    }
                    .into());
                }
            };
            (pkg.items.clone(), pkg.function_slot_map.clone(), g_len)
        }
        _ => unreachable!("pkg_ptr was validated as Object::Package by caller"),
    };

    // F13: cache external function FQN → HeapPtr in a single up-front scan of
    // the engine's frozen globals. Without this, every same-batch instruction
    // that references an external callee would scan all globals linearly.
    let external_fn_cache: HashMap<String, HeapPtr> = {
        let mut cache: HashMap<String, HeapPtr> = HashMap::new();
        for v in vm.globals.as_slice(vm.proof()) {
            if let Value::Object(ptr) = v
                && let Object::Function(f) = vm.get_object(*ptr)
            {
                cache.insert(f.name.clone(), *ptr);
            }
        }
        cache
    };

    // Index program.objects by ObjectIndex → (FQN, kind). Covers functions,
    // classes, and enums; needed both for `GlobalIndex` (Call/LoadGlobal
    // target functions) and `ObjectIndex` (AllocInstance class targets,
    // AllocVariant enum targets, MakeClosure function targets).
    let object_index_lookup: HashMap<usize, (String, LiftedObjectKind)> = program
        .objects
        .iter()
        .enumerate()
        .filter_map(|(idx, obj)| match obj {
            Object::Function(f) => Some((idx, (f.name.clone(), LiftedObjectKind::Function))),
            Object::Class(c) => Some((idx, (c.name.to_string(), LiftedObjectKind::Class))),
            Object::Enum(e) => Some((idx, (e.name.to_string(), LiftedObjectKind::Enum))),
            _ => None,
        })
        .collect();

    // Collect items new to this batch. An item is "new" iff its FQN's package
    // prefix matches `pkg_name` AND its local name isn't already in `pkg.items`
    // (identity is preserved across batches by skipping existing entries).
    //
    // `package_name` on `Function` is set by emit; for `Class`/`Enum` we
    // check the FQN prefix directly via `pkg_dot` since they don't carry an
    // explicit package field.
    let mut new_functions: Vec<Function> = Vec::new();
    let mut new_classes: Vec<bex_vm_types::Class> = Vec::new();
    let mut new_enums: Vec<bex_vm_types::Enum> = Vec::new();
    for obj in &program.objects {
        match obj {
            Object::Function(f) if f.package_name == pkg_name => {
                let local = f
                    .name
                    .strip_prefix(pkg_dot.as_str())
                    .unwrap_or(f.name.as_str());
                if !existing_items.contains_key(local) {
                    new_functions.push((**f).clone());
                }
            }
            Object::Class(c) => {
                let fqn = c.name.to_string();
                if let Some(local) = fqn.strip_prefix(pkg_dot.as_str())
                    && !existing_items.contains_key(local)
                {
                    new_classes.push((**c).clone());
                }
            }
            Object::Enum(e) => {
                let fqn = e.name.to_string();
                if let Some(local) = fqn.strip_prefix(pkg_dot.as_str())
                    && !existing_items.contains_key(local)
                {
                    new_enums.push((**e).clone());
                }
            }
            _ => {}
        }
    }

    // Pass A: pre-allocate gen0 shells for new functions, classes, and enums
    // so we have stable HeapPtrs available before any bytecode rewrite. The
    // bytecode is still in flat-slot form here; pass B mutates each function
    // in place via `get_object_mut`.
    //
    // `name_to_ptr` maps every local name (existing + new, all kinds) to its
    // HeapPtr; the slot map and aux pool population both consult it for
    // same-package targets.
    let mut name_to_ptr: HashMap<String, HeapPtr> = existing_items;
    let mut newly_allocated: Vec<(String, HeapPtr)> = Vec::new();

    // Functions first — we need their HeapPtrs for `resolve_runtime_const`
    // when walking constants in pass B.
    let mut newly_allocated_fns: Vec<(String, HeapPtr)> = Vec::with_capacity(new_functions.len());
    // Consume by value so each allocation is `Box::new(func)` (no double box
    // via `Box::new(func.clone())`). `new_functions` is no longer needed
    // after this loop.
    for mut func in new_functions {
        let local_name = func
            .name
            .strip_prefix(pkg_dot.as_str())
            .unwrap_or(func.name.as_str())
            .to_string();
        // Tag the function with its owning package so runtime dispatch
        // (`resolve_frame_package`) reads `pkg.globals` for `Call`/`LoadGlobal`.
        func.package = pkg_ptr;
        let ptr = vm.tlab.alloc(Object::Function(Box::new(func)));
        name_to_ptr.insert(local_name.clone(), ptr);
        newly_allocated_fns.push((local_name.clone(), ptr));
        newly_allocated.push((local_name, ptr));
    }
    // Then classes and enums. They have no rewrite step — just allocate and
    // index.
    for class in new_classes {
        let local_name = class
            .name
            .to_string()
            .strip_prefix(pkg_dot.as_str())
            .unwrap_or(class.name.display_name.as_str())
            .to_string();
        let ptr = vm.tlab.alloc(Object::Class(Box::new(class)));
        name_to_ptr.insert(local_name.clone(), ptr);
        newly_allocated.push((local_name, ptr));
    }
    for enm in new_enums {
        let local_name = enm
            .name
            .to_string()
            .strip_prefix(pkg_dot.as_str())
            .unwrap_or(enm.name.display_name.as_str())
            .to_string();
        let ptr = vm.tlab.alloc(Object::Enum(Box::new(enm)));
        name_to_ptr.insert(local_name.clone(), ptr);
        newly_allocated.push((local_name, ptr));
    }

    // Pass B: for each newly-allocated function, walk its bytecode for
    // global-slot references AND object-pool references, build the slot map
    // and aux pool, then rewrite the operands and regenerate the compact form.
    let mut new_global_entries: Vec<Value> = Vec::new();
    let mut next_slot = existing_globals_len;

    for (_local_name, fn_ptr) in &newly_allocated_fns {
        // Snapshot instructions to release the heap borrow before mutating.
        let instructions_snapshot: Vec<Instruction> = match vm.get_object(*fn_ptr) {
            Object::Function(f) => f.bytecode.instructions.clone(),
            _ => unreachable!(),
        };

        // ─── Pass B.1: GlobalIndex slot map ───
        for instr in &instructions_snapshot {
            let Some(flat_slot) = instruction_global_slot(instr) else {
                continue;
            };
            let flat_idx = flat_slot.into_raw();
            let cv = program
                .globals
                .get(flat_idx)
                .ok_or_else(|| VmBamlError::Unsupported {
                    message: format!(
                        "reflect.Package.add_compile: instruction references slot {flat_idx} \
                     out of range (program.globals.len() = {})",
                        program.globals.len()
                    ),
                })?;
            let ConstValue::Object(obj_idx) = cv else {
                return Err(VmBamlError::Unsupported {
                    message: format!(
                        "reflect.Package.add_compile: global slot {flat_idx} has \
                         non-function value (kind: {cv:?}); only function targets are \
                         supported in this commit"
                    ),
                }
                .into());
            };
            let (fqn, kind) = object_index_lookup
                .get(&obj_idx.into_raw())
                .cloned()
                .ok_or_else(|| VmBamlError::Unsupported {
                    message: format!(
                        "reflect.Package.add_compile: slot {flat_idx} points to ObjectIndex \
                         {obj_idx:?} which isn't a Function/Class/Enum"
                    ),
                })?;
            if kind != LiftedObjectKind::Function {
                return Err(VmBamlError::Unsupported {
                    message: format!(
                        "reflect.Package.add_compile: global slot {flat_idx} targets a \
                         non-function object ({fqn}, kind={kind:?}); only function targets \
                         are supported in global slots"
                    ),
                }
                .into());
            }
            if slot_map.contains_key(&fqn) {
                continue;
            }
            // Resolve the callee. Same-package callees come from `name_to_ptr`
            // (existing items + newly-allocated). External callees come from
            // the up-front `external_fn_cache` built from the engine's globals.
            let target_ptr = if let Some(local) = fqn.strip_prefix(pkg_dot.as_str()) {
                *name_to_ptr
                    .get(local)
                    .ok_or_else(|| VmBamlError::Unsupported {
                        message: format!(
                            "reflect.Package.add_compile: same-package callee {fqn} not found \
                         in package items"
                        ),
                    })?
            } else {
                *external_fn_cache
                    .get(&fqn)
                    .ok_or_else(|| VmBamlError::Unsupported {
                        message: format!(
                            "reflect.Package.add_compile: external callee {fqn} not found \
                         in the engine's globals; this typically means the callee \
                         lives in a package not currently loaded"
                        ),
                    })?
            };
            slot_map.insert(fqn, next_slot);
            new_global_entries.push(Value::Object(target_ptr));
            next_slot += 1;
        }

        // ─── Pass B.2: ObjectIndex → aux pool ───
        // For each AllocInstance/AllocVariant/MakeClosure operand, resolve
        // the target HeapPtr and assign it a slot in the function's aux pool.
        // `aux_index_for` deduplicates within a single function so multiple
        // references to the same target share one aux slot.
        let mut aux_pool: Vec<HeapPtr> = Vec::new();
        let mut aux_index_for: HashMap<usize, usize> = HashMap::new();
        for instr in &instructions_snapshot {
            let Some(obj_idx) = instruction_object_idx(instr) else {
                continue;
            };
            let raw = obj_idx.into_raw();
            if aux_index_for.contains_key(&raw) {
                continue;
            }
            let (fqn, kind) =
                object_index_lookup
                    .get(&raw)
                    .cloned()
                    .ok_or_else(|| VmBamlError::Unsupported {
                        message: format!(
                            "reflect.Package.add_compile: ObjectIndex {raw} in instruction \
                         {instr:?} doesn't match any Function/Class/Enum in the program \
                         object pool"
                        ),
                    })?;
            // Resolve same-package vs external:
            //  - Same-package: look up local name in `name_to_ptr`.
            //  - External: look up by FQN in the appropriate engine map.
            let target_ptr = if let Some(local) = fqn.strip_prefix(pkg_dot.as_str()) {
                *name_to_ptr
                    .get(local)
                    .ok_or_else(|| VmBamlError::Unsupported {
                        message: format!(
                            "reflect.Package.add_compile: same-package {kind:?} target {fqn} \
                         not found in package items"
                        ),
                    })?
            } else {
                match kind {
                    LiftedObjectKind::Function => {
                        *external_fn_cache
                            .get(&fqn)
                            .ok_or_else(|| VmBamlError::Unsupported {
                                message: format!(
                                    "reflect.Package.add_compile: external function {fqn} \
                             referenced via ObjectIndex not found in engine globals"
                                ),
                            })?
                    }
                    // BAML's runtime type namespace is unified — classes and
                    // enums share `vm.resolved_class_names` (FQNs are unique
                    // across both kinds; see [crates/bex_vm/src/vm.rs](crates/bex_vm/src/vm.rs)
                    // where enum entries are folded into the same map).
                    LiftedObjectKind::Class | LiftedObjectKind::Enum => *vm
                        .resolved_class_names
                        .get(&fqn)
                        .ok_or_else(|| VmBamlError::Unsupported {
                            message: format!(
                                "reflect.Package.add_compile: external {kind:?} {fqn} not \
                                 found in engine class/enum registry"
                            ),
                        })?,
                }
            };
            let slot = aux_pool.len();
            aux_pool.push(target_ptr);
            aux_index_for.insert(raw, slot);
        }

        // Resolve constants for this function.
        let constants_snapshot: Vec<ConstValue> = match vm.get_object(*fn_ptr) {
            Object::Function(f) => f.bytecode.constants.clone(),
            _ => unreachable!(),
        };
        let mut resolved: Vec<Value> = Vec::with_capacity(constants_snapshot.len());
        for cv in &constants_snapshot {
            resolved.push(resolve_runtime_const(
                vm,
                cv,
                program,
                &pkg_dot,
                &name_to_ptr,
            )?);
        }

        // Second pass: mutate the function in place.
        //   1. Rewrite each instruction's GlobalIndex from flat → per-pkg slot.
        //   2. Rewrite each ObjectIndex operand from program-pool → aux-pool.
        //   3. Install `aux_object_ptrs` so `BexVm::resolve_obj_idx` routes
        //      ObjectIndex operands through it at runtime.
        //   4. Set resolved_constants.
        //   5. Re-lower to compact form so the VM's fast-path dispatch sees
        //      the rewritten operands.
        match vm.get_object_mut(*fn_ptr) {
            Object::Function(f) => {
                for instr in &mut f.bytecode.instructions {
                    if let Some(flat_slot) = instruction_global_slot(instr) {
                        let cv = &program.globals[flat_slot.into_raw()];
                        let ConstValue::Object(obj_idx) = cv else {
                            unreachable!("guarded above");
                        };
                        let (fqn, _kind) = &object_index_lookup[&obj_idx.into_raw()];
                        let new_slot = slot_map[fqn];
                        set_instruction_global_slot(instr, GlobalIndex::from_raw(new_slot));
                    }
                    if let Some(obj_idx) = instruction_object_idx(instr) {
                        let new_idx = aux_index_for[&obj_idx.into_raw()];
                        set_instruction_object_idx(
                            instr,
                            bex_vm_types::ObjectIndex::from_raw(new_idx),
                        );
                    }
                }
                f.aux_object_ptrs = aux_pool;
                f.bytecode.resolved_constants = resolved;
                f.bytecode.compact = Some(f.bytecode.lower_to_compact());
            }
            _ => unreachable!(),
        }
    }

    // Write barriers: every object reference that's about to be written
    // into `pkg.items` or `pkg.globals.Dynamic` must be reported to the
    // GC's card table so an older-generation `pkg` correctly tracks
    // outgoing references to younger-generation items. Call these BEFORE
    // `get_object_mut` so the heap borrow doesn't conflict with the
    // mutable package borrow. Matches the pattern at SetField /
    // ArraySet / MapSet (see [crates/bex_vm/src/vm.rs](crates/bex_vm/src/vm.rs)).
    for (_name, ptr) in &newly_allocated {
        vm.heap.write_barrier(pkg_ptr, Value::Object(*ptr));
    }
    for entry in &new_global_entries {
        vm.heap.write_barrier(pkg_ptr, *entry);
    }

    // Commit the new state to the package.
    match vm.get_object_mut(pkg_ptr) {
        Object::Package(pkg) => {
            for (name, ptr) in newly_allocated {
                pkg.items.insert(name, ptr);
            }
            pkg.function_slot_map = slot_map;
            match &mut pkg.globals {
                PackageGlobals::Dynamic(v) => v.extend(new_global_entries),
                PackageGlobals::Static(_) => unreachable!("checked at function entry"),
            }
        }
        _ => unreachable!(),
    }

    Ok(())
}

/// RAII guard wrapping the Salsa runtime files input around an `add_compile`
/// batch. Snapshots the runtime files at construction; reverts on drop unless
/// `commit()` was called. Makes `add_compile` atomic with respect to the
/// project database: a failure (or panic) anywhere in the batch leaves the
/// Salsa input exactly as it was before the call.
struct RuntimeBatchGuard<'a> {
    db: &'a mut baml_project::ProjectDatabase,
    /// `Some` until committed; cleared by `commit()` to suppress the rollback.
    snapshot: Option<Vec<baml_project::SourceFile>>,
}

impl RuntimeBatchGuard<'_> {
    /// Mark the batch as committed so the rollback in `Drop` is skipped.
    fn commit(mut self) {
        self.snapshot = None;
    }
}

impl Drop for RuntimeBatchGuard<'_> {
    fn drop(&mut self) {
        if let Some(snapshot) = self.snapshot.take() {
            self.db.set_runtime_files(snapshot);
        }
    }
}

/// Unwrap a `reflect.Package` BAML instance value to the inner
/// `Object::Package` `HeapPtr` it wraps.
///
/// `reflect.Package.new()` allocates an `Object::Instance` of class
/// `reflect.Package` whose single `_inner` field holds a
/// `Value::Object(<package primitive>)`. This helper performs that two-step
/// unwrap and returns an error if the input isn't shaped that way (which
/// can only happen if a caller built a `reflect.Package`-typed value by
/// some path other than `reflect.Package.new`).
fn unwrap_package_handle(vm: &BexVm, package: &Value) -> Result<HeapPtr, VmRustFnError> {
    let Value::Object(inst_ptr) = *package else {
        return Err(VmBamlError::InvalidArgument {
            message: "reflect.Package method called on non-instance value".to_string(),
        }
        .into());
    };
    let Object::Instance(inst) = vm.get_object(inst_ptr) else {
        return Err(VmBamlError::InvalidArgument {
            message: "reflect.Package method receiver is not an Instance".to_string(),
        }
        .into());
    };
    let Some(Value::Object(pkg_ptr)) = inst.fields.first().copied() else {
        return Err(VmBamlError::InvalidArgument {
            message: "reflect.Package instance has no `_inner` field".to_string(),
        }
        .into());
    };
    if !matches!(vm.get_object(pkg_ptr), Object::Package(_)) {
        return Err(VmBamlError::InvalidArgument {
            message: "reflect.Package._inner is not an Object::Package".to_string(),
        }
        .into());
    }
    Ok(pkg_ptr)
}

impl BamlClassReflectPackage for PackageBamlImpl {
    /// Allocate a fresh, empty runtime-compiled `Package` and return it
    /// wrapped in a `reflect.Package` class instance.
    ///
    /// Two heap allocations:
    ///
    /// 1. The `Object::Package` primitive — runtime packages get
    ///    `PackageGlobals::Dynamic(vec![])` (their own slot space, no shared
    ///    backing with the engine's globals) and an empty items map.
    /// 2. An `Object::Instance` of class `reflect.Package` whose single
    ///    field `_inner` holds a `Value::Object(<primitive ptr>)`. This is
    ///    what's returned to BAML; users hold the instance, methods unwrap
    ///    `_inner` to reach the primitive.
    ///
    /// The wrapper exists so `reflect.Package` is a normal user-facing
    /// class with regular instance semantics — at-call-site code can pass
    /// it around, store it in typed fields, etc. — while the primitive
    /// `Object::Package` keeps the dispatch-relevant state (items, per-pkg
    /// globals) the VM cares about.
    ///
    /// Each new runtime package is tagged with `_pkg_{n}` where `n` is
    /// the next slot from the engine's `runtime_pkg_counter`. The name is
    /// what `add_compile` uses to prefix file paths (`<runtime>/_pkg_{n}/…`)
    /// so the existing `file_package` resolver routes runtime files back
    /// to this package. Atomic across concurrent `Package.new` calls.
    ///
    /// For VMs constructed via `BexVm::from_program` (no engine attached)
    /// the counter is `None`; we fall back to an empty name, which makes
    /// such packages unusable for `add_compile` but harmless for tests
    /// that only need a placeholder handle.
    fn new(vm: &mut BexVm) -> Value {
        let name = vm
            .runtime_pkg_counter
            .as_ref()
            .map(|c| {
                let n = c.fetch_add(1, ::core::sync::atomic::Ordering::Relaxed);
                format!("_pkg_{n}")
            })
            .unwrap_or_default();
        let pkg = Package {
            name,
            items: HashMap::new(),
            globals: PackageGlobals::Dynamic(Vec::new()),
            function_slot_map: HashMap::new(),
        };
        let pkg_ptr = vm.tlab.alloc(Object::Package(Box::new(pkg)));

        let class_ptr = *vm
            .resolved_class_names
            .get("reflect.Package")
            .unwrap_or_else(|| {
                unreachable!("reflect.Package class must be registered by engine init");
            });
        let inst_ptr = vm
            .tlab
            .alloc_instance(class_ptr, vec![Value::Object(pkg_ptr)]);
        Value::Object(inst_ptr)
    }

    /// Compile a batch of source files into this runtime package.
    ///
    /// For each `(path, source)` in `files`, inserts a runtime source
    /// file at `<runtime>/{pkg_name}/{path}` into `Compiler2RuntimeFiles`.
    /// The path prefix routes the file back to this package via the
    /// existing `file_package` path resolver.
    ///
    /// This commit (Phase 5.2c) wires file insertion only — the re-emit
    /// pipeline and item-extraction land in subsequent commits, so calls
    /// after this one don't yet make the new items reachable via
    /// `pkg.get<F>(...)` or `pkg.eval<T>(...)`.
    ///
    /// Throws `Unsupported` if:
    /// - The engine wasn't constructed with reflection support
    ///   (host didn't call `BexEngine::set_project_db`).
    /// - The receiver doesn't carry a name (the engine didn't supply a
    ///   `runtime_pkg_counter` — typically `BexVm::from_program` VMs).
    ///
    /// Throws `InvalidArgument` if a file path or source isn't a string.
    fn add_compile(
        vm: &mut BexVm,
        package: &Value,
        files: &IndexMap<String, Value>,
    ) -> Result<Value, VmRustFnError> {
        let pkg_ptr = unwrap_package_handle(vm, package)?;

        // Snapshot the package's name; we need it as a `String` before
        // touching the DB lock so the borrow doesn't conflict with the
        // mutable VM heap access.
        let pkg_name = match vm.get_object(pkg_ptr) {
            Object::Package(pkg) => pkg.name.clone(),
            _ => unreachable!("unwrap_package_handle returned a non-Package ptr"),
        };
        if pkg_name.is_empty() {
            return Err(VmBamlError::Unsupported {
                message: "reflect.Package.add_compile: this package has no \
                          assigned name; the engine was constructed without a \
                          `runtime_pkg_counter`. Did you call \
                          `BexEngine::set_project_db` and use `call_function` \
                          to drive the runtime?"
                    .to_string(),
            }
            .into());
        }

        let Some(db_handle) = vm.project_db.as_ref() else {
            return Err(VmBamlError::Unsupported {
                message: "reflect.Package.add_compile requires the host engine \
                          to be constructed with `set_project_db`; the current \
                          engine has no project-DB handle attached"
                    .to_string(),
            }
            .into());
        };
        let db_handle = std::sync::Arc::clone(db_handle);
        let mut db_guard = db_handle.lock();

        // Pre-translate each `(path, source)` BAML value to native (String, String)
        // BEFORE acquiring any Salsa state. This way we don't touch `db_guard`
        // unless every file value is valid, and the rollback path stays simple.
        let mut to_insert: Vec<(std::path::PathBuf, String)> = Vec::with_capacity(files.len());
        for (path, source_value) in files {
            let Value::Object(src_ptr) = *source_value else {
                return Err(VmBamlError::InvalidArgument {
                    message: format!(
                        "reflect.Package.add_compile: file {path:?} value is not a string"
                    ),
                }
                .into());
            };
            let source = match vm.get_object(src_ptr) {
                Object::String(s) => s.clone(),
                _ => {
                    return Err(VmBamlError::InvalidArgument {
                        message: format!(
                            "reflect.Package.add_compile: file {path:?} value is not a string"
                        ),
                    }
                    .into());
                }
            };
            let full_path = format!("<runtime>/{pkg_name}/{path}");
            to_insert.push((std::path::PathBuf::from(full_path), source));
        }

        // Snapshot the runtime files Salsa input before mutating, so we can
        // roll back atomically if `add_runtime_file` / `compile_project` /
        // `lift_runtime_package` fails. A `RuntimeBatchGuard` revert-on-drop
        // pattern keeps the input consistent across panics too.
        let snapshot = db_guard.runtime_files_snapshot().ok_or_else(|| {
            VmRustFnError::from(VmBamlError::Unsupported {
                message: "reflect.Package.add_compile: runtime files input not \
                          initialized (set_project_root has not run)"
                    .to_string(),
            })
        })?;
        let batch = RuntimeBatchGuard {
            db: &mut db_guard,
            snapshot: Some(snapshot),
        };

        // Materialize each `(path, source)` pair into a SourceFile. Errors
        // from `add_runtime_file` (duplicate path, project root unset) are
        // surfaced as BAML `Unsupported` throws.
        let mut new_source_files: Vec<baml_project::SourceFile> = Vec::with_capacity(files.len());
        for (full_path, source) in to_insert {
            let file = batch.db.add_runtime_file(full_path, &source).map_err(|e| {
                VmRustFnError::from(VmBamlError::Unsupported {
                    message: format!("reflect.Package.add_compile: {e}"),
                })
            })?;
            new_source_files.push(file);
        }

        // Compile-time error check. `compile_project` itself only fails on
        // internal lowering errors (e.g. circular let-binding deps), not on
        // parse / name-resolution / type errors — those are collected as
        // diagnostics. We must surface error-severity diagnostics that affect
        // the batch so bad user source rejects it (and the guard reverts the
        // Salsa input).
        //
        // Uses the project-wide `collect_compiler2_diagnostics` rather than
        // per-file `check_file` so that cross-file conflicts — most notably
        // HIR `DuplicateName` (E0011) when a later batch redefines an item
        // from an earlier one — are caught. Filters to diagnostics whose
        // primary span lives in one of the newly-added file ids; pre-existing
        // host diagnostics don't gate `add_compile`.
        let new_file_ids: std::collections::HashSet<baml_project::FileId> = new_source_files
            .iter()
            .map(|f| f.file_id(&*batch.db))
            .collect();
        let all_diags = baml_project::collect_compiler2_diagnostics(&*batch.db);
        if let Some(err_diag) = all_diags.iter().find(|d| {
            d.severity == baml_project::Severity::Error
                && d.primary_span()
                    .is_some_and(|span| new_file_ids.contains(&span.file_id))
        }) {
            return Err(VmRustFnError::from(VmBamlError::Unsupported {
                message: format!(
                    "reflect.Package.add_compile: compile error: {}",
                    err_diag.message
                ),
            }));
        }

        // Re-run the emit pipeline against the modified DB. The guard
        // reverts the Salsa input on the early-return.
        //
        // `OptLevel::One` mirrors the default test/runtime opt level;
        // a future commit may expose this as an `add_compile` option.
        let program = batch
            .db
            .compile_project(baml_project::OptLevel::One)
            .map_err(|e| VmBamlError::Unsupported {
                message: format!("reflect.Package.add_compile: compile failed: {e:?}"),
            })?;

        // Lift the new items into the heap. Failure here also triggers the
        // guard's revert so the Salsa input never observes a half-applied
        // batch. (Already-allocated gen0 objects from a failed lift get
        // reclaimed by the next GC.)
        lift_runtime_package(vm, pkg_ptr, &pkg_name, &program)?;

        // All steps succeeded — keep the new Salsa state.
        batch.commit();
        drop(db_guard);

        Ok(*package)
    }
}

impl BamlNamespaceReflect for PackageBamlImpl {}

#[cfg(test)]
mod tests {
    use bex_vm_types::indexable::Index;

    use super::*;

    /// `StoreGlobal` must be recognized by both `instruction_global_slot`
    /// and `set_instruction_global_slot` so any runtime package containing a
    /// top-level `let` binding (which emits `StoreGlobal` from its synthetic
    /// `$init` function) has the slot remapped from flat to per-package
    /// space along with every other Call/LoadGlobal/etc. site.
    #[test]
    fn store_global_slot_helpers_round_trip() {
        let mut instr = Instruction::StoreGlobal(GlobalIndex::from_raw(7));
        assert_eq!(
            instruction_global_slot(&instr).map(Index::into_raw),
            Some(7)
        );
        assert!(set_instruction_global_slot(
            &mut instr,
            GlobalIndex::from_raw(42),
        ));
        assert!(matches!(
            instr,
            Instruction::StoreGlobal(g) if g.into_raw() == 42
        ));
    }
}
