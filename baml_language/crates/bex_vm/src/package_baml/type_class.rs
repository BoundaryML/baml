use bex_vm_types::types::{DynTypeDefs, DynWitnessDef, Object, TypeValue, Value};
use indexmap::IndexMap;

use super::{BamlClassTypeValue, BamlNamespaceType, PackageBamlImpl, resolve};
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
            baml_type::RealizedTy::Class(name, _, _) => name.display_name().to_string(),
            _ => "output".to_string(),
        };
        // Run the bounded specialization analysis first: the subsequent structural
        // interface walk keys every realization and would otherwise follow a
        // non-regular generic transform forever.
        if let Some(message) = first_render_schema_error(vm, &type_value.ty, type_value.defs()) {
            let diagnostic = super::type_kinds::compiler_diagnostic(
                baml_compiler_diagnostics::DiagnosticId::ConflictingTypeDefinitionAtRender,
                message,
            );
            return Err(VmRustFnError::Thrown(
                super::type_kinds::alloc_compilation_error(vm, &[diagnostic]),
            ));
        }
        let mut visited = std::collections::HashSet::new();
        if let Some((field, open_ty)) =
            first_open_interface(vm, &type_value.ty, type_value.defs(), &root, &mut visited)
        {
            let diagnostic =
                baml_compiler_diagnostics::runtime_type::open_interface_at_render(&field, &open_ty);
            return Err(VmRustFnError::Thrown(
                super::type_kinds::alloc_compilation_error(vm, &[diagnostic]),
            ));
        }
        let mut visited = std::collections::HashSet::new();
        if let Some((path, non_data_ty)) =
            first_non_data_type(vm, &type_value.ty, type_value.defs(), &root, &mut visited)
        {
            let diagnostic = if path == root {
                baml_compiler_diagnostics::runtime_type::non_data_type_at_render(&non_data_ty)
            } else {
                baml_compiler_diagnostics::runtime_type::non_data_field_at_render(
                    &path,
                    &non_data_ty,
                )
            };
            return Err(VmRustFnError::Thrown(
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
}

fn realized_class_name(name: &baml_type::TypeName, args: &[baml_type::RealizedTy]) -> String {
    let base = name.display_name();
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

fn rendered_realized_class_name(
    class: &bex_vm_types::Class,
    args: &[baml_type::RealizedTy],
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

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum RenderDefinition {
    Class(baml_type::TypeName),
    Enum(baml_type::TypeName),
    TypeAlias(baml_type::TypeName),
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct RealizedClassIdentity {
    name: baml_type::TypeName,
    args: Vec<baml_type::RealizedTy>,
}

struct RealizedClassFrame {
    identity: RealizedClassIdentity,
    name: baml_type::TypeName,
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
    defs: &'a DynTypeDefs,
    by_display_name: std::collections::HashMap<String, RenderDefinition>,
    visited: std::collections::HashSet<RenderDefinition>,
    class_ancestry: Vec<RealizedClassFrame>,
    recursive_classes: std::collections::HashSet<RealizedClassIdentity>,
    rendered_classes: Vec<RenderedClass>,
}

impl RenderDefinitionValidator<'_> {
    fn visit(
        &mut self,
        ty: &baml_type::RealizedTy,
        origins: &baml_type::template::TyTemplateOrigins,
    ) -> Option<String> {
        match ty {
            baml_type::RealizedTy::Class(name, args, _) => {
                let definition = RenderDefinition::Class(name.clone());
                if let Some(conflict) = self.check_name(&definition) {
                    return Some(conflict);
                }
                let identity = RealizedClassIdentity {
                    name: name.clone(),
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

                let display_name = realized_class_name(name, args);
                for (index, ancestor) in self.class_ancestry.iter().enumerate() {
                    if ancestor.name == *name
                        && origins.class_transform_expands(index, name, ancestor.arity)
                    {
                        return Some(format!(
                            "non-regular recursive generic class `{}` expands from `{}` to `{display_name}` and cannot be rendered as an LLM output schema",
                            name.display_name(),
                            ancestor.display_name,
                        ));
                    }
                }

                let class = find_render_class(self.vm, self.defs, name)?.clone();
                let rendered_name = rendered_realized_class_name(&class, args);
                self.rendered_classes.push(RenderedClass {
                    identity: identity.clone(),
                    display_name: display_name.clone(),
                    rendered_name,
                    definition,
                });

                self.class_ancestry.push(RealizedClassFrame {
                    identity,
                    name: name.clone(),
                    display_name,
                    arity: args.len(),
                });
                for field in &class.fields {
                    if field.skip {
                        continue;
                    }
                    if let Some(runtime) = &field.runtime_type {
                        let runtime_origins = baml_type::template::TyTemplateOrigins::opaque(
                            self.class_ancestry.len(),
                        );
                        if let Some(conflict) = self.visit(&runtime.ty, &runtime_origins) {
                            return Some(conflict);
                        }
                        continue;
                    }
                    let field_ty = field
                        .field_template
                        .substitute(args, self.vm)
                        .ok()
                        .or_else(|| baml_type::RealizedTy::try_from(field.field_type.clone()).ok());
                    if let Some(field_ty) = field_ty {
                        let field_origins =
                            origins.through_field(name, args.len(), &field.field_template);
                        if let Some(conflict) = self.visit(&field_ty, &field_origins) {
                            return Some(conflict);
                        }
                    }
                }
                self.class_ancestry.pop();
                None
            }
            baml_type::RealizedTy::Enum(name, _) => {
                let definition = RenderDefinition::Enum(name.clone());
                if let Some(conflict) = self.check_name(&definition) {
                    return Some(conflict);
                }
                self.visited.insert(definition);
                None
            }
            baml_type::RealizedTy::TypeAlias(name, _) => {
                let definition = RenderDefinition::TypeAlias(name.clone());
                if let Some(conflict) = self.check_name(&definition) {
                    return Some(conflict);
                }
                if !self.visited.insert(definition) {
                    return None;
                }
                self.vm
                    .recursive_type_alias(name)
                    .and_then(|alias| baml_type::RealizedTy::try_from(alias.clone()).ok())
                    .and_then(|alias| {
                        let alias_origins = baml_type::template::TyTemplateOrigins::opaque(
                            self.class_ancestry.len(),
                        );
                        self.visit(&alias, &alias_origins)
                    })
            }
            baml_type::RealizedTy::List(element, _) => {
                let element_origins = origins.list_element();
                self.visit(element, &element_origins)
            }
            baml_type::RealizedTy::Map { key, value, .. } => {
                let key_origins = origins.map_key();
                let value_origins = origins.map_value();
                self.visit(key, &key_origins)
                    .or_else(|| self.visit(value, &value_origins))
            }
            baml_type::RealizedTy::Union(members, _) => {
                members.iter().enumerate().find_map(|(index, member)| {
                    let member_origins = origins.union_member(index);
                    self.visit(member, &member_origins)
                })
            }
            baml_type::RealizedTy::Future(value, error, _) => self
                .visit(value, &origins.future_value())
                .or_else(|| self.visit(error, &origins.future_error())),
            baml_type::RealizedTy::Function {
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
                    && render_definitions_equivalent(
                        self.vm,
                        self.defs,
                        &first.definition,
                        &class.definition,
                    );
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
        let display_name = render_definition_display_name(definition);
        if let Some(previous) = self.by_display_name.get(&display_name) {
            if previous != definition
                && !render_definitions_equivalent(self.vm, self.defs, previous, definition)
            {
                return Some(format!(
                    "type `{display_name}` has non-equivalent definitions in the same LLM render context"
                ));
            }
        } else {
            self.by_display_name
                .insert(display_name, definition.clone());
        }
        None
    }
}

fn first_render_schema_error(
    vm: &BexVm,
    ty: &baml_type::RealizedTy,
    defs: &DynTypeDefs,
) -> Option<String> {
    let mut validator = RenderDefinitionValidator {
        vm,
        defs,
        by_display_name: std::collections::HashMap::new(),
        visited: std::collections::HashSet::new(),
        class_ancestry: Vec::new(),
        recursive_classes: std::collections::HashSet::new(),
        rendered_classes: Vec::new(),
    };
    validator
        .visit(ty, &baml_type::template::TyTemplateOrigins::root())
        .or_else(|| validator.first_recursive_alias_collision())
}

fn render_definition_display_name(definition: &RenderDefinition) -> String {
    match definition {
        RenderDefinition::Class(name)
        | RenderDefinition::Enum(name)
        | RenderDefinition::TypeAlias(name) => name.display_name().to_string(),
    }
}

fn find_render_class<'a>(
    vm: &'a BexVm,
    defs: &DynTypeDefs,
    name: &baml_type::TypeName,
) -> Option<&'a bex_vm_types::Class> {
    let ptr = defs
        .classes
        .get(name)
        .copied()
        .or_else(|| vm.lookup_type(name))?;
    match vm.get_object(ptr) {
        Object::Class(class) => Some(class),
        _ => None,
    }
}

fn find_render_enum<'a>(
    vm: &'a BexVm,
    defs: &DynTypeDefs,
    name: &baml_type::TypeName,
) -> Option<&'a bex_vm_types::Enum> {
    let ptr = defs
        .enums
        .get(name)
        .copied()
        .or_else(|| vm.lookup_type(name))?;
    match vm.get_object(ptr) {
        Object::Enum(enm) => Some(enm),
        _ => None,
    }
}

fn render_definitions_equivalent(
    vm: &BexVm,
    defs: &DynTypeDefs,
    left: &RenderDefinition,
    right: &RenderDefinition,
) -> bool {
    RenderDefinitionEquivalence {
        vm,
        defs,
        left_to_right: std::collections::HashMap::new(),
        right_to_left: std::collections::HashMap::new(),
    }
    .definitions_equivalent(left, right)
}

struct RenderDefinitionEquivalence<'a> {
    vm: &'a BexVm,
    defs: &'a DynTypeDefs,
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
        if render_definition_display_name(left) != render_definition_display_name(right) {
            return false;
        }
        if let Some(mapped) = self.left_to_right.get(left) {
            return mapped == right;
        }
        if let Some(mapped) = self.right_to_left.get(right) {
            return mapped == left;
        }
        self.left_to_right.insert(left.clone(), right.clone());
        self.right_to_left.insert(right.clone(), left.clone());

        match (left, right) {
            (RenderDefinition::Class(left), RenderDefinition::Class(right)) => {
                let (Some(left), Some(right)) = (
                    find_render_class(self.vm, self.defs, left).cloned(),
                    find_render_class(self.vm, self.defs, right).cloned(),
                ) else {
                    return false;
                };
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
                let (Some(left), Some(right)) = (
                    find_render_enum(self.vm, self.defs, left),
                    find_render_enum(self.vm, self.defs, right),
                ) else {
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
                    self.vm.recursive_type_alias(left).cloned(),
                    self.vm.recursive_type_alias(right).cloned(),
                ) else {
                    return false;
                };
                let (Ok(left), Ok(right)) = (
                    baml_type::RealizedTy::try_from(left),
                    baml_type::RealizedTy::try_from(right),
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
        left: &baml_type::RuntimeTy,
        right: &baml_type::RuntimeTy,
    ) -> bool {
        let (Ok(left), Ok(right)) = (
            baml_type::RealizedTy::try_from(left.clone()),
            baml_type::RealizedTy::try_from(right.clone()),
        ) else {
            return left == right;
        };
        self.types_equivalent(&left, &right)
    }

    fn types_equivalent(
        &mut self,
        left: &baml_type::RealizedTy,
        right: &baml_type::RealizedTy,
    ) -> bool {
        use baml_type::RealizedTy;

        match (left, right) {
            (
                RealizedTy::Class(left_name, left_args, left_attr),
                RealizedTy::Class(right_name, right_args, right_attr),
            ) => {
                left_name.display_name() == right_name.display_name()
                    && left_attr == right_attr
                    && self.type_lists_equivalent(left_args, right_args)
                    && self.definitions_equivalent(
                        &RenderDefinition::Class(left_name.clone()),
                        &RenderDefinition::Class(right_name.clone()),
                    )
            }
            (
                RealizedTy::Interface(left_name, left_args, left_assoc, left_attr),
                RealizedTy::Interface(right_name, right_args, right_assoc, right_attr),
            ) => {
                if left_name.display_name() != right_name.display_name()
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
            (RealizedTy::Enum(left_name, left_attr), RealizedTy::Enum(right_name, right_attr)) => {
                left_name.display_name() == right_name.display_name()
                    && left_attr == right_attr
                    && self.definitions_equivalent(
                        &RenderDefinition::Enum(left_name.clone()),
                        &RenderDefinition::Enum(right_name.clone()),
                    )
            }
            (
                RealizedTy::TypeAlias(left_name, left_attr),
                RealizedTy::TypeAlias(right_name, right_attr),
            ) => {
                left_name.display_name() == right_name.display_name()
                    && left_attr == right_attr
                    && self.definitions_equivalent(
                        &RenderDefinition::TypeAlias(left_name.clone()),
                        &RenderDefinition::TypeAlias(right_name.clone()),
                    )
            }
            (
                RealizedTy::EnumVariant(left_name, left_variant, left_attr),
                RealizedTy::EnumVariant(right_name, right_variant, right_attr),
            ) => {
                left_name.display_name() == right_name.display_name()
                    && left_variant == right_variant
                    && left_attr == right_attr
                    && self.definitions_equivalent(
                        &RenderDefinition::Enum(left_name.clone()),
                        &RenderDefinition::Enum(right_name.clone()),
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
        left: &[baml_type::RealizedTy],
        right: &[baml_type::RealizedTy],
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
    ty: &baml_type::RealizedTy,
    defs: &DynTypeDefs,
    path: &str,
    visited: &mut std::collections::HashSet<baml_type::RealizedTy>,
) -> Option<(String, String)> {
    match ty {
        baml_type::RealizedTy::Interface(..) => Some((path.to_string(), ty.to_string())),
        baml_type::RealizedTy::Class(name, args, _) => {
            if !visited.insert(ty.clone()) {
                return None;
            }
            let class_ptr = defs
                .classes
                .get(name)
                .copied()
                .or_else(|| vm.lookup_type(name))?;
            let Object::Class(class) = vm.get_object(class_ptr) else {
                return None;
            };
            for field in class.fields.iter().filter(|field| !field.skip) {
                let child_path = format!("{path}.{}", field.name);
                if let Some(runtime) = &field.runtime_type {
                    if let Some(found) =
                        first_open_interface(vm, &runtime.ty, runtime.defs(), &child_path, visited)
                    {
                        return Some(found);
                    }
                    continue;
                }
                let field_ty = field
                    .field_template
                    .substitute(args, vm)
                    .ok()
                    .or_else(|| baml_type::RealizedTy::try_from(field.field_type.clone()).ok());
                if let Some(field_ty) = field_ty
                    && let Some(found) =
                        first_open_interface(vm, &field_ty, defs, &child_path, visited)
                {
                    return Some(found);
                }
            }
            None
        }
        baml_type::RealizedTy::List(element, _) => {
            first_open_interface(vm, element, defs, path, visited)
        }
        baml_type::RealizedTy::Map { key, value, .. } => {
            first_open_interface(vm, key, defs, path, visited)
                .or_else(|| first_open_interface(vm, value, defs, path, visited))
        }
        baml_type::RealizedTy::Union(members, _) => members
            .iter()
            .find_map(|member| first_open_interface(vm, member, defs, path, visited)),
        baml_type::RealizedTy::Future(value, error, _) => {
            first_open_interface(vm, value, defs, path, visited)
                .or_else(|| first_open_interface(vm, error, defs, path, visited))
        }
        baml_type::RealizedTy::Function {
            params,
            ret,
            throws,
            ..
        } => params
            .iter()
            .find_map(|param| first_open_interface(vm, &param.ty, defs, path, visited))
            .or_else(|| first_open_interface(vm, ret, defs, path, visited))
            .or_else(|| first_open_interface(vm, throws, defs, path, visited)),
        baml_type::RealizedTy::TypeAlias(name, _) => {
            if !visited.insert(ty.clone()) {
                return None;
            }
            vm.recursive_type_alias(name)
                .and_then(|alias| baml_type::RealizedTy::try_from(alias.clone()).ok())
                .and_then(|alias| first_open_interface(vm, &alias, defs, path, visited))
        }
        _ => None,
    }
}

/// Find the first type that has no output-format representation.
///
/// This match is exhaustive over `RealizedTy` so a newly added runtime type
/// must make an explicit renderability decision at the shared LLM boundary.
fn is_non_data_render_type(ty: &baml_type::RealizedTy) -> bool {
    match ty {
        baml_type::RealizedTy::Uint8Array { .. }
        | baml_type::RealizedTy::EnumVariant(..)
        | baml_type::RealizedTy::Function { .. }
        | baml_type::RealizedTy::Future(..)
        | baml_type::RealizedTy::RustType { .. }
        | baml_type::RealizedTy::Type { .. }
        | baml_type::RealizedTy::Resource { .. }
        | baml_type::RealizedTy::PromptAst { .. }
        | baml_type::RealizedTy::Void { .. }
        | baml_type::RealizedTy::BuiltinUnknown { .. }
        | baml_type::RealizedTy::Never { .. } => true,
        baml_type::RealizedTy::Int { .. }
        | baml_type::RealizedTy::Bigint { .. }
        | baml_type::RealizedTy::Float { .. }
        | baml_type::RealizedTy::String { .. }
        | baml_type::RealizedTy::Bool { .. }
        | baml_type::RealizedTy::Null { .. }
        | baml_type::RealizedTy::Media(..)
        | baml_type::RealizedTy::Literal(..)
        | baml_type::RealizedTy::Class(..)
        | baml_type::RealizedTy::Interface(..)
        | baml_type::RealizedTy::Enum(..)
        | baml_type::RealizedTy::List(..)
        | baml_type::RealizedTy::Map { .. }
        | baml_type::RealizedTy::Union(..)
        | baml_type::RealizedTy::TypeAlias(..) => false,
    }
}

fn first_non_data_type(
    vm: &BexVm,
    ty: &baml_type::RealizedTy,
    defs: &DynTypeDefs,
    path: &str,
    visited: &mut std::collections::HashSet<baml_type::RealizedTy>,
) -> Option<(String, String)> {
    if is_non_data_render_type(ty) {
        return Some((path.to_string(), ty.to_string()));
    }

    match ty {
        baml_type::RealizedTy::Class(name, args, _) => {
            if !visited.insert(ty.clone()) {
                return None;
            }
            let class_ptr = defs
                .classes
                .get(name)
                .copied()
                .or_else(|| vm.lookup_type(name))?;
            let Object::Class(class) = vm.get_object(class_ptr) else {
                return None;
            };
            // The output formatter currently walks `ClassField::field_type`,
            // whose class-generic leaves are intentionally unsubstituted.
            // Reject this path before rendering can silently erase the schema;
            // substituting class arguments in `sys_ops` is a separate fix.
            if class.generic_param_count > 0 {
                return Some((path.to_string(), ty.to_string()));
            }
            for field in class.fields.iter().filter(|field| !field.skip) {
                let child_path = format!("{path}.{}", field.name);
                if let Some(runtime) = &field.runtime_type {
                    if let Some(found) =
                        first_non_data_type(vm, &runtime.ty, runtime.defs(), &child_path, visited)
                    {
                        return Some(found);
                    }
                    continue;
                }
                let field_ty = field
                    .field_template
                    .substitute(args, vm)
                    .ok()
                    .or_else(|| baml_type::RealizedTy::try_from(field.field_type.clone()).ok());
                if let Some(field_ty) = field_ty
                    && let Some(found) =
                        first_non_data_type(vm, &field_ty, defs, &child_path, visited)
                {
                    return Some(found);
                }
            }
            None
        }
        baml_type::RealizedTy::List(element, _) => {
            first_non_data_type(vm, element, defs, path, visited)
        }
        baml_type::RealizedTy::Map { key, value, .. } => {
            first_non_data_type(vm, key, defs, path, visited)
                .or_else(|| first_non_data_type(vm, value, defs, path, visited))
        }
        baml_type::RealizedTy::Union(members, _) => members
            .iter()
            .find_map(|member| first_non_data_type(vm, member, defs, path, visited)),
        baml_type::RealizedTy::TypeAlias(name, _) => {
            if !visited.insert(ty.clone()) {
                return None;
            }
            vm.recursive_type_alias(name)
                .and_then(|alias| baml_type::RealizedTy::try_from(alias.clone()).ok())
                .and_then(|alias| first_non_data_type(vm, &alias, defs, path, visited))
        }
        baml_type::RealizedTy::Int { .. }
        | baml_type::RealizedTy::Bigint { .. }
        | baml_type::RealizedTy::Float { .. }
        | baml_type::RealizedTy::String { .. }
        | baml_type::RealizedTy::Bool { .. }
        | baml_type::RealizedTy::Null { .. }
        | baml_type::RealizedTy::Uint8Array { .. }
        | baml_type::RealizedTy::Media(..)
        | baml_type::RealizedTy::Literal(..)
        | baml_type::RealizedTy::Interface(..)
        | baml_type::RealizedTy::Enum(..)
        | baml_type::RealizedTy::EnumVariant(..)
        | baml_type::RealizedTy::Function { .. }
        | baml_type::RealizedTy::Future(..)
        | baml_type::RealizedTy::RustType { .. }
        | baml_type::RealizedTy::Type { .. }
        | baml_type::RealizedTy::Resource { .. }
        | baml_type::RealizedTy::PromptAst { .. }
        | baml_type::RealizedTy::Void { .. }
        | baml_type::RealizedTy::BuiltinUnknown { .. }
        | baml_type::RealizedTy::Never { .. } => None,
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

fn render_witness_interface(witness: &DynWitnessDef) -> String {
    let mut source = witness.interface.render_user_facing();
    if witness.interface_args.is_empty() {
        return source;
    }

    source.push('<');
    for (index, argument) in witness.interface_args.iter().enumerate() {
        if index > 0 {
            source.push_str(", ");
        }
        source.push_str(&render_ty_source(argument));
    }
    source.push('>');
    source
}

fn render_witness_source(source: &mut String, witness: &DynWitnessDef) {
    source.push_str("\n  implements ");
    source.push_str(&render_witness_interface(witness));
    if witness.associated_types.is_empty() && witness.field_links.is_empty() {
        source.push_str(" {}");
        return;
    }

    source.push_str(" {");
    for (name, ty) in &witness.associated_types {
        source.push_str("\n    type ");
        source.push_str(name.as_str());
        source.push_str(" = ");
        source.push_str(&render_ty_source(ty));
    }
    for (interface_field, class_field) in &witness.field_links {
        source.push_str("\n    ");
        source.push_str(interface_field.as_str());
        source.push_str(" as ");
        source.push_str(class_field.as_str());
    }
    source.push_str("\n  }");
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
        if let Some(runtime_type) = &class.runtime_type {
            for witness in &runtime_type.defs.witnesses {
                render_witness_source(&mut source, witness);
            }
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

#[cfg(test)]
mod renderability_tests {
    use super::*;

    fn attr() -> baml_type::TyAttr {
        baml_type::TyAttr::default()
    }

    #[test]
    fn full_realized_type_family_has_an_explicit_renderability_classification() {
        let name = baml_type::TypeName::local(baml_type::Name::new("Example"));
        let non_data = vec![
            baml_type::RealizedTy::Uint8Array { attr: attr() },
            baml_type::RealizedTy::EnumVariant(name.clone(), baml_type::Name::new("VALUE"), attr()),
            baml_type::RealizedTy::Function {
                params: vec![],
                ret: Box::new(baml_type::RealizedTy::int()),
                throws: Box::new(baml_type::RealizedTy::never()),
                attr: attr(),
            },
            baml_type::RealizedTy::Future(
                Box::new(baml_type::RealizedTy::int()),
                Box::new(baml_type::RealizedTy::never()),
                attr(),
            ),
            baml_type::RealizedTy::RustType { attr: attr() },
            baml_type::RealizedTy::Type { attr: attr() },
            baml_type::RealizedTy::Resource { attr: attr() },
            baml_type::RealizedTy::PromptAst { attr: attr() },
            baml_type::RealizedTy::Void { attr: attr() },
            baml_type::RealizedTy::unknown(),
            baml_type::RealizedTy::never(),
        ];
        for ty in non_data {
            assert!(
                is_non_data_render_type(&ty),
                "expected `{ty}` to be rejected before output-format rendering"
            );
        }

        let data = vec![
            baml_type::RealizedTy::int(),
            baml_type::RealizedTy::Bigint { attr: attr() },
            baml_type::RealizedTy::Float { attr: attr() },
            baml_type::RealizedTy::string(),
            baml_type::RealizedTy::Bool { attr: attr() },
            baml_type::RealizedTy::null(),
            baml_type::RealizedTy::Media(baml_type::MediaKind::Image, attr()),
            baml_type::RealizedTy::Literal(
                baml_type::Literal::String("value".to_string()),
                baml_type::Freshness::Regular,
                attr(),
            ),
            baml_type::RealizedTy::Class(name.clone(), vec![], attr()),
            baml_type::RealizedTy::Interface(name.clone(), vec![], vec![], attr()),
            baml_type::RealizedTy::Enum(name.clone(), attr()),
            baml_type::RealizedTy::list(baml_type::RealizedTy::string()),
            baml_type::RealizedTy::Map {
                key: Box::new(baml_type::RealizedTy::string()),
                value: Box::new(baml_type::RealizedTy::int()),
                attr: attr(),
            },
            baml_type::RealizedTy::Union(
                vec![
                    baml_type::RealizedTy::string(),
                    baml_type::RealizedTy::null(),
                ],
                attr(),
            ),
            baml_type::RealizedTy::TypeAlias(name, attr()),
        ];
        for ty in data {
            assert!(
                !is_non_data_render_type(&ty),
                "expected `{ty}` to remain eligible for output-format rendering"
            );
        }
    }
}
