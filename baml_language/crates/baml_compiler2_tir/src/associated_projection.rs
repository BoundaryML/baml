//! Resolution helpers for associated type projections.
//!
//! The type checker and runtime metadata lowering both need the same answer for
//! types like `(AccountRecord as PublicIdentity).Key` or `Self.Record`.
//! Keeping that logic here prevents a projection from type-checking correctly
//! while later losing its value-producing metadata at the VM boundary.

use std::collections::HashMap;

use baml_base::Name;
use baml_compiler2_hir::{
    contributions::Definition,
    package::{PackageId, PackageItems},
};
use rustc_hash::{FxHashMap, FxHashSet};

use crate::{
    package_interface::PackageResolutionContext,
    ty::{FunctionParamTy, QualifiedTypeName, Ty},
};

/// Lookup interface for generic parameter bounds visible to a projection.
pub trait TypeVarBounds {
    fn get_bound(&self, name: &Name) -> Option<&Ty>;
}

impl TypeVarBounds for HashMap<Name, Ty> {
    fn get_bound(&self, name: &Name) -> Option<&Ty> {
        self.get(name)
    }
}

impl TypeVarBounds for FxHashMap<Name, Ty> {
    fn get_bound(&self, name: &Name) -> Option<&Ty> {
        self.get(name)
    }
}

impl TypeVarBounds for () {
    fn get_bound(&self, _name: &Name) -> Option<&Ty> {
        None
    }
}

/// Resolves associated type projections using package item trees, aliases, and
/// generic bounds from the caller's scope.
pub struct AssociatedProjectionResolver<'a, 'db, B: TypeVarBounds + ?Sized> {
    db: &'db dyn crate::Db,
    res_ctx: Option<&'db PackageResolutionContext<'db>>,
    aliases: &'a crate::normalize::ResolvedAliases,
    generic_param_bounds: &'a B,
}

impl<'a, 'db, B: TypeVarBounds + ?Sized> AssociatedProjectionResolver<'a, 'db, B> {
    /// Create a resolver that can load package items directly by package name.
    ///
    /// This is suitable for whole-program lowering paths like VM metadata
    /// emission where the package access check has already happened upstream.
    pub fn new(
        db: &'db dyn crate::Db,
        aliases: &'a crate::normalize::ResolvedAliases,
        generic_param_bounds: &'a B,
    ) -> Self {
        Self {
            db,
            res_ctx: None,
            aliases,
            generic_param_bounds,
        }
    }

    /// Create a resolver that respects the package resolution context of a
    /// type-checking scope.
    pub fn with_resolution_context(
        db: &'db dyn crate::Db,
        res_ctx: &'db PackageResolutionContext<'db>,
        aliases: &'a crate::normalize::ResolvedAliases,
        generic_param_bounds: &'a B,
    ) -> Self {
        Self {
            db,
            res_ctx: Some(res_ctx),
            aliases,
            generic_param_bounds,
        }
    }

    /// Resolve every associated projection contained in `ty` that can be
    /// concretized from the available package and generic-bound context.
    pub fn resolve_deep(&self, ty: &Ty) -> Ty {
        self.resolve_deep_inner(ty, &mut FxHashSet::default())
    }

