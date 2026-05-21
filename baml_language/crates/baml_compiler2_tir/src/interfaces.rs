//! Interface implementation registry for nominal subtyping (BEP-044).
//!
//! Per-package map from class qualified names to the set of interface
//! qualified names that class implements directly via `implements I {}`
//! blocks. Interface `requires` is tracked separately for interface-to-interface
//! subtyping; classes must explicitly implement required parents.
//!
//! `Class T <: Interface I` iff `I ∈ implements(T)` — there is no
//! shape-matching escape hatch.
//!
//! Salsa-tracked so subtype calls don't rebuild the closure on each check.

use baml_base::Name;
use baml_compiler2_hir::{contributions::Definition, package::PackageId};
use rustc_hash::{FxHashMap, FxHashSet};

use crate::{
    lower_type_expr::qualify_def,
    ty::{PrimitiveType, QualifiedTypeName, Ty, TyAttr},
};

/// For every class in a package, the set of interfaces it implements directly.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ImplementsRegistry {
    /// Class QTN → interfaces it implements.
    pub class_implements: FxHashMap<QualifiedTypeName, FxHashSet<QualifiedTypeName>>,
    /// Non-class concrete type → interfaces it implements.
    ///
    /// This is where top-level `implements I for int` lives. Class targets are
    /// still stored in `class_implements` so existing class-oriented callers do
    /// not need to special-case them.
    pub type_implements: FxHashMap<Ty, FxHashSet<QualifiedTypeName>>,
    /// (class QTN, interface QTN) → type args used in `implements I<...>`.
    /// Only populated for generic interfaces; non-generic implements entries
    /// are absent (meaning: no type args to check).
    pub implements_type_args: FxHashMap<(QualifiedTypeName, QualifiedTypeName), Vec<Ty>>,
    /// (non-class concrete type, interface QTN) → type args used in
    /// `implements I<...>`.
    pub type_implements_type_args: FxHashMap<(Ty, QualifiedTypeName), Vec<Ty>>,
    /// Interface QTN → interfaces it requires (transitively), including itself.
    /// Used for interface-to-interface subtyping: `A <: B` iff `B ∈ requires_closure[A]`.
    pub interface_requires: FxHashMap<QualifiedTypeName, FxHashSet<QualifiedTypeName>>,
}

impl ImplementsRegistry {
    /// True iff `class_qtn` nominally implements `iface_qtn`.
    ///
    /// Note: comparison is by `QualifiedTypeName` (package + namespace + name)
    /// so two interfaces with the same simple name in different namespaces
    /// don't accidentally match.
    pub fn implements(&self, class_qtn: &QualifiedTypeName, iface_qtn: &QualifiedTypeName) -> bool {
        self.class_implements
            .get(class_qtn)
            .is_some_and(|set| set.contains(iface_qtn))
    }

    pub fn type_implements(&self, ty: &Ty, iface_qtn: &QualifiedTypeName) -> bool {
        match ty {
            Ty::Class(class_qtn, _, _) => self.implements(class_qtn, iface_qtn),
            _ => implementation_key_for_ty(ty)
                .and_then(|key| self.type_implements.get(&key))
                .is_some_and(|set| set.contains(iface_qtn)),
        }
    }

    /// True iff interface `sub` requires interface `sup` (transitively).
    /// Used for interface-to-interface subtyping: `A <: B` iff `A requires B`.
    pub fn interface_requires(&self, sub: &QualifiedTypeName, sup: &QualifiedTypeName) -> bool {
        self.interface_requires
            .get(sub)
            .is_some_and(|set| set.contains(sup))
    }
}

