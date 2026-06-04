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
    generics,
    lower_type_expr::qualify_def,
    normalize,
    ty::{FunctionParamTy, PrimitiveType, QualifiedTypeName, Ty},
};

pub type TypeBindings = FxHashMap<Name, Ty>;

/// Where an interface implementation rule came from.
///
/// This is intentionally small: semantic matching only needs the rule's TIR
/// types, while MIR can still recover methods from HIR by looking at the
/// original class or out-of-body `implements for` block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InterfaceImplOrigin {
    InBodyClass,
    OutOfBody,
}

/// A unified interface implementation rule.
///
/// Concrete, in-body generic, out-of-body generic class, and bounded type-var
/// implementations all lower to this single shape:
///
/// - `for_ty_pattern`: the implementor pattern, e.g. `Box<T>` or `T`.
/// - `interface_ty`: the implemented interface, e.g. `Printable` or
///   `Container<T>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InterfaceImplRule {
    pub generic_params: Vec<Name>,
    pub generic_param_bounds: Vec<Option<Ty>>,
    pub for_ty_pattern: Ty,
    pub interface_ty: Ty,
    pub origin: InterfaceImplOrigin,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InterfaceImplInstantiation {
    pub bindings: TypeBindings,
    pub for_ty: Ty,
    pub interface_ty: Ty,
}

/// For every class in a package, the set of interfaces it implements directly.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ImplementsRegistry {
    /// Canonical implementation rules. New interface semantics should be
    /// expressed in terms of these rules rather than the `class_implements`
    /// compatibility map below.
    pub interface_impl_rules: Vec<InterfaceImplRule>,
    /// Class QTN → interfaces it implements.
    pub class_implements: FxHashMap<QualifiedTypeName, FxHashSet<QualifiedTypeName>>,
    /// Interface QTN → interfaces it requires (transitively), including itself.
    /// Used for interface-to-interface subtyping: `A <: B` iff `B ∈ requires_closure[A]`.
    pub interface_requires: FxHashMap<QualifiedTypeName, FxHashSet<QualifiedTypeName>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedInterface<'db> {
    pub loc: baml_compiler2_hir::loc::InterfaceLoc<'db>,
    pub qtn: QualifiedTypeName,
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

    /// True iff interface `sub` requires interface `sup` (transitively).
    /// Used for interface-to-interface subtyping: `A <: B` iff `A requires B`.
    pub fn interface_requires(&self, sub: &QualifiedTypeName, sup: &QualifiedTypeName) -> bool {
        self.interface_requires
            .get(sub)
            .is_some_and(|set| set.contains(sup))
    }

    pub fn rule_matches_actual(
        &self,
        rule: &InterfaceImplRule,
        actual_ty: &Ty,
        requested_iface_ty: &Ty,
        aliases: &std::collections::HashMap<QualifiedTypeName, Ty>,
        is_subtype: impl FnMut(&Ty, &Ty) -> bool,
    ) -> Option<InterfaceImplInstantiation> {
        instantiate_rule_inner(
            rule,
            requested_iface_ty,
            Some(actual_ty),
            aliases,
            is_subtype,
            true,
        )
    }

    /// When `actual_ty` *almost* implements `requested_iface_ty` via a blanket
    /// rule — the receiver shape matches but a generic bound fails — return the
    /// first failing `(param, required_bound, actual_arg)`. Used to turn a bare
    /// "type mismatch" into a message naming the unsatisfied bound (wf3 #G18).
    pub fn first_failing_bound(
        &self,
        actual_ty: &Ty,
        requested_iface_ty: &Ty,
        aliases: &std::collections::HashMap<QualifiedTypeName, Ty>,
        mut is_subtype: impl FnMut(&Ty, &Ty) -> bool,
    ) -> Option<(Name, Ty, Ty)> {
        for rule in &self.interface_impl_rules {
            let mut bindings = TypeBindings::default();
            let ctx = MatchCtx::new(&rule.generic_params, aliases);
            if match_ty_pattern_into(&rule.for_ty_pattern, actual_ty, ctx, &mut bindings).is_none()
            {
                continue;
            }
            if match_ty_pattern_into(&rule.interface_ty, requested_iface_ty, ctx, &mut bindings)
                .is_none()
            {
                continue;
            }
            for (param, bound) in rule
                .generic_params
                .iter()
                .zip(rule.generic_param_bounds.iter())
            {
                let Some(bound) = bound else { continue };
                let Some(actual) = bindings.get(param) else {
                    continue;
                };
                let substituted_bound = generics::substitute_ty(bound, &bindings);
                if !is_subtype(actual, &substituted_bound) {
                    return Some((param.clone(), substituted_bound, actual.clone()));
                }
            }
        }
        None
    }

    pub fn type_implements_interface_via_rule(
        &self,
        actual_ty: &Ty,
        requested_iface_ty: &Ty,
        aliases: &std::collections::HashMap<QualifiedTypeName, Ty>,
        mut is_subtype: impl FnMut(&Ty, &Ty) -> bool,
    ) -> bool {
        self.interface_impl_rules.iter().any(|rule| {
            self.rule_matches_actual(
                rule,
                actual_ty,
                requested_iface_ty,
                aliases,
                &mut is_subtype,
            )
            .is_some()
        })
    }

    pub fn instantiate_rule_for_requested_interface(
        &self,
        rule: &InterfaceImplRule,
        requested_iface_ty: &Ty,
        candidate_ty: Option<&Ty>,
        aliases: &std::collections::HashMap<QualifiedTypeName, Ty>,
        is_subtype: impl FnMut(&Ty, &Ty) -> bool,
    ) -> Option<InterfaceImplInstantiation> {
        instantiate_rule_inner(
            rule,
            requested_iface_ty,
            candidate_ty,
            aliases,
            is_subtype,
            false,
        )
    }
}