    fn resolve_deep_inner(&self, ty: &Ty, resolving: &mut FxHashSet<Ty>) -> Ty {
        match ty {
            Ty::AssociatedTypeProjection {
                base,
                interface,
                member,
                attr,
            } => {
                let projected = Ty::AssociatedTypeProjection {
                    base: Box::new(self.resolve_deep_inner(base, resolving)),
                    interface: interface.as_ref().map(|interface| {
                        Box::new(interface.map_tys(|t| self.resolve_deep_inner(t, resolving)))
                    }),
                    member: member.clone(),
                    attr: attr.clone(),
                };
                if !resolving.insert(projected.clone()) {
                    return projected;
                }
                let resolved = self
                    .resolve_projection(&projected)
                    .map(|resolved| {
                        if resolved == projected {
                            projected.clone()
                        } else {
                            self.resolve_deep_inner(&resolved, resolving)
                        }
                    })
                    .unwrap_or_else(|| projected.clone());
                resolving.remove(&projected);
                resolved
            }
            Ty::Class(qtn, args, attr) => Ty::Class(
                qtn.clone(),
                args.iter()
                    .map(|arg| self.resolve_deep_inner(arg, resolving))
                    .collect(),
                attr.clone(),
            ),
            Ty::Interface(qtn, args, associated_bindings, attr) => Ty::Interface(
                qtn.clone(),
                args.iter()
                    .map(|arg| self.resolve_deep_inner(arg, resolving))
                    .collect(),
                associated_bindings
                    .iter()
                    .map(|(name, ty)| (name.clone(), self.resolve_deep_inner(ty, resolving)))
                    .collect(),
                attr.clone(),
            ),
            Ty::List(inner, attr) => Ty::List(
                Box::new(self.resolve_deep_inner(inner, resolving)),
                attr.clone(),
            ),
            Ty::EvolvingList(inner, attr) => Ty::EvolvingList(
                Box::new(self.resolve_deep_inner(inner, resolving)),
                attr.clone(),
            ),
            Ty::Map { key, value, attr } => Ty::Map {
                key: Box::new(self.resolve_deep_inner(key, resolving)),
                value: Box::new(self.resolve_deep_inner(value, resolving)),
                attr: attr.clone(),
            },
            Ty::EvolvingMap(key, value, attr) => Ty::EvolvingMap(
                Box::new(self.resolve_deep_inner(key, resolving)),
                Box::new(self.resolve_deep_inner(value, resolving)),
                attr.clone(),
            ),
            Ty::Union(members, attr) => Ty::Union(
                members
                    .iter()
                    .map(|member| self.resolve_deep_inner(member, resolving))
                    .collect(),
                attr.clone(),
            ),
            Ty::Future(value, error, attr) => Ty::Future(
                Box::new(self.resolve_deep_inner(value, resolving)),
                Box::new(self.resolve_deep_inner(error, resolving)),
                attr.clone(),
            ),
            Ty::Function {
                params,
                ret,
                throws,
                attr,
            } => Ty::Function {
                params: params
                    .iter()
                    .map(|param| FunctionParamTy {
                        name: param.name.clone(),
                        ty: self.resolve_deep_inner(&param.ty, resolving),
                        mode: param.mode,
                    })
                    .collect(),
                ret: Box::new(self.resolve_deep_inner(ret, resolving)),
                throws: Box::new(self.resolve_deep_inner(throws, resolving)),
                attr: attr.clone(),
            },
            _ => ty.clone(),
        }
    }

    pub fn types_equivalent(&self, a: &Ty, b: &Ty) -> bool {
        self.projection_views_equivalent(a, b)
            || crate::normalize::is_same_normalized_type(a, b, self.aliases)
    }

    /// Resolve the declared `extends` bound for an associated projection.
    ///
    /// This is intentionally separate from [`Self::resolve_deep`]: when
    /// `H.Item` cannot be concretized because `H` is still generic, the
    /// projection may still carry a useful interface contract such as
    /// `type Item extends Summarizable`. Member lookup can use that bound while
    /// preserving the abstract projection as the value's actual type.
    pub fn resolve_projection_bound(&self, ty: &Ty) -> Option<Ty> {
        let Ty::AssociatedTypeProjection {
            base,
            interface,
            member,
            ..
        } = ty
        else {
            return None;
        };
        let expanded_base = self
            .type_with_default_associated_bindings(self.expand_alias_chains(base.as_ref().clone()));
        match &expanded_base {
            Ty::Class(class_qtn, class_args, _) => self.resolve_class_projection_bound(
                class_qtn,
                class_args,
                interface.as_deref(),
                member,
            ),
            Ty::Interface(iface_qtn, iface_args, associated_bindings, _) => self
                .resolve_interface_projection_bound(
                    iface_qtn,
                    iface_args,
                    associated_bindings,
                    interface.as_deref(),
                    member,
                ),
            Ty::TypeVar(name, _) => self.generic_param_bounds.get_bound(name).and_then(|bound| {
                let bound_projection = Ty::AssociatedTypeProjection {
                    base: Box::new(bound.clone()),
                    interface: interface.clone(),
                    member: member.clone(),
                    attr: ty.attr().clone(),
                };
                self.resolve_projection_bound(&bound_projection)
            }),
            _ => None,
        }
    }

    pub fn projection_views_equivalent(&self, a: &Ty, b: &Ty) -> bool {
        if let (Some(a_inner), Some(b_inner)) = (
            Self::nullable_without_null(a),
            Self::nullable_without_null(b),
        ) {
            return self.projection_views_equivalent(&a_inner, &b_inner);
        }

        match (a, b) {
            (
                Ty::AssociatedTypeProjection {
                    base: a_base,
                    interface: a_interface,
                    member: a_member,
                    ..
                },
                Ty::AssociatedTypeProjection {
                    base: b_base,
                    interface: b_interface,
                    member: b_member,
                    ..
                },
            ) if a_member == b_member
                && self.types_equivalent_without_projection_views(a_base, b_base) =>
            {
                self.projection_interfaces_equivalent(
                    a_base,
                    a_member,
                    a_interface.as_deref(),
                    b_interface.as_deref(),
                )
            }
            _ => false,
        }
    }

