//! Native implementations for the `reflect` package's runtime classes.
//!
//! Currently only `reflect.Package` lives here. `reflect.type_of<T>()` is a
//! compiler intrinsic (lowered to a `LoadType` instruction at emit time) and
//! has no runtime counterpart.

use std::collections::HashMap;

use bex_vm_types::{HeapPtr, Object, Package, PackageGlobals, Value};
use indexmap::IndexMap;

use super::{BamlClassReflectPackage, BamlNamespaceReflect, PackageBamlImpl};
use crate::{
    BexVm,
    errors::{VmBamlError, VmRustFnError},
};

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
        let _program = db_guard
            .compile_project(baml_project::OptLevel::One)
            .map_err(|e| VmBamlError::Unsupported {
                message: format!("reflect.Package.add_compile: compile failed: {e:?}"),
            })?;

        // TODO(Phase 5.2e): walk `_program` for items whose
        // `package_name` matches `pkg_name`, heap-allocate them in gen0
        // via `emit_package`'s identity-preserving incremental lift, and
        // populate `pkg_ptr`'s `items` + `PackageGlobals::Dynamic` slot
        // space with the per-package slot assignments.
        drop(db_guard);
        Ok(*package)
    }
}

impl BamlNamespaceReflect for PackageBamlImpl {}
