//! Native implementations for the `reflect` package's runtime classes.
//!
//! Currently only `reflect.Package` lives here. `reflect.type_of<T>()` is a
//! compiler intrinsic (lowered to a `LoadType` instruction at emit time) and
//! has no runtime counterpart.

use std::collections::HashMap;

use bex_vm_types::{Object, Package, PackageGlobals, Value};

use super::{BamlClassReflectPackage, BamlNamespaceReflect, PackageBamlImpl};
use crate::BexVm;

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
}

impl BamlNamespaceReflect for PackageBamlImpl {}