    fn nullable_without_null(ty: &Ty) -> Option<Ty> {
        let Ty::Union(members, _) = ty else {
            return None;
        };
        if !members.iter().any(Ty::is_null) {
            return None;
        }
        let non_null: Vec<Ty> = members
            .iter()
            .filter(|member| !member.is_null())
            .cloned()
            .collect();
        match non_null.as_slice() {
            [] => None,
            [single] => Some(single.clone()),
            _ => Some(Ty::Union(non_null, ty.attr().clone())),
        }
    }

    fn resolve_projection(&self, ty: &Ty) -> Option<Ty> {
        let Ty::AssociatedTypeProjection {
            base,
            interface,
            member,
            ..
        } = ty
        else {
            return None;
        };
        let expanded_base = self
            .type_with_default_associated_bindings(self.expand_alias_chains(base.as_ref().clone()));
        match &expanded_base {
            Ty::Class(class_qtn, class_args, _) => {
                self.resolve_class_projection(class_qtn, class_args, interface.as_deref(), member)
            }
            Ty::Interface(iface_qtn, iface_args, associated_bindings, _) => self
                .resolve_interface_projection(
                    iface_qtn,
                    iface_args,
                    associated_bindings,
                    interface.as_deref(),
                    member,
                ),
            Ty::TypeVar(name, _) => self.generic_param_bounds.get_bound(name).and_then(|bound| {
                let bound_projection = Ty::AssociatedTypeProjection {
                    base: Box::new(bound.clone()),
                    interface: interface.clone(),
                    member: member.clone(),
                    attr: ty.attr().clone(),
                };
                self.resolve_projection(&bound_projection)
            }),
            // Primitive / non-class concrete base types (`int`, `string`, …)
            // can carry out-of-body `implements I for int` blocks, so a
            // projection like `int.CompareError` must resolve through those rules.
            // Classes have an in-body path above; this mirrors it for the
            // builtin types stdlib interfaces are implemented on.
            other if crate::interfaces::implementation_key_for_ty(other).is_some() => {
                self.resolve_primitive_projection(other, interface.as_deref(), member)
            }
            _ => None,
        }
    }

    /// Resolve a projection whose base is a primitive / non-class concrete
    /// type by consulting the out-of-body `implements` rules visible to the
    /// resolver.
    fn resolve_primitive_projection(
        &self,
        base_ty: &Ty,
        projected_interface: Option<&baml_type::Interface>,
        member: &Name,
    ) -> Option<Ty> {
        let candidate_ty = crate::interfaces::implementation_key_for_ty(base_ty)
            .unwrap_or_else(|| base_ty.clone());
        let mut matches = Vec::new();
        for pkg_name in self.candidate_registry_packages() {
            let registry = crate::interfaces::package_implements_registry(
                self.db,
                PackageId::new(self.db, pkg_name),
            );
            self.collect_out_of_body_matches_in_registry(
                registry,
                &candidate_ty,
                projected_interface,
                member,
                &mut matches,
            );
        }
        let mut unique = Vec::new();
        for ty in matches {
            if !unique.contains(&ty) {
                unique.push(ty);
            }
        }
        if unique.len() == 1 {
            unique.pop()
        } else {
            None
        }
    }

    /// Packages whose `implements` registries are visible to this resolver.
    ///
    /// Out-of-body impls on primitives live in the package that owns the
    /// `implements … for int` block — which (per the orphan rule) is either
    /// the user's own package or the interface's package. With a resolution
    /// context we know those directly; without one (VM metadata / MIR
    /// lowering) we fall back to every package in the program.
    fn candidate_registry_packages(&self) -> Vec<Name> {
        if let Some(res_ctx) = self.res_ctx {
            let mut pkgs = vec![res_ctx.own_package_name.clone()];
            for (name, _) in &res_ctx.dep_interfaces {
                if !pkgs.contains(name) {
                    pkgs.push(name.clone());
                }
            }
            pkgs
        } else {
            let mut pkgs = Vec::new();
            for file in baml_compiler2_hir::compiler2_all_files(self.db) {
                let pkg = baml_compiler2_hir::file_package::file_package(self.db, file).package;
                if !pkgs.contains(&pkg) {
                    pkgs.push(pkg);
                }
            }
            pkgs
        }
    }

