//! Package interface types and resolution context.
//!
//! `PackageInterface` is a fully-resolved typed summary of everything a package
//! exports — classes, enums, type aliases, functions, and callable identities.
//! Dependent packages consume this instead of reaching into raw `ItemTree` /
//! `TypeExpr` data.
//!
//! `PackageResolutionContext` bundles a package's own `PackageItems` with its
//! dependencies' `PackageInterface`s, providing unified lookup methods.

use std::collections::BTreeSet;

use baml_base::Name;
use baml_compiler2_ast::BuiltinKind;
use baml_compiler2_hir::{
    contributions::Definition,
    file_item_tree, file_package,
    package::{PackageId, PackageItems, package_dependencies, package_items},
};
use rustc_hash::FxHashMap;

use crate::{
    lower_type_expr::{lower_type_expr_in_ns, qualify_def},
    throw_inference::{FunctionThrowSets, function_throw_sets},
    ty::{NominalTypeRef, QualifiedTypeName, Ty, TyAttr},
};

// ── Data types ─────────────────────────────────────────────────────────────

/// Fully-resolved typed interface for a package.
/// Consumers never touch dependency `ItemTree` or raw `TypeExpr`.
#[derive(Debug, Clone, PartialEq)]
pub struct PackageInterface {
    /// All exported types: namespace path -> name -> `ExportedType`
    pub types: FxHashMap<Vec<Name>, FxHashMap<Name, ExportedType>>,
    /// All exported free functions: namespace path -> name -> `ExportedFunction`
    pub functions: FxHashMap<Vec<Name>, FxHashMap<Name, ExportedFunction>>,
    /// Canonical callable keys exported by this package.
    pub callable_keys: BTreeSet<Name>,
}

/// A type exported from a package.
#[derive(Debug, Clone, PartialEq)]
pub enum ExportedType {
    Class {
        qtn: QualifiedTypeName,
        fields: Vec<(Name, Ty)>,
        methods: Vec<ExportedFunction>,
        generic_params: Vec<Name>,
    },
    Enum {
        qtn: QualifiedTypeName,
        variants: Vec<Name>,
    },
    TypeAlias {
        qtn: QualifiedTypeName,
        resolved: Ty,
    },
}

/// A function exported from a package (free function or method).
#[derive(Debug, Clone, PartialEq)]
pub struct ExportedFunction {
    pub name: Name,
    pub callable_key: Name,
    pub params: Vec<(Name, Ty)>,
    pub return_type: Ty,
    pub declared_throws: Option<Ty>,
    pub generic_params: Vec<Name>,
    pub builtin_kind: Option<BuiltinKind>,
}

/// Distinguishes own-package results from dependency results.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolvedSource {
    /// Found in the current package's own `PackageItems`.
    Item,
    /// Found in a dependency's `PackageInterface`.
    Builtin,
}

/// Common output for resolved function signatures.
pub struct ResolvedFunction<'db> {
    pub func_loc: baml_compiler2_hir::loc::FunctionLoc<'db>,
    pub name: Name,
    pub params: Vec<(Name, Ty)>,
    pub return_type: Ty,
    pub outward_throws: Option<Ty>,
    pub generic_params: Vec<Name>,
    pub builtin_kind: Option<BuiltinKind>,
}

/// Common output for resolved method lookups (includes class context).
pub struct ResolvedMethod<'db> {
    pub function: ResolvedFunction<'db>,
    pub class_name: Name,
    pub class_generic_params: Vec<Name>,
}

impl ResolvedFunction<'_> {
    pub fn as_ty(&self) -> Ty {
        Ty::Function {
            params: self
                .params
                .iter()
                .map(|(name, ty)| (Some(name.clone()), ty.clone()))
                .collect(),
            ret: Box::new(self.return_type.clone()),
            throws: Box::new(self.outward_throws.clone().unwrap_or(Ty::Never {
                attr: TyAttr::default(),
            })),
            attr: TyAttr::default(),
        }
    }
}

/// Specialized resolution result for builtin class members.
///
/// Builtin methods can be effect-polymorphic over direct callback parameters, so
/// they still need call-site-aware lowering even though normal dependency
/// resolution flows through exported interfaces.
pub enum ResolvedBuiltinMember<'db> {
    Method {
        ty: Ty,
        class_loc: baml_compiler2_hir::loc::ClassLoc<'db>,
        func_loc: baml_compiler2_hir::loc::FunctionLoc<'db>,
    },
    Field(Ty),
}

pub struct ResolvedDependency<'db> {
    pub name: Name,
    pub package_id: PackageId<'db>,
    pub interface: &'db PackageInterface,
}

/// Bundles a package's own items with its dependencies' pre-resolved interfaces.
/// All cross-package lookups go through this context's methods.
pub struct PackageResolutionContext<'db> {
    pub own_items: &'db PackageItems<'db>,
    pub deps: Vec<ResolvedDependency<'db>>,
    pub own_package_name: Name,
}

impl PartialEq for PackageResolutionContext<'_> {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(self.own_items, other.own_items)
            && self.own_package_name == other.own_package_name
            && self.deps.len() == other.deps.len()
            && self.deps.iter().zip(other.deps.iter()).all(|(d1, d2)| {
                d1.name == d2.name
                    && d1.package_id == d2.package_id
                    && std::ptr::eq(d1.interface, d2.interface)
            })
    }
}

// ── Salsa Update impls ─────────────────────────────────────────────────────

#[allow(unsafe_code)]
unsafe impl salsa::Update for PackageInterface {
    unsafe fn maybe_update(old_pointer: *mut Self, new_value: Self) -> bool {
        #[allow(unsafe_code)]
        let old_ref = unsafe { &*old_pointer };
        if *old_ref == new_value {
            false
        } else {
            #[allow(unsafe_code)]
            unsafe {
                std::ptr::drop_in_place(old_pointer);
                std::ptr::write(old_pointer, new_value);
            }
            true
        }
    }
}

#[allow(unsafe_code)]
unsafe impl salsa::Update for PackageResolutionContext<'_> {
    unsafe fn maybe_update(old_pointer: *mut Self, new_value: Self) -> bool {
        #[allow(unsafe_code)]
        unsafe {
            std::ptr::drop_in_place(old_pointer);
            std::ptr::write(old_pointer, new_value);
        }
        true
    }
}