/// Match `rule` against `requested_iface_ty` (and, when present, `candidate_ty`)
/// and build the resulting instantiation. `require_all_bindings` controls the
/// bound check for params left unbound (see [`validate_rule_bounds`]).
fn instantiate_rule_inner(
    rule: &InterfaceImplRule,
    requested_iface_ty: &Ty,
    candidate_ty: Option<&Ty>,
    aliases: &std::collections::HashMap<QualifiedTypeName, Ty>,
    mut is_subtype: impl FnMut(&Ty, &Ty) -> bool,
    require_all_bindings: bool,
) -> Option<InterfaceImplInstantiation> {
    let mut bindings = TypeBindings::default();
    let ctx = MatchCtx::new(&rule.generic_params, aliases);
    if let Some(candidate_ty) = candidate_ty {
        match_ty_pattern_into(&rule.for_ty_pattern, candidate_ty, ctx, &mut bindings)?;
    }
    match_ty_pattern_into(&rule.interface_ty, requested_iface_ty, ctx, &mut bindings)?;
    validate_rule_bounds(rule, &bindings, &mut is_subtype, require_all_bindings)?;
    let for_ty = generics::substitute_ty(&rule.for_ty_pattern, &bindings);
    let interface_ty = generics::substitute_ty(&rule.interface_ty, &bindings);
    Some(InterfaceImplInstantiation {
        bindings,
        for_ty,
        interface_ty,
    })
}

fn derive_compatibility_views(
    rules: &[InterfaceImplRule],
    all_class_qtns: &[QualifiedTypeName],
) -> FxHashMap<QualifiedTypeName, FxHashSet<QualifiedTypeName>> {
    let mut class_implements: FxHashMap<QualifiedTypeName, FxHashSet<QualifiedTypeName>> =
        FxHashMap::default();
    for class_qtn in all_class_qtns {
        class_implements.entry(class_qtn.clone()).or_default();
    }

    for rule in rules {
        let Ty::Interface(iface_qtn, _) = &rule.interface_ty else {
            continue;
        };
        let Ty::Class(class_qtn, class_args) = &rule.for_ty_pattern else {
            continue;
        };
        // Blanket `implements<T> I for Container<T>` rules dispatch via the rule
        // index, not the class_implements map.
        if matches!(rule.origin, InterfaceImplOrigin::OutOfBody)
            && class_args.iter().any(|arg| matches!(arg, Ty::TypeVar(..)))
        {
            continue;
        }
        class_implements
            .entry(class_qtn.clone())
            .or_default()
            .insert(iface_qtn.clone());
    }

    class_implements
}

fn lower_path_generic_args(
    db: &dyn crate::Db,
    expr: &baml_compiler2_ast::TypeExpr,
    pkg_items: &baml_compiler2_hir::package::PackageItems<'_>,
    namespace_path: &[Name],
    generic_params: &[Name],
    sink: crate::lower_type_expr::DiagSink<'_>,
) -> Vec<Ty> {
    let baml_compiler2_ast::TypeExpr::Path { generic_args, .. } = expr else {
        return Vec::new();
    };
    generic_args
        .iter()
        .map(|arg| {
            crate::lower_type_expr::lower_type_expr_in_ns_into(
                db,
                arg,
                pkg_items,
                namespace_path,
                generic_params,
                sink,
            )
        })
        .collect()
}

fn validate_rule_bounds(
    rule: &InterfaceImplRule,
    bindings: &TypeBindings,
    is_subtype: &mut impl FnMut(&Ty, &Ty) -> bool,
    require_all_bindings: bool,
) -> Option<()> {
    for (param, bound) in rule
        .generic_params
        .iter()
        .zip(rule.generic_param_bounds.iter())
    {
        let Some(bound) = bound else { continue };
        let Some(actual) = bindings.get(param) else {
            if require_all_bindings {
                return None;
            }
            continue;
        };
        let substituted_bound = generics::substitute_ty(bound, bindings);
        if !is_subtype(actual, &substituted_bound) {
            return None;
        }
    }
    Some(())
}

