//! Package interface types and resolution context.
//!
//! `PackageInterface` is a fully-resolved typed summary of everything a package
//! exports — classes, enums, type aliases, functions, and throw sets.
//! Dependent packages consume this instead of reaching into raw `ItemTree` /
//! `TypeExpr` data.
//!
//! `PackageResolutionContext` bundles a package's own `PackageItems` with its
//! dependencies' `PackageInterface`s, providing unified lookup methods.

use baml_base::Name;
use baml_compiler2_ast::BuiltinKind;
use baml_compiler2_hir::{
    contributions::Definition,
    file_package,
    package::{PackageId, PackageItems, package_dependencies},
};
use rustc_hash::FxHashMap;

use crate::{
    infer_context::TirTypeError,
    lower_type_expr::{lower_type_expr_in_ns, qualify_def},
    throw_inference::{FunctionThrowSets, function_throw_sets},
    ty::{FunctionParamMode, FunctionParamTy, QualifiedTypeName, Ty, TyAttr},
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
    /// Throw sets for all functions in this package (transitive, fully inferred).
    pub throw_sets: FunctionThrowSets,
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
    pub params: Vec<FunctionParamTy>,
    pub return_type: Ty,
    pub declared_throws: Option<Ty>,
    pub callable_throws: Ty,
    /// Function-level generic parameters, including any synthetic callback
    /// effect parameters introduced by bounded signature elaboration.
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
pub struct ResolvedFunction {
    pub name: Name,
    pub params: Vec<FunctionParamTy>,
    pub return_type: Ty,
    pub declared_throws: Option<Ty>,
    pub callable_throws: Ty,
    pub generic_params: Vec<Name>,
    pub builtin_kind: Option<BuiltinKind>,
}

/// Common output for resolved method lookups (includes class context).
pub struct ResolvedMethod {
    pub function: ResolvedFunction,
    pub class_name: Name,
    pub class_generic_params: Vec<Name>,
}

/// Bundles a package's own items with its dependencies' pre-resolved interfaces.
/// All cross-package lookups go through this context's methods.
///
/// This query result intentionally owns its contents. Storing borrowed refs to
/// other `returns(ref)` queries here made incremental invalidation unsound
/// under rapid edits, because the cached context could outlive the referenced
/// query storage across revisions.
#[derive(Debug, Clone, PartialEq)]
pub struct PackageResolutionContext<'db> {
    pub own_items: PackageItems<'db>,
    pub dep_interfaces: Vec<(Name, PackageInterface)>,
    pub own_package_name: Name,
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
            ExportedType::Class { qtn, .. } => Ty::Class(qtn.clone(), vec![], TyAttr::default()),
            ExportedType::Enum { qtn, .. } => Ty::Enum(qtn.clone(), TyAttr::default()),
            ExportedType::TypeAlias { qtn, .. } => Ty::TypeAlias(qtn.clone(), TyAttr::default()),
        }
    }
}

fn exported_function_param(name: Name, ty: Ty, has_default: bool) -> FunctionParamTy {
    FunctionParamTy {
        name: Some(name),
        ty,
        mode: if has_default {
            FunctionParamMode::Optional
        } else {
            FunctionParamMode::Required
        },
    }
}

struct LoweredClassMethodSignature {
    params: Vec<FunctionParamTy>,
    return_type: Ty,
    declared_throws: Option<Ty>,
    callable_throws: Ty,
    generic_params: Vec<Name>,
    builtin_kind: Option<BuiltinKind>,
}

