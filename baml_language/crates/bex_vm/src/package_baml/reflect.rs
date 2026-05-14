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
    /// The internal package's `name` is left empty for now; the only
    /// identity that matters at runtime is the `HeapPtr` itself (every
    /// frame caches the owning package by pointer, not by name).
    fn new(vm: &mut BexVm) -> Value {
        let pkg = Package {
            name: String::new(),
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
    /// This is the skeleton: it validates the receiver, looks up the
    /// engine's `ProjectDatabase`, and acquires the project-DB mutex.
    /// The actual file insertion + emit pipeline lands in subsequent
    /// commits.
    ///
    /// Throws `Unsupported` if the engine wasn't constructed with
    /// reflection support (i.e., the host didn't call
    /// `BexEngine::set_project_db`).
    fn add_compile(
        vm: &mut BexVm,
        package: &Value,
        _files: &IndexMap<String, Value>,
    ) -> Result<Value, VmRustFnError> {
        // Validate the receiver up-front so any later compile error sees a
        // well-formed package handle.
        let _pkg_ptr = unwrap_package_handle(vm, package)?;

        let Some(db_handle) = vm.project_db.as_ref() else {
            return Err(VmBamlError::Unsupported {
                message: "reflect.Package.add_compile requires the host engine \
                          to be constructed with `set_project_db`; the current \
                          engine has no project-DB handle attached"
                    .to_string(),
            }
            .into());
        };

        // Acquire the project-DB mutex for the duration of the compile.
        // Concurrent `add_compile` calls on the same engine serialize
        // here. The lock is dropped at the end of this block.
        let _db_guard = db_handle.lock();

        // TODO(Phase 5.2c+): insert `_files` into `Compiler2RuntimeFiles`
        // under the package's path prefix, re-emit, and append the new
        // items + globals to `pkg_ptr`'s owned `PackageGlobals::Dynamic`
        // slot space.
        Ok(*package)
    }
}

impl BamlNamespaceReflect for PackageBamlImpl {}