    fn resolve_interface_projection(
        &self,
        iface_qtn: &QualifiedTypeName,
        iface_args: &[Ty],
        associated_bindings: &[(Name, Ty)],
        projected_interface: Option<&baml_type::Interface>,
        member: &Name,
    ) -> Option<Ty> {
        let pkg_items = self.package_items_for(iface_qtn.package())?;
        let Some(Definition::Interface(iface_loc)) =
            pkg_items.lookup_type(iface_qtn.namespace(), iface_qtn.name())
        else {
            return None;
        };
        let pkg = baml_compiler2_hir::file_package::file_package(self.db, iface_loc.file(self.db));
        let mut matches = Vec::new();
        for (current_loc, current_args, current_assoc) in
            crate::interfaces::interface_closure_locs_with_args_and_assoc(
                self.db,
                iface_loc,
                iface_args,
                associated_bindings,
                pkg_items,
                &pkg.namespace_path,
            )
        {
            let current_tree =
                baml_compiler2_hir::file_item_tree(self.db, current_loc.file(self.db));
            let Some(current_iface) = current_tree.interfaces.get(&current_loc.id(self.db)) else {
                continue;
            };
            if !current_iface
                .associated_types
                .iter()
                .any(|assoc| assoc.name == *member)
            {
                continue;
            }
            let current_qtn = crate::lower_type_expr::qualify_def(
                self.db,
                Definition::Interface(current_loc),
                &current_iface.name,
            );
            if !self.projection_interface_view_matches(
                &current_qtn,
                &current_args,
                &current_assoc,
                projected_interface,
            ) {
                continue;
            }
            if let Some((_, ty)) = current_assoc.iter().find(|(name, _)| name == member) {
                matches.push(ty.clone());
            }
        }
        if matches.len() == 1 {
            matches.into_iter().next()
        } else {
            None
        }
    }

    fn resolve_interface_projection_bound(
        &self,
        iface_qtn: &QualifiedTypeName,
        iface_args: &[Ty],
        associated_bindings: &[(Name, Ty)],
        projected_interface: Option<&baml_type::Interface>,
        member: &Name,
    ) -> Option<Ty> {
        let pkg_items = self.package_items_for(iface_qtn.package())?;
        let Some(Definition::Interface(iface_loc)) =
            pkg_items.lookup_type(iface_qtn.namespace(), iface_qtn.name())
        else {
            return None;
        };
        let pkg = baml_compiler2_hir::file_package::file_package(self.db, iface_loc.file(self.db));
        let mut matches = Vec::new();
        for (current_loc, current_args, current_assoc) in
            crate::interfaces::interface_closure_locs_with_args_and_assoc(
                self.db,
                iface_loc,
                iface_args,
                associated_bindings,
                pkg_items,
                &pkg.namespace_path,
            )
        {
            let current_tree =
                baml_compiler2_hir::file_item_tree(self.db, current_loc.file(self.db));
            let Some(current_iface) = current_tree.interfaces.get(&current_loc.id(self.db)) else {
                continue;
            };
            let Some(assoc) = current_iface
                .associated_types
                .iter()
                .find(|assoc| assoc.name == *member)
            else {
                continue;
            };
            let current_qtn = crate::lower_type_expr::qualify_def(
                self.db,
                Definition::Interface(current_loc),
                &current_iface.name,
            );
            if !self.projection_interface_view_matches(
                &current_qtn,
                &current_args,
                &current_assoc,
                projected_interface,
            ) {
                continue;
            }
            let Some(bound) = &assoc.bound else {
                continue;
            };
            let current_pkg =
                baml_compiler2_hir::file_package::file_package(self.db, current_loc.file(self.db));
            let current_pkg_items = self
                .package_items_for(current_qtn.package())
                .unwrap_or(pkg_items);
            let mut bindings =
                crate::generics::bind_type_vars(&current_iface.generic_params, &current_args);
            for generic_param in &current_iface.generic_params {
                bindings.entry(generic_param.clone()).or_insert_with(|| {
                    Ty::TypeVar(generic_param.clone(), crate::ty::TyAttr::default())
                });
            }
            for (name, ty) in &current_assoc {
                bindings.insert(name.clone(), ty.clone());
            }
            let mut diags = Vec::new();
            let bound_ty = crate::generics::lower_type_expr_with_generics(
                self.db,
                bound,
                current_pkg_items,
                &current_pkg.namespace_path,
                &bindings,
                &mut diags,
            );
            if diags.is_empty() {
                matches.push(bound_ty);
            }
        }
        let mut unique = Vec::new();
        for ty in matches {
            if !unique.contains(&ty) {
                unique.push(ty);
            }
        }
        if unique.len() == 1 {
            unique.pop()
        } else {
            None
        }
    }