/// Read-only context threaded through the pattern-match cluster: the rule's
/// bound generic params and the package's type aliases. Bundled so the four
/// mutually-recursive matchers pass one `Copy` value instead of two refs.
#[derive(Clone, Copy)]
struct MatchCtx<'a> {
    generic_params: &'a [Name],
    aliases: &'a std::collections::HashMap<QualifiedTypeName, Ty>,
}

impl<'a> MatchCtx<'a> {
    fn new(
        generic_params: &'a [Name],
        aliases: &'a std::collections::HashMap<QualifiedTypeName, Ty>,
    ) -> Self {
        Self {
            generic_params,
            aliases,
        }
    }
}

pub fn match_ty_pattern(
    pattern: &Ty,
    concrete: &Ty,
    generic_params: &[Name],
    aliases: &std::collections::HashMap<QualifiedTypeName, Ty>,
) -> Option<TypeBindings> {
    let mut bindings = TypeBindings::default();
    match_ty_pattern_into(
        pattern,
        concrete,
        MatchCtx::new(generic_params, aliases),
        &mut bindings,
    )?;
    Some(bindings)
}

fn match_ty_pattern_into(
    pattern: &Ty,
    concrete: &Ty,
    ctx: MatchCtx,
    bindings: &mut TypeBindings,
) -> Option<()> {
    if let Ty::TypeVar(name) = pattern
        && ctx.generic_params.contains(name)
    {
        return bind_type_var(name, concrete, bindings, ctx.aliases);
    }

    // When the pattern has no bound type vars and neither side carries generic
    // function binders, two types match iff they normalize to the same type —
    // try that fast path first. If it's eligible but fails here, the structural
    // arms can't change the answer, so the fallback arm below must not re-run
    // the (expensive, alias-normalizing) check.
    let normalized_eligible = !contains_bound_typevar(pattern, ctx.generic_params)
        && !contains_generic_function_binders(pattern)
        && !contains_generic_function_binders(concrete);
    if normalized_eligible && normalize::is_same_normalized_type(pattern, concrete, ctx.aliases) {
        return Some(());
    }

    match (pattern, concrete) {
        (Ty::Class(p_qtn, p_args), Ty::Class(c_qtn, c_args))
        | (Ty::Interface(p_qtn, p_args), Ty::Interface(c_qtn, c_args))
            if p_qtn == c_qtn && p_args.len() == c_args.len() =>
        {
            for (p, c) in p_args.iter().zip(c_args.iter()) {
                match_ty_pattern_into(p, c, ctx, bindings)?;
            }
            Some(())
        }
        (Ty::List(p), Ty::List(c))
        | (Ty::EvolvingList(p), Ty::EvolvingList(c))
        | (Ty::Optional(p), Ty::Optional(c)) => match_ty_pattern_into(p, c, ctx, bindings),
        (Ty::Optional(p), Ty::Union(c_members)) => {
            let inner = union_members_without_null(c_members)?;
            match_ty_pattern_into(p, &inner, ctx, bindings)
        }
        (Ty::Map(pk, pv), Ty::Map(ck, cv)) | (Ty::EvolvingMap(pk, pv), Ty::EvolvingMap(ck, cv)) => {
            match_ty_pattern_into(pk, ck, ctx, bindings)?;
            match_ty_pattern_into(pv, cv, ctx, bindings)
        }
        (Ty::Future(pv, pe), Ty::Future(cv, ce)) => {
            match_ty_pattern_into(pv, cv, ctx, bindings)?;
            match_ty_pattern_into(pe, ce, ctx, bindings)
        }
        (Ty::Union(p_members), Ty::Union(c_members)) if p_members.len() == c_members.len() => {
            match_union_members(p_members, c_members, ctx, bindings)
        }
        (Ty::Primitive(primitive), Ty::Literal(literal, _))
            if PrimitiveType::from_literal(literal) == *primitive =>
        {
            Some(())
        }
        (
            Ty::Function {
                generic_params: p_generic_params,
                generic_param_bounds: p_generic_param_bounds,
                params: p_params,
                ret: p_ret,
                throws: p_throws,
                ..
            },
            Ty::Function {
                generic_params: c_generic_params,
                generic_param_bounds: c_generic_param_bounds,
                params: c_params,
                ret: c_ret,
                throws: c_throws,
                ..
            },
        ) if p_params.len() == c_params.len()
            && p_params
                .iter()
                .zip(c_params.iter())
                .all(|(p, c)| p.mode == c.mode) =>
        {
            let canonical_params = match_function_generic_bounds(
                p_generic_params,
                p_generic_param_bounds,
                c_generic_params,
                c_generic_param_bounds,
                ctx,
                bindings,
            )?;
            let p_function_bindings =
                function_generic_bindings(p_generic_params, &canonical_params);
            let c_function_bindings =
                function_generic_bindings(c_generic_params, &canonical_params);

            for (p, c) in p_params.iter().zip(c_params.iter()) {
                let p_ty = generics::substitute_ty(&p.ty, &p_function_bindings);
                let c_ty = generics::substitute_ty(&c.ty, &c_function_bindings);
                match_ty_pattern_into(&p_ty, &c_ty, ctx, bindings)?;
            }
            let p_ret = generics::substitute_ty(p_ret, &p_function_bindings);
            let c_ret = generics::substitute_ty(c_ret, &c_function_bindings);
            match_ty_pattern_into(&p_ret, &c_ret, ctx, bindings)?;
            let p_throws = generics::substitute_ty(p_throws, &p_function_bindings);
            let c_throws = generics::substitute_ty(c_throws, &c_function_bindings);
            match_ty_pattern_into(&p_throws, &c_throws, ctx, bindings)
        }
        // Only re-check normalized equality when the fast path above was skipped
        // (a guard failed); if it was eligible it already returned false.
        _ if !normalized_eligible
            && normalize::is_same_normalized_type(pattern, concrete, ctx.aliases) =>
        {
            Some(())
        }
        _ => None,
    }
}