pub fn implementation_key_for_ty(ty: &Ty) -> Option<Ty> {
    match ty {
        Ty::Primitive(primitive, _) => Some(Ty::Primitive(primitive.clone(), TyAttr::default())),
        Ty::Literal(literal, _, _) => Some(Ty::Primitive(
            PrimitiveType::from_literal(literal),
            TyAttr::default(),
        )),
        Ty::List(inner, _) => Some(Ty::List(
            Box::new(implementation_key_for_ty(inner)?),
            TyAttr::default(),
        )),
        Ty::Map(key, value, _) => Some(Ty::Map(
            Box::new(implementation_key_for_ty(key)?),
            Box::new(implementation_key_for_ty(value)?),
            TyAttr::default(),
        )),
        Ty::Optional(inner, _) => Some(Ty::Optional(
            Box::new(implementation_key_for_ty(inner)?),
            TyAttr::default(),
        )),
        Ty::Union(members, _) => Some(Ty::Union(
            members
                .iter()
                .map(implementation_key_for_ty)
                .collect::<Option<Vec<_>>>()?,
            TyAttr::default(),
        )),
        Ty::Class(..) | Ty::Interface(..) | Ty::Enum(..) | Ty::TypeAlias(..) => None,
        _ => None,
    }
}

/// Build the per-package implements registry.
///
/// Returns `(class_qtn → {iface_qtn})` covering every class in the package.
/// Empty for packages without interfaces; cheap to keep around as a Salsa
/// result.
#[salsa::tracked(returns(ref))]
pub fn package_implements_registry<'db>(
    db: &'db dyn crate::Db,
    pkg_id: PackageId<'db>,
) -> ImplementsRegistry {
    let pkg_items = baml_compiler2_ppir::package_items(db, pkg_id);
    let mut class_implements: FxHashMap<QualifiedTypeName, FxHashSet<QualifiedTypeName>> =
        FxHashMap::default();
    let mut type_implements: FxHashMap<Ty, FxHashSet<QualifiedTypeName>> = FxHashMap::default();
    let mut implements_type_args: FxHashMap<(QualifiedTypeName, QualifiedTypeName), Vec<Ty>> =
        FxHashMap::default();
    let mut type_implements_type_args: FxHashMap<(Ty, QualifiedTypeName), Vec<Ty>> =
        FxHashMap::default();
    // Multiple classes often implement the same interface (or an interface
    // higher up the extends chain). Cache the transitive closure per
    // `InterfaceLoc` so we only walk each chain once per query invocation.
    let mut closure_cache: FxHashMap<
        baml_compiler2_hir::loc::InterfaceLoc<'db>,
        FxHashSet<QualifiedTypeName>,
    > = FxHashMap::default();

    for ns_items in pkg_items.namespaces.values() {
        for def in ns_items.types.values() {
            let Definition::Class(class_loc) = def else {
                continue;
            };
            let hir_tree = baml_compiler2_hir::file_item_tree(db, class_loc.file(db));
            let Some(class_data) = hir_tree.classes.get(&class_loc.id(db)) else {
                continue;
            };
            let class_qtn = qualify_def(db, *def, &class_data.name);

            let mut implemented: FxHashSet<QualifiedTypeName> = FxHashSet::default();
            let class_ns = baml_compiler2_hir::file_package::file_package(db, class_loc.file(db))
                .namespace_path
                .clone();
            for target in &class_data.implements {
                let Some(iface_loc) =
                    resolve_path_to_interface(db, &target.target.expr, pkg_items, &class_ns)
                else {
                    continue;
                };
                // Store generic type args for invariant checking.
                if let baml_compiler2_ast::TypeExpr::Path { generic_args, .. } = &target.target.expr
                {
                    if !generic_args.is_empty() {
                        let iface_tree = baml_compiler2_hir::file_item_tree(db, iface_loc.file(db));
                        if let Some(iface_data) = iface_tree.interfaces.get(&iface_loc.id(db)) {
                            let iface_qtn =
                                qualify_def(db, Definition::Interface(iface_loc), &iface_data.name);
                            let mut diags = Vec::new();
                            let lowered: Vec<Ty> = generic_args
                                .iter()
                                .map(|ga| {
                                    crate::lower_type_expr::lower_type_expr_in_ns(
                                        db,
                                        ga,
                                        pkg_items,
                                        &class_ns,
                                        &class_data.generic_params,
                                        &mut diags,
                                    )
                                })
                                .collect();
                            implements_type_args.insert((class_qtn.clone(), iface_qtn), lowered);
                        }
                    }
                }
                let iface_tree = baml_compiler2_hir::file_item_tree(db, iface_loc.file(db));
                if let Some(iface_data) = iface_tree.interfaces.get(&iface_loc.id(db)) {
                    implemented.insert(qualify_def(
                        db,
                        Definition::Interface(iface_loc),
                        &iface_data.name,
                    ));
                }
            }

            class_implements.insert(class_qtn, implemented);
        }
    }

    for file in baml_compiler2_hir::compiler2_all_files(db) {
        let pkg_info = baml_compiler2_hir::file_package::file_package(db, file);
        if pkg_info.package != *pkg_id.name(db) {
            continue;
        }
        let item_tree = baml_compiler2_hir::file_item_tree(db, file);
        for imp in &item_tree.implements_for {
            let Some(iface_loc) = resolve_path_to_interface(
                db,
                &imp.interface_target.expr,
                pkg_items,
                &pkg_info.namespace_path,
            ) else {
                continue;
            };
            let iface_tree = baml_compiler2_hir::file_item_tree(db, iface_loc.file(db));
            let Some(iface_data) = iface_tree.interfaces.get(&iface_loc.id(db)) else {
                continue;
            };
            let iface_qtn = qualify_def(db, Definition::Interface(iface_loc), &iface_data.name);
            let mut diags = Vec::new();
            let target_ty = crate::lower_type_expr::lower_type_expr_in_ns(
                db,
                &imp.for_target.expr,
                pkg_items,
                &pkg_info.namespace_path,
                &[],
                &mut diags,
            );
            let Some(target_key) = implementation_key_for_ty(&target_ty) else {
                continue;
            };
            if let baml_compiler2_ast::TypeExpr::Path { generic_args, .. } =
                &imp.interface_target.expr
            {
                if !generic_args.is_empty() {
                    let lowered: Vec<Ty> = generic_args
                        .iter()
                        .map(|ga| {
                            crate::lower_type_expr::lower_type_expr_in_ns(
                                db,
                                ga,
                                pkg_items,
                                &pkg_info.namespace_path,
                                &[],
                                &mut diags,
                            )
                        })
                        .collect();
                    type_implements_type_args
                        .insert((target_key.clone(), iface_qtn.clone()), lowered);
                }
            }
            type_implements
                .entry(target_key)
                .or_default()
                .insert(iface_qtn);
        }
    }

    // Build the interface requires closure: for each interface, compute the
    // transitive set of interfaces it requires (including itself).
    let mut interface_requires: FxHashMap<QualifiedTypeName, FxHashSet<QualifiedTypeName>> =
        FxHashMap::default();
    for ns_items in pkg_items.namespaces.values() {
        for def in ns_items.types.values() {
            let Definition::Interface(iface_loc) = def else {
                continue;
            };
            let closure = interface_closure(db, *iface_loc, &mut closure_cache);
            let hir_tree = baml_compiler2_hir::file_item_tree(db, iface_loc.file(db));
            if let Some(iface_data) = hir_tree.interfaces.get(&iface_loc.id(db)) {
                let iface_qtn = qualify_def(db, *def, &iface_data.name);
                interface_requires.insert(iface_qtn, closure);
            }
        }
    }

    ImplementsRegistry {
        class_implements,
        type_implements,
        implements_type_args,
        type_implements_type_args,
        interface_requires,
    }
}