// ── PackageInterface lookup helpers ────────────────────────────────────────

impl PackageInterface {
    /// Look up a type by explicit namespace and item name.
    ///
    /// Single hash lookup — no split-loop ambiguity.
    pub fn lookup_type(&self, namespace: &[Name], item: &Name) -> Option<&ExportedType> {
        self.types.get(namespace)?.get(item)
    }

    /// Look up a function by explicit namespace and item name.
    pub fn lookup_function(&self, namespace: &[Name], item: &Name) -> Option<&ExportedFunction> {
        self.functions.get(namespace)?.get(item)
    }
}

impl ExportedType {
    pub fn qtn(&self) -> &QualifiedTypeName {
        match self {
            ExportedType::Class { qtn, .. } => qtn,
            ExportedType::Enum { qtn, .. } => qtn,
            ExportedType::TypeAlias { qtn, .. } => qtn,
        }
    }

    /// Convert to a Ty (for type resolution results).
    pub fn to_ty(&self) -> Ty {
        match self {
            ExportedType::Class { qtn, .. } => Ty::Class(qtn.clone().into(), TyAttr::default()),
            ExportedType::Enum { qtn, .. } => Ty::Enum(qtn.clone().into(), TyAttr::default()),
            ExportedType::TypeAlias { qtn, .. } => {
                Ty::TypeAlias(qtn.clone().into(), TyAttr::default())
            }
        }
    }
}

// ── package_interface Salsa query ──────────────────────────────────────────

#[salsa::tracked(returns(ref))]
pub fn package_interface<'db>(db: &'db dyn crate::Db, pkg_id: PackageId<'db>) -> PackageInterface {
    let pkg_items = package_items(db, pkg_id);

    let mut types: FxHashMap<Vec<Name>, FxHashMap<Name, ExportedType>> = FxHashMap::default();
    let mut functions: FxHashMap<Vec<Name>, FxHashMap<Name, ExportedFunction>> =
        FxHashMap::default();
    let mut callable_keys = BTreeSet::new();

    for (ns_path, ns_items) in &pkg_items.namespaces {
        // Export types
        for (name, def) in &ns_items.types {
            let exported = match def {
                Definition::Class(class_loc) => {
                    let item_tree = file_item_tree(db, class_loc.file(db));
                    let class_data = &item_tree[class_loc.id(db)];
                    let class_ns =
                        file_package::file_package(db, class_loc.file(db)).namespace_path;

                    // Lower fields
                    let mut fields = Vec::new();
                    let mut diags = Vec::new();
                    for field in &class_data.fields {
                        if let Some(te) = &field.type_expr {
                            let field_ty = lower_type_expr_in_ns(
                                db,
                                &te.expr,
                                pkg_items,
                                &class_ns,
                                &class_data.generic_params,
                                &mut diags,
                            );
                            fields.push((field.name.clone(), field_ty));
                        } else {
                            fields.push((
                                field.name.clone(),
                                Ty::Unknown {
                                    attr: TyAttr::default(),
                                },
                            ));
                        }
                    }

                    // Lower methods
                    let mut methods = Vec::new();
                    for method_id in &class_data.methods {
                        let method_loc = baml_compiler2_hir::loc::FunctionLoc::new(
                            db,
                            class_loc.file(db),
                            *method_id,
                        );
                        let method_data = &item_tree[*method_id];
                        let sig = baml_compiler2_hir::signature::function_signature(db, method_loc);
                        let body = baml_compiler2_hir::body::function_body(db, method_loc);

                        let mut all_generic_params = class_data.generic_params.clone();
                        all_generic_params.extend(method_data.generic_params.iter().cloned());

                        let mut params = Vec::new();
                        for (param_name, param_te) in &sig.params {
                            let param_ty = if param_name.as_str() == "self"
                                && matches!(param_te, baml_compiler2_ast::TypeExpr::Unknown { .. })
                            {
                                build_self_type_for_class(class_data, ns_path)
                            } else {
                                lower_type_expr_in_ns(
                                    db,
                                    param_te,
                                    pkg_items,
                                    &class_ns,
                                    &all_generic_params,
                                    &mut diags,
                                )
                            };
                            params.push((param_name.clone(), param_ty));
                        }

                        let return_type = sig.return_type.as_ref().map_or(
                            Ty::Unknown {
                                attr: TyAttr::default(),
                            },
                            |te| {
                                lower_type_expr_in_ns(
                                    db,
                                    te,
                                    pkg_items,
                                    &class_ns,
                                    &all_generic_params,
                                    &mut diags,
                                )
                            },
                        );

                        let explicit_throws = sig.throws.as_ref().map(|te| {
                            lower_type_expr_in_ns(
                                db,
                                te,
                                pkg_items,
                                &class_ns,
                                &all_generic_params,
                                &mut diags,
                            )
                        });

                        let builtin_kind = match body.as_ref() {
                            baml_compiler2_hir::body::FunctionBody::Builtin(kind) => Some(*kind),
                            _ => None,
                        };

                        methods.push(ExportedFunction {
                            name: method_data.name.clone(),
                            callable_key: crate::throw_inference::callable_throw_key(
                                db, method_loc,
                            ),
                            params,
                            return_type,
                            declared_throws: explicit_throws,
                            generic_params: method_data.generic_params.clone(),
                            builtin_kind,
                        });
                        callable_keys
                            .insert(crate::throw_inference::callable_throw_key(db, method_loc));
                    }

                    let qtn = qualify_def(db, *def, name);
                    ExportedType::Class {
                        qtn,
                        fields,
                        methods,
                        generic_params: class_data.generic_params.clone(),
                    }
                }
                Definition::Enum(enum_loc) => {
                    let item_tree = file_item_tree(db, enum_loc.file(db));
                    let enum_data = &item_tree[enum_loc.id(db)];
                    let qtn = qualify_def(db, *def, name);
                    ExportedType::Enum {
                        qtn,
                        variants: enum_data.variants.iter().map(|v| v.name.clone()).collect(),
                    }
                }
                Definition::TypeAlias(ta_loc) => {
                    let item_tree = file_item_tree(db, ta_loc.file(db));
                    let ta_data = &item_tree[ta_loc.id(db)];
                    let ta_ns = file_package::file_package(db, ta_loc.file(db)).namespace_path;
                    let mut diags = Vec::new();
                    let resolved = ta_data
                        .type_expr
                        .as_ref()
                        .map(|te| {
                            lower_type_expr_in_ns(db, &te.expr, pkg_items, &ta_ns, &[], &mut diags)
                        })
                        .unwrap_or(Ty::Unknown {
                            attr: TyAttr::default(),
                        });
                    let qtn = qualify_def(db, *def, name);
                    ExportedType::TypeAlias { qtn, resolved }
                }
                _ => continue,
            };
            types
                .entry(ns_path.clone())
                .or_default()
                .insert(name.clone(), exported);
        }

        // Export free functions
        for (name, def) in &ns_items.values {
            let Definition::Function(func_loc) = def else {
                continue;
            };
            let item_tree = file_item_tree(db, func_loc.file(db));
            let func_data = &item_tree[func_loc.id(db)];
            let func_ns = file_package::file_package(db, func_loc.file(db)).namespace_path;
            let sig = baml_compiler2_hir::signature::function_signature(db, *func_loc);
            let body = baml_compiler2_hir::body::function_body(db, *func_loc);
            let mut diags = Vec::new();

            let mut params = Vec::new();
            for (param_name, param_te) in &sig.params {
                let param_ty = lower_type_expr_in_ns(
                    db,
                    param_te,
                    pkg_items,
                    &func_ns,
                    &func_data.generic_params,
                    &mut diags,
                );
                params.push((param_name.clone(), param_ty));
            }

            let return_type = sig.return_type.as_ref().map_or(
                Ty::Unknown {
                    attr: TyAttr::default(),
                },
                |te| {
                    lower_type_expr_in_ns(
                        db,
                        te,
                        pkg_items,
                        &func_ns,
                        &func_data.generic_params,
                        &mut diags,
                    )
                },
            );

            let explicit_throws = sig.throws.as_ref().map(|te| {
                lower_type_expr_in_ns(
                    db,
                    te,
                    pkg_items,
                    &func_ns,
                    &func_data.generic_params,
                    &mut diags,
                )
            });

            let builtin_kind = match body.as_ref() {
                baml_compiler2_hir::body::FunctionBody::Builtin(kind) => Some(*kind),
                _ => None,
            };

            functions.entry(ns_path.clone()).or_default().insert(
                name.clone(),
                ExportedFunction {
                    name: name.clone(),
                    callable_key: crate::throw_inference::callable_throw_key(db, *func_loc),
                    params,
                    return_type,
                    declared_throws: explicit_throws,
                    generic_params: func_data.generic_params.clone(),
                    builtin_kind,
                },
            );
            callable_keys.insert(crate::throw_inference::callable_throw_key(db, *func_loc));
        }
    }

    PackageInterface {
        types,
        functions,
        callable_keys,
    }
}