fn match_function_generic_bounds(
    pattern_params: &[Name],
    pattern_bounds: &[Option<Ty>],
    concrete_params: &[Name],
    concrete_bounds: &[Option<Ty>],
    ctx: MatchCtx,
    bindings: &mut TypeBindings,
) -> Option<Vec<Name>> {
    if pattern_params.len() != concrete_params.len() {
        return None;
    }

    let canonical_params = canonical_function_generic_params(pattern_params.len());
    let pattern_bindings = function_generic_bindings(pattern_params, &canonical_params);
    let concrete_bindings = function_generic_bindings(concrete_params, &canonical_params);

    for idx in 0..pattern_params.len() {
        match (
            pattern_bounds.get(idx).and_then(Option::as_ref),
            concrete_bounds.get(idx).and_then(Option::as_ref),
        ) {
            (None, None) => {}
            (Some(_), None) | (None, Some(_)) => return None,
            (Some(pattern_bound), Some(concrete_bound)) => {
                let pattern_bound = generics::substitute_ty(pattern_bound, &pattern_bindings);
                let concrete_bound = generics::substitute_ty(concrete_bound, &concrete_bindings);
                match_ty_pattern_into(&pattern_bound, &concrete_bound, ctx, bindings)?;
            }
        }
    }

    Some(canonical_params)
}

fn canonical_function_generic_params(len: usize) -> Vec<Name> {
    (0..len)
        .map(|idx| Name::new(format!("__fn_generic_{idx}")))
        .collect()
}

fn function_generic_bindings(params: &[Name], canonical_params: &[Name]) -> TypeBindings {
    params
        .iter()
        .zip(canonical_params.iter())
        .map(|(param, canonical)| (param.clone(), Ty::TypeVar(canonical.clone())))
        .collect()
}

fn match_union_members(
    pattern_members: &[Ty],
    concrete_members: &[Ty],
    ctx: MatchCtx,
    bindings: &mut TypeBindings,
) -> Option<()> {
    let Some((pattern_head, pattern_tail)) = pattern_members.split_first() else {
        return concrete_members.is_empty().then_some(());
    };

    for idx in 0..concrete_members.len() {
        let mut trial_bindings = bindings.clone();
        if match_ty_pattern_into(
            pattern_head,
            &concrete_members[idx],
            ctx,
            &mut trial_bindings,
        )
        .is_none()
        {
            continue;
        }

        let remaining = concrete_members
            .iter()
            .enumerate()
            .filter(|(member_idx, _)| *member_idx != idx)
            .map(|(_, member)| member.clone())
            .collect::<Vec<_>>();
        if match_union_members(pattern_tail, &remaining, ctx, &mut trial_bindings).is_some() {
            *bindings = trial_bindings;
            return Some(());
        }
    }

    None
}

fn union_members_without_null(members: &[Ty]) -> Option<Ty> {
    let mut non_null = Vec::new();
    let mut saw_null = false;
    for member in members {
        if matches!(member, Ty::Primitive(PrimitiveType::Null)) {
            saw_null = true;
        } else {
            non_null.push(member.clone());
        }
    }
    if !saw_null || non_null.is_empty() {
        return None;
    }
    if non_null.len() == 1 {
        non_null.into_iter().next()
    } else {
        Some(Ty::Union(non_null))
    }
}

fn bind_type_var(
    name: &Name,
    concrete: &Ty,
    bindings: &mut TypeBindings,
    aliases: &std::collections::HashMap<QualifiedTypeName, Ty>,
) -> Option<()> {
    match bindings.get(name) {
        Some(existing) if normalize::is_same_normalized_type(existing, concrete, aliases) => {
            Some(())
        }
        Some(_) => None,
        None => {
            bindings.insert(name.clone(), concrete.clone());
            Some(())
        }
    }
}