    fn resolve_class_projection(
        &self,
        class_qtn: &QualifiedTypeName,
        class_args: &[Ty],
        projected_interface: Option<&baml_type::Interface>,
        member: &Name,
    ) -> Option<Ty> {
        let pkg_items = self.package_items_for(class_qtn.package())?;
        let Some(Definition::Class(class_loc)) =
            pkg_items.lookup_type(class_qtn.namespace(), class_qtn.name())
        else {
            return None;
        };
        let item_tree = baml_compiler2_ppir::file_item_tree(self.db, class_loc.file(self.db));
        let class_data = &item_tree[class_loc.id(self.db)];
        let class_pkg =
            baml_compiler2_hir::file_package::file_package(self.db, class_loc.file(self.db));
        let class_ns = class_pkg.namespace_path;
        let class_bindings =
            crate::generics::bind_type_vars(&class_data.generic_params, class_args);
        let mut matches = Vec::new();

        'impls: for impl_target in &class_data.implements {
            let Some(iface_loc) = crate::interfaces::resolve_path_to_interface(
                self.db,
                &impl_target.target,
                pkg_items,
                &class_ns,
            ) else {
                continue;
            };
            let iface_tree = baml_compiler2_hir::file_item_tree(self.db, iface_loc.file(self.db));
            let Some(iface_data) = iface_tree.interfaces.get(&iface_loc.id(self.db)) else {
                continue;
            };
            if !iface_data
                .associated_types
                .iter()
                .any(|assoc| assoc.name == *member)
            {
                continue;
            }
            let iface_qtn = crate::lower_type_expr::qualify_def(
                self.db,
                Definition::Interface(iface_loc),
                &iface_data.name,
            );
            let mut diags = Vec::new();
            let lowered_iface = crate::lower_type_expr::lower_type_expr_in_ns(
                self.db,
                &impl_target.target,
                pkg_items,
                &class_ns,
                &class_data.generic_params,
                &mut diags,
            );
            let Ty::Interface(_, raw_iface_args, _, _) = lowered_iface else {
                continue;
            };
            let iface_args: Vec<Ty> = raw_iface_args
                .iter()
                .map(|arg| crate::generics::substitute_ty(arg, &class_bindings))
                .collect();
            if !self.projection_interface_matches(&iface_qtn, &iface_args, projected_interface) {
                continue;
            }
            let mut iface_bindings =
                crate::generics::bind_type_vars(&iface_data.generic_params, &iface_args);
            for (name, ty) in &class_bindings {
                iface_bindings
                    .entry(name.clone())
                    .or_insert_with(|| ty.clone());
            }
            let iface_pkg =
                baml_compiler2_hir::file_package::file_package(self.db, iface_loc.file(self.db));
            let associated_bindings: Vec<(Name, Ty)> = iface_data
                .associated_types
                .iter()
                .filter_map(|assoc| {
                    if let Some(binding) = impl_target
                        .associated_type_bindings
                        .iter()
                        .find(|binding| binding.name == assoc.name)
                        && let Some(te) = &binding.type_expr
                    {
                        let mut binding_diags = Vec::new();
                        return Some((
                            assoc.name.clone(),
                            crate::generics::lower_type_expr_with_generics(
                                self.db,
                                te,
                                pkg_items,
                                &class_ns,
                                &iface_bindings,
                                &mut binding_diags,
                            ),
                        ));
                    }
                    assoc.default.as_ref().map(|default| {
                        let mut default_diags = Vec::new();
                        (
                            assoc.name.clone(),
                            crate::generics::lower_type_expr_with_generics(
                                self.db,
                                default,
                                pkg_items,
                                &iface_pkg.namespace_path,
                                &iface_bindings,
                                &mut default_diags,
                            ),
                        )
                    })
                })
                .collect();
            if let Some(projected) = projected_interface {
                for (projected_name, projected_ty) in &projected.associated_types {
                    let Some((_, actual_ty)) = associated_bindings
                        .iter()
                        .find(|(actual_name, _)| actual_name == projected_name)
                    else {
                        continue 'impls;
                    };
                    if !self.types_equivalent(actual_ty, projected_ty) {
                        continue 'impls;
                    }
                }
            }
            if let Some((_, ty)) = associated_bindings.iter().find(|(name, _)| name == member) {
                matches.push(ty.clone());
            }
        }