/// Resolve a `TypeExpr::Path` to an interface declaration. Returns `None`
/// when the path doesn't resolve to an interface in the package.
pub fn resolve_path_to_interface<'db>(
    db: &'db dyn crate::Db,
    target: &baml_compiler2_ast::TypeExpr,
    pkg_items: &baml_compiler2_hir::package::PackageItems<'db>,
    current_ns: &[Name],
) -> Option<baml_compiler2_hir::loc::InterfaceLoc<'db>> {
    use baml_compiler2_ast::TypeExpr;
    let TypeExpr::Path { segments, .. } = target else {
        return None;
    };
    let (head, name) = segments
        .split_last()
        .map(|(last, head)| (head, last.clone()))?;
    let lookup_ns: &[Name] = if head.is_empty() { current_ns } else { head };
    let _ = db;
    let Definition::Interface(loc) = pkg_items.lookup_type(lookup_ns, &name)? else {
        return None;
    };
    Some(loc)
}

/// Transitive `extends` closure for one interface, including itself. Result
/// is memoised in `cache` so callers that touch the same interface multiple
/// times don't re-walk.
fn interface_closure<'db>(
    db: &'db dyn crate::Db,
    iface_loc: baml_compiler2_hir::loc::InterfaceLoc<'db>,
    cache: &mut FxHashMap<baml_compiler2_hir::loc::InterfaceLoc<'db>, FxHashSet<QualifiedTypeName>>,
) -> FxHashSet<QualifiedTypeName> {
    if let Some(cached) = cache.get(&iface_loc) {
        return cached.clone();
    }
    let mut out: FxHashSet<QualifiedTypeName> = FxHashSet::default();
    let mut stack: Vec<baml_compiler2_hir::loc::InterfaceLoc<'db>> = vec![iface_loc];
    while let Some(loc) = stack.pop() {
        let tree = baml_compiler2_hir::file_item_tree(db, loc.file(db));
        let Some(iface) = tree.interfaces.get(&loc.id(db)) else {
            continue;
        };
        let qtn = qualify_def(db, Definition::Interface(loc), &iface.name);
        // Already-visited check guards cyclic `extends` (validated separately).
        if !out.insert(qtn) {
            continue;
        }
        let pkg_info = baml_compiler2_hir::file_package::file_package(db, loc.file(db));
        let pkg_id = PackageId::new(db, pkg_info.package.clone());
        let pkg_items = baml_compiler2_ppir::package_items(db, pkg_id);
        for parent in &iface.extends {
            if let Some(parent_loc) =
                resolve_path_to_interface(db, &parent.expr, pkg_items, &pkg_info.namespace_path)
            {
                stack.push(parent_loc);
            }
        }
    }
    cache.insert(iface_loc, out.clone());
    out
}