fn lower_class_method_signature<'db>(
    db: &'db dyn crate::Db,
    pkg_items: &PackageItems<'db>,
    class_data: &baml_compiler2_hir::item_tree::Class,
    method_loc: baml_compiler2_hir::loc::FunctionLoc<'db>,
    ns_path: &[Name],
    diags: &mut Vec<TirTypeError>,
) -> LoweredClassMethodSignature {
    let sig = baml_compiler2_ppir::elaborated_function_signature(db, method_loc);
    let body = baml_compiler2_ppir::function_body(db, method_loc);

    let mut all_generic_params = class_data.generic_params.clone();
    all_generic_params.extend(sig.user_generic_params.iter().cloned());
    all_generic_params.extend(sig.synthetic_effect_params.iter().cloned());

    let mut params = Vec::new();
    for param in &sig.params {
        let param_ty = if param.name.as_str() == "self"
            && matches!(param.ty, baml_compiler2_ast::TypeExpr::Unknown { .. })
        {
            build_self_type_for_class(class_data, ns_path)
        } else {
            lower_type_expr_in_ns(
                db,
                &param.ty,
                pkg_items,
                ns_path,
                &all_generic_params,
                diags,
            )
        };
        params.push(exported_function_param(
            param.name.clone(),
            param_ty,
            param.has_default,
        ));
    }

    let return_type = sig.return_type.as_ref().map_or(
        Ty::Unknown {
            attr: TyAttr::default(),
        },
        |te| lower_type_expr_in_ns(db, te, pkg_items, ns_path, &all_generic_params, diags),
    );

    let declared_throws = sig
        .throws
        .as_ref()
        .map(|te| lower_type_expr_in_ns(db, te, pkg_items, ns_path, &all_generic_params, diags));
    let callable_throws = crate::callable::callable_throws(db, method_loc).clone();

    let builtin_kind = match body.as_ref() {
        baml_compiler2_hir::body::FunctionBody::Builtin(kind) => Some(*kind),
        _ => None,
    };

    LoweredClassMethodSignature {
        params,
        return_type,
        declared_throws,
        callable_throws,
        generic_params: sig
            .user_generic_params
            .iter()
            .chain(sig.synthetic_effect_params.iter())
            .cloned()
            .collect(),
        builtin_kind,
    }
}

// ── package_interface Salsa query ──────────────────────────────────────────

#[salsa::tracked(returns(ref))]
pub fn package_interface<'db>(db: &'db dyn crate::Db, pkg_id: PackageId<'db>) -> PackageInterface {
    let pkg_items = baml_compiler2_ppir::package_items(db, pkg_id);

    let mut types: FxHashMap<Vec<Name>, FxHashMap<Name, ExportedType>> = FxHashMap::default();
    let mut functions: FxHashMap<Vec<Name>, FxHashMap<Name, ExportedFunction>> =
        FxHashMap::default();

    for (ns_path, ns_items) in &pkg_items.namespaces {
        // Export types
        for (name, def) in &ns_items.types {
            let exported = match def {
                Definition::Class(class_loc) => {
                    let item_tree = baml_compiler2_ppir::file_item_tree(db, class_loc.file(db));
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
                        let lowered = lower_class_method_signature(
                            db, pkg_items, class_data, method_loc, &class_ns, &mut diags,
                        );

                        methods.push(ExportedFunction {
                            name: method_data.name.clone(),
                            params: lowered.params,
                            return_type: lowered.return_type,
                            declared_throws: lowered.declared_throws,
                            callable_throws: lowered.callable_throws,
                            generic_params: lowered.generic_params,
                            builtin_kind: lowered.builtin_kind,
                        });
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
                    let item_tree = baml_compiler2_ppir::file_item_tree(db, enum_loc.file(db));
                    let enum_data = &item_tree[enum_loc.id(db)];
                    let qtn = qualify_def(db, *def, name);
                    ExportedType::Enum {
                        qtn,
                        variants: enum_data.variants.iter().map(|v| v.name.clone()).collect(),
                    }
                }
                Definition::TypeAlias(ta_loc) => {
                    let item_tree = baml_compiler2_ppir::file_item_tree(db, ta_loc.file(db));
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
            let func_ns = file_package::file_package(db, func_loc.file(db)).namespace_path;
            let sig = baml_compiler2_ppir::elaborated_function_signature(db, *func_loc);
            let body = baml_compiler2_ppir::function_body(db, *func_loc);
            let mut diags = Vec::new();
            let function_generic_params: Vec<Name> = sig
                .user_generic_params
                .iter()
                .chain(sig.synthetic_effect_params.iter())
                .cloned()
                .collect();

            let mut params = Vec::new();
            for param in &sig.params {
                let param_ty = lower_type_expr_in_ns(
                    db,
                    &param.ty,
                    pkg_items,
                    &func_ns,
                    &function_generic_params,
                    &mut diags,
                );
                params.push(exported_function_param(
                    param.name.clone(),
                    param_ty,
                    param.has_default,
                ));
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
                        &function_generic_params,
                        &mut diags,
                    )
                },
            );

            let declared_throws = sig.throws.as_ref().map(|te| {
                lower_type_expr_in_ns(
                    db,
                    te,
                    pkg_items,
                    &func_ns,
                    &function_generic_params,
                    &mut diags,
                )
            });
            let callable_throws = crate::callable::callable_throws(db, *func_loc).clone();

            let builtin_kind = match body.as_ref() {
                baml_compiler2_hir::body::FunctionBody::Builtin(kind) => Some(*kind),
                _ => None,
            };

            functions.entry(ns_path.clone()).or_default().insert(
                name.clone(),
                ExportedFunction {
                    name: name.clone(),
                    params,
                    return_type,
                    declared_throws,
                    callable_throws,
                    generic_params: function_generic_params,
                    builtin_kind,
                },
            );
        }
    }

    // Compute throw sets for this package
    let throw_sets = function_throw_sets(db, pkg_id);

    PackageInterface {
        types,
        functions,
        throw_sets: throw_sets.clone(),
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
            Ty::Class(qtn, vec![], TyAttr::default())
        }
    }
}

