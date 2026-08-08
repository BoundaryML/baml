use bex_heap::TlabHolder;
use bex_vm_types::types::{Object, TypeValue, Value};
use indexmap::IndexMap;

use super::{BamlClassTypeValue, BamlNamespaceType, PackageBamlImpl, copy, resolve};
use crate::{BexVm, errors::VmRustFnError};

impl BamlNamespaceType for PackageBamlImpl {
    /// BEP-066 K-13: `type.of_value(v)` — the runtime `type` value describing
    /// `v`'s concrete type, reconstructed by `BexVm::value_concrete_ty`.
    ///
    /// K-12 holds by construction: `value_concrete_ty` reports value types
    /// (`int` for `5`), never literal types. A value with no reconstructable
    /// BAML type — a compile-time definition object (package, class, enum,
    /// interface, impl rule) or an opaque native handle — yields the `unknown`
    /// type value, the same fail-open convention `reflect.signature` /
    /// `reflect.call_any` use for unreconstructable argument types.
    fn of_value(vm: &mut BexVm, v: &Value) -> Result<Value, VmRustFnError> {
        if let Some(ptr) = v.as_object_ptr() {
            let nominal = match vm.get_object(ptr) {
                Object::Instance(instance) => Some((instance.class, true)),
                Object::Variant(variant) => Some((variant.enm, false)),
                _ => None,
            };
            if let Some((definition_ptr, is_class)) = nominal {
                let reconstructed = match vm.get_object(definition_ptr) {
                    Object::Class(class) if is_class => {
                        class.runtime_type.as_ref().map(|runtime| {
                            let mut defs = runtime.defs.clone();
                            defs.classes.insert(class.name.clone(), definition_ptr);
                            let ty = baml_type::RealizedTy::Class(
                                class.name.clone(),
                                Vec::new(),
                                baml_type::TyAttr::default(),
                            );
                            if runtime.owner.is_null() {
                                TypeValue::from_parts_with_defs(ty, runtime.mint, defs)
                            } else {
                                TypeValue::runtime_with_defs(ty, runtime.mint, defs, runtime.owner)
                            }
                        })
                    }
                    Object::Enum(enm) if !is_class => enm.runtime_type.as_ref().map(|runtime| {
                        let mut defs = runtime.defs.clone();
                        defs.enums.insert(enm.name.clone(), definition_ptr);
                        let ty = baml_type::RealizedTy::Enum(
                            enm.name.clone(),
                            baml_type::TyAttr::default(),
                        );
                        if runtime.owner.is_null() {
                            TypeValue::from_parts_with_defs(ty, runtime.mint, defs)
                        } else {
                            TypeValue::runtime_with_defs(ty, runtime.mint, defs, runtime.owner)
                        }
                    }),
                    _ => None,
                };
                if let Some(type_value) = reconstructed {
                    return Ok(Value::object(vm.tlab.alloc_type(type_value)));
                }
            }
        }
        let ty = vm
            .value_concrete_ty(*v)
            .map_or_else(baml_type::RealizedTy::unknown, baml_type::RealizedTy::from);
        Ok(Value::object(vm.alloc_static_type(ty)))
    }
}

impl BamlClassTypeValue for PackageBamlImpl {
    fn array(vm: &mut BexVm, self_value: &Value) -> Value {
        let type_value = cloned_type_value(vm, *self_value);
        alloc_runtime_type(
            vm,
            baml_type::RealizedTy::List(
                Box::new(type_value.ty.clone()),
                baml_type::TyAttr::default(),
            ),
            type_value.defs().clone(),
            type_value.owner,
        )
    }

    fn optional(vm: &mut BexVm, self_value: &Value) -> Value {
        let type_value = cloned_type_value(vm, *self_value);
        let mut members = match &type_value.ty {
            baml_type::RealizedTy::Union(members, _) => members.clone(),
            other => vec![other.clone()],
        };
        if !members.iter().any(baml_type::RealizedTy::is_null) {
            members.push(baml_type::RealizedTy::null());
        }
        alloc_runtime_type(
            vm,
            baml_type::RealizedTy::Union(members, baml_type::TyAttr::default()),
            type_value.defs().clone(),
            type_value.owner,
        )
    }

