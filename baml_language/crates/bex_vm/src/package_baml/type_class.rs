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
    /// (`int` for `5`), never literal types. A runtime-created declaration needs
    /// no special handling: the reconstructed type is headed at the value's own
    /// `Instance::class` / `Variant::enm` pointer, which is the declaration
    /// whether or not the program image contains it.
    ///
    /// A value with no reconstructable type is an engine leak, not a user
    /// error: the set is exactly the objects BAML has no expression for — a
    /// bare declaration, an unscheduled future, an opaque native handle. They
    /// live on the heap but nothing in the language denotes one, so reaching
    /// here means the engine handed a non-value to a value position. Reporting
    /// `unknown` instead would be a false answer rather than a missing one:
    /// `unknown` is the top type, and every value is already a member of it.
    fn of_value(vm: &mut BexVm, v: &Value) -> Result<Value, VmRustFnError> {
        let Some(ty) = vm.value_concrete_ty(*v) else {
            return Err(VmRustFnError::InternalError(
                bex_vm_types::errors::VmInternalError::TypeError {
                    expected: bex_vm_types::types::Type::Object(bex_vm_types::ObjectType::Any),
                    got: vm.type_of(v),
                },
            ));
        };
        // Not `alloc_static_type`: a reconstructed type can name a runtime
        // declaration — directly, or through a compiled generic's arguments
        // (`Box<RuntimeFoo>`) or a container's element type.
        let ty = bex_vm_types::RealizedTy::from(ty);
        Ok(Value::object(vm.tlab.alloc_type(TypeValue::new(ty))))
    }
}

impl BamlClassTypeValue for PackageBamlImpl {
    fn array(vm: &mut BexVm, self_value: &Value) -> Value {
        let type_value = cloned_type_value(vm, *self_value);
        alloc_runtime_type(
            vm,
            bex_vm_types::RealizedTy::List(Box::new(type_value.ty), baml_type::TyAttr::default()),
        )
    }