fn contains_bound_typevar(ty: &Ty, generic_params: &[Name]) -> bool {
    match ty {
        Ty::TypeVar(name) => generic_params.contains(name),
        Ty::Class(_, args) | Ty::Interface(_, args) | Ty::Union(args) => args
            .iter()
            .any(|arg| contains_bound_typevar(arg, generic_params)),
        Ty::List(inner) | Ty::EvolvingList(inner) | Ty::Optional(inner) => {
            contains_bound_typevar(inner, generic_params)
        }
        Ty::Map(k, v) | Ty::EvolvingMap(k, v) | Ty::Future(k, v) => {
            contains_bound_typevar(k, generic_params) || contains_bound_typevar(v, generic_params)
        }
        Ty::Function {
            generic_param_bounds,
            params,
            ret,
            throws,
            ..
        } => {
            generic_param_bounds.iter().any(|bound| {
                bound
                    .as_ref()
                    .is_some_and(|bound| contains_bound_typevar(bound, generic_params))
            }) || params
                .iter()
                .any(|FunctionParamTy { ty, .. }| contains_bound_typevar(ty, generic_params))
                || contains_bound_typevar(ret, generic_params)
                || contains_bound_typevar(throws, generic_params)
        }
        _ => false,
    }
}

fn contains_generic_function_binders(ty: &Ty) -> bool {
    match ty {
        Ty::Class(_, args) | Ty::Interface(_, args) | Ty::Union(args) => {
            args.iter().any(contains_generic_function_binders)
        }
        Ty::List(inner) | Ty::EvolvingList(inner) | Ty::Optional(inner) => {
            contains_generic_function_binders(inner)
        }
        Ty::Map(k, v) | Ty::EvolvingMap(k, v) | Ty::Future(k, v) => {
            contains_generic_function_binders(k) || contains_generic_function_binders(v)
        }
        Ty::Function {
            generic_params,
            generic_param_bounds,
            params,
            ret,
            throws,
            ..
        } => {
            !generic_params.is_empty()
                || generic_param_bounds.iter().any(|bound| {
                    bound
                        .as_ref()
                        .is_some_and(contains_generic_function_binders)
                })
                || params
                    .iter()
                    .any(|FunctionParamTy { ty, .. }| contains_generic_function_binders(ty))
                || contains_generic_function_binders(ret)
                || contains_generic_function_binders(throws)
        }
        _ => false,
    }
}

pub fn implementation_key_for_ty(ty: &Ty) -> Option<Ty> {
    match ty {
        Ty::Primitive(primitive) => Some(Ty::Primitive(primitive.clone())),
        Ty::Literal(literal, _) => Some(Ty::Primitive(PrimitiveType::from_literal(literal))),
        Ty::List(inner) => Some(Ty::List(Box::new(implementation_key_for_ty(inner)?))),
        Ty::Map(key, value) => Some(Ty::Map(
            Box::new(implementation_key_for_ty(key)?),
            Box::new(implementation_key_for_ty(value)?),
        )),
        Ty::Optional(inner) => Some(Ty::Optional(Box::new(implementation_key_for_ty(inner)?))),
        Ty::Union(members) => {
            let mut keys = members
                .iter()
                .map(implementation_key_for_ty)
                .collect::<Option<Vec<_>>>()?;
            keys.sort();
            keys.dedup();
            Some(Ty::Union(keys))
        }
        // Class/Interface/Enum/TypeAlias and all other constructors are unkeyable.
        _ => None,
    }
}