    fn to_baml(vm: &BexVm, self_value: &Value) -> bex_str::BexStr {
        let type_value = cloned_type_value(vm, *self_value);
        bex_str::BexStr::from(render_type_value_source(vm, &type_value))
    }

    fn meta(
        vm: &mut BexVm,
        self_value: &Value,
        alias: Option<&bex_str::BexStr>,
        description: Option<&bex_str::BexStr>,
        docstring: Option<&bex_str::BexStr>,
        other: Option<&IndexMap<bex_str::BexStr, Value>>,
    ) -> Value {
        fn opt_string(vm: &mut BexVm, value: Option<&bex_str::BexStr>) -> Value {
            value.map_or(Value::NULL, |s| Value::object(vm.alloc_string(s.clone())))
        }

        let entries = other
            .into_iter()
            .flatten()
            .map(|(key, value)| {
                let value = vm
                    .as_string(value)
                    .expect("map<string, string> value checked by native glue")
                    .clone();
                (key.clone(), Value::object(vm.alloc_string(value)))
            })
            .collect();
        let other = Value::object(vm.alloc_map(
            baml_type::RealizedTy::string(),
            baml_type::RealizedTy::string(),
            entries,
        ));
        let alias = opt_string(vm, alias);
        let description = opt_string(vm, description);
        let docstring = opt_string(vm, docstring);
        copy::reflect::WithMeta {
            ty: *self_value,
            alias,
            description,
            docstring,
            other,
        }
        .to_value(vm)
    }

    fn kind(_vm: &BexVm, self_value: &Value) -> Value {
        *self_value
    }

    fn as_class(vm: &BexVm, self_value: &Value) -> Option<Value> {
        as_kind(vm, *self_value, baml_type::type_kind::TypeKind::Class)
    }

    fn as_enum(vm: &BexVm, self_value: &Value) -> Option<Value> {
        as_kind(vm, *self_value, baml_type::type_kind::TypeKind::Enum)
    }

    fn as_union(vm: &BexVm, self_value: &Value) -> Option<Value> {
        as_kind(vm, *self_value, baml_type::type_kind::TypeKind::Union)
    }

    fn as_literal(vm: &BexVm, self_value: &Value) -> Option<Value> {
        as_kind(vm, *self_value, baml_type::type_kind::TypeKind::Literal)
    }

    fn as_array(vm: &BexVm, self_value: &Value) -> Option<Value> {
        as_kind(vm, *self_value, baml_type::type_kind::TypeKind::Array)
    }

    fn as_map(vm: &BexVm, self_value: &Value) -> Option<Value> {
        as_kind(vm, *self_value, baml_type::type_kind::TypeKind::Map)
    }

    fn as_interface(vm: &BexVm, self_value: &Value) -> Option<Value> {
        as_kind(vm, *self_value, baml_type::type_kind::TypeKind::Interface)
    }

    fn as_primitive(vm: &BexVm, self_value: &Value) -> Option<Value> {
        as_kind(vm, *self_value, baml_type::type_kind::TypeKind::Primitive)
    }