/// Build the self-type for a class with `TypeVar` placeholders for generic params.
fn build_self_type_for_class(
    class_data: &baml_compiler2_hir::item_tree::Class,
    ns_path: &[Name],
) -> Ty {
    // For known builtin containers, return the corresponding Ty variant
    match class_data.name.as_str() {
        "Array" if class_data.generic_params.len() == 1 => Ty::List(
            Box::new(Ty::TypeVar(
                class_data.generic_params[0].clone(),
                TyAttr::default(),
            )),
            TyAttr::default(),
        ),
        "Map" if class_data.generic_params.len() == 2 => Ty::Map(
            Box::new(Ty::TypeVar(
                class_data.generic_params[0].clone(),
                TyAttr::default(),
            )),
            Box::new(Ty::TypeVar(
                class_data.generic_params[1].clone(),
                TyAttr::default(),
            )),
            TyAttr::default(),
        ),
        _ => {
            let qtn = QualifiedTypeName::new_with_generic_params(
                Name::new("baml"),
                ns_path.to_vec(),
                class_data.name.clone(),
                class_data.generic_params.clone(),
            );
            Ty::Class(qtn.into(), TyAttr::default())
        }
    }
}

fn outward_throws_from_key(throw_sets: &FunctionThrowSets, key: &Name) -> Option<Ty> {
    let facts = throw_sets.transitive_for(key)?;
    if facts.is_empty() {
        None
    } else {
        Some(crate::throws_semantics::concrete_throws_ty_from_facts(
            facts.clone(),
        ))
    }
}

fn builtin_runtime_class_ty(
    package: &Name,
    namespace_path: &[Name],
    class_name: &Name,
    class_generic_params: &[Name],
    type_args: &[Ty],
) -> Ty {
    if type_args.is_empty() {
        Ty::Class(
            QualifiedTypeName::new_with_generic_params(
                package.clone(),
                namespace_path.to_vec(),
                class_name.clone(),
                class_generic_params.to_vec(),
            )
            .into(),
            TyAttr::default(),
        )
    } else if type_args.len() == 1 && class_name.as_str() == "Array" {
        Ty::List(Box::new(type_args[0].clone()), TyAttr::default())
    } else if type_args.len() == 2 && class_name.as_str() == "Map" {
        Ty::Map(
            Box::new(type_args[0].clone()),
            Box::new(type_args[1].clone()),
            TyAttr::default(),
        )
    } else {
        Ty::Class(
            QualifiedTypeName::new(package.clone(), namespace_path.to_vec(), class_name.clone())
                .into(),
            TyAttr::default(),
        )
    }
}

fn synthetic_effect_throws_ty(effect_vars: &[Name]) -> Ty {
    match effect_vars.len() {
        0 => Ty::Never {
            attr: TyAttr::default(),
        },
        1 => Ty::TypeVar(effect_vars[0].clone(), TyAttr::default()),
        _ => Ty::Union(
            effect_vars
                .iter()
                .map(|name| Ty::TypeVar(name.clone(), TyAttr::default()))
                .collect(),
            TyAttr::default(),
        ),
    }
}