/// Build the per-package implements registry.
///
/// Returns the full [`ImplementsRegistry`] (rules, index, and requires closure)
/// for every class in the package. Empty for packages without interfaces; cheap
/// to keep around as a Salsa result.
#[salsa::tracked(returns(ref))]
pub fn package_implements_registry<'db>(
    db: &'db dyn crate::Db,
    pkg_id: PackageId<'db>,
) -> ImplementsRegistry {
    let pkg_items = baml_compiler2_ppir::package_items(db, pkg_id);
    let mut interface_impl_rules: Vec<InterfaceImplRule> = Vec::new();
    let mut all_class_qtns: Vec<QualifiedTypeName> = Vec::new();
    // Multiple classes often implement the same interface (or an interface
    // higher up the requires chain). Cache the transitive closure per
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
            all_class_qtns.push(class_qtn.clone());

            let class_ns = baml_compiler2_hir::file_package::file_package(db, class_loc.file(db))
                .namespace_path
                .clone();
            for target in &class_data.implements {
                let Some(iface_loc) =
                    resolve_path_to_interface(db, &target.target.expr, pkg_items, &class_ns)
                else {
                    continue;
                };
                let iface_tree = baml_compiler2_hir::file_item_tree(db, iface_loc.file(db));
                if let Some(iface_data) = iface_tree.interfaces.get(&iface_loc.id(db)) {
                    let iface_qtn =
                        qualify_def(db, Definition::Interface(iface_loc), &iface_data.name);
                    let interface_args = lower_path_generic_args(
                        db,
                        &target.target.expr,
                        pkg_items,
                        &class_ns,
                        &class_data.generic_params,
                        &mut |_diag| {},
                    );
                    let for_ty_pattern = Ty::Class(
                        class_qtn.clone(),
                        class_data
                            .generic_params
                            .iter()
                            .map(|param| Ty::TypeVar(param.clone()))
                            .collect(),
                    );
                    interface_impl_rules.push(InterfaceImplRule {
                        generic_params: class_data.generic_params.clone(),
                        generic_param_bounds: vec![None; class_data.generic_params.len()],
                        for_ty_pattern,
                        interface_ty: Ty::Interface(iface_qtn.clone(), interface_args),
                        origin: InterfaceImplOrigin::InBodyClass,
                    });
                }
            }
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
            let target_ty = crate::lower_type_expr::lower_type_expr_in_ns_into(
                db,
                &imp.for_target.expr,
                pkg_items,
                &pkg_info.namespace_path,
                &imp.generic_params,
                &mut |_diag| {},
            );
            let interface_args = lower_path_generic_args(
                db,
                &imp.interface_target.expr,
                pkg_items,
                &pkg_info.namespace_path,
                &imp.generic_params,
                &mut |_diag| {},
            );
            let interface_ty = Ty::Interface(iface_qtn.clone(), interface_args.clone());
            let bounds: Vec<Option<Ty>> = imp
                .generic_param_bounds
                .iter()
                .map(|b| {
                    b.as_ref().map(|te| {
                        crate::lower_type_expr::lower_type_expr_in_ns_into(
                            db,
                            te,
                            pkg_items,
                            &pkg_info.namespace_path,
                            &imp.generic_params,
                            &mut |_diag| {},
                        )
                    })
                })
                .collect();
            interface_impl_rules.push(InterfaceImplRule {
                generic_params: imp.generic_params.clone(),
                generic_param_bounds: bounds.clone(),
                for_ty_pattern: target_ty.clone(),
                interface_ty,
                origin: InterfaceImplOrigin::OutOfBody,
            });
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

    let class_implements = derive_compatibility_views(&interface_impl_rules, &all_class_qtns);

    ImplementsRegistry {
        interface_impl_rules,
        class_implements,
        interface_requires,
    }
}

/// Resolve a `TypeExpr::Path` to an interface declaration and its fully
/// qualified identity. Returns `None` when the path doesn't resolve to an
/// interface.
pub fn resolve_path_to_interface_identity<'db>(
    db: &'db dyn crate::Db,
    target: &baml_compiler2_ast::TypeExpr,
    pkg_items: &baml_compiler2_hir::package::PackageItems<'db>,
    current_ns: &[Name],
) -> Option<ResolvedInterface<'db>> {
    let Ty::Interface(qtn, _) = crate::lower_type_expr::lower_type_expr_in_ns_into(
        db,
        target,
        pkg_items,
        current_ns,
        &[],
        &mut |_diag| {},
    ) else {
        return None;
    };
    let pkg_id = PackageId::new(db, qtn.package().clone());
    let resolved_pkg_items = baml_compiler2_ppir::package_items(db, pkg_id);
    let Definition::Interface(loc) = resolved_pkg_items.lookup_type(qtn.namespace(), qtn.name())?
    else {
        return None;
    };
    Some(ResolvedInterface { loc, qtn })
}

/// Resolve a `TypeExpr::Path` to an interface declaration. Returns `None`
/// when the path doesn't resolve to an interface.
pub fn resolve_path_to_interface<'db>(
    db: &'db dyn crate::Db,
    target: &baml_compiler2_ast::TypeExpr,
    pkg_items: &baml_compiler2_hir::package::PackageItems<'db>,
    current_ns: &[Name],
) -> Option<baml_compiler2_hir::loc::InterfaceLoc<'db>> {
    resolve_path_to_interface_identity(db, target, pkg_items, current_ns)
        .map(|resolved| resolved.loc)
}

/// Transitive `requires` closure for one interface, including itself. Result
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
        // Already-visited check guards cyclic `requires` (validated separately).
        if !out.insert(qtn) {
            continue;
        }
        let pkg_info = baml_compiler2_hir::file_package::file_package(db, loc.file(db));
        let pkg_id = PackageId::new(db, pkg_info.package.clone());
        let pkg_items = baml_compiler2_ppir::package_items(db, pkg_id);
        for parent in &iface.requires {
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

/// Walk the transitive `requires` closure of `root_iface` and return every
/// interface in it (including `root_iface` itself), in BFS order so the
/// receiver appears before its parents. Cycles are skipped silently — they
/// are reported elsewhere (E0118).
pub fn interface_closure_locs<'db>(
    db: &'db dyn crate::Db,
    root_iface: baml_compiler2_hir::loc::InterfaceLoc<'db>,
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
        let pkg_info = baml_compiler2_hir::file_package::file_package(db, loc.file(db));
        let pkg_id = PackageId::new(db, pkg_info.package.clone());
        let parent_pkg_items = baml_compiler2_ppir::package_items(db, pkg_id);
        for parent in &iface.requires {
            if let Some(parent_loc) = resolve_path_to_interface(
                db,
                &parent.expr,
                parent_pkg_items,
                &pkg_info.namespace_path,
            ) {
                queue.push_back(parent_loc);
            }
        }
    }
    out
}

