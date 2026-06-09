use bex_vm_types::types::{InterfaceAssociatedBindings, InterfaceImplementorEntry, Object, Value};

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
    fn to_string(vm: &BexVm, self_value: &Value) -> bex_str::BexStr {
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
    /// Looks `class_t`'s `TypeName` up in the program's interface→implementors
    /// table under the key `iface_t`. The table is populated at codegen time
    /// with the transitive `extends`/`implements` closure, so transitive
    /// satisfaction is a single hash lookup.
    fn implements(vm: &BexVm, self_value: &Value, other: &Value) -> bool {
        let Some(class_name) = ty_name(vm, *self_value) else {
            return false;
        };
        let Some((iface_name, iface_args, iface_assoc)) = ty_name_args_and_assoc(vm, *other) else {
            return false;
        };
        // BEP-044: a generic interface request (`Box<string>`) must match only
        // implementors recorded at those exact type args. An interface request
        // with no args (`Box` / a non-generic interface) matches any.
        vm.interface_implementors
            .get(&iface_name)
            .is_some_and(|impls| {
                impls.iter().any(|(impl_class, impl_args, impl_assoc)| {
                    *impl_class == class_name
                        && (iface_args.is_empty()
                            || impl_args.is_empty()
                            || ty_args_equivalent(impl_args, &iface_args))
                        && associated_bindings_equivalent(impl_assoc, &iface_assoc)
                })
            })
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
        let Some((iface_name, iface_args, iface_assoc)) = ty_name_args_and_assoc(vm, *self_value)
        else {
            return Vec::new();
        };
        let Some(entries) = vm.interface_implementors.get(&iface_name).cloned() else {
            return Vec::new();
        };
        entries
            .into_iter()
            // Keep only implementors recorded at the requested instantiation
            // (any, when the request or implementor entry carries no type args).
            .filter(|(_, impl_args, impl_assoc)| {
                (iface_args.is_empty()
                    || impl_args.is_empty()
                    || ty_args_equivalent(impl_args, &iface_args))
                    && associated_bindings_equivalent(impl_assoc, &iface_assoc)
            })
            .map(|(name, _, _)| {
                let ty = baml_type::Ty::Class(name, Vec::new(), baml_type::TyAttr::default());
                Value::object(vm.tlab.alloc(Object::Type(Box::new(ty))))
            })
            .collect()
    }
}

/// Compare two generic-argument lists for *semantic* equivalence. Union
/// arguments are compared as unordered sets, so `Box<int | string>` and
/// `Box<string | int>` are the same instantiation (union member order is
/// semantically irrelevant — the type checker already treats `int | string`
/// and `string | int` as identical). Falls back to structural `==` for
/// non-union leaves.
fn ty_args_equivalent(a: &[baml_type::Ty], b: &[baml_type::Ty]) -> bool {
    a.len() == b.len() && a.iter().zip(b.iter()).all(|(x, y)| ty_equivalent(x, y))
}

fn ty_equivalent(a: &baml_type::Ty, b: &baml_type::Ty) -> bool {
    use baml_type::Ty;
    match (a, b) {
        // Order-insensitive comparison of union members via a one-to-one
        // matching: each `a` member must pair with a *distinct* equivalent `b`
        // member. A plain `all(|x| any(|y| ...))` would wrongly accept
        // `int | int` as equivalent to `int | string` (both members of the left
        // match the single `int` on the right), so consume each matched member.
        (Ty::Union(am, _), Ty::Union(bm, _)) => {
            if am.len() != bm.len() {
                return false;
            }
            let mut used = vec![false; bm.len()];
            'next_a: for x in am {
                for (j, y) in bm.iter().enumerate() {
                    if !used[j] && ty_equivalent(x, y) {
                        used[j] = true;
                        continue 'next_a;
                    }
                }
                return false;
            }
            true
        }
        // Recurse into nested generic instantiations (`Box<Slot<int | string>>`).
        (Ty::Class(an, aa, _), Ty::Class(bn, ba, _)) => an == bn && ty_args_equivalent(aa, ba),
        (Ty::Interface(an, aa, ab, _), Ty::Interface(bn, ba, bb, _)) => {
            an == bn && ty_args_equivalent(aa, ba) && associated_bindings_exactly_equivalent(ab, bb)
        }
        // Recurse through container/optional wrappers so a union nested inside
        // them is still compared order-insensitively (`Box<(int | string)?>` ==
        // `Box<(string | int)?>`); otherwise the wrapper would fall to the
        // structural `==` below and defeat the union-set comparison.
        (Ty::List(ai, _), Ty::List(bi, _)) => ty_equivalent(ai, bi),
        (
            Ty::Map {
                key: ak, value: av, ..
            },
            Ty::Map {
                key: bk, value: bv, ..
            },
        ) => ty_equivalent(ak, bk) && ty_equivalent(av, bv),
        _ => a == b,
    }
}