    fn optional(vm: &mut BexVm, self_value: &Value) -> Value {
        let type_value = cloned_type_value(vm, *self_value);
        let mut members = match &type_value.ty {
            bex_vm_types::RealizedTy::Union(members, _) => members.clone(),
            other => vec![other.clone()],
        };
        if !members.iter().any(bex_vm_types::RealizedTy::is_null) {
            members.push(bex_vm_types::RealizedTy::null());
        }
        alloc_runtime_type(
            vm,
            bex_vm_types::RealizedTy::Union(members, baml_type::TyAttr::default()),
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
            bex_vm_types::RealizedTy::string(),
            bex_vm_types::RealizedTy::string(),
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

    fn _validate_renderable(vm: &mut BexVm, self_value: &Value) -> Result<(), VmRustFnError> {
        let type_value = cloned_type_value(vm, *self_value);
        let root = match &type_value.ty {
            bex_vm_types::RealizedTy::Class(head, _, _) => {
                baml_type::HeadDisplay::head_display_name(head)
            }
            _ => "output".to_string(),
        };
        let mut visited = std::collections::HashSet::new();
        let Some((field, open_ty)) = first_open_interface(vm, &type_value.ty, &root, &mut visited)
        else {
            if let Some(name) = first_conflicting_render_name(vm, &type_value.ty) {
                let diagnostic = super::type_kinds::compiler_diagnostic(
                    baml_compiler_diagnostics::DiagnosticId::ConflictingTypeDefinitionAtRender,
                    format!(
                        "type `{name}` has non-equivalent definitions in the same LLM render context"
                    ),
                );
                return Err(VmRustFnError::Thrown(
                    super::type_kinds::alloc_compilation_error(vm, &[diagnostic]),
                ));
            }
            return Ok(());
        };
        let diagnostic =
            baml_compiler_diagnostics::runtime_type::open_interface_at_render(&field, &open_ty);
        Err(VmRustFnError::Thrown(
            super::type_kinds::alloc_compilation_error(vm, &[diagnostic]),
        ))
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
        let Some((iface_head, iface_args, iface_assoc)) = reflected_interface(vm, *other) else {
            return false;
        };
        resolve::ImplResolver::for_value(vm, *self_value).type_implements(
            &self_ty,
            iface_head,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum RenderDefinition {
    Class(bex_vm_types::HeapPtr),
    Enum(bex_vm_types::HeapPtr),
    TypeAlias(bex_vm_types::HeapPtr),
}

struct RenderDefinitionValidator<'a> {
    vm: &'a BexVm,
    by_display_name: std::collections::HashMap<String, RenderDefinition>,
    visited: std::collections::HashSet<RenderDefinition>,
}

impl RenderDefinitionValidator<'_> {
    fn visit(&mut self, ty: &bex_vm_types::RealizedTy) -> Option<String> {
        match ty {
            bex_vm_types::RealizedTy::Class(head, args, _) => {
                let definition = RenderDefinition::Class(head.ptr());
                if let Some(conflict) = self.check_name(&definition) {
                    return Some(conflict);
                }
                if !self.visited.insert(definition) {
                    return None;
                }
                let class = find_render_class(self.vm, *head)?;
                for field in &class.fields {
                    if field.skip {
                        continue;
                    }
                    if let Some(runtime) = &field.runtime_type {
                        if let Some(conflict) = self.visit(&runtime.ty) {
                            return Some(conflict);
                        }
                        continue;
                    }
                    let field_ty =
                        field
                            .field_template
                            .substitute(args, self.vm)
                            .ok()
                            .or_else(|| {
                                bex_vm_types::RealizedTy::try_from(field.field_type.clone()).ok()
                            });
                    if let Some(field_ty) = field_ty
                        && let Some(conflict) = self.visit(&field_ty)
                    {
                        return Some(conflict);
                    }
                }
                None
            }
            bex_vm_types::RealizedTy::Enum(head, _) => {
                let definition = RenderDefinition::Enum(head.ptr());
                if let Some(conflict) = self.check_name(&definition) {
                    return Some(conflict);
                }
                self.visited.insert(definition);
                None
            }
            bex_vm_types::RealizedTy::TypeAlias(head, _) => {
                let definition = RenderDefinition::TypeAlias(head.ptr());
                if let Some(conflict) = self.check_name(&definition) {
                    return Some(conflict);
                }
                if !self.visited.insert(definition) {
                    return None;
                }
                self.vm
                    .type_alias_definition(head.ptr())
                    .cloned()
                    .and_then(|alias| self.visit(&alias))
            }
            bex_vm_types::RealizedTy::List(element, _) => self.visit(element),
            bex_vm_types::RealizedTy::Map { key, value, .. } => {
                self.visit(key).or_else(|| self.visit(value))
            }
            bex_vm_types::RealizedTy::Union(members, _) => {
                members.iter().find_map(|member| self.visit(member))
            }
            bex_vm_types::RealizedTy::Future(value, error, _) => {
                self.visit(value).or_else(|| self.visit(error))
            }
            bex_vm_types::RealizedTy::Function {
                params,
                ret,
                throws,
                ..
            } => params
                .iter()
                .find_map(|param| self.visit(&param.ty))
                .or_else(|| self.visit(ret))
                .or_else(|| self.visit(throws)),
            _ => None,
        }
    }

    fn check_name(&mut self, definition: &RenderDefinition) -> Option<String> {
        let display_name = render_definition_display_name(self.vm, definition);
        if let Some(previous) = self.by_display_name.get(&display_name) {
            if previous != definition
                && !render_definitions_equivalent(self.vm, previous, definition)
            {
                return Some(display_name);
            }
        } else {
            self.by_display_name.insert(display_name, *definition);
        }
        None
    }
}

fn first_conflicting_render_name(vm: &BexVm, ty: &bex_vm_types::RealizedTy) -> Option<String> {
    let mut validator = RenderDefinitionValidator {
        vm,
        by_display_name: std::collections::HashMap::new(),
        visited: std::collections::HashSet::new(),
    };
    validator.visit(ty)
}

fn render_definition_display_name(vm: &BexVm, definition: &RenderDefinition) -> String {
    let (RenderDefinition::Class(ptr)
    | RenderDefinition::Enum(ptr)
    | RenderDefinition::TypeAlias(ptr)) = definition;
    match vm.get_object(*ptr) {
        Object::Class(class) => class.name.display_name().to_string(),
        Object::Enum(enm) => enm.name.display_name().to_string(),
        Object::TypeAlias(alias) => alias.name.display_name().to_string(),
        _ => unreachable!("a render definition points at a nominal declaration"),
    }
}

fn find_render_class(vm: &BexVm, head: bex_vm_types::TypeHead) -> Option<&bex_vm_types::Class> {
    match vm.get_object(head.ptr()) {
        Object::Class(class) => Some(class),
        _ => None,
    }
}

fn render_definitions_equivalent(
    vm: &BexVm,
    left: &RenderDefinition,
    right: &RenderDefinition,
) -> bool {
    RenderDefinitionEquivalence {
        vm,
        left_to_right: std::collections::HashMap::new(),
        right_to_left: std::collections::HashMap::new(),
    }
    .definitions_equivalent(left, right)
}

/// A head's declaration name, for the render walk's *name* comparisons — this
/// check is about what two declarations would print as, which is exactly the
/// one question a name still answers better than an identity.
fn head_name(head: &bex_vm_types::TypeHead) -> String {
    baml_type::HeadDisplay::head_display_name(head)
}

struct RenderDefinitionEquivalence<'a> {
    vm: &'a BexVm,
    // A bidirectional definition mapping makes recursive equivalence
    // alpha-invariant while preserving graph sharing: A -> A is not equivalent
    // to a same-shaped B -> A when the candidate counterpart is B -> B.
    left_to_right: std::collections::HashMap<RenderDefinition, RenderDefinition>,
    right_to_left: std::collections::HashMap<RenderDefinition, RenderDefinition>,
}

impl RenderDefinitionEquivalence<'_> {
    fn definitions_equivalent(
        &mut self,
        left: &RenderDefinition,
        right: &RenderDefinition,
    ) -> bool {
        if render_definition_display_name(self.vm, left)
            != render_definition_display_name(self.vm, right)
        {
            return false;
        }
        if let Some(mapped) = self.left_to_right.get(left) {
            return mapped == right;
        }
        if let Some(mapped) = self.right_to_left.get(right) {
            return mapped == left;
        }
        self.left_to_right.insert(*left, *right);
        self.right_to_left.insert(*right, *left);

        match (left, right) {
            (RenderDefinition::Class(left), RenderDefinition::Class(right)) => {
                let (Object::Class(left), Object::Class(right)) =
                    (self.vm.get_object(*left), self.vm.get_object(*right))
                else {
                    return false;
                };
                let (left, right) = ((**left).clone(), (**right).clone());
                // `other` is intentionally absent (I-6), and docstring emission
                // remains undecided. Everything currently prompt/parse-visible is
                // compared, including order, aliases, descriptions, and SAP attrs.
                if left.description != right.description
                    || left.alias != right.alias
                    || left.ty_attr != right.ty_attr
                    || left.generic_param_count != right.generic_param_count
                    || left.fields.len() != right.fields.len()
                {
                    return false;
                }
                for (left, right) in left.fields.iter().zip(&right.fields) {
                    if left.name != right.name
                        || left.description != right.description
                        || left.alias != right.alias
                        || left.skip != right.skip
                        || !self.runtime_types_equivalent(&left.field_type, &right.field_type)
                    {
                        return false;
                    }
                }
                true
            }
            (RenderDefinition::Enum(left), RenderDefinition::Enum(right)) => {
                let (Object::Enum(left), Object::Enum(right)) =
                    (self.vm.get_object(*left), self.vm.get_object(*right))
                else {
                    return false;
                };
                left.description == right.description
                    && left.alias == right.alias
                    && left.ty_attr == right.ty_attr
                    && left.variants.len() == right.variants.len()
                    && left
                        .variants
                        .iter()
                        .zip(&right.variants)
                        .all(|(left, right)| {
                            left.name == right.name
                                && left.description == right.description
                                && left.alias == right.alias
                                && left.skip == right.skip
                        })
            }
            (RenderDefinition::TypeAlias(left), RenderDefinition::TypeAlias(right)) => {
                let (Some(left), Some(right)) = (
                    self.vm.type_alias_definition(*left).cloned(),
                    self.vm.type_alias_definition(*right).cloned(),
                ) else {
                    return false;
                };
                self.types_equivalent(&left, &right)
            }
            _ => false,
        }
    }

    fn runtime_types_equivalent(
        &mut self,
        left: &bex_vm_types::RuntimeTy,
        right: &bex_vm_types::RuntimeTy,
    ) -> bool {
        let (Ok(left), Ok(right)) = (
            bex_vm_types::RealizedTy::try_from(left.clone()),
            bex_vm_types::RealizedTy::try_from(right.clone()),
        ) else {
            return left == right;
        };
        self.types_equivalent(&left, &right)
    }

    fn types_equivalent(
        &mut self,
        left: &bex_vm_types::RealizedTy,
        right: &bex_vm_types::RealizedTy,
    ) -> bool {
        use bex_vm_types::RealizedTy;

        match (left, right) {
            (
                RealizedTy::Class(left_head, left_args, left_attr),
                RealizedTy::Class(right_head, right_args, right_attr),
            ) => {
                head_name(left_head) == head_name(right_head)
                    && left_attr == right_attr
                    && self.type_lists_equivalent(left_args, right_args)
                    && self.definitions_equivalent(
                        &RenderDefinition::Class(left_head.ptr()),
                        &RenderDefinition::Class(right_head.ptr()),
                    )
            }
            (
                RealizedTy::Interface(left_head, left_args, left_assoc, left_attr),
                RealizedTy::Interface(right_head, right_args, right_assoc, right_attr),
            ) => {
                if head_name(left_head) != head_name(right_head)
                    || left_attr != right_attr
                    || !self.type_lists_equivalent(left_args, right_args)
                    || left_assoc.len() != right_assoc.len()
                {
                    return false;
                }
                for ((left_name, left_ty), (right_name, right_ty)) in
                    left_assoc.iter().zip(right_assoc)
                {
                    if left_name != right_name || !self.types_equivalent(left_ty, right_ty) {
                        return false;
                    }
                }
                true
            }
            (RealizedTy::Enum(left_head, left_attr), RealizedTy::Enum(right_head, right_attr)) => {
                head_name(left_head) == head_name(right_head)
                    && left_attr == right_attr
                    && self.definitions_equivalent(
                        &RenderDefinition::Enum(left_head.ptr()),
                        &RenderDefinition::Enum(right_head.ptr()),
                    )
            }
            (
                RealizedTy::TypeAlias(left_head, left_attr),
                RealizedTy::TypeAlias(right_head, right_attr),
            ) => {
                head_name(left_head) == head_name(right_head)
                    && left_attr == right_attr
                    && self.definitions_equivalent(
                        &RenderDefinition::TypeAlias(left_head.ptr()),
                        &RenderDefinition::TypeAlias(right_head.ptr()),
                    )
            }
            (
                RealizedTy::EnumVariant(left_head, left_variant, left_attr),
                RealizedTy::EnumVariant(right_head, right_variant, right_attr),
            ) => {
                head_name(left_head) == head_name(right_head)
                    && left_variant == right_variant
                    && left_attr == right_attr
                    && self.definitions_equivalent(
                        &RenderDefinition::Enum(left_head.ptr()),
                        &RenderDefinition::Enum(right_head.ptr()),
                    )
            }
            (RealizedTy::List(left, left_attr), RealizedTy::List(right, right_attr)) => {
                left_attr == right_attr && self.types_equivalent(left, right)
            }
            (
                RealizedTy::Map {
                    key: left_key,
                    value: left_value,
                    attr: left_attr,
                },
                RealizedTy::Map {
                    key: right_key,
                    value: right_value,
                    attr: right_attr,
                },
            ) => {
                left_attr == right_attr
                    && self.types_equivalent(left_key, right_key)
                    && self.types_equivalent(left_value, right_value)
            }
            (RealizedTy::Union(left, left_attr), RealizedTy::Union(right, right_attr)) => {
                left_attr == right_attr && self.type_lists_equivalent(left, right)
            }
            (
                RealizedTy::Function {
                    params: left_params,
                    ret: left_ret,
                    throws: left_throws,
                    attr: left_attr,
                },
                RealizedTy::Function {
                    params: right_params,
                    ret: right_ret,
                    throws: right_throws,
                    attr: right_attr,
                },
            ) => {
                if left_attr != right_attr || left_params.len() != right_params.len() {
                    return false;
                }
                for (left, right) in left_params.iter().zip(right_params) {
                    if left.name != right.name
                        || left.mode != right.mode
                        || !self.types_equivalent(&left.ty, &right.ty)
                    {
                        return false;
                    }
                }
                self.types_equivalent(left_ret, right_ret)
                    && self.types_equivalent(left_throws, right_throws)
            }
            (
                RealizedTy::Future(left_value, left_error, left_attr),
                RealizedTy::Future(right_value, right_error, right_attr),
            ) => {
                left_attr == right_attr
                    && self.types_equivalent(left_value, right_value)
                    && self.types_equivalent(left_error, right_error)
            }
            _ => left == right,
        }
    }

    fn type_lists_equivalent(
        &mut self,
        left: &[bex_vm_types::RealizedTy],
        right: &[bex_vm_types::RealizedTy],
    ) -> bool {
        if left.len() != right.len() {
            return false;
        }
        for (left, right) in left.iter().zip(right) {
            if !self.types_equivalent(left, right) {
                return false;
            }
        }
        true
    }
}

fn first_open_interface(
    vm: &BexVm,
    ty: &bex_vm_types::RealizedTy,
    path: &str,
    visited: &mut std::collections::HashSet<bex_vm_types::HeapPtr>,
) -> Option<(String, String)> {
    match ty {
        bex_vm_types::RealizedTy::Interface(..) => Some((path.to_string(), ty.to_string())),
        bex_vm_types::RealizedTy::Class(head, args, _) => {
            if !visited.insert(head.ptr()) {
                return None;
            }
            let Object::Class(class) = vm.get_object(head.ptr()) else {
                return None;
            };
            for field in &class.fields {
                let child_path = format!("{path}.{}", field.name);
                if let Some(runtime) = &field.runtime_type {
                    if let Some(found) = first_open_interface(vm, &runtime.ty, &child_path, visited)
                    {
                        return Some(found);
                    }
                    continue;
                }
                let field_ty =
                    field.field_template.substitute(args, vm).ok().or_else(|| {
                        bex_vm_types::RealizedTy::try_from(field.field_type.clone()).ok()
                    });
                if let Some(field_ty) = field_ty
                    && let Some(found) = first_open_interface(vm, &field_ty, &child_path, visited)
                {
                    return Some(found);
                }
            }
            None
        }
        bex_vm_types::RealizedTy::List(element, _) => {
            first_open_interface(vm, element, path, visited)
        }
        bex_vm_types::RealizedTy::Map { key, value, .. } => {
            first_open_interface(vm, key, path, visited)
                .or_else(|| first_open_interface(vm, value, path, visited))
        }
        bex_vm_types::RealizedTy::Union(members, _) => members
            .iter()
            .find_map(|member| first_open_interface(vm, member, path, visited)),
        bex_vm_types::RealizedTy::Future(value, error, _) => {
            first_open_interface(vm, value, path, visited)
                .or_else(|| first_open_interface(vm, error, path, visited))
        }
        bex_vm_types::RealizedTy::Function {
            params,
            ret,
            throws,
            ..
        } => params
            .iter()
            .find_map(|param| first_open_interface(vm, &param.ty, path, visited))
            .or_else(|| first_open_interface(vm, ret, path, visited))
            .or_else(|| first_open_interface(vm, throws, path, visited)),
        bex_vm_types::RealizedTy::TypeAlias(head, _) => {
            if !visited.insert(head.ptr()) {
                return None;
            }
            vm.type_alias_definition(head.ptr())
                .cloned()
                .and_then(|alias| first_open_interface(vm, &alias, path, visited))
        }
        _ => None,
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

fn alloc_runtime_type(vm: &mut BexVm, ty: bex_vm_types::RealizedTy) -> Value {
    Value::object(vm.tlab.alloc_type(TypeValue::new(ty)))
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

fn render_ty_source(ty: &bex_vm_types::RealizedTy) -> String {
    match ty {
        bex_vm_types::RealizedTy::Union(members, _) => {
            let mut non_null: Vec<_> = members.iter().filter(|member| !member.is_null()).collect();
            let has_null = non_null.len() != members.len();
            if has_null && non_null.len() == 1 {
                let member = non_null
                    .pop()
                    .unwrap_or_else(|| unreachable!("length checked"));
                let rendered = render_ty_source(member);
                if matches!(member, bex_vm_types::RealizedTy::Function { .. }) {
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
        bex_vm_types::RealizedTy::List(element, _) => {
            let rendered = render_ty_source(element);
            if matches!(element.as_ref(), bex_vm_types::RealizedTy::Union(..)) {
                format!("({rendered})[]")
            } else {
                format!("{rendered}[]")
            }
        }
        bex_vm_types::RealizedTy::Map { key, value, .. } => {
            format!(
                "map<{}, {}>",
                render_ty_source(key),
                render_ty_source(value)
            )
        }
        bex_vm_types::RealizedTy::Class(head, args, _) if args.is_empty() => head_name(head),
        bex_vm_types::RealizedTy::Enum(head, _) => head_name(head),
        other => other.to_string(),
    }
}

/// Render a runtime class's `implements` blocks from the impl rules dispatch
/// actually consults.
///
/// The rules are the witness — there is no parallel description of one to read
/// instead, so what this prints and what a virtual call resolves cannot
/// disagree. A rule's `field_links` are physical field slots, so the interface's
/// declared field order supplies the left-hand names and `class` the right.
fn render_witness_sources(vm: &BexVm, source: &mut String, class_ptr: bex_vm_types::HeapPtr) {
    let Object::Class(class) = vm.get_object(class_ptr) else {
        return;
    };
    for rule_ptr in vm.dynamic_dispatch.rules_for_class(class_ptr) {
        let Object::ImplRule(rule) = vm.get_object(rule_ptr) else {
            continue;
        };
        let Object::Interface(interface) = vm.get_object(rule.interface_head) else {
            continue;
        };
        source.push_str("\n  implements ");
        source.push_str(interface.name.render_user_facing().as_str());
        let args = rule
            .interface_args
            .iter()
            .filter_map(|arg| bex_vm_types::RealizedTy::try_from(arg).ok())
            .map(|arg| render_ty_source(&arg))
            .collect::<Vec<_>>();
        if !args.is_empty() {
            source.push('<');
            source.push_str(&args.join(", "));
            source.push('>');
        }
        let assoc = rule
            .interface_assoc
            .iter()
            .filter_map(|(name, ty)| {
                bex_vm_types::RealizedTy::try_from(ty)
                    .ok()
                    .map(|ty| (name, ty))
            })
            .collect::<Vec<_>>();
        if assoc.is_empty() && rule.field_links.is_empty() {
            source.push_str(" {}");
            continue;
        }
        source.push_str(" {");
        for (name, ty) in assoc {
            source.push_str("\n    type ");
            source.push_str(name.as_str());
            source.push_str(" = ");
            source.push_str(&render_ty_source(&ty));
        }
        for (declared, slot) in interface.fields.iter().zip(&*rule.field_links) {
            let Some(field) = class.fields.get(*slot as usize) else {
                continue;
            };
            source.push_str("\n    ");
            source.push_str(declared.name.as_str());
            source.push_str(" as ");
            source.push_str(&field.name);
        }
        source.push_str("\n  }");
    }
}

fn render_type_value_source(vm: &BexVm, type_value: &TypeValue) -> String {
    // The type's heads reach every declaration it depends on, and each runtime
    // declaration's `owner` reaches the package that declared it — so the set
    // to render is a walk, not a table. A package contributes its whole surface
    // (a `Package.compile` result renders as the source it was compiled from),
    // minus the `$stream` companions, which are synthesized rather than written.
    let (mut class_ptrs, mut enum_ptrs) = crate::reachable::runtime_nominals(vm, &type_value.ty);
    let mut owners = class_ptrs
        .iter()
        .chain(&enum_ptrs)
        .filter_map(|ptr| match vm.get_object(*ptr) {
            Object::Class(class) => Some(class.owner),
            Object::Enum(enm) => Some(enm.owner),
            _ => None,
        })
        .filter(|owner| !owner.is_null())
        .collect::<Vec<_>>();
    owners.dedup();
    for owner in owners {
        let Object::Package(package) = vm.get_object(owner) else {
            continue;
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
            } else if let Ok(field_type) = bex_vm_types::RealizedTy::try_from(&field.field_type) {
                source.push_str(&render_ty_source(&field_type));
            } else {
                source.push_str(&field.field_type.to_string());
            }
            source.push_str(&render_meta_suffix(
                field.alias.as_deref(),
                field.description.as_deref(),
            ));
        }
        render_witness_sources(vm, &mut source, *ptr);
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
        bex_vm_types::RealizedTy::Class(head, _, _) => class_ptrs.contains(&head.ptr()),
        bex_vm_types::RealizedTy::Enum(head, _) => enum_ptrs.contains(&head.ptr()),
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
pub(super) fn type_value_ty(vm: &BexVm, value: Value) -> Option<bex_vm_types::RealizedTy> {
    match vm.get_object(value.as_object_ptr()?) {
        Object::Type(type_value) => Some(type_value.ty.clone()),
        _ => None,
    }
}

fn as_kind(vm: &BexVm, value: Value, expected: baml_type::type_kind::TypeKind) -> Option<Value> {
    let ty = type_value_ty(vm, value)?;
    (baml_type::type_kind::classify_type(&ty) == expected).then_some(value)
}

/// A realized interface instantiation as reflected off a `type` value: which
/// interface, its realized generic arguments, and its associated bindings.
type RealizedInterfaceInstantiation = (
    bex_vm_types::TypeHead,
    Vec<bex_vm_types::RealizedTy>,
    Vec<(baml_type::Name, bex_vm_types::RealizedTy)>,
);

/// The interface a `type` value denotes, or `None` when it denotes anything
/// else.
///
/// Narrow on purpose: the one caller asks "does X implement this", and only an
/// interface can answer. A class or primitive in that position used to be
/// carried along as a bare name and then matched no rule — the same answer,
/// arrived at by pretending the name might have been an interface.
fn reflected_interface(vm: &BexVm, value: Value) -> Option<RealizedInterfaceInstantiation> {
    let ptr = value.as_object_ptr()?;
    let Object::Type(type_value) = vm.get_object(ptr) else {
        return None;
    };
    match &type_value.ty {
        bex_vm_types::RealizedTy::Interface(head, args, associated_bindings, _) => {
            Some((*head, args.clone(), associated_bindings.clone()))
        }
        _ => None,
    }
}
