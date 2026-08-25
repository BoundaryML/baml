use bex_vm_types::types::{Object, TypeValue, Value};
use indexmap::IndexMap;

use super::{BamlClassType, PackageReflectImpl, resolve};
use crate::{BexVm, errors::VmRustFnError};

impl BamlClassType for PackageReflectImpl {
    /// BEP-066 K-13: `reflect.Type.of_value(v)` — the runtime `type` value describing
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
        let ty = bex_vm_types::RealizedTy::from(ty);
        Ok(Value::object(vm.tlab.alloc_type(TypeValue::new(ty))))
    }

    fn array(vm: &mut BexVm, self_value: &Value) -> Value {
        let type_value = cloned_type_value(vm, *self_value);
        let array_ty = alloc_runtime_type(
            vm,
            bex_vm_types::RealizedTy::List(Box::new(type_value.ty), baml_type::TyAttr::default()),
        );
        super::type_kinds::alloc_kind_view(vm, baml_type::type_kind::TypeKind::Array, array_ty)
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
        let union_ty = alloc_runtime_type(
            vm,
            bex_vm_types::RealizedTy::Union(members, baml_type::TyAttr::default()),
        );
        super::type_kinds::alloc_kind_view(vm, baml_type::type_kind::TypeKind::Union, union_ty)
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
        let other = super::type_kinds::string_map_rows(vm, other);
        super::type_kinds::alloc_with_meta(
            vm,
            *self_value,
            alias.map(bex_str::BexStr::as_str),
            description.map(bex_str::BexStr::as_str),
            docstring.map(bex_str::BexStr::as_str),
            &other,
        )
    }

    fn kind(vm: &mut BexVm, self_value: &Value) -> Value {
        let ty = type_value_ty(vm, *self_value)
            .unwrap_or_else(|| unreachable!("kind receiver must be Object::Type"));
        let kind = baml_type::type_kind::classify_type(&ty);
        super::type_kinds::alloc_kind_view(vm, kind, *self_value)
    }

    fn as_class(vm: &mut BexVm, self_value: &Value) -> Option<Value> {
        as_kind(vm, *self_value, baml_type::type_kind::TypeKind::Class)
    }

    fn as_enum(vm: &mut BexVm, self_value: &Value) -> Option<Value> {
        as_kind(vm, *self_value, baml_type::type_kind::TypeKind::Enum)
    }

    fn as_union(vm: &mut BexVm, self_value: &Value) -> Option<Value> {
        as_kind(vm, *self_value, baml_type::type_kind::TypeKind::Union)
    }

    fn as_literal(vm: &mut BexVm, self_value: &Value) -> Option<Value> {
        as_kind(vm, *self_value, baml_type::type_kind::TypeKind::Literal)
    }

    fn as_array(vm: &mut BexVm, self_value: &Value) -> Option<Value> {
        as_kind(vm, *self_value, baml_type::type_kind::TypeKind::Array)
    }

    fn as_map(vm: &mut BexVm, self_value: &Value) -> Option<Value> {
        as_kind(vm, *self_value, baml_type::type_kind::TypeKind::Map)
    }

    fn as_interface(vm: &mut BexVm, self_value: &Value) -> Option<Value> {
        as_kind(vm, *self_value, baml_type::type_kind::TypeKind::Interface)
    }

    fn as_primitive(vm: &mut BexVm, self_value: &Value) -> Option<Value> {
        as_kind(vm, *self_value, baml_type::type_kind::TypeKind::Primitive)
    }

    fn as_function(vm: &mut BexVm, self_value: &Value) -> Option<Value> {
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
        // Run the bounded specialization analysis first: the subsequent structural
        // interface walk keys every realization and would otherwise follow a
        // non-regular generic transform forever.
        if let Some(message) = first_render_schema_error(vm, &type_value.ty) {
            let diagnostic = super::type_kinds::compiler_diagnostic(
                baml_compiler_diagnostics::DiagnosticId::ConflictingTypeDefinitionAtRender,
                message,
            );
            return Err(VmRustFnError::thrown_fresh(
                super::type_kinds::alloc_compilation_error(vm, &[diagnostic]),
            ));
        }
        let mut visited = std::collections::HashSet::new();
        if let Some((field, open_ty)) =
            first_open_interface(vm, &type_value.ty, &root, &mut visited)
        {
            let diagnostic =
                baml_compiler_diagnostics::runtime_type::open_interface_at_render(&field, &open_ty);
            return Err(VmRustFnError::thrown_fresh(
                super::type_kinds::alloc_compilation_error(vm, &[diagnostic]),
            ));
        }

        let mut visited = std::collections::HashSet::new();
        if let Some((path, non_data_ty)) =
            first_non_data_type(vm, &type_value.ty, &root, &mut visited)
        {
            let diagnostic = if path == root {
                baml_compiler_diagnostics::runtime_type::non_data_type_at_render(&non_data_ty)
            } else {
                baml_compiler_diagnostics::runtime_type::non_data_field_at_render(
                    &path,
                    &non_data_ty,
                )
            };
            return Err(VmRustFnError::thrown_fresh(
                super::type_kinds::alloc_compilation_error(vm, &[diagnostic]),
            ));
        }

        Ok(())
    }

    /// Returns the `RealizedTy`'s display name.  Includes namespaces and (for
    /// non-`user` packages) the package prefix, so two distinct types never
    /// collide on this string — package names are unique within a workspace,
    /// so eliding the implicit `user.` prefix is unambiguous.
    ///
    /// This identity guarantee makes the result usable as a stable key in
    /// `map<string, V>` until generic-K interfaces enable a real
    /// `map<reflect.Type, V>`.
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

