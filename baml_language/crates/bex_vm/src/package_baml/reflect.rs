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

/// Extract the [`GlobalIndex`] operand from an instruction if it has one.
///
/// Used by the `add_compile` lift to find every global slot reference in a
/// runtime function's bytecode so they can be remapped from the build-time
/// emit's flat slot space to per-package slot indices.
fn instruction_global_slot(instr: &Instruction) -> Option<GlobalIndex> {
    match *instr {
        Instruction::LoadGlobal(s)
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

/// Lift the runtime package's items from a freshly compiled `Program` into
/// `pkg_ptr`'s `Package.items` + `Package.globals.Dynamic`, preserving
/// `HeapPtr` identity for items defined in earlier `add_compile` batches.
///
/// For each new function whose `package_name` matches this package:
/// 1. Walk its bytecode to discover global-slot references.
/// 2. Assign per-package slots for the targets:
///    - Same-package callees → the corresponding entry from `pkg.items` (existing
///      or newly-lifted).
///    - External callees (stdlib / host / `deps`) → currently throw
///      `Unsupported`; supported in a follow-up commit.
/// 3. Rewrite the bytecode's slot operands from flat to per-package and
///    regenerate the compact form.
/// 4. Resolve the function's bytecode constants (`ConstValue::*`) to
///    runtime `Value::*`, allocating strings into gen0.
/// 5. Allocate `Object::Function` in gen0 and register it in `pkg.items`
///    under its local name.
/// 6. Append the new slot assignments to `pkg.globals.Dynamic` and
///    `pkg.function_slot_map`.
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

    // Index program.objects by ObjectIndex → FQN for callee lookup. We
    // only care about Function-typed entries for now; class / enum lifting
    // lands in a later commit.
    let object_index_to_fqn: HashMap<usize, String> = program
        .objects
        .iter()
        .enumerate()
        .filter_map(|(idx, obj)| match obj {
            Object::Function(f) => Some((idx, f.name.clone())),
            _ => None,
        })
        .collect();

    // Collect new functions: those whose `package_name` matches and whose
    // local name isn't already in `pkg.items` (identity preserved across
    // batches by skipping these).
    let new_functions: Vec<Function> = program
        .objects
        .iter()
        .filter_map(|obj| match obj {
            Object::Function(f) if f.package_name == pkg_name => {
                let local_name = f
                    .name
                    .strip_prefix(pkg_dot.as_str())
                    .unwrap_or(f.name.as_str());
                if existing_items.contains_key(local_name) {
                    None
                } else {
                    Some((**f).clone())
                }
            }
            _ => None,
        })
        .collect();

    // Pass A: pre-allocate `Object::Function` shells in gen0 so we have
    // stable HeapPtrs to reference from each function's rewritten bytecode.
    // We allocate with the original (flat-slot) bytecode here; pass B
    // mutates the bytecode in place via `get_object_mut`.
    //
    // `name_to_ptr` maps every local name (existing + new) to its HeapPtr;
    // slot map building references this.
    let mut name_to_ptr: HashMap<String, HeapPtr> = existing_items;
    let mut newly_allocated: Vec<(String, HeapPtr)> = Vec::with_capacity(new_functions.len());
    for func in &new_functions {
        let local_name = func
            .name
            .strip_prefix(pkg_dot.as_str())
            .unwrap_or(func.name.as_str())
            .to_string();
        // Tag the function with its owning package so runtime dispatch
        // (`resolve_frame_package`) reads `pkg.globals` for `Call`/`LoadGlobal`.
        let mut func_clone = func.clone();
        func_clone.package = pkg_ptr;
        let ptr = vm.tlab.alloc(Object::Function(Box::new(func_clone)));
        name_to_ptr.insert(local_name.clone(), ptr);
        newly_allocated.push((local_name, ptr));
    }

    // Pass B: for each newly-allocated function, walk its bytecode for
    // global-slot references, build the slot map, then rewrite the slots
    // and regenerate the compact form.
    let mut new_global_entries: Vec<Value> = Vec::new();
    let mut next_slot = existing_globals_len;

    for (_local_name, fn_ptr) in &newly_allocated {
        // First pass: collect every flat slot used in this function and
        // ensure each one has a `slot_map` entry. We snapshot the
        // instructions to release the heap borrow before mutating.
        let instructions_snapshot: Vec<Instruction> = match vm.get_object(*fn_ptr) {
            Object::Function(f) => f.bytecode.instructions.clone(),
            _ => unreachable!(),
        };
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
            let fqn = object_index_to_fqn
                .get(&obj_idx.into_raw())
                .cloned()
                .ok_or_else(|| VmBamlError::Unsupported {
                    message: format!(
                        "reflect.Package.add_compile: slot {flat_idx} points to ObjectIndex \
                         {obj_idx:?} which isn't a Function"
                    ),
                })?;
            if slot_map.contains_key(&fqn) {
                continue;
            }
            // Resolve the callee. Same-package callees come from the
            // local `name_to_ptr` map (existing items + newly-allocated).
            // External callees (stdlib, host, deps) are looked up in the
            // engine's frozen globals by FQN.
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
                vm.find_function_by_name(&fqn)
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

        // Second pass: mutate the function in place. We need:
        //   1. Rewrite each instruction's GlobalIndex from flat → per-pkg slot.
        //   2. Set resolved_constants.
        //   3. Re-lower to compact form so the VM's fast-path dispatch sees
        //      the rewritten slots.
        match vm.get_object_mut(*fn_ptr) {
            Object::Function(f) => {
                for instr in &mut f.bytecode.instructions {
                    if let Some(flat_slot) = instruction_global_slot(instr) {
                        let cv = &program.globals[flat_slot.into_raw()];
                        let ConstValue::Object(obj_idx) = cv else {
                            unreachable!("guarded above");
                        };
                        let fqn = &object_index_to_fqn[&obj_idx.into_raw()];
                        let new_slot = slot_map[fqn];
                        set_instruction_global_slot(instr, GlobalIndex::from_raw(new_slot));
                    }
                }
                f.bytecode.resolved_constants = resolved;
                f.bytecode.compact = Some(f.bytecode.lower_to_compact());
            }
            _ => unreachable!(),
        }
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

        // Materialize each `(path, source)` pair into a SourceFile under
        // `<runtime>/{pkg_name}/{path}`.
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
            db_guard.add_runtime_file(std::path::PathBuf::from(full_path), &source);
        }

        // Re-run the emit pipeline against the modified DB. If the new
        // sources have parse / type / lowering errors this returns Err
        // and we surface it as a BAML throw. Successful compile is the
        // dependency for the incremental `emit_package` item lift that
        // lands in the next commit (Phase 5.2e).
        //
        // `OptLevel::One` mirrors the default test/runtime opt level;
        // a future commit may expose this as an `add_compile` option.
        let program = db_guard
            .compile_project(baml_project::OptLevel::One)
            .map_err(|e| VmBamlError::Unsupported {
                message: format!("reflect.Package.add_compile: compile failed: {e:?}"),
            })?;
        drop(db_guard);

        lift_runtime_package(vm, pkg_ptr, &pkg_name, &program)?;

        Ok(*package)
    }
}

impl BamlNamespaceReflect for PackageBamlImpl {}