#[allow(clippy::too_many_arguments)]
fn lower_builtin_method_ty<'db>(
    db: &'db dyn crate::Db,
    baml_items: &'db PackageItems<'db>,
    stub_ns: &[Name],
    package: &Name,
    namespace_path: &[Name],
    class_name: &Name,
    class_generic_params: &[Name],
    method_generic_params: &[Name],
    sig: &baml_compiler2_hir::signature::FunctionSignature,
    type_args: &[Ty],
    bindings: &FxHashMap<Name, Ty>,
) -> Ty {
    let builtin_class_ty = builtin_runtime_class_ty(
        package,
        namespace_path,
        class_name,
        class_generic_params,
        type_args,
    );
    let mut diags = Vec::new();
    let mut synthetic_effect_vars: Vec<Name> = Vec::new();
    let params: Vec<(Option<Name>, Ty)> = sig
        .params
        .iter()
        .map(|(n, te)| {
            let ty = if n.as_str() == "self"
                && matches!(te, baml_compiler2_ast::TypeExpr::Unknown { .. })
            {
                builtin_class_ty.clone()
            } else if matches!(te, baml_compiler2_ast::TypeExpr::Function { .. }) {
                let lowered = crate::lower_type_expr::lower_type_expr_with_fn_context(
                    db,
                    te,
                    baml_items,
                    stub_ns,
                    method_generic_params,
                    &mut diags,
                    &crate::lower_type_expr::FnTypeLoweringContext::DirectParamRoot {
                        param_name: n.clone(),
                    },
                    &mut synthetic_effect_vars,
                );
                crate::generics::substitute_ty(&lowered, bindings)
            } else {
                crate::generics::lower_type_expr_with_generics(
                    db, te, baml_items, stub_ns, bindings, &mut diags,
                )
            };
            (Some(n.clone()), ty)
        })
        .collect();

    let ret = sig
        .return_type
        .as_ref()
        .map(|te| {
            crate::generics::lower_type_expr_with_generics(
                db, te, baml_items, stub_ns, bindings, &mut diags,
            )
        })
        .unwrap_or(Ty::Void {
            attr: TyAttr::default(),
        });

    Ty::Function {
        params,
        ret: Box::new(ret),
        throws: Box::new(synthetic_effect_throws_ty(&synthetic_effect_vars)),
        attr: TyAttr::default(),
    }
}

fn lower_builtin_field_ty<'db>(
    db: &'db dyn crate::Db,
    baml_items: &'db PackageItems<'db>,
    stub_ns: &[Name],
    type_expr: Option<&baml_compiler2_ast::SpannedTypeExpr>,
    bindings: &FxHashMap<Name, Ty>,
) -> Ty {
    let mut diags = Vec::new();
    type_expr
        .map(|te| {
            crate::generics::lower_type_expr_with_generics(
                db, &te.expr, baml_items, stub_ns, bindings, &mut diags,
            )
        })
        .unwrap_or(Ty::Unknown {
            attr: TyAttr::default(),
        })
}

// ── package_resolution_context Salsa query ─────────────────────────────────

#[salsa::tracked(returns(ref))]
pub fn package_resolution_context<'db>(
    db: &'db dyn crate::Db,
    pkg_id: PackageId<'db>,
) -> PackageResolutionContext<'db> {
    let own_items = package_items(db, pkg_id);
    let deps = package_dependencies(db, pkg_id);
    let deps: Vec<ResolvedDependency<'db>> = deps
        .iter()
        .map(|dep_id| ResolvedDependency {
            name: dep_id.name(db),
            package_id: *dep_id,
            interface: package_interface(db, *dep_id),
        })
        .collect();
    PackageResolutionContext {
        own_items,
        deps,
        own_package_name: pkg_id.name(db),
    }
}

// ── PackageResolutionContext lookup methods ─────────────────────────────────

impl<'db> PackageResolutionContext<'db> {
    /// Get `PackageItems` for an accessible package (own or declared dependency).
    ///
    /// Returns `Some` for the own package and any declared dependency,
    /// `None` for undeclared packages.
    fn items_for_package(
        &self,
        db: &'db dyn crate::Db,
        pkg_name: &Name,
    ) -> Option<&'db PackageItems<'db>> {
        if pkg_name.as_str() == self.own_package_name.as_str() {
            Some(self.own_items)
        } else if let Some(dep) = self
            .deps
            .iter()
            .find(|dep| dep.name.as_str() == pkg_name.as_str())
        {
            Some(package_items(db, dep.package_id))
        } else {
            None
        }
    }

    pub fn lookup_function(
        &self,
        db: &'db dyn crate::Db,
        path: &[Name],
        ns_context: &[Name],
    ) -> Option<(ResolvedSource, ResolvedFunction<'db>)> {
        let item = path.last()?;
        if !ns_context.is_empty() {
            let ns: Vec<_> = ns_context
                .iter()
                .chain(path[..path.len() - 1].iter())
                .cloned()
                .collect();
            if let Some(result) = self.lookup_function_in_own_then_deps(db, &ns, item) {
                return Some(result);
            }
        }
        if ns_context.is_empty() {
            if let Some(result) =
                self.lookup_function_in_own_then_deps(db, &path[..path.len() - 1], item)
            {
                return Some(result);
            }
        }
        if path.len() >= 2 {
            if path[0].as_str() == "root" {
                if let Some(function) = self.lookup_own_function(db, &path[1..path.len() - 1], item)
                {
                    return Some((ResolvedSource::Item, function));
                }
            }
            for dep in &self.deps {
                if path[0] == dep.name
                    && let Some(function) =
                        Self::lookup_dep_function(db, dep, &path[1..path.len() - 1], item)
                {
                    return Some((ResolvedSource::Builtin, function));
                }
            }
        }
        None
    }

    fn lookup_function_in_own_then_deps(
        &self,
        db: &'db dyn crate::Db,
        namespace: &[Name],
        item: &Name,
    ) -> Option<(ResolvedSource, ResolvedFunction<'db>)> {
        if let Some(function) = self.lookup_own_function(db, namespace, item) {
            return Some((ResolvedSource::Item, function));
        }
        for dep in &self.deps {
            if let Some(function) = Self::lookup_dep_function(db, dep, namespace, item) {
                return Some((ResolvedSource::Builtin, function));
            }
        }
        None
    }