        self.collect_out_of_body_class_projection_matches(
            class_qtn,
            class_args,
            projected_interface,
            member,
            &mut matches,
        );

        let mut unique = Vec::new();
        for ty in matches {
            if !unique.contains(&ty) {
                unique.push(ty);
            }
        }

        if unique.len() == 1 {
            unique.pop()
        } else {
            None
        }
    }

    fn resolve_class_projection_bound(
        &self,
        class_qtn: &QualifiedTypeName,
        class_args: &[Ty],
        projected_interface: Option<&baml_type::Interface>,
        member: &Name,
    ) -> Option<Ty> {
        let pkg_items = self.package_items_for(class_qtn.package())?;
        let Some(Definition::Class(class_loc)) =
            pkg_items.lookup_type(class_qtn.namespace(), class_qtn.name())
        else {
            return None;
        };
        let item_tree = baml_compiler2_ppir::file_item_tree(self.db, class_loc.file(self.db));
        let class_data = &item_tree[class_loc.id(self.db)];
        let class_pkg =
            baml_compiler2_hir::file_package::file_package(self.db, class_loc.file(self.db));
        let class_bindings =
            crate::generics::bind_type_vars(&class_data.generic_params, class_args);
        let mut matches = Vec::new();

        for impl_target in &class_data.implements {
            let Some(iface_loc) = crate::interfaces::resolve_path_to_interface(
                self.db,
                &impl_target.target,
                pkg_items,
                &class_pkg.namespace_path,
            ) else {
                continue;
            };
            let iface_tree = baml_compiler2_hir::file_item_tree(self.db, iface_loc.file(self.db));
            let Some(iface_data) = iface_tree.interfaces.get(&iface_loc.id(self.db)) else {
                continue;
            };
            let Some(assoc) = iface_data
                .associated_types
                .iter()
                .find(|assoc| assoc.name == *member)
            else {
                continue;
            };
            let iface_qtn = crate::lower_type_expr::qualify_def(
                self.db,
                Definition::Interface(iface_loc),
                &iface_data.name,
            );
            let mut diags = Vec::new();
            let lowered_iface = crate::lower_type_expr::lower_type_expr_in_ns(
                self.db,
                &impl_target.target,
                pkg_items,
                &class_pkg.namespace_path,
                &class_data.generic_params,
                &mut diags,
            );
            let Ty::Interface(_, raw_iface_args, _, _) = lowered_iface else {
                continue;
            };
            let iface_args: Vec<Ty> = raw_iface_args
                .iter()
                .map(|arg| crate::generics::substitute_ty(arg, &class_bindings))
                .collect();
            if !self.projection_interface_matches(&iface_qtn, &iface_args, projected_interface) {
                continue;
            }
            let Some(bound) = &assoc.bound else {
                continue;
            };
            let iface_pkg =
                baml_compiler2_hir::file_package::file_package(self.db, iface_loc.file(self.db));
            let mut bindings =
                crate::generics::bind_type_vars(&iface_data.generic_params, &iface_args);
            for (name, ty) in &class_bindings {
                bindings.entry(name.clone()).or_insert_with(|| ty.clone());
            }
            let mut bound_diags = Vec::new();
            let bound_ty = crate::generics::lower_type_expr_with_generics(
                self.db,
                bound,
                pkg_items,
                &iface_pkg.namespace_path,
                &bindings,
                &mut bound_diags,
            );
            if bound_diags.is_empty() {
                matches.push(bound_ty);
            }
        }

        let mut unique = Vec::new();
        for ty in matches {
            if !unique.contains(&ty) {
                unique.push(ty);
            }
        }
        if unique.len() == 1 {
            unique.pop()
        } else {
            None
        }
    }

    fn collect_out_of_body_class_projection_matches(
        &self,
        class_qtn: &QualifiedTypeName,
        class_args: &[Ty],
        projected_interface: Option<&baml_type::Interface>,
        member: &Name,
        matches: &mut Vec<Ty>,
    ) {
        let registry = crate::interfaces::package_implements_registry(
            self.db,
            PackageId::new(self.db, class_qtn.package().clone()),
        );
        let candidate_ty = Ty::Class(
            class_qtn.clone(),
            class_args.to_vec(),
            crate::ty::TyAttr::default(),
        );
        self.collect_out_of_body_matches_in_registry(
            registry,
            &candidate_ty,
            projected_interface,
            member,
            matches,
        );
    }

    /// Scan one package's out-of-body `implements` rules for a rule whose
    /// `for` target matches `candidate_ty` and whose interface declares
    /// `member`, pushing the projected member type into `matches`.
    fn collect_out_of_body_matches_in_registry(
        &self,
        registry: &crate::interfaces::ImplementsRegistry,
        candidate_ty: &Ty,
        projected_interface: Option<&baml_type::Interface>,
        member: &Name,
        matches: &mut Vec<Ty>,
    ) {
        for rule in &registry.interface_impl_rules {
            if !matches!(
                rule.origin,
                crate::interfaces::InterfaceImplOrigin::OutOfBody
            ) {
                continue;
            }

            let requested_iface_ty = projected_interface
                .map(baml_type::Interface::to_ty)
                .unwrap_or_else(|| rule.interface_ty.clone());
            let Some(instantiation) = registry.instantiate_rule_for_requested_interface(
                rule,
                &requested_iface_ty,
                Some(candidate_ty),
                self.aliases,
                |actual, bound| self.type_satisfies_bound(actual, bound),
            ) else {
                continue;
            };

            let Ty::Interface(iface_qtn, iface_args, associated_bindings, _) =
                &instantiation.interface_ty
            else {
                continue;
            };
            if let Some(ty) = self.resolve_interface_projection(
                iface_qtn,
                iface_args,
                associated_bindings,
                projected_interface,
                member,
            ) {
                matches.push(ty);
            }
        }
    }

    fn projection_interface_matches(
        &self,
        iface_qtn: &QualifiedTypeName,
        iface_args: &[Ty],
        projected_interface: Option<&baml_type::Interface>,
    ) -> bool {
        let Some(projected_interface) = projected_interface else {
            return true;
        };
        iface_qtn == &projected_interface.name
            && iface_args.len() == projected_interface.generics.len()
            && iface_args
                .iter()
                .zip(projected_interface.generics.iter())
                .all(|(a, b)| self.types_equivalent(a, b))
    }

    fn type_satisfies_bound(&self, actual: &Ty, bound: &Ty) -> bool {
        self.types_equivalent(actual, bound)
            || match actual {
                Ty::TypeVar(name, _) => self
                    .generic_param_bounds
                    .get_bound(name)
                    .is_some_and(|actual_bound| self.type_satisfies_bound(actual_bound, bound)),
                _ => {
                    let Ty::Interface(iface_qtn, iface_args, associated_bindings, _) = bound else {
                        return false;
                    };
                    let registry_pkg = match actual {
                        Ty::Class(class_qtn, _, _) => class_qtn.package().clone(),
                        Ty::Interface(actual_qtn, _, _, _) => actual_qtn.package().clone(),
                        _ => iface_qtn.package().clone(),
                    };
                    let registry = crate::interfaces::package_implements_registry(
                        self.db,
                        PackageId::new(self.db, registry_pkg),
                    );
                    let requested_iface_ty = Ty::Interface(
                        iface_qtn.clone(),
                        iface_args.clone(),
                        associated_bindings.clone(),
                        crate::ty::TyAttr::default(),
                    );
                    registry.type_implements_interface_via_rule(
                        actual,
                        &requested_iface_ty,
                        self.aliases,
                        |actual, bound| self.type_satisfies_bound(actual, bound),
                    )
                }
            }
    }

    fn projection_interface_view_matches(
        &self,
        iface_qtn: &QualifiedTypeName,
        iface_args: &[Ty],
        iface_assoc: &[(Name, Ty)],
        projected_interface: Option<&baml_type::Interface>,
    ) -> bool {
        if !self.projection_interface_matches(iface_qtn, iface_args, projected_interface) {
            return false;
        }
        let Some(projected) = projected_interface else {
            return true;
        };
        projected
            .associated_types
            .iter()
            .all(|(projected_name, projected_ty)| {
                iface_assoc
                    .iter()
                    .find(|(actual_name, _)| actual_name == projected_name)
                    .is_some_and(|(_, actual_ty)| self.types_equivalent(actual_ty, projected_ty))
            })
    }

    fn types_equivalent_without_projection_views(&self, a: &Ty, b: &Ty) -> bool {
        crate::normalize::is_same_normalized_type(a, b, self.aliases)
    }

    fn projection_interfaces_equivalent(
        &self,
        base: &Ty,
        member: &Name,
        a: Option<&baml_type::Interface>,
        b: Option<&baml_type::Interface>,
    ) -> bool {
        match (a, b) {
            (None, None) => true,
            (Some(a), Some(b)) => {
                self.types_equivalent_without_projection_views(&a.to_ty(), &b.to_ty())
            }
            (None, Some(projected)) | (Some(projected), None) => self
                .unqualified_projection_source_matches_projected_interface(base, member, projected),
        }
    }

    fn unqualified_projection_source_matches_projected_interface(
        &self,
        base: &Ty,
        member: &Name,
        projected_interface: &baml_type::Interface,
    ) -> bool {
        let Ty::TypeVar(name, _) = base else {
            return false;
        };
        let Some(Ty::Interface(bound_qtn, bound_args, bound_assoc, _)) =
            self.generic_param_bounds.get_bound(name)
        else {
            return false;
        };
        let Some(pkg_items) = self.package_items_for(bound_qtn.package()) else {
            return false;
        };
        let Some(Definition::Interface(bound_loc)) =
            pkg_items.lookup_type(bound_qtn.namespace(), bound_qtn.name())
        else {
            return false;
        };
        let pkg = baml_compiler2_hir::file_package::file_package(self.db, bound_loc.file(self.db));
        let mut sources = Vec::new();
        for (current_loc, current_args, current_assoc) in
            crate::interfaces::interface_closure_locs_with_args_and_assoc(
                self.db,
                bound_loc,
                bound_args,
                bound_assoc,
                pkg_items,
                &pkg.namespace_path,
            )
        {
            let current_tree =
                baml_compiler2_hir::file_item_tree(self.db, current_loc.file(self.db));
            let Some(current_iface) = current_tree.interfaces.get(&current_loc.id(self.db)) else {
                continue;
            };
            if !current_iface
                .associated_types
                .iter()
                .any(|assoc| assoc.name == *member)
            {
                continue;
            }
            let current_qtn = crate::lower_type_expr::qualify_def(
                self.db,
                Definition::Interface(current_loc),
                &current_iface.name,
            );
            sources.push((current_qtn, current_args, current_assoc));
        }
        if sources.len() != 1 {
            return false;
        }
        let (source_qtn, source_args, source_assoc) = &sources[0];
        self.projection_interface_view_matches(
            source_qtn,
            source_args,
            source_assoc,
            Some(projected_interface),
        )
    }

    fn type_with_default_associated_bindings(&self, ty: Ty) -> Ty {
        let Ty::Interface(iface_qtn, iface_args, associated_bindings, attr) = ty else {
            return ty;
        };
        let Some(pkg_items) = self.package_items_for(iface_qtn.package()) else {
            return Ty::Interface(iface_qtn, iface_args, associated_bindings, attr);
        };
        let Some(Definition::Interface(iface_loc)) =
            pkg_items.lookup_type(iface_qtn.namespace(), iface_qtn.name())
        else {
            return Ty::Interface(iface_qtn, iface_args, associated_bindings, attr);
        };
        let pkg = baml_compiler2_hir::file_package::file_package(self.db, iface_loc.file(self.db));
        let completed = crate::interfaces::interface_closure_locs_with_args_and_assoc(
            self.db,
            iface_loc,
            &iface_args,
            &associated_bindings,
            pkg_items,
            &pkg.namespace_path,
        )
        .into_iter()
        .next()
        .map(|(_, _, assoc)| assoc)
        .unwrap_or_else(|| associated_bindings.clone());
        Ty::Interface(iface_qtn, iface_args, completed, attr)
    }

    fn expand_alias_chains(&self, ty: Ty) -> Ty {
        let mut ty = ty;
        for _ in 0..64 {
            match &ty {
                Ty::TypeAlias(qtn, _) => match self.aliases.aliases.get(qtn) {
                    Some(expanded) => ty = expanded.clone(),
                    None => break,
                },
                _ => break,
            }
        }
        ty
    }

    fn package_items_for(&self, pkg_name: &Name) -> Option<&'db PackageItems<'db>> {
        if let Some(res_ctx) = self.res_ctx {
            res_ctx.items_for_package(self.db, pkg_name)
        } else {
            let pkg_id = PackageId::new(self.db, pkg_name.clone());
            Some(baml_compiler2_ppir::package_items(self.db, pkg_id))
        }
    }
}