/// Walk the transitive `extends` closure of `root_iface` and return every
/// interface in it (including `root_iface` itself), in BFS order so the
/// receiver appears before its parents. Cycles are skipped silently — they
/// are reported elsewhere (E0118).
pub fn interface_closure_locs<'db>(
    db: &'db dyn crate::Db,
    root_iface: baml_compiler2_hir::loc::InterfaceLoc<'db>,
    pkg_items: &baml_compiler2_hir::package::PackageItems<'db>,
    current_ns: &[Name],
) -> Vec<baml_compiler2_hir::loc::InterfaceLoc<'db>> {
    let mut out: Vec<baml_compiler2_hir::loc::InterfaceLoc<'db>> = Vec::new();
    let mut seen: FxHashSet<baml_compiler2_hir::loc::InterfaceLoc<'db>> = FxHashSet::default();
    let mut queue: std::collections::VecDeque<baml_compiler2_hir::loc::InterfaceLoc<'db>> =
        std::collections::VecDeque::new();
    queue.push_back(root_iface);
    while let Some(loc) = queue.pop_front() {
        if !seen.insert(loc) {
            continue;
        }
        out.push(loc);
        let tree = baml_compiler2_hir::file_item_tree(db, loc.file(db));
        let Some(iface) = tree.interfaces.get(&loc.id(db)) else {
            continue;
        };
        for parent in &iface.extends {
            if let Some(parent_loc) =
                resolve_path_to_interface(db, &parent.expr, pkg_items, current_ns)
            {
                queue.push_back(parent_loc);
            }
        }
    }
    out
}