    fn lookup_own_function(
        &self,
        db: &'db dyn crate::Db,
        namespace: &[Name],
        item: &Name,
    ) -> Option<ResolvedFunction<'db>> {
        let Definition::Function(func_loc) = self.own_items.lookup_value(namespace, item)? else {
            return None;
        };
        let item_tree = file_item_tree(db, func_loc.file(db));
        let func_data = &item_tree[func_loc.id(db)];
        let func_ns = file_package::file_package(db, func_loc.file(db)).namespace_path;
        let sig = baml_compiler2_hir::signature::function_signature(db, func_loc);
        let body = baml_compiler2_hir::body::function_body(db, func_loc);
        let mut diags = Vec::new();
        let mut params = Vec::new();
        for (param_name, param_te) in &sig.params {
            let param_ty = lower_type_expr_in_ns(
                db,
                param_te,
                self.own_items,
                &func_ns,
                &func_data.generic_params,
                &mut diags,
            );
            params.push((param_name.clone(), param_ty));
        }
        let return_type = sig.return_type.as_ref().map_or(
            Ty::Unknown {
                attr: TyAttr::default(),
            },
            |te| {
                lower_type_expr_in_ns(
                    db,
                    te,
                    self.own_items,
                    &func_ns,
                    &func_data.generic_params,
                    &mut diags,
                )
            },
        );
        let explicit_throws = sig.throws.as_ref().map(|te| {
            lower_type_expr_in_ns(
                db,
                te,
                self.own_items,
                &func_ns,
                &func_data.generic_params,
                &mut diags,
            )
        });
        let builtin_kind = match body.as_ref() {
            baml_compiler2_hir::body::FunctionBody::Builtin(kind) => Some(*kind),
            _ => None,
        };
        drop(diags);
        Some(ResolvedFunction {
            func_loc,
            name: func_data.name.clone(),
            params,
            return_type,
            outward_throws: explicit_throws,
            generic_params: func_data.generic_params.clone(),
            builtin_kind,
        })
    }

    fn lookup_dep_function(
        db: &'db dyn crate::Db,
        dep: &ResolvedDependency<'db>,
        namespace: &[Name],
        item: &Name,
    ) -> Option<ResolvedFunction<'db>> {
        let exported = dep.interface.lookup_function(namespace, item)?;
        let Definition::Function(func_loc) =
            package_items(db, dep.package_id).lookup_value(namespace, item)?
        else {
            return None;
        };
        Some(ResolvedFunction {
            func_loc,
            name: exported.name.clone(),
            params: exported.params.clone(),
            return_type: exported.return_type.clone(),
            outward_throws: exported.declared_throws.clone().or_else(|| {
                outward_throws_from_key(
                    function_throw_sets(db, dep.package_id),
                    &exported.callable_key,
                )
            }),
            generic_params: exported.generic_params.clone(),
            builtin_kind: exported.builtin_kind,
        })
    }

    /// Resolve a type by path. Own-package via `PackageItems`, then deps.
    pub fn resolve_type(
        &self,
        db: &'db dyn crate::Db,
        path: &[Name],
        ns_context: &[Name],
    ) -> Option<(ResolvedSource, Ty)> {
        let item = path.last()?;
        // Try namespace-qualified path first
        if !ns_context.is_empty() {
            let ns: Vec<_> = ns_context
                .iter()
                .chain(path[..path.len() - 1].iter())
                .cloned()
                .collect();
            if let Some(result) = self.resolve_type_in_own_then_deps(db, &ns, item) {
                return Some(result);
            }
        }

        // When ns_context is empty, try unqualified path (same-namespace for root files)
        if ns_context.is_empty() {
            if let Some(result) =
                self.resolve_type_in_own_then_deps(db, &path[..path.len() - 1], item)
            {
                return Some(result);
            }
        }
        // No bare fallback from non-root namespaces — cross-namespace requires explicit qualification

        // Try package-prefixed path (first segment is package name)
        if path.len() >= 2 {
            if path[0].as_str() == "root" {
                if let Some(def) = self.own_items.lookup_type(&path[1..path.len() - 1], item) {
                    let ty = def_to_ty(db, def);
                    return Some((ResolvedSource::Item, ty));
                }
            }
            for dep in &self.deps {
                if path[0] == dep.name {
                    if let Some(exported) =
                        dep.interface.lookup_type(&path[1..path.len() - 1], item)
                    {
                        return Some((ResolvedSource::Builtin, exported.to_ty()));
                    }
                }
            }
        }

        None
    }

    fn resolve_type_in_own_then_deps(
        &self,
        db: &'db dyn crate::Db,
        namespace: &[Name],
        item: &Name,
    ) -> Option<(ResolvedSource, Ty)> {
        if let Some(def) = self.own_items.lookup_type(namespace, item) {
            let ty = def_to_ty(db, def);
            return Some((ResolvedSource::Item, ty));
        }
        for dep in &self.deps {
            if let Some(exported) = dep.interface.lookup_type(namespace, item) {
                return Some((ResolvedSource::Builtin, exported.to_ty()));
            }
        }
        None
    }

    /// Resolve a function/value by path. Returns the Definition for own-package values.
    pub fn resolve_value(
        &self,
        db: &'db dyn crate::Db,
        path: &[Name],
        ns_context: &[Name],
    ) -> Option<(ResolvedSource, Definition<'db>)> {
        let item = path.last()?;
        if !ns_context.is_empty() {
            let ns: Vec<_> = ns_context
                .iter()
                .chain(path[..path.len() - 1].iter())
                .cloned()
                .collect();
            if let Some(result) = self.resolve_value_in_own(&ns, item) {
                return Some(result);
            }
        }
        // When ns_context is empty, try unqualified path (same-namespace for root files)
        if ns_context.is_empty() {
            if let Some(result) = self.resolve_value_in_own(&path[..path.len() - 1], item) {
                return Some(result);
            }
        }
        // No bare fallback from non-root namespaces — cross-namespace requires explicit qualification
        // root.* prefix handling (parity with resolve_type)
        if path.len() >= 2 {
            if path[0].as_str() == "root" {
                if let Some(def) = self.own_items.lookup_value(&path[1..path.len() - 1], item) {
                    return Some((ResolvedSource::Item, def));
                }
            }
            // dep-prefixed search (parity with resolve_type)
            for dep in &self.deps {
                if path[0] == dep.name {
                    if let Some(def) = self.lookup_value_definition_in_package(
                        db,
                        &dep.name,
                        &path[1..path.len() - 1],
                        item,
                    ) {
                        return Some((ResolvedSource::Builtin, def));
                    }
                }
            }
        }
        None
    }

    fn resolve_value_in_own(
        &self,
        namespace: &[Name],
        item: &Name,
    ) -> Option<(ResolvedSource, Definition<'db>)> {
        if let Some(def) = self.own_items.lookup_value(namespace, item) {
            return Some((ResolvedSource::Item, def));
        }
        None
    }

    pub fn lookup_value_definition_in_package(
        &self,
        db: &'db dyn crate::Db,
        pkg_name: &Name,
        namespace: &[Name],
        item: &Name,
    ) -> Option<Definition<'db>> {
        let pkg_items = self.items_for_package(db, pkg_name)?;
        pkg_items.lookup_value(namespace, item)
    }

    pub fn lookup_type_definition_in_package(
        &self,
        db: &'db dyn crate::Db,
        pkg_name: &Name,
        namespace: &[Name],
        item: &Name,
    ) -> Option<Definition<'db>> {
        let pkg_items = self.items_for_package(db, pkg_name)?;
        pkg_items.lookup_type(namespace, item)
    }

    pub fn lookup_class_loc(
        &self,
        db: &'db dyn crate::Db,
        qtn: &QualifiedTypeName,
    ) -> Option<baml_compiler2_hir::loc::ClassLoc<'db>> {
        match self.lookup_type_definition_in_package(
            db,
            qtn.package(),
            qtn.namespace(),
            qtn.name(),
        )? {
            Definition::Class(class_loc) => Some(class_loc),
            _ => None,
        }
    }

    pub fn lookup_enum_loc(
        &self,
        db: &'db dyn crate::Db,
        qtn: &QualifiedTypeName,
    ) -> Option<baml_compiler2_hir::loc::EnumLoc<'db>> {
        match self.lookup_type_definition_in_package(
            db,
            qtn.package(),
            qtn.namespace(),
            qtn.name(),
        )? {
            Definition::Enum(enum_loc) => Some(enum_loc),
            _ => None,
        }
    }

    pub fn lookup_enum_variants(
        &self,
        db: &'db dyn crate::Db,
        enum_name: &QualifiedTypeName,
    ) -> Vec<Name> {
        if enum_name.package().as_str() == self.own_package_name.as_str() {
            let Some(Definition::Enum(enum_loc)) = self.lookup_type_definition_in_package(
                db,
                enum_name.package(),
                enum_name.namespace(),
                enum_name.name(),
            ) else {
                return Vec::new();
            };
            let item_tree = file_item_tree(db, enum_loc.file(db));
            let enum_data = &item_tree[enum_loc.id(db)];
            enum_data.variants.iter().map(|v| v.name.clone()).collect()
        } else {
            self.deps
                .iter()
                .find_map(|dep| {
                    if &dep.name != enum_name.package() {
                        return None;
                    }
                    match dep
                        .interface
                        .lookup_type(enum_name.namespace(), enum_name.name())?
                    {
                        ExportedType::Enum { variants, .. } => Some(variants.clone()),
                        _ => None,
                    }
                })
                .unwrap_or_default()
        }
    }

    /// Look up class fields. Dual dispatch:
    /// - Own-package: `ItemTree` -> lower fields + apply class-level generic substitution
    /// - Dependency: `ExportedType::Class` { fields } + apply class-level generic substitution
    pub fn lookup_class_fields(
        &self,
        db: &'db dyn crate::Db,
        class_name: &NominalTypeRef,
    ) -> Vec<(Name, Ty)> {
        let class_pkg = class_name.package();
        if class_pkg.as_str() == self.own_package_name.as_str() {
            let raw_fields = self.lookup_own_class_fields(db, class_name.qtn());
            self.apply_class_substitution(db, class_name, raw_fields)
        } else {
            for dep in &self.deps {
                if &dep.name != class_pkg {
                    continue;
                }
                if let Some(ExportedType::Class {
                    fields,
                    generic_params,
                    ..
                }) = dep
                    .interface
                    .lookup_type(class_name.namespace(), class_name.name())
                {
                    if class_name.type_args().is_empty() || generic_params.is_empty() {
                        return fields.clone();
                    }
                    let bindings =
                        crate::generics::bind_type_vars(generic_params, class_name.type_args());
                    return fields
                        .iter()
                        .map(|(n, ty)| (n.clone(), crate::generics::substitute_ty(ty, &bindings)))
                        .collect();
                }
            }
            Vec::new()
        }
    }

    /// Apply class-level generic substitution to own-package fields.
    fn apply_class_substitution(
        &self,
        db: &'db dyn crate::Db,
        class_name: &NominalTypeRef,
        raw_fields: Vec<(Name, Ty)>,
    ) -> Vec<(Name, Ty)> {
        if class_name.type_args().is_empty() {
            return raw_fields;
        }
        // Get generic_params from item tree
        let Some(def) = self
            .own_items
            .lookup_type(class_name.namespace(), class_name.name())
        else {
            return raw_fields;
        };
        let Definition::Class(class_loc) = def else {
            return raw_fields;
        };
        let item_tree = file_item_tree(db, class_loc.file(db));
        let class_data = &item_tree[class_loc.id(db)];
        let bindings =
            crate::generics::bind_type_vars(&class_data.generic_params, class_name.type_args());
        raw_fields
            .into_iter()
            .map(|(n, ty)| (n, crate::generics::substitute_ty(&ty, &bindings)))
            .collect()
    }

    /// Look up a class method. Dual dispatch.
    /// Applies class-level generic substitution to the returned method type.
    pub fn lookup_class_method(
        &self,
        db: &'db dyn crate::Db,
        class_name: &NominalTypeRef,
        method_name: &Name,
    ) -> Option<ResolvedMethod<'db>> {
        let class_pkg = class_name.package();
        if class_pkg.as_str() == self.own_package_name.as_str() {
            self.lookup_own_class_method(db, class_name.qtn(), method_name)
                .map(|mut rm| {
                    Self::apply_method_substitution(class_name, &mut rm);
                    rm
                })
        } else {
            for dep in &self.deps {
                if &dep.name != class_pkg {
                    continue;
                }
                if let Some(ExportedType::Class {
                    methods,
                    generic_params,
                    ..
                }) = dep
                    .interface
                    .lookup_type(class_name.namespace(), class_name.name())
                {
                    if let Some(method) = methods.iter().find(|m| &m.name == method_name) {
                        let Some((_, func_loc)) =
                            self.lookup_class_method_locs(db, class_name, method_name)
                        else {
                            continue;
                        };
                        let mut resolved_method = ResolvedMethod {
                            function: ResolvedFunction {
                                func_loc,
                                name: method.name.clone(),
                                params: method.params.clone(),
                                return_type: method.return_type.clone(),
                                outward_throws: method.declared_throws.clone().or_else(|| {
                                    outward_throws_from_key(
                                        function_throw_sets(db, dep.package_id),
                                        &method.callable_key,
                                    )
                                }),
                                generic_params: method.generic_params.clone(),
                                builtin_kind: method.builtin_kind,
                            },
                            class_name: class_name.name().clone(),
                            class_generic_params: generic_params.clone(),
                        };
                        // Apply class-level substitution using generic_params from ExportedType
                        if !class_name.type_args().is_empty() && !generic_params.is_empty() {
                            let bindings = crate::generics::bind_type_vars(
                                generic_params,
                                class_name.type_args(),
                            );
                            resolved_method.function.params = resolved_method
                                .function
                                .params
                                .into_iter()
                                .map(|(n, ty)| (n, crate::generics::substitute_ty(&ty, &bindings)))
                                .collect();
                            resolved_method.function.return_type = crate::generics::substitute_ty(
                                &resolved_method.function.return_type,
                                &bindings,
                            );
                            if let Some(ref outward_throws) =
                                resolved_method.function.outward_throws.clone()
                            {
                                resolved_method.function.outward_throws =
                                    Some(crate::generics::substitute_ty(outward_throws, &bindings));
                            }
                        }
                        return Some(resolved_method);
                    }
                }
            }
            None
        }
    }

    pub fn lookup_class_method_locs(
        &self,
        db: &'db dyn crate::Db,
        class_name: &NominalTypeRef,
        method_name: &Name,
    ) -> Option<(
        baml_compiler2_hir::loc::ClassLoc<'db>,
        baml_compiler2_hir::loc::FunctionLoc<'db>,
    )> {
        let class_loc = self.lookup_class_loc(db, class_name.qtn())?;
        let item_tree = file_item_tree(db, class_loc.file(db));
        let class_data = &item_tree[class_loc.id(db)];
        let func_loc = class_data.methods.iter().find_map(|&method_id| {
            let method_data = &item_tree[method_id];
            if method_data.name == *method_name {
                Some(baml_compiler2_hir::loc::FunctionLoc::new(
                    db,
                    class_loc.file(db),
                    method_id,
                ))
            } else {
                None
            }
        })?;
        Some((class_loc, func_loc))
    }

    /// Resolve a member on a builtin class declared in the `baml` package.
    ///
    /// This keeps the raw builtin stub inspection inside `PackageResolutionContext`
    /// so callers do not need to reach into foreign `PackageItems` or `ItemTree`
    /// directly. Method parameters still get specialized lowering so omitted
    /// throws on direct callback params become synthetic effect variables.
    pub fn resolve_builtin_member(
        &self,
        db: &'db dyn crate::Db,
        class_path: &[Name],
        type_args: &[Ty],
        member_name: &Name,
    ) -> Option<ResolvedBuiltinMember<'db>> {
        let baml_items = self.items_for_package(db, &Name::new("baml"))?;
        let item = class_path.last()?;
        let def = baml_items.lookup_type(&class_path[..class_path.len() - 1], item)?;
        let Definition::Class(class_loc) = def else {
            return None;
        };

        let file = class_loc.file(db);
        let stub_pkg = file_package::file_package(db, file);
        let stub_ns: &[Name] = &stub_pkg.namespace_path;
        let item_tree = file_item_tree(db, file);
        let class_data = &item_tree[class_loc.id(db)];

        let mut bindings = crate::generics::bind_type_vars(&class_data.generic_params, type_args);

        for &method_id in &class_data.methods {
            let method_data = &item_tree[method_id];
            if method_data.name != *member_name {
                continue;
            }

            for gp in &method_data.generic_params {
                bindings
                    .entry(gp.clone())
                    .or_insert_with(|| Ty::TypeVar(gp.clone(), TyAttr::default()));
            }

            let func_loc = baml_compiler2_hir::loc::FunctionLoc::new(db, file, method_id);
            let sig = baml_compiler2_hir::signature::function_signature(db, func_loc);
            let method_generic_params: Vec<Name> = class_data
                .generic_params
                .iter()
                .chain(method_data.generic_params.iter())
                .cloned()
                .collect();
            return Some(ResolvedBuiltinMember::Method {
                ty: lower_builtin_method_ty(
                    db,
                    baml_items,
                    stub_ns,
                    &stub_pkg.package,
                    &stub_pkg.namespace_path,
                    &class_data.name,
                    &class_data.generic_params,
                    &method_generic_params,
                    sig.as_ref(),
                    type_args,
                    &bindings,
                ),
                class_loc,
                func_loc,
            });
        }

        for field in &class_data.fields {
            if field.name != *member_name {
                continue;
            }

            return Some(ResolvedBuiltinMember::Field(lower_builtin_field_ty(
                db,
                baml_items,
                stub_ns,
                field.type_expr.as_ref(),
                &bindings,
            )));
        }

        None
    }

    /// Apply class-level generic substitution to a `ResolvedMethod`'s function signature.
    /// Uses `generic_params` from the own-package item tree.
    fn apply_method_substitution(
        class_name: &NominalTypeRef,
        resolved_method: &mut ResolvedMethod<'_>,
    ) {
        if class_name.type_args().is_empty() {
            return;
        }
        // class_generic_params was set by lookup_own_class_method from item tree
        if resolved_method.class_generic_params.is_empty() {
            return;
        }
        let bindings = crate::generics::bind_type_vars(
            &resolved_method.class_generic_params,
            class_name.type_args(),
        );
        resolved_method.function.params = resolved_method
            .function
            .params
            .iter()
            .map(|(n, ty)| (n.clone(), crate::generics::substitute_ty(ty, &bindings)))
            .collect();
        resolved_method.function.return_type =
            crate::generics::substitute_ty(&resolved_method.function.return_type, &bindings);
        if let Some(ref outward_throws) = resolved_method.function.outward_throws.clone() {
            resolved_method.function.outward_throws =
                Some(crate::generics::substitute_ty(outward_throws, &bindings));
        }
    }

    fn lookup_own_class_fields(
        &self,
        db: &'db dyn crate::Db,
        class_name: &QualifiedTypeName,
    ) -> Vec<(Name, Ty)> {
        let Some(def) = self
            .own_items
            .lookup_type(class_name.namespace(), class_name.name())
        else {
            return Vec::new();
        };
        let Definition::Class(class_loc) = def else {
            return Vec::new();
        };
        let item_tree = file_item_tree(db, class_loc.file(db));
        let class_data = &item_tree[class_loc.id(db)];
        let ns = file_package::file_package(db, class_loc.file(db)).namespace_path;
        let mut diags = Vec::new();
        let mut fields = Vec::new();
        for field in &class_data.fields {
            if let Some(te) = &field.type_expr {
                let field_ty = lower_type_expr_in_ns(
                    db,
                    &te.expr,
                    self.own_items,
                    &ns,
                    &class_data.generic_params,
                    &mut diags,
                );
                fields.push((field.name.clone(), field_ty));
            } else {
                fields.push((
                    field.name.clone(),
                    Ty::Unknown {
                        attr: TyAttr::default(),
                    },
                ));
            }
        }
        fields
    }

    fn lookup_own_class_method(
        &self,
        db: &'db dyn crate::Db,
        class_name: &QualifiedTypeName,
        method_name: &Name,
    ) -> Option<ResolvedMethod<'db>> {
        let def = self
            .own_items
            .lookup_type(class_name.namespace(), class_name.name())?;
        let Definition::Class(class_loc) = def else {
            return None;
        };
        let item_tree = file_item_tree(db, class_loc.file(db));
        let class_data = &item_tree[class_loc.id(db)];
        let ns = file_package::file_package(db, class_loc.file(db)).namespace_path;
        let mut diags = Vec::new();

        for method_id in &class_data.methods {
            let method_data = &item_tree[*method_id];
            if &method_data.name != method_name {
                continue;
            }
            let method_loc =
                baml_compiler2_hir::loc::FunctionLoc::new(db, class_loc.file(db), *method_id);
            let sig = baml_compiler2_hir::signature::function_signature(db, method_loc);
            let body = baml_compiler2_hir::body::function_body(db, method_loc);

            let mut all_generic_params = class_data.generic_params.clone();
            all_generic_params.extend(method_data.generic_params.iter().cloned());

            let mut params = Vec::new();
            for (param_name, param_te) in &sig.params {
                let param_ty = lower_type_expr_in_ns(
                    db,
                    param_te,
                    self.own_items,
                    &ns,
                    &all_generic_params,
                    &mut diags,
                );
                params.push((param_name.clone(), param_ty));
            }
            let return_type = sig.return_type.as_ref().map_or(
                Ty::Unknown {
                    attr: TyAttr::default(),
                },
                |te| {
                    lower_type_expr_in_ns(
                        db,
                        te,
                        self.own_items,
                        &ns,
                        &all_generic_params,
                        &mut diags,
                    )
                },
            );
            let explicit_throws = sig.throws.as_ref().map(|te| {
                lower_type_expr_in_ns(db, te, self.own_items, &ns, &all_generic_params, &mut diags)
            });

            let builtin_kind = match body.as_ref() {
                baml_compiler2_hir::body::FunctionBody::Builtin(kind) => Some(*kind),
                _ => None,
            };

            return Some(ResolvedMethod {
                function: ResolvedFunction {
                    func_loc: method_loc,
                    name: method_data.name.clone(),
                    params,
                    return_type,
                    outward_throws: explicit_throws,
                    generic_params: method_data.generic_params.clone(),
                    builtin_kind,
                },
                class_name: class_name.name().clone(),
                class_generic_params: class_data.generic_params.clone(),
            });
        }
        None
    }
}