// ── package_resolution_context Salsa query ─────────────────────────────────

#[salsa::tracked(returns(ref))]
pub fn package_resolution_context<'db>(
    db: &'db dyn crate::Db,
    pkg_id: PackageId<'db>,
) -> PackageResolutionContext<'db> {
    let own_items = baml_compiler2_ppir::package_items(db, pkg_id).clone();
    let deps = package_dependencies(db, pkg_id);
    let dep_interfaces: Vec<(Name, PackageInterface)> = deps
        .iter()
        .map(|dep_id| {
            let name = dep_id.name(db);
            let iface = package_interface(db, *dep_id).clone();
            (name, iface)
        })
        .collect();
    PackageResolutionContext {
        own_items,
        dep_interfaces,
        own_package_name: pkg_id.name(db),
    }
}

// ── PackageResolutionContext lookup methods ─────────────────────────────────

impl<'db> PackageResolutionContext<'db> {
    /// Get `PackageItems` for an accessible package (own or declared dependency).
    ///
    /// Returns `Some` for the own package and any declared dependency,
    /// `None` for undeclared packages.
    pub fn items_for_package(
        &'db self,
        db: &'db dyn crate::Db,
        pkg_name: &Name,
    ) -> Option<&'db PackageItems<'db>> {
        if pkg_name.as_str() == self.own_package_name.as_str() {
            Some(&self.own_items)
        } else if self
            .dep_interfaces
            .iter()
            .any(|(n, _)| n.as_str() == pkg_name.as_str())
        {
            let pkg_id = PackageId::new(db, pkg_name.clone());
            Some(baml_compiler2_ppir::package_items(db, pkg_id))
        } else {
            None
        }
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
            for (dep_name, dep_iface) in &self.dep_interfaces {
                if &path[0] == dep_name {
                    if let Some(exported) = dep_iface.lookup_type(&path[1..path.len() - 1], item) {
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
        for (_dep_name, dep_iface) in &self.dep_interfaces {
            if let Some(exported) = dep_iface.lookup_type(namespace, item) {
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
            for (dep_name, _dep_iface) in &self.dep_interfaces {
                if &path[0] == dep_name {
                    let dep_pkg_id = PackageId::new(db, dep_name.clone());
                    let dep_items = baml_compiler2_ppir::package_items(db, dep_pkg_id);
                    if let Some(def) = dep_items.lookup_value(&path[1..path.len() - 1], item) {
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

    /// Look up class fields. Dual dispatch:
    /// - Own-package: `ItemTree` -> lower fields
    /// - Dependency: `ExportedType::Class` { fields }
    pub fn lookup_class_fields(
        &self,
        db: &'db dyn crate::Db,
        class_name: &QualifiedTypeName,
    ) -> Vec<(Name, Ty)> {
        let class_pkg = class_name.package();
        if class_pkg.as_str() == self.own_package_name.as_str() {
            self.lookup_own_class_fields(db, class_name)
        } else {
            for (dep_name, dep_iface) in &self.dep_interfaces {
                if dep_name != class_pkg {
                    continue;
                }
                if let Some(ExportedType::Class { fields, .. }) =
                    dep_iface.lookup_type(class_name.namespace(), class_name.name())
                {
                    return fields.clone();
                }
            }
            Vec::new()
        }
    }

    /// Look up a class method. Dual dispatch.
    pub fn lookup_class_method(
        &self,
        db: &'db dyn crate::Db,
        class_name: &QualifiedTypeName,
        method_name: &Name,
    ) -> Option<ResolvedMethod> {
        let class_pkg = class_name.package();
        if class_pkg.as_str() == self.own_package_name.as_str() {
            self.lookup_own_class_method(db, class_name, method_name)
        } else {
            for (dep_name, dep_iface) in &self.dep_interfaces {
                if dep_name != class_pkg {
                    continue;
                }
                if let Some(ExportedType::Class {
                    methods,
                    generic_params,
                    ..
                }) = dep_iface.lookup_type(class_name.namespace(), class_name.name())
                {
                    if let Some(method) = methods.iter().find(|m| &m.name == method_name) {
                        return Some(ResolvedMethod {
                            function: ResolvedFunction {
                                name: method.name.clone(),
                                params: method.params.clone(),
                                return_type: method.return_type.clone(),
                                declared_throws: method.declared_throws.clone(),
                                callable_throws: method.callable_throws.clone(),
                                generic_params: method.generic_params.clone(),
                                builtin_kind: method.builtin_kind,
                            },
                            class_name: class_name.name().clone(),
                            class_generic_params: generic_params.clone(),
                        });
                    }
                }
            }
            None
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
        let item_tree = baml_compiler2_ppir::file_item_tree(db, class_loc.file(db));
        let class_data = &item_tree[class_loc.id(db)];
        let ns = file_package::file_package(db, class_loc.file(db)).namespace_path;
        let mut diags = Vec::new();
        let mut fields = Vec::new();
        for field in &class_data.fields {
            if let Some(te) = &field.type_expr {
                let field_ty = lower_type_expr_in_ns(
                    db,
                    &te.expr,
                    &self.own_items,
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
    ) -> Option<ResolvedMethod> {
        let def = self
            .own_items
            .lookup_type(class_name.namespace(), class_name.name())?;
        let Definition::Class(class_loc) = def else {
            return None;
        };
        let item_tree = baml_compiler2_ppir::file_item_tree(db, class_loc.file(db));
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
            let lowered = lower_class_method_signature(
                db,
                &self.own_items,
                class_data,
                method_loc,
                &ns,
                &mut diags,
            );

            return Some(ResolvedMethod {
                function: ResolvedFunction {
                    name: method_data.name.clone(),
                    params: lowered.params,
                    return_type: lowered.return_type,
                    declared_throws: lowered.declared_throws,
                    callable_throws: lowered.callable_throws,
                    generic_params: lowered.generic_params,
                    builtin_kind: lowered.builtin_kind,
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
            let item_tree = baml_compiler2_ppir::file_item_tree(db, loc.file(db));
            let data = &item_tree[loc.id(db)];
            data.name.clone()
        }
        Definition::Enum(loc) => {
            let item_tree = baml_compiler2_ppir::file_item_tree(db, loc.file(db));
            let data = &item_tree[loc.id(db)];
            data.name.clone()
        }
        Definition::TypeAlias(loc) => {
            let item_tree = baml_compiler2_ppir::file_item_tree(db, loc.file(db));
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
        Definition::Class(_) => Ty::Class(qualify_def(db, def, &name), vec![], TyAttr::default()),
        Definition::Enum(_) => Ty::Enum(qualify_def(db, def, &name), TyAttr::default()),
        Definition::TypeAlias(_) => Ty::TypeAlias(qualify_def(db, def, &name), TyAttr::default()),
        _ => Ty::Unknown {
            attr: TyAttr::default(),
        },
    }
}