    fn as_function(vm: &BexVm, self_value: &Value) -> Option<Value> {
        as_kind(vm, *self_value, baml_type::type_kind::TypeKind::Function)
    }

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
            Object::Type(type_value) => bex_str::BexStr::from(type_value.ty.to_string()),
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
        resolve::ImplResolver::for_value(vm, *self_value).type_implements(
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

    /// BEP-044: `iface_t.implementors()` returns the concrete classes that
    /// nominally satisfy this interface, in deterministic lexicographic order by
    /// qualified name. Returns `[]` when `self_value` is not an interface (e.g. a
    /// class type or a primitive type).
    ///
    /// Derived from the same per-package `interface_impls` registry as
    /// [`Self::implements`] (via `resolve::implementor_entries`), so the two
    /// reflection directions cannot disagree. A generic class is reported by its
    /// base and a blanket impl by every loaded class its bounds admit, so a
    /// specific generic instantiation (`Box<int>`) is not separately enumerable.
    ///
    /// Returns a raw `Vec<Value>`; the codegen glue wraps it into an
    /// `Object::Array` allocation. The element `Object::Type` values are
    /// allocated here because they each require a fresh TLAB slot.
    fn implementors(vm: &mut BexVm, self_value: &Value) -> Vec<Value> {
        let Some((iface_name, iface_args, iface_assoc)) = ty_name_args_and_assoc(vm, *self_value)
        else {
            return Vec::new();
        };
        // Materialize the filtered entries first: the resolver holds a shared
        // borrow of the VM, which must end before the TLAB allocations below
        // take unique access.
        let resolver = resolve::ImplResolver::for_value(vm, *self_value);
        let entries: Vec<_> = resolver
            .implementor_entries(&iface_name)
            .into_iter()
            // Keep only implementors recorded at the requested instantiation
            // (any, when the request or implementor entry carries no type args /
            // associated bindings) — args and assoc handled symmetrically.
            .filter(|(_, impl_args, impl_assoc)| {
                (iface_args.is_empty()
                    || impl_args.is_empty()
                    || resolver.ty_args_equivalent(impl_args, &iface_args))
                    && (impl_assoc.is_empty()
                        || resolver.associated_bindings_equivalent(impl_assoc, &iface_assoc))
            })
            .collect();
        entries
            .into_iter()
            .map(|(ty, _, _)| Value::object(vm.alloc_static_type(ty)))
            .collect()
    }
}

fn cloned_type_value(vm: &BexVm, value: Value) -> TypeValue {
    let ptr = value
        .as_object_ptr()
        .unwrap_or_else(|| unreachable!("type method receiver must be Object::Type"));
    let Object::Type(type_value) = vm.get_object(ptr) else {
        unreachable!("type method receiver must be Object::Type")
    };
    (**type_value).clone()
}

fn alloc_runtime_type(
    vm: &mut BexVm,
    ty: baml_type::RealizedTy,
    defs: bex_vm_types::types::DynTypeDefs,
    owner: bex_vm_types::HeapPtr,
) -> Value {
    let mint = vm.tlab.heap().mint_runtime_id();
    let value = if owner.is_null() {
        TypeValue::from_parts_with_defs(ty, mint, defs)
    } else {
        TypeValue::runtime_with_defs(ty, mint, defs, owner)
    };
    Value::object(vm.tlab.alloc_type(value))
}

fn quoted(value: &str) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| unreachable!("serializing a Rust string"))
}

fn render_meta_suffix(alias: Option<&str>, description: Option<&str>) -> String {
    let mut out = String::new();
    if let Some(alias) = alias {
        out.push_str(" @alias(");
        out.push_str(&quoted(alias));
        out.push(')');
    }
    if let Some(description) = description {
        out.push_str(" @description(");
        out.push_str(&quoted(description));
        out.push(')');
    }
    out
}

fn render_ty_source(ty: &baml_type::RealizedTy) -> String {
    match ty {
        baml_type::RealizedTy::Union(members, _) => {
            let mut non_null: Vec<_> = members.iter().filter(|member| !member.is_null()).collect();
            let has_null = non_null.len() != members.len();
            if has_null && non_null.len() == 1 {
                let member = non_null
                    .pop()
                    .unwrap_or_else(|| unreachable!("length checked"));
                let rendered = render_ty_source(member);
                if matches!(member, baml_type::RealizedTy::Function { .. }) {
                    format!("({rendered})?")
                } else {
                    format!("{rendered}?")
                }
            } else {
                let mut rendered: Vec<String> =
                    non_null.into_iter().map(render_ty_source).collect();
                if has_null {
                    rendered.push("null".to_string());
                }
                rendered.join(" | ")
            }
        }
        baml_type::RealizedTy::List(element, _) => {
            let rendered = render_ty_source(element);
            if matches!(element.as_ref(), baml_type::RealizedTy::Union(..)) {
                format!("({rendered})[]")
            } else {
                format!("{rendered}[]")
            }
        }
        baml_type::RealizedTy::Map { key, value, .. } => {
            format!(
                "map<{}, {}>",
                render_ty_source(key),
                render_ty_source(value)
            )
        }
        baml_type::RealizedTy::Class(name, args, _) if args.is_empty() => {
            name.display_name().to_string()
        }
        baml_type::RealizedTy::Enum(name, _) => name.display_name().to_string(),
        other => other.to_string(),
    }
}

