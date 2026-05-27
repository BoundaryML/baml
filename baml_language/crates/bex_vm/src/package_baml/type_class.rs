use bex_vm_types::types::{Object, Value};

use super::{BamlClassTypeValue, PackageBamlImpl};
use crate::BexVm;

impl BamlClassTypeValue for PackageBamlImpl {
    /// Returns the `Ty`'s display name.  Includes namespaces and (for
    /// non-`user` packages) the package prefix, so two distinct types never
    /// collide on this string — package names are unique within a workspace,
    /// so eliding the implicit `user.` prefix is unambiguous.
    ///
    /// This identity guarantee makes the result usable as a stable key in
    /// `map<string, V>` until generic-K interfaces enable a real
    /// `map<type, V>`.
    fn to_string(vm: &BexVm, self_value: &Value) -> String {
        let Some(ptr) = self_value.as_object_ptr() else {
            return "<type: ?>".to_string();
        };
        match vm.get_object(ptr) {
            Object::Type(ty) => ty.to_string(),
            _ => "<type: ?>".to_string(),
        }
    }

    /// BEP-044: `class_t.implements(iface_t)`.
    ///
    /// Looks `class_t`'s `TypeName` up in the program's interface→implementors
    /// table under the key `iface_t`. The table is populated at codegen time
    /// with the transitive `extends`/`implements` closure, so transitive
    /// satisfaction is a single hash lookup.
    fn implements(vm: &BexVm, self_value: &Value, other: &Value) -> bool {
        let Some(class_name) = ty_name(vm, self_value) else {
            return false;
        };
        let Some(iface_name) = ty_name(vm, other) else {
            return false;
        };
        vm.interface_implementors
            .get(&iface_name)
            .is_some_and(|impls| impls.contains(&class_name))
    }

    /// BEP-044: `iface_t.implemented_by(class_t)` — same answer as
    /// `class_t.implements(iface_t)` but with the receiver flipped.
    fn implemented_by(vm: &BexVm, self_value: &Value, other: &Value) -> bool {
        Self::implements(vm, other, self_value)
    }

    /// BEP-044: `iface_t.implementors()` returns the concrete classes that
    /// nominally satisfy this interface, in stable declaration order.
    /// Returns `[]` when `self_value` is not an interface (e.g. a class type,
    /// a primitive type, or a `type` value for a generic instantiation that
    /// the codegen pass didn't enumerate).
    ///
    /// Returns a raw `Vec<Value>`; the codegen glue wraps it into an
    /// `Object::Array` allocation. The element `Object::Type` values are
    /// allocated here because they each require a fresh TLAB slot.
    fn implementors(vm: &mut BexVm, self_value: &Value) -> Vec<Value> {
        let Some(iface_name) = ty_name(vm, self_value) else {
            return Vec::new();
        };
        let Some(class_names) = vm.interface_implementors.get(&iface_name).cloned() else {
            return Vec::new();
        };
        class_names
            .into_iter()
            .map(|name| {
                let ty = baml_type::Ty::Class(name, Vec::new(), baml_type::TyAttr::default());
                Value::object(vm.tlab.alloc(Object::Type(Box::new(ty))))
            })
            .collect()
    }
}

/// Project a `Value::Object(Object::Type)` to its underlying `TypeName`,
/// when the wrapped `Ty` is name-bearing (class, enum, interface — all of
/// which round-trip through `Ty::Class` at the runtime layer; see
/// `baml_compiler2_mir/src/lower.rs`). Returns `None` for primitive types,
/// containers, and anything else without a stable name.
fn ty_name(vm: &BexVm, value: &Value) -> Option<baml_type::TypeName> {
    let ptr = value.as_object_ptr()?;
    let Object::Type(ty) = vm.get_object(ptr) else {
        return None;
    };
    match ty.as_ref() {
        baml_type::Ty::Class(name, _, _) | baml_type::Ty::Enum(name, _) => Some(name.clone()),
        _ => None,
    }
}