fn associated_bindings_equivalent(
    impl_bindings: &InterfaceAssociatedBindings,
    requested_bindings: &InterfaceAssociatedBindings,
) -> bool {
    requested_bindings.is_empty()
        || requested_bindings
            .iter()
            .all(|(requested_name, requested_ty)| {
                impl_bindings
                    .iter()
                    .find(|(impl_name, _)| impl_name == requested_name)
                    .is_some_and(|(_, impl_ty)| ty_equivalent(impl_ty, requested_ty))
            })
}

fn associated_bindings_exactly_equivalent(
    a: &InterfaceAssociatedBindings,
    b: &InterfaceAssociatedBindings,
) -> bool {
    a.len() == b.len()
        && associated_bindings_equivalent(a, b)
        && associated_bindings_equivalent(b, a)
}

/// Like [`ty_name`] but also returns the type's generic arguments (e.g.
/// `[string]` for `Box<string>`). Used by reflection to discriminate generic
/// interface instantiations.
fn ty_name_args_and_assoc(vm: &BexVm, value: Value) -> Option<InterfaceImplementorEntry> {
    let ptr = value.as_object_ptr()?;
    let Object::Type(ty) = vm.get_object(ptr) else {
        return None;
    };
    match ty.as_ref() {
        baml_type::Ty::Class(name, args, _) => Some((name.clone(), args.clone(), Vec::new())),
        baml_type::Ty::Interface(name, args, associated_bindings, _) => {
            Some((name.clone(), args.clone(), associated_bindings.clone()))
        }
        baml_type::Ty::Enum(name, _) => Some((name.clone(), Vec::new(), Vec::new())),
        other => primitive_type_name(other).map(|name| (name, Vec::new(), Vec::new())),
    }
}

/// BEP-044 wf3 #G19: a synthetic `TypeName` for a primitive type so an
/// out-of-body `implements I for int` is visible to reflection
/// (`reflect.type_of<int>().implements(...)` / `I.implementors()`). Must match
/// the naming used by `baml_compiler2_emit`'s Pass 7.6 when it bakes the
/// implementor table.
fn primitive_type_name(ty: &baml_type::Ty) -> Option<baml_type::TypeName> {
    let name = match ty {
        baml_type::Ty::Int { .. } => "int",
        baml_type::Ty::Bigint { .. } => "bigint",
        baml_type::Ty::Float { .. } => "float",
        baml_type::Ty::String { .. } => "string",
        baml_type::Ty::Bool { .. } => "bool",
        baml_type::Ty::Null { .. } => "null",
        _ => return None,
    };
    Some(baml_type::TypeName {
        name: baml_type::Name::new(name),
        module_path: Vec::new(),
        display_name: baml_type::Name::new(name),
    })
}

/// Project a `Value::Object(Object::Type)` to its underlying `TypeName`,
/// when the wrapped `Ty` is name-bearing (class, enum, interface — all of
/// which round-trip through `Ty::Class` at the runtime layer; see
/// `baml_compiler2_mir/src/lower.rs`). Returns `None` for primitive types,
/// containers, and anything else without a stable name.
fn ty_name(vm: &BexVm, value: Value) -> Option<baml_type::TypeName> {
    let ptr = value.as_object_ptr()?;
    let Object::Type(ty) = vm.get_object(ptr) else {
        return None;
    };
    match ty.as_ref() {
        baml_type::Ty::Class(name, _, _)
        | baml_type::Ty::Interface(name, _, _, _)
        | baml_type::Ty::Enum(name, _) => Some(name.clone()),
        other => primitive_type_name(other),
    }
}