/// One realization of a generic class — the declaration's identity plus the
/// exact arguments — so `Foo<int>` and `Foo<string>` are two entries while two
/// mentions of `Foo<int>` are one.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct RealizedClassIdentity {
    head: bex_vm_types::TypeHead,
    args: Vec<bex_vm_types::RealizedTy>,
}

struct RealizedClassFrame {
    identity: RealizedClassIdentity,
    head: bex_vm_types::TypeHead,
    display_name: String,
    arity: usize,
}

struct RenderedClass {
    identity: RealizedClassIdentity,
    display_name: String,
    rendered_name: String,
    definition: RenderDefinition,
}

struct RenderDefinitionValidator<'a> {
    vm: &'a BexVm,
    by_display_name: std::collections::HashMap<String, RenderDefinition>,
    visited: std::collections::HashSet<RenderDefinition>,
    class_ancestry: Vec<RealizedClassFrame>,
    recursive_classes: std::collections::HashSet<RealizedClassIdentity>,
    rendered_classes: Vec<RenderedClass>,
}

/// The origins lane used by the render walk: symbolic realizations headed by
/// the same declaration identities the walk itself keys on.
type RenderOrigins = baml_type::template::TyTemplateOrigins<bex_vm_types::TypeHead>;

impl RenderDefinitionValidator<'_> {
    fn visit(&mut self, ty: &bex_vm_types::RealizedTy, origins: &RenderOrigins) -> Option<String> {
        match ty {
            bex_vm_types::RealizedTy::Class(head, args, _) => {
                let definition = RenderDefinition::Class(head.ptr());
                if let Some(conflict) = self.check_name(&definition) {
                    return Some(conflict);
                }
                let identity = RealizedClassIdentity {
                    head: *head,
                    args: args.clone(),
                };
                if let Some(start) = self
                    .class_ancestry
                    .iter()
                    .position(|ancestor| ancestor.identity == identity)
                {
                    self.recursive_classes.extend(
                        self.class_ancestry[start..]
                            .iter()
                            .map(|ancestor| ancestor.identity.clone()),
                    );
                    return None;
                }

                let display_name = realized_class_name(*head, args);
                for (index, ancestor) in self.class_ancestry.iter().enumerate() {
                    if ancestor.head == *head
                        && origins.class_transform_expands(index, head, ancestor.arity)
                    {
                        return Some(format!(
                            "non-regular recursive generic class `{}` expands from `{}` to `{display_name}` and cannot be rendered as an LLM output schema",
                            baml_type::HeadDisplay::head_display_name(head),
                            ancestor.display_name,
                        ));
                    }
                }

                let class = find_render_class(self.vm, *head)?;
                let rendered_name = rendered_realized_class_name(class, args);
                self.rendered_classes.push(RenderedClass {
                    identity: identity.clone(),
                    display_name: display_name.clone(),
                    rendered_name,
                    definition,
                });
                self.class_ancestry.push(RealizedClassFrame {
                    identity,
                    head: *head,
                    display_name,
                    arity: args.len(),
                });
                for field in &class.fields {
                    if field.skip {
                        continue;
                    }
                    if let Some(runtime) = &field.runtime_type {
                        let runtime_origins = RenderOrigins::opaque(self.class_ancestry.len());
                        if let Some(conflict) = self.visit(&runtime.ty, &runtime_origins) {
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
                    if let Some(field_ty) = field_ty {
                        let field_origins =
                            origins.through_field(head, args.len(), &field.field_template);
                        if let Some(conflict) = self.visit(&field_ty, &field_origins) {
                            return Some(conflict);
                        }
                    }
                }
                self.class_ancestry.pop();
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
                    .and_then(|alias| {
                        let alias_origins = RenderOrigins::opaque(self.class_ancestry.len());
                        self.visit(&alias, &alias_origins)
                    })
            }
            bex_vm_types::RealizedTy::List(element, _) => {
                self.visit(element, &origins.list_element())
            }
            bex_vm_types::RealizedTy::Map { key, value, .. } => self
                .visit(key, &origins.map_key())
                .or_else(|| self.visit(value, &origins.map_value())),
            bex_vm_types::RealizedTy::Union(members, _) => members
                .iter()
                .enumerate()
                .find_map(|(index, member)| self.visit(member, &origins.union_member(index))),
            bex_vm_types::RealizedTy::Future(value, error, _) => self
                .visit(value, &origins.future_value())
                .or_else(|| self.visit(error, &origins.future_error())),
            bex_vm_types::RealizedTy::Function {
                params,
                ret,
                throws,
                ..
            } => params
                .iter()
                .enumerate()
                .find_map(|(index, param)| self.visit(&param.ty, &origins.function_param(index)))
                .or_else(|| self.visit(ret, &origins.function_return()))
                .or_else(|| self.visit(throws, &origins.function_throws())),
            _ => None,
        }
    }

    fn first_recursive_alias_collision(&self) -> Option<String> {
        let mut by_rendered_name = std::collections::HashMap::<&str, &RenderedClass>::new();
        for class in self
            .rendered_classes
            .iter()
            .filter(|class| self.recursive_classes.contains(&class.identity))
        {
            if let Some(first) = by_rendered_name.get(class.rendered_name.as_str()) {
                let equivalent = first.identity.args == class.identity.args
                    && render_definitions_equivalent(self.vm, &first.definition, &class.definition);
                if first.identity != class.identity && !equivalent {
                    return Some(format!(
                        "classes `{}` and `{}` both render as `{}` in the same LLM render context",
                        first.display_name, class.display_name, class.rendered_name,
                    ));
                }
            } else {
                by_rendered_name.insert(class.rendered_name.as_str(), class);
            }
        }
        None
    }

    fn check_name(&mut self, definition: &RenderDefinition) -> Option<String> {
        let display_name = render_definition_display_name(self.vm, definition);
        if let Some(previous) = self.by_display_name.get(&display_name) {
            if previous != definition
                && !render_definitions_equivalent(self.vm, previous, definition)
            {
                return Some(format!(
                    "type `{display_name}` has non-equivalent definitions in the same LLM render context"
                ));
            }
        } else {
            self.by_display_name.insert(display_name, *definition);
        }
        None
    }
}

/// The first reason `ty` cannot be rendered as an LLM output schema: a display
/// name shared by non-equivalent declarations, a non-regular recursive generic
/// (whose specializations would grow forever), or two distinct recursive
/// classes hoisted under one rendered name.
fn first_render_schema_error(vm: &BexVm, ty: &bex_vm_types::RealizedTy) -> Option<String> {
    let mut validator = RenderDefinitionValidator {
        vm,
        by_display_name: std::collections::HashMap::new(),
        visited: std::collections::HashSet::new(),
        class_ancestry: Vec::new(),
        recursive_classes: std::collections::HashSet::new(),
        rendered_classes: Vec::new(),
    };
    validator
        .visit(ty, &RenderOrigins::root())
        .or_else(|| validator.first_recursive_alias_collision())
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

/// The instantiation's display label: `Foo` bare, `Foo<int, string>` applied.
/// A label, never a key — the walk keys on [`RealizedClassIdentity`].
fn realized_class_name(head: bex_vm_types::TypeHead, args: &[bex_vm_types::RealizedTy]) -> String {
    let base = baml_type::HeadDisplay::head_display_name(&head);
    if args.is_empty() {
        base
    } else {
        format!(
            "{base}<{}>",
            args.iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        )
    }
}

/// [`realized_class_name`] as the schema renders it: the class alias wins over
/// the declared name when one is set.
fn rendered_realized_class_name(
    class: &bex_vm_types::Class,
    args: &[bex_vm_types::RealizedTy],
) -> String {
    let display_name = class.name.display_name();
    let base = class.alias.as_deref().unwrap_or(display_name.as_str());
    if args.is_empty() {
        base.to_string()
    } else {
        format!(
            "{base}<{}>",
            args.iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        )
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
    visited: &mut std::collections::HashSet<bex_vm_types::RealizedTy>,
) -> Option<(String, String)> {
    match ty {
        bex_vm_types::RealizedTy::Interface(..) => Some((path.to_string(), ty.to_string())),
        bex_vm_types::RealizedTy::Class(head, args, _) => {
            // Deduplicate by *instantiation*, never by declaration: the walk
            // substitutes class arguments into field templates, so `Box<int>`
            // and `Box<OpenIface>` reach different field types — a declaration
            // key would let whichever arrives first swallow the other's walk.
            if !visited.insert(ty.clone()) {
                return None;
            }
            let Object::Class(class) = vm.get_object(head.ptr()) else {
                return None;
            };
            for field in class.fields.iter().filter(|field| !field.skip) {
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
            if !visited.insert(ty.clone()) {
                return None;
            }
            vm.type_alias_definition(head.ptr())
                .cloned()
                .and_then(|alias| first_open_interface(vm, &alias, path, visited))
        }
        _ => None,
    }
}

/// Find the first type that has no output-format representation.
///
/// This match is exhaustive over `RealizedTy` so a newly added runtime type
/// must make an explicit renderability decision at the shared LLM boundary.
fn is_non_data_render_type(ty: &bex_vm_types::RealizedTy) -> bool {
    match ty {
        bex_vm_types::RealizedTy::Uint8Array { .. }
        | bex_vm_types::RealizedTy::EnumVariant(..)
        | bex_vm_types::RealizedTy::Function { .. }
        | bex_vm_types::RealizedTy::Future(..)
        | bex_vm_types::RealizedTy::RustType { .. }
        | bex_vm_types::RealizedTy::Type { .. }
        | bex_vm_types::RealizedTy::Resource { .. }
        | bex_vm_types::RealizedTy::PromptAst { .. }
        | bex_vm_types::RealizedTy::Void { .. }
        | bex_vm_types::RealizedTy::BuiltinUnknown { .. }
        | bex_vm_types::RealizedTy::Never { .. } => true,
        bex_vm_types::RealizedTy::Int { .. }
        | bex_vm_types::RealizedTy::Bigint { .. }
        | bex_vm_types::RealizedTy::Float { .. }
        | bex_vm_types::RealizedTy::String { .. }
        | bex_vm_types::RealizedTy::Bool { .. }
        | bex_vm_types::RealizedTy::Null { .. }
        | bex_vm_types::RealizedTy::Media(..)
        | bex_vm_types::RealizedTy::Literal(..)
        | bex_vm_types::RealizedTy::Class(..)
        | bex_vm_types::RealizedTy::Interface(..)
        | bex_vm_types::RealizedTy::Enum(..)
        | bex_vm_types::RealizedTy::List(..)
        | bex_vm_types::RealizedTy::Map { .. }
        | bex_vm_types::RealizedTy::Union(..)
        | bex_vm_types::RealizedTy::TypeAlias(..) => false,
    }
}

fn first_non_data_type(
    vm: &BexVm,
    ty: &bex_vm_types::RealizedTy,
    path: &str,
    visited: &mut std::collections::HashSet<bex_vm_types::HeapPtr>,
) -> Option<(String, String)> {
    if is_non_data_render_type(ty) {
        return Some((path.to_string(), ty.to_string()));
    }

    match ty {
        bex_vm_types::RealizedTy::Class(head, args, _) => {
            if !visited.insert(head.ptr()) {
                return None;
            }
            let Object::Class(class) = vm.get_object(head.ptr()) else {
                return None;
            };
            for field in class.fields.iter().filter(|field| !field.skip) {
                let child_path = format!("{path}.{}", field.name);
                if let Some(runtime) = &field.runtime_type {
                    if let Some(found) = first_non_data_type(vm, &runtime.ty, &child_path, visited)
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
                    && let Some(found) = first_non_data_type(vm, &field_ty, &child_path, visited)
                {
                    return Some(found);
                }
            }
            None
        }
        bex_vm_types::RealizedTy::List(element, _) => {
            first_non_data_type(vm, element, path, visited)
        }
        bex_vm_types::RealizedTy::Map { key, value, .. } => {
            first_non_data_type(vm, key, path, visited)
                .or_else(|| first_non_data_type(vm, value, path, visited))
        }
        bex_vm_types::RealizedTy::Union(members, _) => members
            .iter()
            .find_map(|member| first_non_data_type(vm, member, path, visited)),
        bex_vm_types::RealizedTy::TypeAlias(head, _) => {
            if !visited.insert(head.ptr()) {
                return None;
            }
            vm.type_alias_definition(head.ptr())
                .cloned()
                .and_then(|alias| first_non_data_type(vm, &alias, path, visited))
        }
        bex_vm_types::RealizedTy::Int { .. }
        | bex_vm_types::RealizedTy::Bigint { .. }
        | bex_vm_types::RealizedTy::Float { .. }
        | bex_vm_types::RealizedTy::String { .. }
        | bex_vm_types::RealizedTy::Bool { .. }
        | bex_vm_types::RealizedTy::Null { .. }
        | bex_vm_types::RealizedTy::Uint8Array { .. }
        | bex_vm_types::RealizedTy::Media(..)
        | bex_vm_types::RealizedTy::Literal(..)
        | bex_vm_types::RealizedTy::Interface(..)
        | bex_vm_types::RealizedTy::Enum(..)
        | bex_vm_types::RealizedTy::EnumVariant(..)
        | bex_vm_types::RealizedTy::Function { .. }
        | bex_vm_types::RealizedTy::Future(..)
        | bex_vm_types::RealizedTy::RustType { .. }
        | bex_vm_types::RealizedTy::Type { .. }
        | bex_vm_types::RealizedTy::Resource { .. }
        | bex_vm_types::RealizedTy::PromptAst { .. }
        | bex_vm_types::RealizedTy::Void { .. }
        | bex_vm_types::RealizedTy::BuiltinUnknown { .. }
        | bex_vm_types::RealizedTy::Never { .. } => None,
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

fn render_ty_source(
    ty: &bex_vm_types::RealizedTy,
    spellings: &indexmap::IndexMap<baml_type::typetag::TypeTag, String>,
) -> String {
    let ty = ty.map_heads(&mut |head| {
        let name = spellings.get(&head.tag()).map_or_else(
            || {
                head.tagged_name()
                    .map(|tagged| tagged.name().clone())
                    .unwrap_or_else(|| unreachable!("a live type head names a declaration"))
            },
            |name| baml_type::DeclarationName::Anonymous(baml_type::Name::new(name)),
        );
        baml_type::TaggedTypeName::new(head.tag(), name)
    });
    render_named_ty_source(&ty)
}

fn render_named_ty_source(ty: &baml_type::RealizedTy<baml_type::TaggedTypeName>) -> String {
    match ty {
        baml_type::RealizedTy::Union(members, _) => {
            let mut non_null: Vec<_> = members.iter().filter(|member| !member.is_null()).collect();
            let has_null = non_null.len() != members.len();
            if has_null && non_null.len() == 1 {
                let member = non_null
                    .pop()
                    .unwrap_or_else(|| unreachable!("length checked"));
                let rendered = render_named_ty_source(member);
                if matches!(member, baml_type::RealizedTy::Function { .. }) {
                    format!("({rendered})?")
                } else {
                    format!("{rendered}?")
                }
            } else {
                let mut rendered: Vec<String> =
                    non_null.into_iter().map(render_named_ty_source).collect();
                if has_null {
                    rendered.push("null".to_string());
                }
                rendered.join(" | ")
            }
        }
        baml_type::RealizedTy::List(element, _) => {
            let rendered = render_named_ty_source(element);
            if matches!(element.as_ref(), baml_type::RealizedTy::Union(..)) {
                format!("({rendered})[]")
            } else {
                format!("{rendered}[]")
            }
        }
        baml_type::RealizedTy::Map { key, value, .. } => {
            format!(
                "map<{}, {}>",
                render_named_ty_source(key),
                render_named_ty_source(value)
            )
        }
        baml_type::RealizedTy::Class(head, args, _) if args.is_empty() => {
            head.display_name().to_string()
        }
        baml_type::RealizedTy::Enum(head, _) => head.display_name().to_string(),
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
fn render_witness_sources(
    vm: &BexVm,
    source: &mut String,
    class_ptr: bex_vm_types::HeapPtr,
    spellings: &indexmap::IndexMap<baml_type::typetag::TypeTag, String>,
) {
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
            .map(|arg| render_ty_source(&arg, spellings))
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
            source.push_str(&render_ty_source(&ty, spellings));
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
    let (mut class_ptrs, mut enum_ptrs) = crate::reachable::all_nominals(vm, &type_value.ty);
    let owners = class_ptrs
        .iter()
        .chain(&enum_ptrs)
        .filter(|ptr| !vm.heap.is_compile_time_ptr(**ptr))
        .filter_map(|ptr| match vm.get_object(*ptr) {
            Object::Class(class) => Some(class.owner),
            Object::Enum(enm) => Some(enm.owner),
            _ => None,
        })
        .filter(|owner| !owner.is_null())
        .collect::<indexmap::IndexSet<_>>();
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
                Object::Class(class) if class.name.item_name().as_str().ends_with("$stream")
            )
        }) {
            if !class_ptrs.contains(&ptr) {
                class_ptrs.push(ptr);
            }
        }
    }

    // Package expansion can add declarations whose fields reach static
    // classes. Close that graph too so every rendered reference has a block.
    let mut class_index = 0;
    while class_index < class_ptrs.len() {
        let ptr = class_ptrs[class_index];
        class_index += 1;
        let Object::Class(class) = vm.get_object(ptr) else {
            continue;
        };
        for field in &class.fields {
            let field_ty = field
                .runtime_type
                .as_ref()
                .map(|runtime| runtime.ty.clone())
                .or_else(|| bex_vm_types::RealizedTy::try_from(&field.field_type).ok());
            let Some(field_ty) = field_ty else {
                continue;
            };
            let (reached_classes, reached_enums) = crate::reachable::all_nominals(vm, &field_ty);
            for reached in reached_classes {
                if !class_ptrs.contains(&reached) {
                    class_ptrs.push(reached);
                }
            }
            for reached in reached_enums {
                if !enum_ptrs.contains(&reached) {
                    enum_ptrs.push(reached);
                }
            }
        }
    }

    let mut spellings = indexmap::IndexMap::new();
    let mut used = std::collections::HashSet::new();
    let mut next_suffix = indexmap::IndexMap::<String, usize>::new();
    for ptr in enum_ptrs.iter().chain(&class_ptrs) {
        let (tag, base) = match vm.get_object(*ptr) {
            Object::Class(class) => (class.type_tag, class.name.display_name().to_string()),
            Object::Enum(enm) => (enm.type_tag, enm.name.display_name().to_string()),
            _ => continue,
        };
        if spellings.contains_key(&tag) {
            continue;
        }
        let next = next_suffix.entry(base.clone()).or_insert(1);
        let mut candidate = if *next == 1 {
            base.clone()
        } else {
            format!("{base}_{next}")
        };
        while used.contains(&candidate) {
            *next += 1;
            candidate = format!("{base}_{next}");
        }
        *next += 1;
        used.insert(candidate.clone());
        spellings.insert(tag, candidate);
    }

    let mut declarations = Vec::new();
    for ptr in &enum_ptrs {
        let Object::Enum(enm) = vm.get_object(*ptr) else {
            continue;
        };
        let name = spellings
            .get(&enm.type_tag)
            .map_or_else(|| enm.name.display_name().to_string(), Clone::clone);
        let mut source = format!("enum {name} {{");
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
        let name = spellings
            .get(&class.type_tag)
            .map_or_else(|| class.name.display_name().to_string(), Clone::clone);
        let mut source = format!("class {name} {{");
        for field in &class.fields {
            source.push_str("\n  ");
            source.push_str(&field.name);
            source.push(' ');
            if let Some(field_type) = &field.runtime_type {
                source.push_str(&render_ty_source(&field_type.ty, &spellings));
            } else if let Ok(field_type) = bex_vm_types::RealizedTy::try_from(&field.field_type) {
                source.push_str(&render_ty_source(&field_type, &spellings));
            } else {
                source.push_str(&field.field_type.to_string());
            }
            source.push_str(&render_meta_suffix(
                field.alias.as_deref(),
                field.description.as_deref(),
            ));
        }
        render_witness_sources(vm, &mut source, *ptr, &spellings);
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
            render_ty_source(&type_value.ty, &spellings)
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

fn as_kind(
    vm: &mut BexVm,
    value: Value,
    expected: baml_type::type_kind::TypeKind,
) -> Option<Value> {
    let ty = type_value_ty(vm, value)?;
    (baml_type::type_kind::classify_type(&ty) == expected)
        .then(|| super::type_kinds::alloc_kind_view(vm, expected, value))
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

#[cfg(test)]
mod renderability_tests {
    use super::*;

    fn attr() -> baml_type::TyAttr {
        baml_type::TyAttr::default()
    }

    #[test]
    fn full_realized_type_family_has_an_explicit_renderability_classification() {
        // Heads never reach the classifier — it matches on variant shape alone —
        // so an unresolved one is the honest stand-in for "some declaration".
        let name = bex_vm_types::TypeHead::of_name(&baml_type::TypeName::local(
            baml_type::Name::new("Example"),
        ));
        let non_data = vec![
            bex_vm_types::RealizedTy::Uint8Array { attr: attr() },
            bex_vm_types::RealizedTy::EnumVariant(name, baml_type::Name::new("VALUE"), attr()),
            bex_vm_types::RealizedTy::Function {
                params: vec![],
                ret: Box::new(bex_vm_types::RealizedTy::int()),
                throws: Box::new(bex_vm_types::RealizedTy::never()),
                attr: attr(),
            },
            bex_vm_types::RealizedTy::Future(
                Box::new(bex_vm_types::RealizedTy::int()),
                Box::new(bex_vm_types::RealizedTy::never()),
                attr(),
            ),
            bex_vm_types::RealizedTy::RustType { attr: attr() },
            bex_vm_types::RealizedTy::Type { attr: attr() },
            bex_vm_types::RealizedTy::Resource { attr: attr() },
            bex_vm_types::RealizedTy::PromptAst { attr: attr() },
            bex_vm_types::RealizedTy::Void { attr: attr() },
            bex_vm_types::RealizedTy::unknown(),
            bex_vm_types::RealizedTy::never(),
        ];
        for ty in non_data {
            assert!(
                is_non_data_render_type(&ty),
                "expected `{ty}` to be rejected before output-format rendering"
            );
        }

        let data = vec![
            bex_vm_types::RealizedTy::int(),
            bex_vm_types::RealizedTy::Bigint { attr: attr() },
            bex_vm_types::RealizedTy::Float { attr: attr() },
            bex_vm_types::RealizedTy::string(),
            bex_vm_types::RealizedTy::Bool { attr: attr() },
            bex_vm_types::RealizedTy::null(),
            bex_vm_types::RealizedTy::Media(baml_type::MediaKind::Image, attr()),
            bex_vm_types::RealizedTy::Literal(
                baml_type::Literal::String("value".to_string()),
                baml_type::Freshness::Regular,
                attr(),
            ),
            bex_vm_types::RealizedTy::Class(name, vec![], attr()),
            bex_vm_types::RealizedTy::Interface(name, vec![], vec![], attr()),
            bex_vm_types::RealizedTy::Enum(name, attr()),
            bex_vm_types::RealizedTy::list(bex_vm_types::RealizedTy::string()),
            bex_vm_types::RealizedTy::Map {
                key: Box::new(bex_vm_types::RealizedTy::string()),
                value: Box::new(bex_vm_types::RealizedTy::int()),
                attr: attr(),
            },
            bex_vm_types::RealizedTy::Union(
                vec![
                    bex_vm_types::RealizedTy::string(),
                    bex_vm_types::RealizedTy::null(),
                ],
                attr(),
            ),
            bex_vm_types::RealizedTy::TypeAlias(name, attr()),
        ];
        for ty in data {
            assert!(
                !is_non_data_render_type(&ty),
                "expected `{ty}` to remain eligible for output-format rendering"
            );
        }
    }
}