/// Walk the transitive `requires` closure of `root_iface`, carrying the concrete
/// generic arguments for each interface in the closure. For example,
/// `Child<int> requires Parent<T>` yields `(Child, [int])` and `(Parent, [int])`.
pub fn interface_closure_locs_with_args<'db>(
    db: &'db dyn crate::Db,
    root_iface: baml_compiler2_hir::loc::InterfaceLoc<'db>,
    root_args: &[Ty],
) -> Vec<(baml_compiler2_hir::loc::InterfaceLoc<'db>, Vec<Ty>)> {
    let mut out: Vec<(baml_compiler2_hir::loc::InterfaceLoc<'db>, Vec<Ty>)> = Vec::new();
    let mut seen: FxHashSet<(baml_compiler2_hir::loc::InterfaceLoc<'db>, Vec<Ty>)> =
        FxHashSet::default();
    let mut queue: std::collections::VecDeque<(
        baml_compiler2_hir::loc::InterfaceLoc<'db>,
        Vec<Ty>,
        FxHashSet<baml_compiler2_hir::loc::InterfaceLoc<'db>>,
    )> = std::collections::VecDeque::new();
    queue.push_back((root_iface, root_args.to_vec(), FxHashSet::default()));

    while let Some((loc, args, ancestors)) = queue.pop_front() {
        if ancestors.contains(&loc) {
            continue;
        }
        if !seen.insert((loc, args.clone())) {
            continue;
        }
        out.push((loc, args.clone()));
        let mut child_ancestors = ancestors.clone();
        child_ancestors.insert(loc);

        let tree = baml_compiler2_hir::file_item_tree(db, loc.file(db));
        let Some(iface) = tree.interfaces.get(&loc.id(db)) else {
            continue;
        };
        let pkg_info = baml_compiler2_hir::file_package::file_package(db, loc.file(db));
        let pkg_id = PackageId::new(db, pkg_info.package.clone());
        let parent_pkg_items = baml_compiler2_ppir::package_items(db, pkg_id);
        let bindings = generics::bind_type_vars(&iface.generic_params, &args);

        for parent in &iface.requires {
            let Some(parent_loc) = resolve_path_to_interface(
                db,
                &parent.expr,
                parent_pkg_items,
                &pkg_info.namespace_path,
            ) else {
                continue;
            };
            let parent_args = match &parent.expr {
                baml_compiler2_ast::TypeExpr::Path { generic_args, .. } => generic_args
                    .iter()
                    .map(|arg| {
                        generics::lower_type_expr_with_generics_into(
                            db,
                            arg,
                            parent_pkg_items,
                            &pkg_info.namespace_path,
                            &bindings,
                            &mut |_diag| {},
                        )
                    })
                    .collect(),
                _ => Vec::new(),
            };
            queue.push_back((parent_loc, parent_args, child_ancestors.clone()));
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn qtn(namespace: &[&str], name: &str) -> QualifiedTypeName {
        QualifiedTypeName::new(
            Name::new("user"),
            namespace.iter().map(|part| Name::new(*part)).collect(),
            Name::new(name),
        )
    }

    fn class(namespace: &[&str], name: &str, args: Vec<Ty>) -> Ty {
        Ty::Class(qtn(namespace, name), args)
    }

    fn interface(name: &str, args: Vec<Ty>) -> Ty {
        Ty::Interface(qtn(&[], name), args)
    }

    fn int() -> Ty {
        Ty::Primitive(PrimitiveType::Int)
    }

    fn string() -> Ty {
        Ty::Primitive(PrimitiveType::String)
    }

    fn type_var(name: &str) -> Ty {
        Ty::TypeVar(Name::new(name))
    }

    fn never() -> Ty {
        Ty::Never
    }

    fn function(
        generic_params: Vec<&str>,
        generic_param_bounds: Vec<Option<Ty>>,
        params: Vec<Ty>,
        ret: Ty,
    ) -> Ty {
        Ty::Function {
            generic_params: generic_params.into_iter().map(Name::new).collect(),
            generic_param_bounds,
            params: params
                .into_iter()
                .map(|ty| FunctionParamTy {
                    name: None,
                    ty,
                    mode: crate::ty::FunctionParamMode::Required,
                })
                .collect(),
            ret: Box::new(ret),
            throws: Box::new(never()),
        }
    }

    #[test]
    fn implementation_key_for_ty_canonicalizes_union_members() {
        let int = Ty::Primitive(PrimitiveType::Int);
        let string = Ty::Primitive(PrimitiveType::String);
        let lhs = Ty::Union(vec![int.clone(), string.clone()]);
        let rhs = Ty::Union(vec![string, int]);

        assert_eq!(
            implementation_key_for_ty(&lhs),
            implementation_key_for_ty(&rhs)
        );
    }

    #[test]
    fn implementation_key_for_ty_dedupes_union_members() {
        let int = Ty::Primitive(PrimitiveType::Int);
        let duplicated = Ty::Union(vec![int.clone(), int.clone()]);

        assert_eq!(
            implementation_key_for_ty(&duplicated),
            Some(Ty::Union(vec![int]))
        );
    }

    #[test]
    fn match_ty_pattern_rejects_repeated_type_var_conflict() {
        let pattern = class(&[], "Pair", vec![type_var("T"), type_var("T")]);
        let good = class(&[], "Pair", vec![int(), int()]);
        let bad = class(&[], "Pair", vec![int(), string()]);
        let params = vec![Name::new("T")];

        assert!(
            match_ty_pattern(
                &pattern,
                &good,
                &params,
                &std::collections::HashMap::default()
            )
            .is_some()
        );
        assert!(
            match_ty_pattern(
                &pattern,
                &bad,
                &params,
                &std::collections::HashMap::default()
            )
            .is_none()
        );
    }

    #[test]
    fn match_ty_pattern_handles_nested_interface_args() {
        let pattern = interface("Container", vec![Ty::List(Box::new(type_var("T")))]);
        let actual = interface("Container", vec![Ty::List(Box::new(int()))]);
        let params = vec![Name::new("T")];

        let bindings = match_ty_pattern(
            &pattern,
            &actual,
            &params,
            &std::collections::HashMap::default(),
        )
        .expect("nested list arg should bind T");
        assert_eq!(bindings.get(&Name::new("T")), Some(&int()));
    }

    #[test]
    fn match_ty_pattern_rejects_function_generic_count_mismatch() {
        let pattern = function(vec!["T"], vec![None], vec![int()], int());
        let actual = function(vec![], vec![], vec![int()], int());

        assert!(
            match_ty_pattern(
                &pattern,
                &actual,
                &[],
                &std::collections::HashMap::default()
            )
            .is_none()
        );
    }

    #[test]
    fn match_ty_pattern_rejects_function_generic_bound_mismatch() {
        let pattern = function(
            vec!["T"],
            vec![Some(interface("Readable", vec![]))],
            vec![int()],
            int(),
        );
        let actual = function(
            vec!["U"],
            vec![Some(interface("Writable", vec![]))],
            vec![int()],
            int(),
        );

        assert!(
            match_ty_pattern(
                &pattern,
                &actual,
                &[],
                &std::collections::HashMap::default()
            )
            .is_none()
        );
    }

    #[test]
    fn match_ty_pattern_uses_full_qualified_type_names() {
        let pattern = class(&["alpha"], "Thing", vec![]);
        let same_short_name = class(&["beta"], "Thing", vec![]);

        assert!(
            match_ty_pattern(
                &pattern,
                &same_short_name,
                &[],
                &std::collections::HashMap::default()
            )
            .is_none(),
            "same short name in different namespaces must not match"
        );
    }

    #[test]
    fn match_ty_pattern_unions_are_order_insensitive_with_bindings() {
        let pattern = Ty::Union(vec![type_var("T"), string()]);
        let actual = Ty::Union(vec![string(), int()]);
        let params = vec![Name::new("T")];

        let bindings = match_ty_pattern(
            &pattern,
            &actual,
            &params,
            &std::collections::HashMap::default(),
        )
        .expect("union members should be matched by type, not position");
        assert_eq!(bindings.get(&Name::new("T")), Some(&int()));
    }

    #[test]
    fn rule_matches_actual_rejects_conflicting_interface_binding() {
        let registry = ImplementsRegistry {
            interface_impl_rules: Vec::new(),
            class_implements: FxHashMap::default(),
            interface_requires: FxHashMap::default(),
        };
        let rule = InterfaceImplRule {
            generic_params: vec![Name::new("T")],
            generic_param_bounds: vec![None],
            for_ty_pattern: class(&[], "Wrapper", vec![type_var("T")]),
            interface_ty: interface("Container", vec![type_var("T")]),
            origin: InterfaceImplOrigin::OutOfBody,
        };
        let actual = class(&[], "Wrapper", vec![int()]);
        let requested = interface("Container", vec![string()]);

        assert!(
            registry
                .rule_matches_actual(
                    &rule,
                    &actual,
                    &requested,
                    &std::collections::HashMap::default(),
                    |_, _| true,
                )
                .is_none()
        );
    }
}