fn render_type_value_source(vm: &BexVm, type_value: &TypeValue) -> String {
    let mut enum_ptrs: Vec<_> = type_value.defs().enums.values().copied().collect();
    let mut class_ptrs: Vec<_> = type_value.defs().classes.values().copied().collect();
    if !type_value.owner.as_ptr().is_null() {
        let Object::Package(package) = vm.get_object(type_value.owner) else {
            unreachable!("runtime type owner must be a Package")
        };
        for ptr in package.enums.values().copied() {
            if !enum_ptrs.contains(&ptr) {
                enum_ptrs.push(ptr);
            }
        }
        for ptr in package.classes.values().copied().filter(|ptr| {
            !matches!(
                vm.get_object(*ptr),
                Object::Class(class) if class.name.name().as_str().ends_with("$stream")
            )
        }) {
            if !class_ptrs.contains(&ptr) {
                class_ptrs.push(ptr);
            }
        }
    }

    let mut declarations = Vec::new();
    for ptr in &enum_ptrs {
        let Object::Enum(enm) = vm.get_object(*ptr) else {
            continue;
        };
        let mut source = format!("enum {} {{", enm.name.display_name());
        for variant in &enm.variants {
            source.push_str("\n  ");
            source.push_str(&variant.name);
            source.push_str(&render_meta_suffix(
                variant.alias.as_deref(),
                variant.description.as_deref(),
            ));
        }
        if let Some(alias) = enm.alias.as_deref() {
            source.push_str("\n  @@alias(");
            source.push_str(&quoted(alias));
            source.push(')');
        }
        if let Some(description) = enm.description.as_deref() {
            source.push_str("\n  @@description(");
            source.push_str(&quoted(description));
            source.push(')');
        }
        source.push_str("\n}");
        declarations.push(source);
    }
    for ptr in &class_ptrs {
        let Object::Class(class) = vm.get_object(*ptr) else {
            continue;
        };
        let mut source = format!("class {} {{", class.name.display_name());
        for field in &class.fields {
            source.push_str("\n  ");
            source.push_str(&field.name);
            source.push(' ');
            if let Some(field_type) = &field.runtime_type {
                source.push_str(&render_ty_source(&field_type.ty));
            } else if let Ok(field_type) = baml_type::RealizedTy::try_from(&field.field_type) {
                source.push_str(&render_ty_source(&field_type));
            } else {
                source.push_str(&field.field_type.to_string());
            }
            source.push_str(&render_meta_suffix(
                field.alias.as_deref(),
                field.description.as_deref(),
            ));
        }
        if let Some(alias) = class.alias.as_deref() {
            source.push_str("\n  @@alias(");
            source.push_str(&quoted(alias));
            source.push(')');
        }
        if let Some(description) = class.description.as_deref() {
            source.push_str("\n  @@description(");
            source.push_str(&quoted(description));
            source.push(')');
        }
        source.push_str("\n}");
        declarations.push(source);
    }
    let root_is_declared = match &type_value.ty {
        baml_type::RealizedTy::Class(name, _, _) => class_ptrs
            .iter()
            .any(|ptr| matches!(vm.get_object(*ptr), Object::Class(class) if class.name == *name)),
        baml_type::RealizedTy::Enum(name, _) => enum_ptrs
            .iter()
            .any(|ptr| matches!(vm.get_object(*ptr), Object::Enum(enm) if enm.name == *name)),
        _ => false,
    };
    if !root_is_declared {
        declarations.push(format!(
            "type RuntimeType = {}",
            render_ty_source(&type_value.ty)
        ));
    }
    declarations.join("\n\n")
}

/// The concrete `RealizedTy` wrapped by a `type` value (class, enum, interface,
/// primitive, container, …), or `None` if `value` isn't a `type`.
pub(super) fn type_value_ty(vm: &BexVm, value: Value) -> Option<baml_type::RealizedTy> {
    match vm.get_object(value.as_object_ptr()?) {
        Object::Type(type_value) => Some(type_value.ty.clone()),
        _ => None,
    }
}

fn as_kind(vm: &BexVm, value: Value, expected: baml_type::type_kind::TypeKind) -> Option<Value> {
    let ty = type_value_ty(vm, value)?;
    (baml_type::type_kind::classify_type(&ty) == expected).then_some(value)
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
    let Object::Type(type_value) = vm.get_object(ptr) else {
        return None;
    };
    match &type_value.ty {
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
/// primitive type value (`type.of<int>()`) has a name to key by, the way
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
