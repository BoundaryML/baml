use bex_vm_types::types::{Object, Value};

use super::{BamlClassTypeValue, PackageBamlImpl, resolve};
use crate::BexVm;

impl BamlClassTypeValue for PackageBamlImpl {
    /// Returns the `RealizedTy`'s display name.  Includes namespaces and (for
    /// non-`user` packages) the package prefix, so two distinct types never
    /// collide on this string — package names are unique within a workspace,
    /// so eliding the implicit `user.` prefix is unambiguous.
    ///
    /// This identity guarantee makes the result usable as a stable key in
    /// `map<string, V>` until generic-K interfaces enable a real
    /// `map<type, V>`.
    fn _to_string_impl(vm: &BexVm, self_value: &Value) -> bex_str::BexStr {
        let Some(ptr) = self_value.as_object_ptr() else {
            return bex_str::BexStr::from("<type: ?>");
        };
        match vm.get_object(ptr) {
            Object::Type(ty) => bex_str::BexStr::from(ty.to_string()),
            _ => bex_str::BexStr::from("<type: ?>"),
        }
    }

    /// BEP-044: `class_t.implements(iface_t)`.
    ///
    /// Selects over the program-wide impl-rule index: an impl applies when its
    /// `for_ty_pattern` matches `class_t` (with bounds satisfied) and its
    /// implemented-interface args / associated bindings match the requested
    /// instantiation. Candidates are every impl of the interface in the program —
    /// the orphan rule does *not* localize them to `class_t`'s or the interface's
    /// package (see [`crate::package_load::PackageIndex`]); bound obligations
    /// recurse the same way. Because the compiler (E0125) forces a class to
    /// implement every interface in its `requires` closure, "direct impl" already
    /// covers transitive satisfaction.
    fn implements(vm: &BexVm, self_value: &Value, other: &Value) -> bool {
        let Some(self_ty) = type_value_ty(vm, *self_value) else {
            return false;
        };
        let Some((iface_name, iface_args, iface_assoc)) = ty_name_args_and_assoc(vm, *other) else {
            return false;
        };
        resolve::ImplResolver::new(vm).type_implements(
            &self_ty,
            &iface_name,
            &iface_args,
            &iface_assoc,
        )
    }

    /// BEP-044: `iface_t.implemented_by(class_t)` — same answer as
    /// `class_t.implements(iface_t)` but with the receiver flipped.
    fn implemented_by(vm: &BexVm, self_value: &Value, other: &Value) -> bool {
        Self::implements(vm, other, self_value)
    }
}

/// The concrete `RealizedTy` wrapped by a `type` value (class, enum, interface,
/// primitive, container, …), or `None` if `value` isn't a `type`.
fn type_value_ty(vm: &BexVm, value: Value) -> Option<baml_type::RealizedTy> {
    match vm.get_object(value.as_object_ptr()?) {
        Object::Type(ty) => Some(ty.as_ref().clone()),
        _ => None,
    }
}

/// A realized interface instantiation as reflected off a value: the type's
/// qualified name, its realized generic arguments, and its associated bindings.
type RealizedTypeInstantiation = (
    baml_type::TypeName,
    Vec<baml_type::RealizedTy>,
    Vec<(baml_type::Name, baml_type::RealizedTy)>,
);

/// Returns the type's base name plus its generic arguments (e.g.
/// `[string]` for `Box<string>`). Used by reflection to discriminate generic
/// interface instantiations.
fn ty_name_args_and_assoc(vm: &BexVm, value: Value) -> Option<RealizedTypeInstantiation> {
    let ptr = value.as_object_ptr()?;
    let Object::Type(ty) = vm.get_object(ptr) else {
        return None;
    };
    match ty.as_ref() {
        baml_type::RealizedTy::Class(name, args, _) => {
            Some((name.clone(), args.clone(), Vec::new()))
        }
        baml_type::RealizedTy::Interface(name, args, associated_bindings, _) => {
            Some((name.clone(), args.clone(), associated_bindings.clone()))
        }
        baml_type::RealizedTy::Enum(name, _) => Some((name.clone(), Vec::new(), Vec::new())),
        other => primitive_type_name(other).map(|name| (name, Vec::new(), Vec::new())),
    }
}

/// BEP-044 wf3 #G19: a synthetic `TypeName` for a primitive type, so reflection on a
/// primitive type value (`reflect.type_of<int>()`) has a name to key by, the way
/// non-primitive types carry their own `TypeName`. Impl *matching* for primitives is
/// structural — the registry bakes their for-types as `Concrete(RuntimeTy::Int { .. })`
/// etc. (`baml_compiler2_mir`'s `tir2_to_template`), matched by `resolve::match_template`
/// — so this is a reflection key, never compared against a baked pattern.
fn primitive_type_name(ty: &baml_type::RealizedTy) -> Option<baml_type::TypeName> {
    let name = match ty {
        baml_type::RealizedTy::Int { .. } => "int",
        baml_type::RealizedTy::Bigint { .. } => "bigint",
        baml_type::RealizedTy::Float { .. } => "float",
        baml_type::RealizedTy::String { .. } => "string",
        baml_type::RealizedTy::Bool { .. } => "bool",
        baml_type::RealizedTy::Null { .. } => "null",
        _ => return None,
    };
    Some(baml_type::QualifiedTypeName::local(baml_type::Name::new(
        name,
    )))
}