/// Convert a Definition to Ty (own-package path).
fn def_to_ty<'db>(db: &'db dyn crate::Db, def: Definition<'db>) -> Ty {
    let name = match def {
        Definition::Class(loc) => {
            let item_tree = file_item_tree(db, loc.file(db));
            let data = &item_tree[loc.id(db)];
            data.name.clone()
        }
        Definition::Enum(loc) => {
            let item_tree = file_item_tree(db, loc.file(db));
            let data = &item_tree[loc.id(db)];
            data.name.clone()
        }
        Definition::TypeAlias(loc) => {
            let item_tree = file_item_tree(db, loc.file(db));
            let data = &item_tree[loc.id(db)];
            data.name.clone()
        }
        _ => {
            return Ty::Unknown {
                attr: TyAttr::default(),
            };
        }
    };
    match def {
        Definition::Class(_) => Ty::Class(qualify_def(db, def, &name).into(), TyAttr::default()),
        Definition::Enum(_) => Ty::Enum(qualify_def(db, def, &name).into(), TyAttr::default()),
        Definition::TypeAlias(_) => {
            Ty::TypeAlias(qualify_def(db, def, &name).into(), TyAttr::default())
        }
        _ => Ty::Unknown {
            attr: TyAttr::default(),
        },
    }
}
