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

use baml_base::{Literal, Name, Span};
use baml_compiler2_hir::{contributions::Definition, package::PackageId};
use rustc_hash::{FxHashMap, FxHashSet};

use crate::{
    generics,
    lower_type_expr::qualify_def,
    normalize,
    ty::{FunctionParamTy, QualifiedTypeName, Ty, TyAttr},
};

pub type TypeBindings = FxHashMap<Name, Ty>;
pub type AssociatedBindings = Vec<(Name, Ty)>;
pub type InterfaceClosureEntry<'db> = (
    baml_compiler2_hir::loc::InterfaceLoc<'db>,
    Vec<Ty>,
    AssociatedBindings,
);
type InterfaceClosureQueueEntry<'db> = (
    baml_compiler2_hir::loc::InterfaceLoc<'db>,
    Vec<Ty>,
    AssociatedBindings,
    FxHashSet<baml_compiler2_hir::loc::InterfaceLoc<'db>>,
);

#[derive(Clone, Copy)]
struct InterfaceTypeAssocLowering<'a, 'db> {
    db: &'db dyn crate::Db,
    iface: &'a baml_compiler2_hir::item_tree::Interface,
    interface_args: &'a [Ty],
    explicit_associated_bindings: &'a [baml_compiler2_ast::AssociatedTypeBinding],
    iface_pkg_items: &'a baml_compiler2_hir::package::PackageItems<'db>,
    binding_pkg_items: &'a baml_compiler2_hir::package::PackageItems<'db>,
    iface_namespace_path: &'a [Name],
    binding_namespace_path: &'a [Name],
    outer_bindings: &'a TypeBindings,
}

/// Where an interface implementation rule came from.
///
/// This is intentionally small: semantic matching only needs the rule's TIR
/// types, while MIR can still recover methods from HIR by looking at the
/// original class or out-of-body `implements for` block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InterfaceImplOrigin {
    InBodyClass { class_qtn: QualifiedTypeName },
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
    /// Source location of the impl, used to attribute coherence diagnostics.
    /// `None` for rules synthesized during lowering, which have no source text.
    pub source_span: Option<Span>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InterfaceImplInstantiation {
    pub bindings: TypeBindings,
    pub for_ty: Ty,
    pub interface_ty: Ty,
}

/// Compatibility view for old callers while rule-based matching is being
/// plumbed through the compiler.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlanketClassImpl {
    pub class_qtn: QualifiedTypeName,
    pub generic_params: Vec<Name>,
    pub generic_param_bounds: Vec<Option<Ty>>,
    pub interface_qtn: QualifiedTypeName,
    pub interface_type_args: Vec<Ty>,
    pub for_target_ty: Ty,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InterfaceImplRuleIndex {
    /// Interface QTN -> all rules that can possibly satisfy that interface.
    pub by_interface: FxHashMap<QualifiedTypeName, Vec<usize>>,
    /// Interface QTN -> class QTN -> rules with a class-shaped `for` pattern.
    pub by_class: FxHashMap<QualifiedTypeName, FxHashMap<QualifiedTypeName, Vec<usize>>>,
    /// Interface QTN -> canonical non-class type key -> matching rules.
    pub by_type: FxHashMap<QualifiedTypeName, FxHashMap<Ty, Vec<usize>>>,
    /// Interface QTN -> rules whose implementor pattern is too open to key.
    pub fallback_by_interface: FxHashMap<QualifiedTypeName, Vec<usize>>,
}

impl InterfaceImplRuleIndex {
    fn from_rules(rules: &[InterfaceImplRule]) -> Self {
        let mut index = Self::default();
        for (idx, rule) in rules.iter().enumerate() {
            let Some(iface_qtn) = interface_qtn(&rule.interface_ty) else {
                continue;
            };
            index
                .by_interface
                .entry(iface_qtn.clone())
                .or_default()
                .push(idx);

            match &rule.for_ty_pattern {
                Ty::Class(class_qtn, _, _) => {
                    index
                        .by_class
                        .entry(iface_qtn.clone())
                        .or_default()
                        .entry(class_qtn.clone())
                        .or_default()
                        .push(idx);
                }
                ty => {
                    if let Some(key) = implementation_key_for_ty(ty) {
                        index
                            .by_type
                            .entry(iface_qtn.clone())
                            .or_default()
                            .entry(key)
                            .or_default()
                            .push(idx);
                    } else {
                        index
                            .fallback_by_interface
                            .entry(iface_qtn.clone())
                            .or_default()
                            .push(idx);
                    }
                }
            }
        }
        index
    }
}

/// For every class in a package, the set of interfaces it implements directly.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ImplementsRegistry {
    /// Canonical implementation rules. New interface semantics should be
    /// expressed in terms of these rules rather than the compatibility maps
    /// below.
    pub interface_impl_rules: Vec<InterfaceImplRule>,
    /// Lookup tables for `interface_impl_rules`, used by subtype checks to
    /// avoid probing unrelated implementation rules.
    pub interface_impl_rule_index: InterfaceImplRuleIndex,
    /// Class QTN → interfaces it implements.
    pub class_implements: FxHashMap<QualifiedTypeName, FxHashSet<QualifiedTypeName>>,
    /// Non-class concrete type → interfaces it implements.
    ///
    /// This is where top-level `implements I for int` lives. Class targets are
    /// still stored in `class_implements` so existing class-oriented callers do
    /// not need to special-case them.
    pub type_implements: FxHashMap<Ty, FxHashSet<QualifiedTypeName>>,
    /// Blanket class implementations: `implements<T> I for Container<T>`.
    /// These cannot be keyed by a single `Ty` since the for-target contains
    /// type variables; dispatch checks them by class QTN + arity.
    pub blanket_class_implements: Vec<BlanketClassImpl>,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedInterface<'db> {
    pub loc: baml_compiler2_hir::loc::InterfaceLoc<'db>,
    pub qtn: QualifiedTypeName,
}

#[derive(Debug, Default)]
struct RegistryCompatibilityViews {
    class_implements: FxHashMap<QualifiedTypeName, FxHashSet<QualifiedTypeName>>,
    type_implements: FxHashMap<Ty, FxHashSet<QualifiedTypeName>>,
    blanket_class_implements: Vec<BlanketClassImpl>,
    implements_type_args: FxHashMap<(QualifiedTypeName, QualifiedTypeName), Vec<Ty>>,
    type_implements_type_args: FxHashMap<(Ty, QualifiedTypeName), Vec<Ty>>,
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
        let aliases = std::collections::HashMap::default();
        self.type_implements_qtn_via_rule(ty, iface_qtn, &aliases, |actual, bound| {
            self.compatibility_subtype(actual, bound)
        })
    }

    /// True iff interface `sub` requires interface `sup` (transitively).
    /// Used for interface-to-interface subtyping: `A <: B` iff `A requires B`.
    pub fn interface_requires(&self, sub: &QualifiedTypeName, sup: &QualifiedTypeName) -> bool {
        self.interface_requires
            .get(sub)
            .is_some_and(|set| set.contains(sup))
    }

    fn type_implements_qtn_via_rule(
        &self,
        actual_ty: &Ty,
        iface_qtn: &QualifiedTypeName,
        aliases: &std::collections::HashMap<QualifiedTypeName, Ty>,
        mut is_subtype: impl FnMut(&Ty, &Ty) -> bool,
    ) -> bool {
        self.interface_impl_rule_index
            .by_interface
            .get(iface_qtn)
            .is_some_and(|indices| {
                indices.iter().any(|idx| {
                    let Some(rule) = self.interface_impl_rules.get(*idx) else {
                        return false;
                    };
                    let mut bindings = TypeBindings::default();
                    match_ty_pattern_into(
                        &rule.for_ty_pattern,
                        actual_ty,
                        &rule.generic_params,
                        aliases,
                        &mut bindings,
                    )
                    .is_some()
                        && validate_rule_bounds(rule, &bindings, &mut is_subtype, true).is_some()
                })
            })
    }

    fn compatibility_subtype(&self, actual: &Ty, bound: &Ty) -> bool {
        if actual == bound {
            return true;
        }

        if let Ty::Union(members, _) = actual
            && !members.is_empty()
        {
            return members
                .iter()
                .all(|member| self.compatibility_subtype(member, bound));
        }

        if let Ty::Interface(iface_qtn, iface_args, associated_bindings, _) = bound
            && !matches!(actual, Ty::Interface(..))
        {
            let aliases = std::collections::HashMap::default();
            let requested_iface_ty = Ty::Interface(
                iface_qtn.clone(),
                iface_args.clone(),
                associated_bindings.clone(),
                TyAttr::default(),
            );
            return self.type_implements_interface_via_rule(
                actual,
                &requested_iface_ty,
                &aliases,
                |inner_actual, inner_bound| self.compatibility_subtype(inner_actual, inner_bound),
            );
        }

        normalize::is_subtype_of(actual, bound, &std::collections::HashMap::default())
    }

    /// True iff `class_qtn<class_type_args>` nominally implements `iface_qtn`
    /// via a blanket `implements<T> I for Container<T>` declaration.
    ///
    /// Checks: same class QTN, same interface QTN, arity matches
    /// (type args just need to have matching arity — they unify with anything
    /// since the blanket's vars are unconstrained for Form 1).
    pub fn blanket_class_implements_interface(
        &self,
        class_qtn: &QualifiedTypeName,
        class_type_args: &[Ty],
        iface_qtn: &QualifiedTypeName,
    ) -> bool {
        self.blanket_class_implements.iter().any(|blanket| {
            &blanket.class_qtn == class_qtn
                && &blanket.interface_qtn == iface_qtn
                && blanket.generic_params.len() == class_type_args.len()
        })
    }

    pub fn rule_matches_actual(
        &self,
        rule: &InterfaceImplRule,
        actual_ty: &Ty,
        requested_iface_ty: &Ty,
        aliases: &std::collections::HashMap<QualifiedTypeName, Ty>,
        mut is_subtype: impl FnMut(&Ty, &Ty) -> bool,
    ) -> Option<InterfaceImplInstantiation> {
        let mut bindings = TypeBindings::default();
        match_ty_pattern_into(
            &rule.for_ty_pattern,
            actual_ty,
            &rule.generic_params,
            aliases,
            &mut bindings,
        )?;
        if all_rule_generic_params_bound(rule, &bindings) {
            let instantiated_interface_ty = generics::substitute_ty(&rule.interface_ty, &bindings);
            let requested_iface_ty = generics::substitute_ty(requested_iface_ty, &bindings);
            if !interface_ty_satisfies_request(
                &instantiated_interface_ty,
                &requested_iface_ty,
                aliases,
                &mut is_subtype,
            ) {
                return None;
            }
            validate_rule_bounds(rule, &bindings, &mut is_subtype, true)?;
            return Some(InterfaceImplInstantiation {
                bindings: bindings.clone(),
                for_ty: generics::substitute_ty(&rule.for_ty_pattern, &bindings),
                interface_ty: instantiated_interface_ty,
            });
        }
        let interface_pattern = generics::substitute_ty(&rule.interface_ty, &bindings);
        match_ty_pattern_into(
            &interface_pattern,
            requested_iface_ty,
            &rule.generic_params,
            aliases,
            &mut bindings,
        )?;
        validate_rule_bounds(rule, &bindings, &mut is_subtype, true)?;
        Some(InterfaceImplInstantiation {
            bindings: bindings.clone(),
            for_ty: generics::substitute_ty(&rule.for_ty_pattern, &bindings),
            interface_ty: generics::substitute_ty(&rule.interface_ty, &bindings),
        })
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
            if match_ty_pattern_into(
                &rule.for_ty_pattern,
                actual_ty,
                &rule.generic_params,
                aliases,
                &mut bindings,
            )
            .is_none()
            {
                continue;
            }
            if all_rule_generic_params_bound(rule, &bindings) {
                let instantiated_interface_ty =
                    generics::substitute_ty(&rule.interface_ty, &bindings);
                let requested_iface_ty = generics::substitute_ty(requested_iface_ty, &bindings);
                if !interface_ty_satisfies_request(
                    &instantiated_interface_ty,
                    &requested_iface_ty,
                    aliases,
                    &mut is_subtype,
                ) {
                    continue;
                }
            } else {
                let interface_pattern = generics::substitute_ty(&rule.interface_ty, &bindings);
                if match_ty_pattern_into(
                    &interface_pattern,
                    requested_iface_ty,
                    &rule.generic_params,
                    aliases,
                    &mut bindings,
                )
                .is_none()
                {
                    continue;
                }
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
        let Some(iface_qtn) = interface_qtn(requested_iface_ty) else {
            return self.interface_impl_rules.iter().any(|rule| {
                self.rule_matches_actual(
                    rule,
                    actual_ty,
                    requested_iface_ty,
                    aliases,
                    &mut is_subtype,
                )
                .is_some()
            });
        };

        match actual_ty {
            Ty::Class(class_qtn, _, _) => {
                if let Some(indices_by_class) =
                    self.interface_impl_rule_index.by_class.get(iface_qtn)
                    && let Some(indices) = indices_by_class.get(class_qtn)
                    && self.any_indexed_rule_matches(
                        indices,
                        actual_ty,
                        requested_iface_ty,
                        aliases,
                        &mut is_subtype,
                    )
                {
                    return true;
                }
                if let Some(indices) = self
                    .interface_impl_rule_index
                    .fallback_by_interface
                    .get(iface_qtn)
                {
                    return self.any_indexed_rule_matches(
                        indices,
                        actual_ty,
                        requested_iface_ty,
                        aliases,
                        &mut is_subtype,
                    );
                }
                false
            }
            _ => {
                if let Some(key) = implementation_key_for_ty(actual_ty) {
                    if let Some(indices_by_type) =
                        self.interface_impl_rule_index.by_type.get(iface_qtn)
                        && let Some(indices) = indices_by_type.get(&key)
                        && self.any_indexed_rule_matches(
                            indices,
                            actual_ty,
                            requested_iface_ty,
                            aliases,
                            &mut is_subtype,
                        )
                    {
                        return true;
                    }
                    if let Some(indices) = self
                        .interface_impl_rule_index
                        .fallback_by_interface
                        .get(iface_qtn)
                    {
                        return self.any_indexed_rule_matches(
                            indices,
                            actual_ty,
                            requested_iface_ty,
                            aliases,
                            &mut is_subtype,
                        );
                    }
                    return false;
                }

                self.interface_impl_rule_index
                    .by_interface
                    .get(iface_qtn)
                    .is_some_and(|indices| {
                        self.any_indexed_rule_matches(
                            indices,
                            actual_ty,
                            requested_iface_ty,
                            aliases,
                            &mut is_subtype,
                        )
                    })
            }
        }
    }

    fn any_indexed_rule_matches(
        &self,
        indices: &[usize],
        actual_ty: &Ty,
        requested_iface_ty: &Ty,
        aliases: &std::collections::HashMap<QualifiedTypeName, Ty>,
        is_subtype: &mut impl FnMut(&Ty, &Ty) -> bool,
    ) -> bool {
        indices.iter().any(|idx| {
            self.interface_impl_rules
                .get(*idx)
                .and_then(|rule| {
                    self.rule_matches_actual(
                        rule,
                        actual_ty,
                        requested_iface_ty,
                        aliases,
                        &mut *is_subtype,
                    )
                })
                .is_some()
        })
    }

    pub fn instantiate_rule_for_requested_interface(
        &self,
        rule: &InterfaceImplRule,
        requested_iface_ty: &Ty,
        candidate_ty: Option<&Ty>,
        aliases: &std::collections::HashMap<QualifiedTypeName, Ty>,
        mut is_subtype: impl FnMut(&Ty, &Ty) -> bool,
    ) -> Option<InterfaceImplInstantiation> {
        let mut bindings = TypeBindings::default();
        if let Some(candidate_ty) = candidate_ty {
            match_ty_pattern_into(
                &rule.for_ty_pattern,
                candidate_ty,
                &rule.generic_params,
                aliases,
                &mut bindings,
            )?;
        }
        if all_rule_generic_params_bound(rule, &bindings) {
            let instantiated_interface_ty = generics::substitute_ty(&rule.interface_ty, &bindings);
            let requested_iface_ty = generics::substitute_ty(requested_iface_ty, &bindings);
            if !interface_ty_satisfies_request(
                &instantiated_interface_ty,
                &requested_iface_ty,
                aliases,
                &mut is_subtype,
            ) {
                return None;
            }
            validate_rule_bounds(rule, &bindings, &mut is_subtype, false)?;
            return Some(InterfaceImplInstantiation {
                bindings: bindings.clone(),
                for_ty: generics::substitute_ty(&rule.for_ty_pattern, &bindings),
                interface_ty: instantiated_interface_ty,
            });
        }
        let interface_pattern = generics::substitute_ty(&rule.interface_ty, &bindings);
        match_ty_pattern_into(
            &interface_pattern,
            requested_iface_ty,
            &rule.generic_params,
            aliases,
            &mut bindings,
        )?;
        validate_rule_bounds(rule, &bindings, &mut is_subtype, false)?;
        Some(InterfaceImplInstantiation {
            bindings: bindings.clone(),
            for_ty: generics::substitute_ty(&rule.for_ty_pattern, &bindings),
            interface_ty: generics::substitute_ty(&rule.interface_ty, &bindings),
        })
    }
}

fn derive_compatibility_views(
    rules: &[InterfaceImplRule],
    all_class_qtns: &[QualifiedTypeName],
) -> RegistryCompatibilityViews {
    let mut views = RegistryCompatibilityViews::default();
    for class_qtn in all_class_qtns {
        views.class_implements.entry(class_qtn.clone()).or_default();
    }

    for rule in rules {
        let Ty::Interface(iface_qtn, interface_type_args, _, _) = &rule.interface_ty else {
            continue;
        };

        match &rule.for_ty_pattern {
            Ty::Class(class_qtn, class_args, _)
                if matches!(rule.origin, InterfaceImplOrigin::OutOfBody)
                    && class_args.iter().any(|arg| matches!(arg, Ty::TypeVar(..))) =>
            {
                views.blanket_class_implements.push(BlanketClassImpl {
                    class_qtn: class_qtn.clone(),
                    generic_params: rule.generic_params.clone(),
                    generic_param_bounds: rule.generic_param_bounds.clone(),
                    interface_qtn: iface_qtn.clone(),
                    interface_type_args: interface_type_args.clone(),
                    for_target_ty: rule.for_ty_pattern.clone(),
                });
            }
            Ty::Class(class_qtn, _, _) => {
                if rule.generic_param_bounds.iter().all(Option::is_none) {
                    views
                        .class_implements
                        .entry(class_qtn.clone())
                        .or_default()
                        .insert(iface_qtn.clone());
                    if !interface_type_args.is_empty() {
                        views.implements_type_args.insert(
                            (class_qtn.clone(), iface_qtn.clone()),
                            interface_type_args.clone(),
                        );
                    }
                }
            }
            target_ty => {
                let Some(target_key) = implementation_key_for_ty(target_ty) else {
                    continue;
                };
                views
                    .type_implements
                    .entry(target_key.clone())
                    .or_default()
                    .insert(iface_qtn.clone());
                if !interface_type_args.is_empty() {
                    views
                        .type_implements_type_args
                        .insert((target_key, iface_qtn.clone()), interface_type_args.clone());
                }
            }
        }
    }

    views
}

#[allow(clippy::too_many_arguments)]
fn lower_interface_associated_bindings(
    db: &dyn crate::Db,
    iface: &baml_compiler2_hir::item_tree::Interface,
    interface_args: &[Ty],
    block_associated_bindings: &[baml_compiler2_ast::AssociatedTypeBindingDef],
    iface_pkg_items: &baml_compiler2_hir::package::PackageItems<'_>,
    binding_pkg_items: &baml_compiler2_hir::package::PackageItems<'_>,
    iface_namespace_path: &[Name],
    binding_namespace_path: &[Name],
    generic_params: &[Name],
    diagnostics: &mut Vec<crate::infer_context::TirTypeError>,
) -> Vec<(Name, Ty)> {
    let mut bindings = generics::bind_type_vars(&iface.generic_params, interface_args);
    for param in generic_params {
        bindings
            .entry(param.clone())
            .or_insert_with(|| Ty::TypeVar(param.clone(), TyAttr::default()));
    }

    iface
        .associated_types
        .iter()
        .filter_map(|assoc| {
            let ty = if let Some(binding) = block_associated_bindings
                .iter()
                .find(|binding| binding.name == assoc.name)
                && let Some(type_expr) = &binding.type_expr
            {
                generics::lower_type_expr_with_generics(
                    db,
                    &type_expr.expr,
                    binding_pkg_items,
                    binding_namespace_path,
                    &bindings,
                    diagnostics,
                )
            } else {
                let default = assoc.default.as_ref()?;
                generics::lower_type_expr_with_generics(
                    db,
                    &default.expr,
                    iface_pkg_items,
                    iface_namespace_path,
                    &bindings,
                    diagnostics,
                )
            };
            bindings.insert(assoc.name.clone(), ty.clone());
            Some((assoc.name.clone(), ty))
        })
        .collect()
}

fn complete_interface_associated_bindings_from_tys(
    db: &dyn crate::Db,
    iface: &baml_compiler2_hir::item_tree::Interface,
    interface_args: &[Ty],
    associated_bindings: &[(Name, Ty)],
    pkg_items: &baml_compiler2_hir::package::PackageItems<'_>,
    iface_namespace_path: &[Name],
    diagnostics: &mut Vec<crate::infer_context::TirTypeError>,
) -> Vec<(Name, Ty)> {
    let mut bindings = generics::bind_type_vars(&iface.generic_params, interface_args);
    for (name, ty) in associated_bindings {
        bindings.insert(name.clone(), ty.clone());
    }

    iface
        .associated_types
        .iter()
        .filter_map(|assoc| {
            if let Some((_, ty)) = associated_bindings
                .iter()
                .find(|(name, _)| name == &assoc.name)
            {
                let ty = generics::substitute_ty(ty, &bindings);
                bindings.insert(assoc.name.clone(), ty.clone());
                return Some((assoc.name.clone(), ty));
            }
            assoc.default.as_ref().map(|default| {
                let ty = generics::lower_type_expr_with_generics(
                    db,
                    &default.expr,
                    pkg_items,
                    iface_namespace_path,
                    &bindings,
                    diagnostics,
                );
                bindings.insert(assoc.name.clone(), ty.clone());
                (assoc.name.clone(), ty)
            })
        })
        .collect()
}

fn lower_interface_type_associated_bindings(
    ctx: InterfaceTypeAssocLowering<'_, '_>,
    diagnostics: &mut Vec<crate::infer_context::TirTypeError>,
) -> Vec<(Name, Ty)> {
    let mut bindings = generics::bind_type_vars(&ctx.iface.generic_params, ctx.interface_args);
    for (name, ty) in ctx.outer_bindings {
        bindings.entry(name.clone()).or_insert_with(|| ty.clone());
    }

    ctx.iface
        .associated_types
        .iter()
        .filter_map(|assoc| {
            if let Some(binding) = ctx
                .explicit_associated_bindings
                .iter()
                .find(|binding| binding.name == assoc.name)
            {
                let ty = generics::lower_type_expr_with_generics(
                    ctx.db,
                    &binding.ty,
                    ctx.binding_pkg_items,
                    ctx.binding_namespace_path,
                    &bindings,
                    diagnostics,
                );
                bindings.insert(assoc.name.clone(), ty.clone());
                return Some((assoc.name.clone(), ty));
            }
            assoc.default.as_ref().map(|default| {
                let ty = generics::lower_type_expr_with_generics(
                    ctx.db,
                    &default.expr,
                    ctx.iface_pkg_items,
                    ctx.iface_namespace_path,
                    &bindings,
                    diagnostics,
                );
                bindings.insert(assoc.name.clone(), ty.clone());
                (assoc.name.clone(), ty)
            })
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

fn interface_qtn(ty: &Ty) -> Option<&QualifiedTypeName> {
    match ty {
        Ty::Interface(qtn, _, _, _) => Some(qtn),
        _ => None,
    }
}

fn all_rule_generic_params_bound(rule: &InterfaceImplRule, bindings: &TypeBindings) -> bool {
    rule.generic_params
        .iter()
        .all(|param| bindings.contains_key(param))
}

fn interface_ty_satisfies_request(
    actual: &Ty,
    requested: &Ty,
    aliases: &std::collections::HashMap<QualifiedTypeName, Ty>,
    is_subtype: &mut impl FnMut(&Ty, &Ty) -> bool,
) -> bool {
    let (
        Ty::Interface(actual_qtn, actual_args, actual_assoc, _),
        Ty::Interface(requested_qtn, requested_args, requested_assoc, _),
    ) = (actual, requested)
    else {
        return normalize::is_same_normalized_type(actual, requested, aliases);
    };
    actual_qtn == requested_qtn
        && actual_args.len() == requested_args.len()
        && actual_args
            .iter()
            .zip(requested_args.iter())
            .all(|(actual_arg, requested_arg)| {
                types_equivalent_for_rule_match(actual_arg, requested_arg, aliases, is_subtype)
            })
        && requested_assoc
            .iter()
            .all(|(requested_name, requested_ty)| {
                actual_assoc
                    .iter()
                    .find(|(actual_name, _)| actual_name == requested_name)
                    .is_some_and(|(_, actual_ty)| {
                        types_equivalent_for_rule_match(
                            actual_ty,
                            requested_ty,
                            aliases,
                            is_subtype,
                        )
                    })
            })
}

fn types_equivalent_for_rule_match(
    actual: &Ty,
    requested: &Ty,
    aliases: &std::collections::HashMap<QualifiedTypeName, Ty>,
    is_subtype: &mut impl FnMut(&Ty, &Ty) -> bool,
) -> bool {
    if normalize::is_same_normalized_type(actual, requested, aliases) {
        return true;
    }

    // Concrete interface args/bindings must not be "proven" equivalent by a
    // permissive probing predicate. Symbolic projections and type vars still
    // need the semantic subtype relation because bounds can resolve cases like
    // `T.Item == string` even when normalized syntax is still a projection.
    (contains_rule_match_symbolic_ty(actual) || contains_rule_match_symbolic_ty(requested))
        && is_subtype(actual, requested)
        && is_subtype(requested, actual)
}

fn contains_rule_match_symbolic_ty(ty: &Ty) -> bool {
    match ty {
        Ty::TypeVar(..) | Ty::AssociatedTypeProjection { .. } => true,
        Ty::List(inner, _) | Ty::EvolvingList(inner, _) => contains_rule_match_symbolic_ty(inner),
        Ty::Map {
            key: k, value: v, ..
        }
        | Ty::EvolvingMap(k, v, _) => {
            contains_rule_match_symbolic_ty(k) || contains_rule_match_symbolic_ty(v)
        }
        Ty::Union(tys, _) => tys.iter().any(contains_rule_match_symbolic_ty),
        Ty::Future(value, error, _) => {
            contains_rule_match_symbolic_ty(value) || contains_rule_match_symbolic_ty(error)
        }
        Ty::Function {
            generic_param_bounds,
            params,
            ret,
            throws,
            ..
        } => {
            generic_param_bounds
                .iter()
                .any(|bound| bound.as_ref().is_some_and(contains_rule_match_symbolic_ty))
                || params
                    .iter()
                    .any(|param| contains_rule_match_symbolic_ty(&param.ty))
                || contains_rule_match_symbolic_ty(ret)
                || contains_rule_match_symbolic_ty(throws)
        }
        Ty::Class(_, type_args, _) => type_args.iter().any(contains_rule_match_symbolic_ty),
        Ty::Interface(_, type_args, associated_bindings, _) => {
            type_args.iter().any(contains_rule_match_symbolic_ty)
                || associated_bindings
                    .iter()
                    .any(|(_, ty)| contains_rule_match_symbolic_ty(ty))
        }
        _ => false,
    }
}

pub fn match_ty_pattern(
    pattern: &Ty,
    concrete: &Ty,
    generic_params: &[Name],
    aliases: &std::collections::HashMap<QualifiedTypeName, Ty>,
) -> Option<TypeBindings> {
    let mut bindings = TypeBindings::default();
    match_ty_pattern_into(pattern, concrete, generic_params, aliases, &mut bindings)?;
    Some(bindings)
}

fn match_ty_pattern_into(
    pattern: &Ty,
    concrete: &Ty,
    generic_params: &[Name],
    aliases: &std::collections::HashMap<QualifiedTypeName, Ty>,
    bindings: &mut TypeBindings,
) -> Option<()> {
    if let Ty::TypeVar(name, _) = pattern
        && generic_params.contains(name)
    {
        return bind_type_var(name, concrete, bindings, aliases);
    }

    if !contains_bound_typevar(pattern, generic_params)
        && !contains_generic_function_binders(pattern)
        && !contains_generic_function_binders(concrete)
        && normalize::is_same_normalized_type(pattern, concrete, aliases)
    {
        return Some(());
    }

    match (pattern, concrete) {
        (Ty::Class(p_qtn, p_args, _), Ty::Class(c_qtn, c_args, _))
            if p_qtn == c_qtn && p_args.len() == c_args.len() =>
        {
            for (p, c) in p_args.iter().zip(c_args.iter()) {
                match_ty_pattern_into(p, c, generic_params, aliases, bindings)?;
            }
            Some(())
        }
        (Ty::Interface(p_qtn, p_args, p_assoc, _), Ty::Interface(c_qtn, c_args, c_assoc, _))
            if p_qtn == c_qtn && p_args.len() == c_args.len() =>
        {
            for (p, c) in p_args.iter().zip(c_args.iter()) {
                match_ty_pattern_into(p, c, generic_params, aliases, bindings)?;
            }
            for (name, concrete_ty) in c_assoc {
                let (_, pattern_ty) = p_assoc.iter().find(|(p_name, _)| p_name == name)?;
                match_ty_pattern_into(pattern_ty, concrete_ty, generic_params, aliases, bindings)?;
            }
            Some(())
        }
        (Ty::List(p, _), Ty::List(c, _)) | (Ty::EvolvingList(p, _), Ty::EvolvingList(c, _)) => {
            match_ty_pattern_into(p, c, generic_params, aliases, bindings)
        }
        (
            Ty::Map {
                key: pk, value: pv, ..
            },
            Ty::Map {
                key: ck, value: cv, ..
            },
        )
        | (Ty::EvolvingMap(pk, pv, _), Ty::EvolvingMap(ck, cv, _)) => {
            match_ty_pattern_into(pk, ck, generic_params, aliases, bindings)?;
            match_ty_pattern_into(pv, cv, generic_params, aliases, bindings)
        }
        (Ty::Future(pv, pe, _), Ty::Future(cv, ce, _)) => {
            match_ty_pattern_into(pv, cv, generic_params, aliases, bindings)?;
            match_ty_pattern_into(pe, ce, generic_params, aliases, bindings)
        }
        (Ty::Union(p_members, _), Ty::Union(c_members, _))
            if p_members.len() == c_members.len() =>
        {
            match_union_members(p_members, c_members, generic_params, aliases, bindings)
        }
        (Ty::Int { .. }, Ty::Literal(Literal::Int(_), _, _))
        | (Ty::Bigint { .. }, Ty::Literal(Literal::Bigint(_), _, _))
        | (Ty::Float { .. }, Ty::Literal(Literal::Float(_), _, _))
        | (Ty::String { .. }, Ty::Literal(Literal::String(_), _, _))
        | (Ty::Bool { .. }, Ty::Literal(Literal::Bool(_), _, _)) => Some(()),
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
                generic_params,
                aliases,
                bindings,
            )?;
            let p_function_bindings =
                function_generic_bindings(p_generic_params, &canonical_params);
            let c_function_bindings =
                function_generic_bindings(c_generic_params, &canonical_params);

            for (p, c) in p_params.iter().zip(c_params.iter()) {
                let p_ty = generics::substitute_ty(&p.ty, &p_function_bindings);
                let c_ty = generics::substitute_ty(&c.ty, &c_function_bindings);
                match_ty_pattern_into(&p_ty, &c_ty, generic_params, aliases, bindings)?;
            }
            let p_ret = generics::substitute_ty(p_ret, &p_function_bindings);
            let c_ret = generics::substitute_ty(c_ret, &c_function_bindings);
            match_ty_pattern_into(&p_ret, &c_ret, generic_params, aliases, bindings)?;
            let p_throws = generics::substitute_ty(p_throws, &p_function_bindings);
            let c_throws = generics::substitute_ty(c_throws, &c_function_bindings);
            match_ty_pattern_into(&p_throws, &c_throws, generic_params, aliases, bindings)
        }
        _ if normalize::is_same_normalized_type(pattern, concrete, aliases) => Some(()),
        _ => None,
    }
}

fn match_function_generic_bounds(
    pattern_params: &[Name],
    pattern_bounds: &[Option<Ty>],
    concrete_params: &[Name],
    concrete_bounds: &[Option<Ty>],
    generic_params: &[Name],
    aliases: &std::collections::HashMap<QualifiedTypeName, Ty>,
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
                match_ty_pattern_into(
                    &pattern_bound,
                    &concrete_bound,
                    generic_params,
                    aliases,
                    bindings,
                )?;
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
        .map(|(param, canonical)| {
            (
                param.clone(),
                Ty::TypeVar(canonical.clone(), TyAttr::default()),
            )
        })
        .collect()
}

fn match_union_members(
    pattern_members: &[Ty],
    concrete_members: &[Ty],
    generic_params: &[Name],
    aliases: &std::collections::HashMap<QualifiedTypeName, Ty>,
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
            generic_params,
            aliases,
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
        if match_union_members(
            pattern_tail,
            &remaining,
            generic_params,
            aliases,
            &mut trial_bindings,
        )
        .is_some()
        {
            *bindings = trial_bindings;
            return Some(());
        }
    }

    None
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
        Ty::TypeVar(name, _) => generic_params.contains(name),
        Ty::Class(_, args, _) | Ty::Union(args, _) => args
            .iter()
            .any(|arg| contains_bound_typevar(arg, generic_params)),
        Ty::Interface(_, args, associated_bindings, _) => {
            args.iter()
                .any(|arg| contains_bound_typevar(arg, generic_params))
                || associated_bindings
                    .iter()
                    .any(|(_, ty)| contains_bound_typevar(ty, generic_params))
        }
        Ty::List(inner, _) | Ty::EvolvingList(inner, _) => {
            contains_bound_typevar(inner, generic_params)
        }
        Ty::Map {
            key: k, value: v, ..
        }
        | Ty::EvolvingMap(k, v, _)
        | Ty::Future(k, v, _) => {
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
        Ty::Class(_, args, _) | Ty::Union(args, _) => {
            args.iter().any(contains_generic_function_binders)
        }
        Ty::Interface(_, args, associated_bindings, _) => {
            args.iter().any(contains_generic_function_binders)
                || associated_bindings
                    .iter()
                    .any(|(_, ty)| contains_generic_function_binders(ty))
        }
        Ty::List(inner, _) | Ty::EvolvingList(inner, _) => contains_generic_function_binders(inner),
        Ty::Map {
            key: k, value: v, ..
        }
        | Ty::EvolvingMap(k, v, _)
        | Ty::Future(k, v, _) => {
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
        Ty::Int { .. } => Some(Ty::Int {
            attr: TyAttr::default(),
        }),
        Ty::Bigint { .. } => Some(Ty::Bigint {
            attr: TyAttr::default(),
        }),
        Ty::Float { .. } => Some(Ty::Float {
            attr: TyAttr::default(),
        }),
        Ty::String { .. } => Some(Ty::String {
            attr: TyAttr::default(),
        }),
        Ty::Bool { .. } => Some(Ty::Bool {
            attr: TyAttr::default(),
        }),
        Ty::Null { .. } => Some(Ty::Null {
            attr: TyAttr::default(),
        }),
        Ty::Uint8Array { .. } => Some(Ty::Uint8Array {
            attr: TyAttr::default(),
        }),
        Ty::Media(kind, _) => Some(Ty::Media(*kind, TyAttr::default())),
        Ty::Literal(literal, _, _) => Some(match literal {
            Literal::Int(_) => Ty::Int {
                attr: TyAttr::default(),
            },
            Literal::Bigint(_) => Ty::Bigint {
                attr: TyAttr::default(),
            },
            Literal::Float(_) => Ty::Float {
                attr: TyAttr::default(),
            },
            Literal::String(_) => Ty::String {
                attr: TyAttr::default(),
            },
            Literal::Bool(_) => Ty::Bool {
                attr: TyAttr::default(),
            },
        }),
        Ty::List(inner, _) => Some(Ty::List(
            Box::new(implementation_key_for_ty(inner)?),
            TyAttr::default(),
        )),
        Ty::Map { key, value, .. } => Some(Ty::Map {
            key: Box::new(implementation_key_for_ty(key)?),
            value: Box::new(implementation_key_for_ty(value)?),
            attr: TyAttr::default(),
        }),
        Ty::Union(members, _) => {
            let mut keys = members
                .iter()
                .map(implementation_key_for_ty)
                .collect::<Option<Vec<_>>>()?;
            keys.sort();
            keys.dedup();
            Some(Ty::Union(keys, TyAttr::default()))
        }
        Ty::Class(..) | Ty::Interface(..) | Ty::Enum(..) | Ty::TypeAlias(..) => None,
        _ => None,
    }
}

/// Returns `true` if `ty` implements interface `iface_qtn`, searching
/// `package_id`'s implements registry **and** those of its direct dependencies.
///
/// [`package_implements_registry`] only records `implement` blocks written in
/// that one package. But coherence (the orphan rule, BEP-044) lets an impl live
/// in either the interface's package or the implementing type's package, so the
/// impls for a single interface are spread across packages. In particular the
/// builtin `Equals`/`Compare` impls for primitives and containers live in the
/// `baml` package — so a query from user code (or any dependent) must also
/// consult the dependency registries, `baml` above all.
pub fn type_implements_with_deps<'db>(
    db: &'db dyn crate::Db,
    package_id: PackageId<'db>,
    ty: &Ty,
    iface_qtn: &QualifiedTypeName,
) -> bool {
    if package_implements_registry(db, package_id).type_implements(ty, iface_qtn) {
        return true;
    }
    baml_compiler2_hir::package::package_dependency_closure(db, package_id)
        .iter()
        .any(|dep| package_implements_registry(db, *dep).type_implements(ty, iface_qtn))
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
    let mut interface_impl_rules: Vec<InterfaceImplRule> = Vec::new();
    let mut all_class_qtns: Vec<QualifiedTypeName> = Vec::new();
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
                    let mut diags = Vec::new();
                    let lowered_interface = crate::lower_type_expr::lower_type_expr_in_ns(
                        db,
                        &target.target.expr,
                        pkg_items,
                        &class_ns,
                        &class_data.generic_params,
                        &mut diags,
                    );
                    let interface_args =
                        if let Ty::Interface(_, interface_args, _, _) = lowered_interface {
                            interface_args
                        } else {
                            Vec::new()
                        };
                    let iface_pkg_info =
                        baml_compiler2_hir::file_package::file_package(db, iface_loc.file(db));
                    let iface_pkg_id = PackageId::new(db, iface_pkg_info.package.clone());
                    let iface_pkg_items = baml_compiler2_ppir::package_items(db, iface_pkg_id);
                    let iface_namespace_path = iface_pkg_info.namespace_path;
                    let associated_bindings = lower_interface_associated_bindings(
                        db,
                        iface_data,
                        &interface_args,
                        &target.associated_type_bindings,
                        iface_pkg_items,
                        pkg_items,
                        &iface_namespace_path,
                        &class_ns,
                        &class_data.generic_params,
                        &mut diags,
                    );
                    let for_ty_pattern = Ty::Class(
                        class_qtn.clone(),
                        class_data
                            .generic_params
                            .iter()
                            .map(|param| Ty::TypeVar(param.clone(), TyAttr::default()))
                            .collect(),
                        TyAttr::default(),
                    );
                    let class_bound_tys = crate::builder::lower_generic_param_bounds(
                        db,
                        &class_data.generic_param_bounds,
                        pkg_items,
                        &class_ns,
                        &class_data.generic_params,
                        None,
                        &mut diags,
                    );
                    interface_impl_rules.push(InterfaceImplRule {
                        generic_params: class_data.generic_params.clone(),
                        generic_param_bounds: class_bound_tys,
                        for_ty_pattern,
                        interface_ty: Ty::Interface(
                            iface_qtn.clone(),
                            interface_args,
                            associated_bindings,
                            TyAttr::default(),
                        ),
                        origin: InterfaceImplOrigin::InBodyClass {
                            class_qtn: class_qtn.clone(),
                        },
                        source_span: Some(Span::new(
                            class_loc.file(db).file_id(db),
                            target.target.span,
                        )),
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
            let mut diags = Vec::new();
            let target_ty = crate::lower_type_expr::lower_type_expr_in_ns(
                db,
                &imp.for_target.expr,
                pkg_items,
                &pkg_info.namespace_path,
                &imp.generic_params,
                &mut diags,
            );
            let lowered_interface = crate::lower_type_expr::lower_type_expr_in_ns(
                db,
                &imp.interface_target.expr,
                pkg_items,
                &pkg_info.namespace_path,
                &imp.generic_params,
                &mut diags,
            );
            let interface_args = if let Ty::Interface(_, interface_args, _, _) = lowered_interface {
                interface_args
            } else {
                Vec::new()
            };
            let iface_pkg_info =
                baml_compiler2_hir::file_package::file_package(db, iface_loc.file(db));
            let iface_pkg_id = PackageId::new(db, iface_pkg_info.package.clone());
            let iface_pkg_items = baml_compiler2_ppir::package_items(db, iface_pkg_id);
            let iface_namespace_path = iface_pkg_info.namespace_path;
            let associated_bindings = lower_interface_associated_bindings(
                db,
                iface_data,
                &interface_args,
                &imp.associated_type_bindings,
                iface_pkg_items,
                pkg_items,
                &iface_namespace_path,
                &pkg_info.namespace_path,
                &imp.generic_params,
                &mut diags,
            );
            let interface_ty = Ty::Interface(
                iface_qtn.clone(),
                interface_args.clone(),
                associated_bindings,
                TyAttr::default(),
            );
            let bounds: Vec<Option<Ty>> = imp
                .generic_param_bounds
                .iter()
                .map(|b| {
                    b.as_ref().map(|te| {
                        crate::lower_type_expr::lower_type_expr_in_ns(
                            db,
                            te,
                            pkg_items,
                            &pkg_info.namespace_path,
                            &imp.generic_params,
                            &mut diags,
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
                source_span: Some(Span::new(file.file_id(db), imp.span)),
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

    let interface_impl_rule_index = InterfaceImplRuleIndex::from_rules(&interface_impl_rules);
    let compatibility_views = derive_compatibility_views(&interface_impl_rules, &all_class_qtns);

    ImplementsRegistry {
        interface_impl_rules,
        interface_impl_rule_index,
        class_implements: compatibility_views.class_implements,
        type_implements: compatibility_views.type_implements,
        blanket_class_implements: compatibility_views.blanket_class_implements,
        implements_type_args: compatibility_views.implements_type_args,
        type_implements_type_args: compatibility_views.type_implements_type_args,
        interface_requires,
    }
}

/// Three-valued result of an overlap decision. Overlap is undecidable in general
/// (it's ACI-unification — NP-hard — once unions with variables are involved), so
/// the checker can report "couldn't tell" rather than guessing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Overlap {
    /// A common instance provably exists — the impls overlap.
    Yes,
    /// No common instance can exist — the impls are provably disjoint.
    No,
    /// Could not be decided within the search bounds (a too-large or
    /// variable-bearing union). Callers treat it as a possible overlap (the sound
    /// direction) but report it distinctly.
    Unknown,
}

impl Overlap {
    /// Kleene conjunction: a compound overlaps only if *every* part can —
    /// `No` dominates (one disjoint part ⇒ disjoint), then `Unknown`, then `Yes`.
    fn and(self, other: Overlap) -> Overlap {
        match (self, other) {
            (Overlap::No, _) | (_, Overlap::No) => Overlap::No,
            (Overlap::Unknown, _) | (_, Overlap::Unknown) => Overlap::Unknown,
            (Overlap::Yes, Overlap::Yes) => Overlap::Yes,
        }
    }
}

/// Budget on the number of `cover` trials the covering search may perform before it
/// gives up with `Overlap::Unknown` ("this type is too complex to decide — simplify
/// it"). ACI-unification is NP-hard, so the search is bounded rather than risking a
/// factorial blow-up. Easy cases (e.g. linear members) resolve in far fewer steps; the
/// budget is reached only by large, deeply-coupled variable-bearing unions.
const MAX_OVERLAP_SEARCH_STEPS: usize = 4096;

/// Map an overlap result to a reportable violation: `None` = disjoint (no
/// diagnostic); `Some(indeterminate)` = report it, where `indeterminate` is
/// `true` for the conservative "couldn't prove disjoint" case.
fn overlap_violation(overlap: Overlap) -> Option<bool> {
    match overlap {
        Overlap::No => None,
        Overlap::Yes => Some(false),
        Overlap::Unknown => Some(true),
    }
}

/// A coherence violation: two implementations of the same interface that overlap,
/// or that could not be proven disjoint. With no specialization, either is a hard
/// error; `indeterminate` lets the caller word the diagnostic correctly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CoherenceViolation {
    /// The offending impl. Always owned by the package being checked, so the
    /// diagnostic lands on a file the user can edit.
    pub primary: Span,
    /// The impl it overlaps with. May live in a dependency package.
    pub secondary: Span,
    /// `true` when overlap could be neither proven nor disproven (conservatively
    /// rejected) rather than a definite overlap.
    pub indeterminate: bool,
}

/// Per-package interface coherence check.
///
/// Reports overlapping implementations of the same interface across the whole
/// package *and its dependency closure* — the BAML analog of rustc's per-crate
/// coherence plus knowability. The orphan rule (E0139) guarantees every blanket
/// impl lives in its interface's package, and writing `implement I for …`
/// requires depending on `pkg(I)`, so any overlapping pair has one side's
/// package depending on the other's (or is intra-package). Checking each
/// package against its dependencies is therefore complete without a
/// whole-program pass, and stays sound under dynamic loading.
///
/// Only pairs with at least one impl owned by `pkg_id` are reported;
/// dependency-internal conflicts are attributed to the dependency when *its*
/// coherence is checked, so nothing is double-reported.
#[salsa::tracked(returns(ref))]
pub fn package_coherence_diagnostics<'db>(
    db: &'db dyn crate::Db,
    pkg_id: PackageId<'db>,
) -> Vec<CoherenceViolation> {
    let mut own_rules: Vec<&InterfaceImplRule> = package_implements_registry(db, pkg_id)
        .interface_impl_rules
        .iter()
        .collect();
    // Sort by source position so the "first implementation is here" attribution tracks
    // the textually-earlier impl rather than registry (hash) iteration order, and stays
    // stable when an unrelated item is added to the package.
    own_rules.sort_by_key(|r| r.source_span.map(|s| u32::from(s.range.start())));
    let deps = baml_compiler2_hir::package::package_dependency_closure(db, pkg_id);

    // Type aliases (own package plus dependency exports) so alias-referencing
    // for-types and interface args normalize before the overlap comparison.
    let mut aliases =
        crate::inference::collect_type_aliases(db, baml_compiler2_ppir::package_items(db, pkg_id));
    for dep in deps {
        for (qtn, ty) in
            crate::inference::collect_type_aliases(db, baml_compiler2_ppir::package_items(db, *dep))
        {
            aliases.entry(qtn).or_insert(ty);
        }
    }

    let dep_rules: Vec<&InterfaceImplRule> = deps
        .iter()
        .flat_map(|dep| {
            package_implements_registry(db, *dep)
                .interface_impl_rules
                .iter()
        })
        .collect();

    let mut violations = Vec::new();
    for (i, own) in own_rules.iter().enumerate() {
        let Some(own_span) = own.source_span else {
            continue;
        };
        // own × own — each unordered pair once; the later impl carries the error.
        for other in &own_rules[i + 1..] {
            let Some(other_span) = other.source_span else {
                continue;
            };
            if let Some(indeterminate) =
                overlap_violation(impls_conflict(db, pkg_id, own, other, &aliases))
            {
                violations.push(CoherenceViolation {
                    primary: other_span,
                    secondary: own_span,
                    indeterminate,
                });
            }
        }
        // own × dependency — the owning package's impl carries the error.
        for dep_rule in &dep_rules {
            let Some(dep_span) = dep_rule.source_span else {
                continue;
            };
            if let Some(indeterminate) =
                overlap_violation(impls_conflict(db, pkg_id, own, dep_rule, &aliases))
            {
                violations.push(CoherenceViolation {
                    primary: own_span,
                    secondary: dep_span,
                    indeterminate,
                });
            }
        }
    }
    violations
}

/// Resolve a chain of top-level type aliases to the underlying type via `aliases`,
/// bounded against alias cycles (those are a separate diagnostic). Only the *head* is
/// resolved — aliases nested under a constructor are handled by `is_same_normalized_type`.
/// Mirrors `expand_type_alias` in the diagnostics layer so the coherence valid-subject
/// gate sees through the same aliases the E0138 concreteness gate does.
fn expand_alias_head(ty: &Ty, aliases: &std::collections::HashMap<QualifiedTypeName, Ty>) -> Ty {
    let mut current = ty.clone();
    for _ in 0..64 {
        let Ty::TypeAlias(qtn, _) = &current else {
            break;
        };
        match aliases.get(qtn) {
            Some(next) => current = next.clone(),
            None => break,
        }
    }
    current
}

/// True iff two impls of the *same* interface conflict (overlap with no
/// specialization to rescue them). Distinct interfaces never conflict, and two
/// in-body blocks of the same class for the same interface are a duplicate
/// (reported separately), not an overlap.
fn impls_conflict<'db>(
    db: &'db dyn crate::Db,
    pkg_id: PackageId<'db>,
    a: &InterfaceImplRule,
    b: &InterfaceImplRule,
    aliases: &std::collections::HashMap<QualifiedTypeName, Ty>,
) -> Overlap {
    let (Some(a_qtn), Some(b_qtn)) = (
        interface_qtn(&a.interface_ty),
        interface_qtn(&b.interface_ty),
    ) else {
        return Overlap::No;
    };
    if a_qtn != b_qtn {
        return Overlap::No;
    }
    // An impl whose for-target is not a valid implementor (rejected by the E0138
    // concreteness gate — union, interface, literal, enum variant, or an error
    // type) must not contribute a coherence overlap, or it would stack a spurious
    // E0132 on top of that rejection. The gate is applied to the *alias-expanded*
    // for-type: a bare `type AliasC = C` for-type lowers to `Ty::TypeAlias` (not itself
    // a valid subject), but E0138 expands it and accepts it, so coherence must expand it
    // too — otherwise `impl I for C` + `impl I for AliasC` would slip past both gates,
    // leaving two impls for the same concrete type. (Aliases *under* a constructor are
    // resolved later by `is_same_normalized_type`; only the head matters here.)
    if !expand_alias_head(&a.for_ty_pattern, aliases).is_valid_impl_subject()
        || !expand_alias_head(&b.for_ty_pattern, aliases).is_valid_impl_subject()
    {
        return Overlap::No;
    }
    if same_in_body_origin(a, b)
        && normalize::is_same_normalized_type(
            &strip_interface_assoc(&a.interface_ty),
            &strip_interface_assoc(&b.interface_ty),
            aliases,
        )
    {
        return Overlap::No;
    }
    impls_overlap(db, pkg_id, a, b, aliases)
}

/// Conservative symmetric overlap test over two impls of the same interface.
///
/// Two impls overlap iff their *subjects* — the for-type plus the interface
/// type-args — have a **common instance**: one concrete type + arg list that
/// both impls would apply to. We decide this by first-order unification with
/// *both* impls' generic params as fresh unification variables (renamed to
/// disjoint names so they can bind on either side). This finds complementary
/// pairs like `Pair<T, int>` vs `Pair<string, U>` (common instance
/// `Pair<string, int>`) that a one-directional matcher misses.
///
/// Associated bindings are interface *outputs*, so only the args participate.
///
/// Once the subjects unify, a bound on a param the unifier pinned to a *ground*
/// type is decided precisely (`type_implements_with_deps`): if that type
/// provably does not satisfy the bound, the bounded impl cannot apply to the
/// common instance, so the impls are disjoint. Bounds on params that remain
/// variables (blanket-vs-blanket) stay conservative — without negative impls we
/// cannot prove two bounded blankets disjoint, so they are assumed to overlap.
fn impls_overlap<'db>(
    db: &'db dyn crate::Db,
    pkg_id: PackageId<'db>,
    a: &InterfaceImplRule,
    b: &InterfaceImplRule,
    aliases: &std::collections::HashMap<QualifiedTypeName, Ty>,
) -> Overlap {
    let enum_variants = |qtn: &QualifiedTypeName| enum_variant_names(db, qtn);
    let (a_for, a_args) = renamed_subject(a, 'a', &enum_variants);
    let (b_for, b_args) = renamed_subject(b, 'b', &enum_variants);
    if a_args.len() != b_args.len() {
        return Overlap::No;
    }
    let mut vars: Vec<Name> = Vec::with_capacity(a.generic_params.len() + b.generic_params.len());
    vars.extend((0..a.generic_params.len()).map(|i| renamed_var('a', i)));
    vars.extend((0..b.generic_params.len()).map(|i| renamed_var('b', i)));

    let mut bindings = TypeBindings::default();
    // Unify the for-type and each interface arg. A provably-disjoint part
    // short-circuits the whole pair to disjoint; an undecidable part downgrades a
    // would-be overlap to `Unknown`.
    let mut result = unify_into(&a_for, &b_for, &vars, aliases, &mut bindings);
    if result == Overlap::No {
        return Overlap::No;
    }
    for (x, y) in a_args.iter().zip(b_args.iter()) {
        match unify_into(x, y, &vars, aliases, &mut bindings) {
            Overlap::No => return Overlap::No,
            Overlap::Unknown => result = Overlap::Unknown,
            Overlap::Yes => {}
        }
    }

    // The subjects share a common instance; the impls are still disjoint if either
    // carries a bound the common instance provably violates (a ground subject that
    // does not satisfy its bound). That conclusion holds even if `result` is
    // `Unknown`, so it can override to `No`.
    let a_subject: Vec<&Ty> = std::iter::once(&a_for).chain(a_args.iter()).collect();
    let b_subject: Vec<&Ty> = std::iter::once(&b_for).chain(b_args.iter()).collect();
    if !bounds_hold_at_common_instance(db, pkg_id, a, 'a', &vars, &bindings, &a_subject)
        || !bounds_hold_at_common_instance(db, pkg_id, b, 'b', &vars, &bindings, &b_subject)
    {
        return Overlap::No;
    }
    result
}

/// Whether every bound of `rule` could hold at the common instance the unifier
/// produced. Returns `false` as soon as a bound whose subject the unifier pinned
/// to a *forced* ground type is provably unsatisfiable — which makes the two impls
/// disjoint. A bound whose subject is still a variable is undecidable in an open
/// world and is assumed satisfiable.
///
/// The disproof is only sound when the binding is *principal* (forced by structural
/// unification). A param that appears inside a union in this impl's `subject` (the
/// for-type and interface args) is instead bound by `cover_search` to *one* of several
/// possible witnesses, so disproving its bound against that single witness would
/// unsoundly reject an overlap that a different witness satisfies — those are skipped.
fn bounds_hold_at_common_instance<'db>(
    db: &'db dyn crate::Db,
    pkg_id: PackageId<'db>,
    rule: &InterfaceImplRule,
    prefix: char,
    vars: &[Name],
    bindings: &TypeBindings,
    subject: &[&Ty],
) -> bool {
    for i in 0..rule.generic_params.len() {
        let Some(Some(bound)) = rule.generic_param_bounds.get(i) else {
            continue;
        };
        let Some(bound_qtn) = interface_qtn(bound) else {
            continue;
        };
        let var_i = renamed_var(prefix, i);
        // Non-principal (union-cover) binding ⇒ the witness is arbitrary; don't disprove.
        if subject.iter().any(|t| var_under_union(&var_i, t)) {
            continue;
        }
        let subject = chase_var(&Ty::TypeVar(var_i, TyAttr::default()), vars, bindings);
        if contains_bound_typevar(&subject, vars) {
            continue;
        }
        if !type_implements_with_deps(db, pkg_id, &subject, bound_qtn) {
            return false;
        }
    }
    true
}

/// Whether `name` occurs anywhere *inside a union* within `ty`. A bounded param that
/// does is bound by `cover_search` to one of several possible witnesses, so its
/// `chase_var` representative is non-principal — see `bounds_hold_at_common_instance`.
fn var_under_union(name: &Name, ty: &Ty) -> bool {
    fn occurs(name: &Name, ty: &Ty, in_union: bool) -> bool {
        match ty {
            Ty::TypeVar(n, _) => in_union && n == name,
            Ty::Union(members, _) => members.iter().any(|m| occurs(name, m, true)),
            Ty::Class(_, args, _) => args.iter().any(|a| occurs(name, a, in_union)),
            Ty::Interface(_, args, assoc, _) => {
                args.iter().any(|a| occurs(name, a, in_union))
                    || assoc.iter().any(|(_, t)| occurs(name, t, in_union))
            }
            Ty::List(inner, _) | Ty::EvolvingList(inner, _) | Ty::WatchAccessor(inner, _) => {
                occurs(name, inner, in_union)
            }
            Ty::Map {
                key: k, value: v, ..
            }
            | Ty::EvolvingMap(k, v, _)
            | Ty::Future(k, v, _) => occurs(name, k, in_union) || occurs(name, v, in_union),
            Ty::AssociatedTypeProjection {
                base, interface, ..
            } => {
                occurs(name, base, in_union)
                    || interface
                        .as_ref()
                        .is_some_and(|i| occurs(name, i, in_union))
            }
            Ty::Function {
                generic_param_bounds,
                params,
                ret,
                throws,
                ..
            } => {
                generic_param_bounds
                    .iter()
                    .any(|b| b.as_ref().is_some_and(|b| occurs(name, b, in_union)))
                    || params
                        .iter()
                        .any(|FunctionParamTy { ty, .. }| occurs(name, ty, in_union))
                    || occurs(name, ret, in_union)
                    || occurs(name, throws, in_union)
            }
            // Leaf types hold no nested type, so no variable occurs in them. Listed
            // explicitly (no total wildcard) so a new `Ty` variant must be classified.
            Ty::Int { .. }
            | Ty::Bigint { .. }
            | Ty::Float { .. }
            | Ty::String { .. }
            | Ty::Bool { .. }
            | Ty::Null { .. }
            | Ty::Uint8Array { .. }
            | Ty::Media(..)
            | Ty::Literal(..)
            | Ty::Enum(..)
            | Ty::EnumVariant(..)
            | Ty::RustType { .. }
            | Ty::Type { .. }
            | Ty::Resource { .. }
            | Ty::PromptAst { .. }
            | Ty::Void { .. }
            | Ty::TypeAlias(..)
            | Ty::BuiltinUnknown { .. }
            | Ty::Never { .. }
            | Ty::Unknown { .. }
            | Ty::Error { .. } => false,
        }
    }
    occurs(name, ty, false)
}

/// Fresh unification-variable name for the `idx`-th generic param of the impl
/// on side `prefix`. Guillemets can't appear in user type-var names, so the two
/// impls' renamed params are guaranteed disjoint from each other and from any
/// real type.
fn renamed_var(prefix: char, idx: usize) -> Name {
    Name::new(format!("«{prefix}{idx}»"))
}

/// Looks up an enum's full set of variant names (`None` if it can't be resolved).
type EnumVariants<'a> = &'a dyn Fn(&QualifiedTypeName) -> Option<Vec<Name>>;

/// Normalize an impl subject toward the union canonical form the covering solver assumes
/// (the db-aware part of CNF), recursing into every argument. For unions this flattens
/// nested unions, drops `never`, absorbs `unknown`, deduplicates, drops members subsumed
/// by a co-member (`1 | int → int`, `Color.Red | Color → Color`), and folds a *complete*
/// finite base back to its base (`true | false → bool`; all of an enum's variants → the
/// enum, via `enum_variants`). A var-bearing union opposite a finite base — which cannot
/// be folded away — is handled conservatively in `unify_into`.
fn nf(ty: &Ty, enum_variants: EnumVariants) -> Ty {
    match ty {
        Ty::Union(members, attr) => normalize_union(
            members.iter().map(|m| nf(m, enum_variants)).collect(),
            attr.clone(),
            enum_variants,
        ),
        Ty::List(inner, attr) => Ty::List(Box::new(nf(inner, enum_variants)), attr.clone()),
        Ty::EvolvingList(inner, attr) => {
            Ty::EvolvingList(Box::new(nf(inner, enum_variants)), attr.clone())
        }
        Ty::Map { key, value, attr } => Ty::Map {
            key: Box::new(nf(key, enum_variants)),
            value: Box::new(nf(value, enum_variants)),
            attr: attr.clone(),
        },
        Ty::EvolvingMap(k, v, attr) => Ty::EvolvingMap(
            Box::new(nf(k, enum_variants)),
            Box::new(nf(v, enum_variants)),
            attr.clone(),
        ),
        Ty::Future(value, error, attr) => Ty::Future(
            Box::new(nf(value, enum_variants)),
            Box::new(nf(error, enum_variants)),
            attr.clone(),
        ),
        Ty::Class(name, args, attr) => Ty::Class(
            name.clone(),
            args.iter().map(|a| nf(a, enum_variants)).collect(),
            attr.clone(),
        ),
        Ty::Interface(name, args, bindings, attr) => Ty::Interface(
            name.clone(),
            args.iter().map(|a| nf(a, enum_variants)).collect(),
            bindings
                .iter()
                .map(|(n, t)| (n.clone(), nf(t, enum_variants)))
                .collect(),
            attr.clone(),
        ),
        Ty::AssociatedTypeProjection {
            base,
            interface,
            member,
            attr,
        } => Ty::AssociatedTypeProjection {
            base: Box::new(nf(base, enum_variants)),
            interface: interface.as_ref().map(|i| Box::new(nf(i, enum_variants))),
            member: member.clone(),
            attr: attr.clone(),
        },
        Ty::Function {
            generic_params,
            generic_param_bounds,
            params,
            ret,
            throws,
            attr,
        } => Ty::Function {
            generic_params: generic_params.clone(),
            generic_param_bounds: generic_param_bounds
                .iter()
                .map(|b| b.as_ref().map(|t| nf(t, enum_variants)))
                .collect(),
            params: params
                .iter()
                .map(|p| FunctionParamTy {
                    name: p.name.clone(),
                    ty: nf(&p.ty, enum_variants),
                    mode: p.mode,
                })
                .collect(),
            ret: Box::new(nf(ret, enum_variants)),
            throws: Box::new(nf(throws, enum_variants)),
            attr: attr.clone(),
        },
        _ => ty.clone(),
    }
}

/// The union laws of [`nf`]. `members` are already individually normalized.
fn normalize_union(members: Vec<Ty>, attr: TyAttr, enum_variants: EnumVariants) -> Ty {
    // Flatten, drop `never`, absorb `unknown`, deduplicate.
    let mut flat: Vec<Ty> = Vec::new();
    for member in members {
        match member {
            Ty::Never { .. } => {}
            Ty::BuiltinUnknown { .. } => return Ty::BuiltinUnknown { attr },
            Ty::Union(inner, _) => {
                for inner_member in inner {
                    match inner_member {
                        Ty::Never { .. } => {}
                        Ty::BuiltinUnknown { .. } => return Ty::BuiltinUnknown { attr },
                        other if !flat.contains(&other) => flat.push(other),
                        _ => {}
                    }
                }
            }
            other if !flat.contains(&other) => flat.push(other),
            _ => {}
        }
    }

    // Drop members subsumed by a co-member (`literal <: base`, `variant <: enum`).
    flat = (0..flat.len())
        .filter(|&i| !(0..flat.len()).any(|j| i != j && is_literal_subtype(&flat[i], &flat[j])))
        .map(|i| flat[i].clone())
        .collect();

    fold_finite_bases(&mut flat, enum_variants);

    // Canonical member order: overlap decisions are order-insensitive (they go through
    // `is_same_normalized_type` / set-covering), but a deterministic order keeps `nf`'s
    // output stable across runs, avoiding spurious Salsa-cache churn.
    flat.sort();

    match flat.len() {
        0 => Ty::Never { attr },
        1 => flat
            .pop()
            .unwrap_or_else(|| unreachable!("a length-1 vec has an element")),
        _ => Ty::Union(flat, attr),
    }
}

/// Fold complete finite bases in a flattened, deduplicated member list: `true | false`
/// becomes `bool`, and an enum all of whose variants are present becomes the enum.
fn fold_finite_bases(flat: &mut Vec<Ty>, enum_variants: EnumVariants) {
    let has_bool_literal = |value: bool| {
        flat.iter()
            .any(|m| matches!(m, Ty::Literal(Literal::Bool(v), _, _) if *v == value))
    };
    if has_bool_literal(true) && has_bool_literal(false) {
        flat.retain(|m| !matches!(m, Ty::Literal(Literal::Bool(_), _, _)));
        flat.push(Ty::Bool {
            attr: TyAttr::default(),
        });
    }

    // Each enum that has a variant member: fold if *all* its variants are present.
    let mut enums: Vec<QualifiedTypeName> = Vec::new();
    for member in flat.iter() {
        if let Ty::EnumVariant(enum_name, _, _) = member
            && !enums.contains(enum_name)
        {
            enums.push(enum_name.clone());
        }
    }
    for enum_name in enums {
        let Some(all_variants) = enum_variants(&enum_name) else {
            continue;
        };
        if all_variants.is_empty() {
            continue;
        }
        let present: std::collections::HashSet<&Name> = flat
            .iter()
            .filter_map(|m| match m {
                Ty::EnumVariant(en, v, _) if *en == enum_name => Some(v),
                _ => None,
            })
            .collect();
        if all_variants.iter().all(|v| present.contains(v)) {
            flat.retain(|m| !matches!(m, Ty::EnumVariant(en, _, _) if *en == enum_name));
            flat.push(Ty::Enum(enum_name.clone(), TyAttr::default()));
        }
    }
}

/// Resolve an enum's full set of variant names, or `None` if it can't be resolved. Used
/// by `nf` to fold a complete variant union (`Cmp.Less | Cmp.Equal | Cmp.More`) back to
/// its enum (`Cmp`).
fn enum_variant_names(db: &dyn crate::Db, enum_qtn: &QualifiedTypeName) -> Option<Vec<Name>> {
    let package_id = PackageId::new(db, enum_qtn.package().clone());
    let items = baml_compiler2_hir::package::package_items(db, package_id);
    let Some(Definition::Enum(enum_loc)) = items.lookup_type(enum_qtn.namespace(), enum_qtn.name())
    else {
        return None;
    };
    let file = enum_loc.file(db);
    let item_tree = baml_compiler2_ppir::file_item_tree(db, file);
    let enum_data = &item_tree[enum_loc.id(db)];
    Some(enum_data.variants.iter().map(|v| v.name.clone()).collect())
}

/// The impl's subject — for-type and interface args — normalized (CNF) with its generic
/// params renamed to side-`prefix` unification variables. Associated bindings are
/// dropped (interface outputs, not part of overlap).
fn renamed_subject(
    rule: &InterfaceImplRule,
    prefix: char,
    enum_variants: EnumVariants,
) -> (Ty, Vec<Ty>) {
    let rename: TypeBindings = rule
        .generic_params
        .iter()
        .enumerate()
        .map(|(i, p)| {
            (
                p.clone(),
                Ty::TypeVar(renamed_var(prefix, i), TyAttr::default()),
            )
        })
        .collect();
    let for_ty = nf(
        &generics::substitute_ty(&rule.for_ty_pattern, &rename),
        enum_variants,
    );
    let args = match &rule.interface_ty {
        Ty::Interface(_, args, _, _) => args
            .iter()
            .map(|arg| nf(&generics::substitute_ty(arg, &rename), enum_variants))
            .collect(),
        _ => Vec::new(),
    };
    (for_ty, args)
}

/// Symmetric first-order **equality** unification: is there a substitution of the
/// unification variables `vars` (the combined, renamed-disjoint var set of the two
/// impls; either side may bind) that makes `x` and `y` the *same* type? Returns the
/// tri-state `Overlap`, committing the binding on `Yes`. This is the structural engine
/// of the overlap check — invariant constructor args and the for-types must be *equal*
/// for the two impls to share a common instance.
///
/// Equality, not subtyping: `int` and `Literal(1)` are distinct types here (`No`), as
/// are `K<int>` and `K<1>`. The `literal <: base` / `variant <: enum` subtyping lives in
/// `cover` (the covering oracle, its only consumer). Variants with no structural arm
/// fall through to `No`: anything equal already unified above via the normalizer, so the
/// rest are disjoint — conservative, losing precision only for var-bearing
/// function-typed args (never impl subjects today), treated as disjoint.
fn unify_into(
    x: &Ty,
    y: &Ty,
    vars: &[Name],
    aliases: &std::collections::HashMap<QualifiedTypeName, Ty>,
    bindings: &mut TypeBindings,
) -> Overlap {
    let x = chase_var(x, vars, bindings);
    let y = chase_var(y, vars, bindings);
    // Resolve a type alias to its definition before matching so the structural arms and
    // variable binding see through it — e.g. blanket `Box<T>` vs `Box<int>` spelled via
    // `type BI = Box<int>` must unify `T = int`. (`is_same_normalized_type` below also
    // resolves aliases, but only for the exact-equality fast path; the var-binding
    // structural match needs the alias gone too, or it falls to the disjoint arm.)
    let x = expand_alias_head(&x, aliases);
    let y = expand_alias_head(&y, aliases);

    // The error sentinel never unifies: an unresolved for-type or arg already has
    // its own diagnostic, so treating it as a common instance would stack a
    // spurious overlap. The *inhabited* top type `unknown` (`BuiltinUnknown`) is
    // deliberately not bailed here: it binds an opposing variable (below), and is
    // otherwise a distinct atomic type compared by equality — `Box<unknown>` is
    // disjoint from `Box<int>`, exactly how the runtime resolver matches it.
    if matches!(x, Ty::Unknown { .. }) || matches!(y, Ty::Unknown { .. }) {
        return Overlap::No;
    }

    if let Ty::TypeVar(n, _) = &x
        && vars.contains(n)
    {
        return bind_unify_var(n, &y, vars, aliases, bindings);
    }
    if let Ty::TypeVar(n, _) = &y
        && vars.contains(n)
    {
        return bind_unify_var(n, &x, vars, aliases, bindings);
    }

    // Structurally-equal (or alias-equal) subjects unify with no new bindings;
    // this also resolves ground unions order-insensitively via the normalizer.
    if normalize::is_same_normalized_type(&x, &y, aliases) {
        return Overlap::Yes;
    }

    match (&x, &y) {
        (Ty::Class(xq, xa, _), Ty::Class(yq, ya, _)) if xq == yq && xa.len() == ya.len() => {
            unify_all(xa, ya, vars, aliases, bindings)
        }
        (Ty::Interface(xq, xa, xb, _), Ty::Interface(yq, ya, yb, _))
            if xq == yq && xa.len() == ya.len() =>
        {
            // Generic args *and* associated bindings are part of an interface-existential
            // type's identity: `I<Item=int>` and `I<Item=string>` are distinct (disjoint)
            // types, because coherence gives each concrete type a single `impl I`, hence
            // one `Item`. (Distinct from the *impl's own* interface, where the bindings
            // are outputs and dropped by `renamed_subject`.)
            unify_all(xa, ya, vars, aliases, bindings)
                .and(unify_associated_bindings(xb, yb, vars, aliases, bindings))
        }
        (Ty::List(xi, _), Ty::List(yi, _)) | (Ty::EvolvingList(xi, _), Ty::EvolvingList(yi, _)) => {
            unify_into(xi, yi, vars, aliases, bindings)
        }
        (
            Ty::Map {
                key: xk, value: xv, ..
            },
            Ty::Map {
                key: yk, value: yv, ..
            },
        )
        | (Ty::EvolvingMap(xk, xv, _), Ty::EvolvingMap(yk, yv, _)) => {
            unify_into(xk, yk, vars, aliases, bindings)
                .and(unify_into(xv, yv, vars, aliases, bindings))
        }
        (Ty::Future(xv, xe, _), Ty::Future(yv, ye, _)) => {
            unify_into(xv, yv, vars, aliases, bindings)
                .and(unify_into(xe, ye, vars, aliases, bindings))
        }
        // Unions compare by covering on their member sets (ACI), so a non-union
        // operand is treated as the singleton union `{S}` and routed through the same
        // covering. That decides a variable- or literal-bearing union opposite a single
        // type precisely: `1 | T` vs `int` overlaps at `T = int` (`1 <: int` collapses),
        // `true | T` vs `bool` at `T = false`, and `C | T` vs `C` at `T = C`
        // (idempotency), while `D | T` vs `C` (with `C ≠ D`) stays disjoint. Routing
        // *every* union here — not only `Union` vs `Union` — is what stops a var-bearing
        // union opposite a single type from being wrongly judged disjoint by the
        // wildcard arm below. (The finite-base residual `true | T` / `Cmp.Less | T` is
        // covered precisely via `cover`'s `literal <: base` / `variant <: enum` oracle.)
        (Ty::Union(xm, _), Ty::Union(ym, _)) => {
            unify_union_members(xm, ym, vars, aliases, bindings)
        }
        (Ty::Union(xm, _), _) => {
            unify_union_members(xm, std::slice::from_ref(&y), vars, aliases, bindings)
        }
        (_, Ty::Union(ym, _)) => {
            unify_union_members(std::slice::from_ref(&x), ym, vars, aliases, bindings)
        }
        // Everything else is disjoint under equality: equal subjects already unified
        // above (the normalizer), unions are handled above, and any remaining pair is
        // distinct types. A literal and its base are *distinct* here — `unify_into`
        // decides equality, so `int ≢ 1`; the `literal <: base` subtyping is `cover`'s
        // job (the covering oracle), not equality's. A same-constructor pair whose guard
        // failed (different name/arity) or whose other side isn't that constructor lands
        // here too. Every variant is named (no total wildcard) so a new `Ty` must be
        // classified here rather than silently treated as disjoint.
        (
            Ty::Class(..)
            | Ty::Interface(..)
            | Ty::List(..)
            | Ty::EvolvingList(..)
            | Ty::Map { .. }
            | Ty::EvolvingMap(..)
            | Ty::Future(..)
            | Ty::Int { .. }
            | Ty::Bigint { .. }
            | Ty::Float { .. }
            | Ty::String { .. }
            | Ty::Bool { .. }
            | Ty::Null { .. }
            | Ty::Uint8Array { .. }
            | Ty::Media(..)
            | Ty::Literal(..)
            | Ty::Enum(..)
            | Ty::EnumVariant(..)
            | Ty::Function { .. }
            | Ty::RustType { .. }
            | Ty::Type { .. }
            | Ty::Resource { .. }
            | Ty::PromptAst { .. }
            | Ty::Void { .. }
            | Ty::WatchAccessor(..)
            | Ty::TypeAlias(..)
            | Ty::TypeVar(..)
            | Ty::AssociatedTypeProjection { .. }
            | Ty::BuiltinUnknown { .. }
            | Ty::Never { .. }
            | Ty::Unknown { .. }
            | Ty::Error { .. },
            _,
        ) => Overlap::No,
    }
}

/// Unify two equal-length type-argument lists position-wise (a conjunction): a
/// disjoint position short-circuits to `No`; an undecidable one downgrades the
/// result to `Unknown`.
fn unify_all(
    xs: &[Ty],
    ys: &[Ty],
    vars: &[Name],
    aliases: &std::collections::HashMap<QualifiedTypeName, Ty>,
    bindings: &mut TypeBindings,
) -> Overlap {
    debug_assert_eq!(xs.len(), ys.len());
    let mut result = Overlap::Yes;
    for (x, y) in xs.iter().zip(ys.iter()) {
        match unify_into(x, y, vars, aliases, bindings) {
            Overlap::No => return Overlap::No,
            Overlap::Unknown => result = Overlap::Unknown,
            Overlap::Yes => {}
        }
    }
    result
}

/// Unify two interface-existentials' associated bindings (`Item=…`) — a conjunction
/// over names common to both. A name on only one side does not constrain (conservative:
/// well-formed existentials specify the same associated types, so this is exact in
/// practice; a missing name only ever loosens toward `Yes`, never a wrong `No`).
fn unify_associated_bindings(
    xb: &[(Name, Ty)],
    yb: &[(Name, Ty)],
    vars: &[Name],
    aliases: &std::collections::HashMap<QualifiedTypeName, Ty>,
    bindings: &mut TypeBindings,
) -> Overlap {
    let mut result = Overlap::Yes;
    for (name, xty) in xb {
        if let Some((_, yty)) = yb.iter().find(|(n, _)| n == name) {
            match unify_into(xty, yty, vars, aliases, bindings) {
                Overlap::No => return Overlap::No,
                Overlap::Unknown => result = Overlap::Unknown,
                Overlap::Yes => {}
            }
        }
    }
    result
}

/// Unify two unions — the `(Union, Union)` arm of `unify_into`. The overlap check asks
/// whether two impl subjects share a common instance; at a union position that is: does
/// some substitution of the unification variables unify these two unions into one type?
/// Returns the tri-state `Overlap` (`Yes`/`No` when provable, `Unknown` when the search
/// is truncated). Only `cover_search` writes to `bindings` (committing the bindings it
/// discovers on a `Yes`); the all-ground and both-bare special-cases return `Yes` without
/// touching `bindings`, and `No`/`Unknown` leave it unchanged — so a `Yes` does not imply
/// a witness substitution was recorded.
///
/// Unions are ACI (associative, commutative, idempotent), hence *sets*: equality
/// ignores order and duplicates, and a bare top-level type-variable member can be any
/// type (including a union), so it may absorb several members at once. Deciding this is
/// therefore ACI-unification, which is NP-hard; the search is bounded by
/// `MAX_OVERLAP_SEARCH_STEPS`, past which it yields `Unknown` ("type too complex;
/// simplify it") - the only thing that ever produces `Unknown`.
///
/// Members are matched by **covering**, not a one-to-one pairing: idempotency
/// (`A | A = A`) lets several members collapse onto one, so a member need only unify with
/// *some* member of the other side (many-to-one), not a private partner. An injective or
/// bijective match would unsoundly reject collapses like `{C<T>, C<U>, C<W>}` vs
/// `{C<X>, C<Y>}` (unifiable via `T=X, U=Y, W=X`) — do not "simplify" it back. Cheap
/// special-cases run first (all-ground reduces to set equality; a bare variable on *each*
/// side is immediately `Yes`, as each absorbs the other); otherwise the covering
/// obligations — one-directional when a bare variable absorbs one side, mutual when
/// neither does — are solved jointly by `cover_search`.
fn unify_union_members(
    xs: &[Ty],
    ys: &[Ty],
    vars: &[Name],
    aliases: &std::collections::HashMap<QualifiedTypeName, Ty>,
    bindings: &mut TypeBindings,
) -> Overlap {
    // No variable on either side: overlap is exact set equality (any size).
    if let Some(result) = try_union_set_equality(xs, ys, vars, aliases) {
        return result;
    }
    // A bare variable on *each* side absorbs the other entirely ⇒ always overlap.
    if let Some(result) = try_union_mutual_absorption(xs, ys, vars) {
        return result;
    }

    let rigids = |members: &[Ty]| -> Vec<Ty> {
        members
            .iter()
            .filter(|m| !is_bare_var(m, vars))
            .cloned()
            .collect()
    };
    let xn = rigids(xs);
    let yn = rigids(ys);
    let x_has_bare = xs.iter().any(|m| is_bare_var(m, vars));
    let y_has_bare = ys.iter().any(|m| is_bare_var(m, vars));

    // Build the covering obligations. A side with a bare variable absorbs the *other*
    // side's leftovers, so only its own rigids must be covered (one direction). With no
    // bare variable anywhere, equality requires *mutual* covering (both directions),
    // solved jointly so the substitution chosen for one direction is consistent with
    // the other (a greedy two-pass would be unsound).
    let mut obligations: Vec<(Ty, Vec<Ty>)> = Vec::new();
    if x_has_bare {
        for m in &xn {
            obligations.push((m.clone(), yn.clone()));
        }
    } else if y_has_bare {
        for m in &yn {
            obligations.push((m.clone(), xn.clone()));
        }
    } else {
        for m in &xn {
            obligations.push((m.clone(), yn.clone()));
        }
        for m in &yn {
            obligations.push((m.clone(), xn.clone()));
        }
    }

    let mut budget = MAX_OVERLAP_SEARCH_STEPS;
    cover_search(&obligations, vars, aliases, bindings, &mut budget)
}

/// Special case: with no variable on either side, overlap is exact set equality —
/// decidable precisely at any size with no search. `None` if a variable is present.
fn try_union_set_equality(
    xs: &[Ty],
    ys: &[Ty],
    vars: &[Name],
    aliases: &std::collections::HashMap<QualifiedTypeName, Ty>,
) -> Option<Overlap> {
    let has_var = |members: &[Ty]| members.iter().any(|m| contains_bound_typevar(m, vars));
    if has_var(xs) || has_var(ys) {
        return None;
    }
    Some(if unions_set_equal(xs, ys, aliases) {
        Overlap::Yes
    } else {
        Overlap::No
    })
}

/// Special case: a bare (top-level) variable can be instantiated to a *union*, so
/// it absorbs any set of the other side's members. If both sides have one, a
/// common instance always exists — a proven overlap, regardless of whether any
/// package instantiates it that way. `None` if either side lacks a bare variable.
fn try_union_mutual_absorption(xs: &[Ty], ys: &[Ty], vars: &[Name]) -> Option<Overlap> {
    let has_bare = |members: &[Ty]| members.iter().any(|m| is_bare_var(m, vars));
    (has_bare(xs) && has_bare(ys)).then_some(Overlap::Yes)
}

/// True iff `m` is a bare unification-variable member — a top-level type variable
/// in `vars`, as opposed to a variable nested inside a constructor.
fn is_bare_var(m: &Ty, vars: &[Name]) -> bool {
    matches!(m, Ty::TypeVar(n, _) if vars.contains(n))
}

/// Whether two ground unions denote the same set of types (order-insensitive).
/// Members are de-duplicated, so equal cardinality plus "every member of `xs`
/// has an equal member in `ys`" implies a bijection.
fn unions_set_equal(
    xs: &[Ty],
    ys: &[Ty],
    aliases: &std::collections::HashMap<QualifiedTypeName, Ty>,
) -> bool {
    xs.len() == ys.len()
        && xs.iter().all(|x| {
            ys.iter()
                .any(|y| normalize::is_same_normalized_type(x, y, aliases))
        })
}

/// Whether `member` can be a *subtype* of `candidate` under some substitution
/// (committing the bindings on success). Covering uses subtype, not equality, because a
/// union member lies inside the other union iff it is a subtype of one of its members.
///
/// `unify_into` supplies the bulk (equality ⟹ subtype: same-constructor members,
/// variable binding, ground equality, and same-name interfaces via their generic args +
/// associated bindings). On top of it sit the two top-level subtypings that invariance
/// leaves above the constructor level — `literal <: base` and `variant <: enum`
/// (`is_literal_subtype`) — and the pairs `unify_into` cannot decide without the impl
/// registry — a concrete type vs an interface, two *different* interfaces, an opaque
/// `$rust_type` (`needs_conservative_membership`) — which are treated conservatively as
/// a *possible* overlap (`Yes`), never a wrong `No`. All of these arms only fire when
/// `unify_into` already returned `No` without binding anything, so they leave `bindings`
/// untouched. Registry-precise `C implements I` / `I requires J` is a later refinement.
fn cover(
    member: &Ty,
    candidate: &Ty,
    vars: &[Name],
    aliases: &std::collections::HashMap<QualifiedTypeName, Ty>,
    bindings: &mut TypeBindings,
) -> Overlap {
    match unify_into(member, candidate, vars, aliases, bindings) {
        Overlap::No
            if is_literal_subtype(member, candidate)
                || needs_conservative_membership(member, candidate) =>
        {
            Overlap::Yes
        }
        decided => decided,
    }
}

/// Whether `member` is a top-level subtype of `candidate` by the only subtypings
/// invariance leaves above the constructor level: a literal is a subtype of its base
/// primitive (`1 <: int`), and an enum variant is a subtype of its enum
/// (`Color.Red <: Color`). Directional — `int` is *not* a subtype of `1`.
fn is_literal_subtype(member: &Ty, candidate: &Ty) -> bool {
    match (member, candidate) {
        (Ty::Literal(Literal::Int(_), _, _), Ty::Int { .. })
        | (Ty::Literal(Literal::Bigint(_), _, _), Ty::Bigint { .. })
        | (Ty::Literal(Literal::Float(_), _, _), Ty::Float { .. })
        | (Ty::Literal(Literal::String(_), _, _), Ty::String { .. })
        | (Ty::Literal(Literal::Bool(_), _, _), Ty::Bool { .. }) => true,
        (Ty::EnumVariant(variant_enum, _, _), Ty::Enum(base_enum, _)) => variant_enum == base_enum,
        _ => false,
    }
}

/// Whether this pair's subtyping cannot be decided without the impl registry, so the
/// covering oracle must fall back to a conservative `Yes`: a concrete type vs an
/// interface (needs `C implements I`), two *different* interfaces (needs `I requires J`),
/// or an opaque `$rust_type`. Two interfaces of the *same* name are decided precisely by
/// `unify_into` (generic args + associated bindings), so they are **not** conservative.
fn needs_conservative_membership(a: &Ty, b: &Ty) -> bool {
    if matches!(a, Ty::RustType { .. }) || matches!(b, Ty::RustType { .. }) {
        return true;
    }
    match (a, b) {
        (Ty::Interface(qa, ..), Ty::Interface(qb, ..)) => qa != qb,
        (Ty::Interface(..), _) | (_, Ty::Interface(..)) => true,
        _ => false,
    }
}

/// Solve the covering obligations jointly: is there one substitution under which every
/// `(member, candidates)` obligation holds — `member` a subtype of some candidate? A
/// candidate may cover several members (covering is many-to-one), and obligations may
/// carry different candidate sets, so this serves both the one-directional case (a bare
/// var absorbs one side) and the mutual case (no bare var).
///
/// Picks the most-constrained obligation first (MRV): a single viable candidate is
/// forced (unit propagation), none fails fast (`No`), otherwise it backtracks over the
/// candidates. `budget` caps the number of `cover` trials; exhausting it yields
/// `Overlap::Unknown` — the NP-hard ceiling — so easy cases (e.g. linear members)
/// decide exactly while pathological ones degrade to "simplify the type".
fn cover_search(
    obligations: &[(Ty, Vec<Ty>)],
    vars: &[Name],
    aliases: &std::collections::HashMap<QualifiedTypeName, Ty>,
    bindings: &mut TypeBindings,
    budget: &mut usize,
) -> Overlap {
    if obligations.is_empty() {
        return Overlap::Yes;
    }

    // Choose the obligation with the fewest viable candidates; fail fast on a member
    // that nothing can cover.
    let mut chosen: Option<(usize, Vec<usize>)> = None;
    for (oi, (member, candidates)) in obligations.iter().enumerate() {
        let mut viable: Vec<usize> = Vec::new();
        for (ci, candidate) in candidates.iter().enumerate() {
            if *budget == 0 {
                return Overlap::Unknown;
            }
            *budget -= 1;
            let mut trial = bindings.clone();
            if cover(member, candidate, vars, aliases, &mut trial) != Overlap::No {
                viable.push(ci);
            }
        }
        if viable.is_empty() {
            return Overlap::No;
        }
        let improves = match &chosen {
            None => true,
            Some((_, best)) => viable.len() < best.len(),
        };
        if improves {
            let forced = viable.len() == 1;
            chosen = Some((oi, viable));
            if forced {
                break; // can't be more constrained than a single candidate
            }
        }
    }

    let (oi, viable) =
        chosen.unwrap_or_else(|| unreachable!("non-empty obligations always yield a choice"));
    let (member, candidates) = &obligations[oi];
    let rest: Vec<(Ty, Vec<Ty>)> = obligations
        .iter()
        .enumerate()
        .filter(|(i, _)| *i != oi)
        .map(|(_, o)| o.clone())
        .collect();

    let mut result = Overlap::No;
    for ci in viable {
        let mut trial = bindings.clone();
        // `ci` was viable, so this is `Yes`/`Unknown`, never `No`.
        let here = cover(member, &candidates[ci], vars, aliases, &mut trial);
        match here.and(cover_search(&rest, vars, aliases, &mut trial, budget)) {
            Overlap::Yes => {
                *bindings = trial;
                return Overlap::Yes;
            }
            Overlap::Unknown => result = Overlap::Unknown,
            Overlap::No => {}
        }
    }
    result
}

/// Resolve a type through the current bindings: while it is a bound unification
/// variable, replace it with its binding (so callers see the representative).
fn chase_var(ty: &Ty, vars: &[Name], bindings: &TypeBindings) -> Ty {
    let mut current = ty.clone();
    while let Ty::TypeVar(name, _) = &current {
        if vars.contains(name)
            && let Some(bound) = bindings.get(name)
        {
            current = bound.clone();
        } else {
            break;
        }
    }
    current
}

/// Bind unification variable `n` to (already-chased) `t`, unifying with any
/// existing binding and rejecting cyclic bindings (the occurs check).
fn bind_unify_var(
    n: &Name,
    t: &Ty,
    vars: &[Name],
    aliases: &std::collections::HashMap<QualifiedTypeName, Ty>,
    bindings: &mut TypeBindings,
) -> Overlap {
    if let Ty::TypeVar(tn, _) = t
        && tn == n
    {
        return Overlap::Yes;
    }
    if let Some(existing) = bindings.get(n).cloned() {
        return unify_into(&existing, t, vars, aliases, bindings);
    }
    if occurs_in(n, t, vars, bindings) {
        return Overlap::No;
    }
    bindings.insert(n.clone(), t.clone());
    Overlap::Yes
}

/// Occurs check: does unification variable `n` appear anywhere in `t` (chasing
/// bound vars)? A positive answer means binding `n := t` would build an
/// infinite type, so the two subjects have no finite common instance.
fn occurs_in(n: &Name, t: &Ty, vars: &[Name], bindings: &TypeBindings) -> bool {
    let t = chase_var(t, vars, bindings);
    match &t {
        Ty::TypeVar(m, _) => m == n,
        Ty::Class(_, args, _) | Ty::Union(args, _) => {
            args.iter().any(|a| occurs_in(n, a, vars, bindings))
        }
        Ty::Interface(_, args, assoc, _) => {
            args.iter().any(|a| occurs_in(n, a, vars, bindings))
                || assoc.iter().any(|(_, ty)| occurs_in(n, ty, vars, bindings))
        }
        Ty::List(inner, _) | Ty::EvolvingList(inner, _) | Ty::WatchAccessor(inner, _) => {
            occurs_in(n, inner, vars, bindings)
        }
        Ty::Map {
            key: k, value: v, ..
        }
        | Ty::EvolvingMap(k, v, _)
        | Ty::Future(k, v, _) => occurs_in(n, k, vars, bindings) || occurs_in(n, v, vars, bindings),
        Ty::AssociatedTypeProjection {
            base, interface, ..
        } => {
            occurs_in(n, base, vars, bindings)
                || interface
                    .as_ref()
                    .is_some_and(|i| occurs_in(n, i, vars, bindings))
        }
        Ty::Function {
            generic_param_bounds,
            params,
            ret,
            throws,
            ..
        } => {
            generic_param_bounds
                .iter()
                .any(|b| b.as_ref().is_some_and(|b| occurs_in(n, b, vars, bindings)))
                || params
                    .iter()
                    .any(|FunctionParamTy { ty, .. }| occurs_in(n, ty, vars, bindings))
                || occurs_in(n, ret, vars, bindings)
                || occurs_in(n, throws, vars, bindings)
        }
        // Leaf types hold no nested type, so a variable cannot occur in them. Listed
        // explicitly (no total wildcard) so a new `Ty` variant must be classified here.
        Ty::Int { .. }
        | Ty::Bigint { .. }
        | Ty::Float { .. }
        | Ty::String { .. }
        | Ty::Bool { .. }
        | Ty::Null { .. }
        | Ty::Uint8Array { .. }
        | Ty::Media(..)
        | Ty::Literal(..)
        | Ty::Enum(..)
        | Ty::EnumVariant(..)
        | Ty::RustType { .. }
        | Ty::Type { .. }
        | Ty::Resource { .. }
        | Ty::PromptAst { .. }
        | Ty::Void { .. }
        | Ty::TypeAlias(..)
        | Ty::BuiltinUnknown { .. }
        | Ty::Never { .. }
        | Ty::Unknown { .. }
        | Ty::Error { .. } => false,
    }
}

fn strip_interface_assoc(ty: &Ty) -> Ty {
    match ty {
        Ty::Interface(qtn, args, _, attr) => {
            Ty::Interface(qtn.clone(), args.clone(), Vec::new(), attr.clone())
        }
        other => other.clone(),
    }
}

fn same_in_body_origin(a: &InterfaceImplRule, b: &InterfaceImplRule) -> bool {
    matches!(
        (&a.origin, &b.origin),
        (
            InterfaceImplOrigin::InBodyClass { class_qtn: ca },
            InterfaceImplOrigin::InBodyClass { class_qtn: cb },
        ) if ca == cb
    )
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
    let mut diagnostics = Vec::new();
    let Ty::Interface(qtn, _, _, _) = crate::lower_type_expr::lower_type_expr_in_ns(
        db,
        target,
        pkg_items,
        current_ns,
        &[],
        &mut diagnostics,
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

/// Walk the transitive `extends` closure of `root_iface` and return every
/// interface in it (including `root_iface` itself), in BFS order so the
/// receiver appears before its parents. Cycles are skipped silently — they
/// are reported elsewhere (E0118).
pub fn interface_closure_locs<'db>(
    db: &'db dyn crate::Db,
    root_iface: baml_compiler2_hir::loc::InterfaceLoc<'db>,
    _pkg_items: &baml_compiler2_hir::package::PackageItems<'db>,
    _current_ns: &[Name],
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
/// Walk the transitive `requires` closure of `root_iface`, carrying generic
/// arguments and associated type bindings for each interface in the closure.
/// For example, `Child requires Parent<Item = int>` yields `(Child, [], [])`
/// and `(Parent, [], [(Item, int)])`.
pub fn interface_closure_locs_with_args_and_assoc<'db>(
    db: &'db dyn crate::Db,
    root_iface: baml_compiler2_hir::loc::InterfaceLoc<'db>,
    root_args: &[Ty],
    root_associated_bindings: &[(Name, Ty)],
    _pkg_items: &baml_compiler2_hir::package::PackageItems<'db>,
    _current_ns: &[Name],
) -> Vec<InterfaceClosureEntry<'db>> {
    let mut out: Vec<InterfaceClosureEntry<'db>> = Vec::new();
    let mut seen: FxHashSet<InterfaceClosureEntry<'db>> = FxHashSet::default();
    let mut queue: std::collections::VecDeque<InterfaceClosureQueueEntry<'db>> =
        std::collections::VecDeque::new();
    queue.push_back((
        root_iface,
        root_args.to_vec(),
        root_associated_bindings.to_vec(),
        FxHashSet::default(),
    ));

    while let Some((loc, args, associated_bindings, ancestors)) = queue.pop_front() {
        if ancestors.contains(&loc) {
            continue;
        }
        let tree = baml_compiler2_hir::file_item_tree(db, loc.file(db));
        let Some(iface) = tree.interfaces.get(&loc.id(db)) else {
            continue;
        };
        let pkg_info = baml_compiler2_hir::file_package::file_package(db, loc.file(db));
        let pkg_id = PackageId::new(db, pkg_info.package.clone());
        let parent_pkg_items = baml_compiler2_ppir::package_items(db, pkg_id);
        let mut diags = Vec::new();
        let associated_bindings = complete_interface_associated_bindings_from_tys(
            db,
            iface,
            &args,
            &associated_bindings,
            parent_pkg_items,
            &pkg_info.namespace_path,
            &mut diags,
        );
        if !seen.insert((loc, args.clone(), associated_bindings.clone())) {
            continue;
        }
        out.push((loc, args.clone(), associated_bindings.clone()));
        let mut child_ancestors = ancestors.clone();
        child_ancestors.insert(loc);

        let mut bindings = generics::bind_type_vars(&iface.generic_params, &args);
        for (name, ty) in &associated_bindings {
            bindings.insert(name.clone(), ty.clone());
        }

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
                baml_compiler2_ast::TypeExpr::Path { generic_args, .. } => {
                    let mut diags = Vec::new();
                    generic_args
                        .iter()
                        .map(|arg| {
                            generics::lower_type_expr_with_generics(
                                db,
                                arg,
                                parent_pkg_items,
                                &pkg_info.namespace_path,
                                &bindings,
                                &mut diags,
                            )
                        })
                        .collect()
                }
                _ => Vec::new(),
            };
            let parent_tree = baml_compiler2_hir::file_item_tree(db, parent_loc.file(db));
            let Some(parent_iface) = parent_tree.interfaces.get(&parent_loc.id(db)) else {
                continue;
            };
            let parent_pkg =
                baml_compiler2_hir::file_package::file_package(db, parent_loc.file(db));
            let parent_iface_pkg_id = PackageId::new(db, parent_pkg.package.clone());
            let parent_iface_pkg_items =
                baml_compiler2_ppir::package_items(db, parent_iface_pkg_id);
            let (parent_explicit_assoc, parent_binding_ns): (
                &[baml_compiler2_ast::AssociatedTypeBinding],
                &[Name],
            ) = match &parent.expr {
                baml_compiler2_ast::TypeExpr::Path {
                    associated_type_bindings,
                    ..
                } => (
                    associated_type_bindings.as_slice(),
                    &pkg_info.namespace_path,
                ),
                _ => (&[][..], &pkg_info.namespace_path),
            };
            let parent_assoc = lower_interface_type_associated_bindings(
                InterfaceTypeAssocLowering {
                    db,
                    iface: parent_iface,
                    interface_args: &parent_args,
                    explicit_associated_bindings: parent_explicit_assoc,
                    iface_pkg_items: parent_iface_pkg_items,
                    binding_pkg_items: parent_pkg_items,
                    iface_namespace_path: &parent_pkg.namespace_path,
                    binding_namespace_path: parent_binding_ns,
                    outer_bindings: &bindings,
                },
                &mut diags,
            );
            queue.push_back((
                parent_loc,
                parent_args,
                parent_assoc,
                child_ancestors.clone(),
            ));
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
        Ty::Class(qtn(namespace, name), args, TyAttr::default())
    }

    fn interface(name: &str, args: Vec<Ty>) -> Ty {
        Ty::Interface(qtn(&[], name), args, vec![], TyAttr::default())
    }

    fn interface_with_assoc(name: &str, assoc: Vec<(&str, Ty)>) -> Ty {
        Ty::Interface(
            qtn(&[], name),
            vec![],
            assoc
                .into_iter()
                .map(|(name, ty)| (Name::new(name), ty))
                .collect(),
            TyAttr::default(),
        )
    }

    fn int() -> Ty {
        Ty::Int {
            attr: TyAttr::default(),
        }
    }

    fn string() -> Ty {
        Ty::String {
            attr: TyAttr::default(),
        }
    }

    fn type_var(name: &str) -> Ty {
        Ty::TypeVar(Name::new(name), TyAttr::default())
    }

    fn int_literal(n: i64) -> Ty {
        Ty::Literal(
            Literal::Int(n),
            crate::ty::Freshness::Regular,
            TyAttr::default(),
        )
    }

    fn bool_literal(b: bool) -> Ty {
        Ty::Literal(
            Literal::Bool(b),
            crate::ty::Freshness::Regular,
            TyAttr::default(),
        )
    }

    fn union(members: Vec<Ty>) -> Ty {
        Ty::Union(members, TyAttr::default())
    }

    fn bool_ty() -> Ty {
        Ty::Bool {
            attr: TyAttr::default(),
        }
    }

    fn builtin_unknown() -> Ty {
        Ty::BuiltinUnknown {
            attr: TyAttr::default(),
        }
    }

    fn enum_ty(name: &str) -> Ty {
        Ty::Enum(qtn(&[], name), TyAttr::default())
    }

    fn enum_variant(enum_name: &str, variant: &str) -> Ty {
        Ty::EnumVariant(qtn(&[], enum_name), Name::new(variant), TyAttr::default())
    }

    /// Stub enum schema for `nf` tests: `Cmp` has variants `Less`, `Equal`, `More`.
    fn stub_enum_variants(qtn: &QualifiedTypeName) -> Option<Vec<Name>> {
        (qtn.name().as_str() == "Cmp")
            .then(|| vec![Name::new("Less"), Name::new("Equal"), Name::new("More")])
    }

    fn associated_projection(base: Ty, interface: Ty, member: &str) -> Ty {
        Ty::AssociatedTypeProjection {
            base: Box::new(base),
            interface: Some(Box::new(interface)),
            member: Name::new(member),
            attr: TyAttr::default(),
        }
    }

    fn never() -> Ty {
        Ty::Never {
            attr: TyAttr::default(),
        }
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
            attr: TyAttr::default(),
        }
    }

    #[test]
    fn implementation_key_for_ty_canonicalizes_union_members() {
        let int = Ty::Int {
            attr: TyAttr::default(),
        };
        let string = Ty::String {
            attr: TyAttr::default(),
        };
        let lhs = Ty::Union(vec![int.clone(), string.clone()], TyAttr::default());
        let rhs = Ty::Union(vec![string, int], TyAttr::default());

        assert_eq!(
            implementation_key_for_ty(&lhs),
            implementation_key_for_ty(&rhs)
        );
    }

    #[test]
    fn implementation_key_for_ty_dedupes_union_members() {
        let int = Ty::Int {
            attr: TyAttr::default(),
        };
        let duplicated = Ty::Union(vec![int.clone(), int.clone()], TyAttr::default());

        assert_eq!(
            implementation_key_for_ty(&duplicated),
            Some(Ty::Union(vec![int], TyAttr::default()))
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
        let pattern = interface(
            "Container",
            vec![Ty::List(Box::new(type_var("T")), TyAttr::default())],
        );
        let actual = interface(
            "Container",
            vec![Ty::List(Box::new(int()), TyAttr::default())],
        );
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
    fn compatibility_type_implements_respects_generic_bounds() {
        let dog_qtn = qtn(&[], "Dog");
        let box_qtn = qtn(&[], "Box");
        let named_qtn = qtn(&[], "Named");
        let printable_qtn = qtn(&[], "Printable");
        let rules = vec![
            InterfaceImplRule {
                generic_params: vec![],
                generic_param_bounds: vec![],
                for_ty_pattern: Ty::Class(dog_qtn.clone(), vec![], TyAttr::default()),
                interface_ty: Ty::Interface(named_qtn.clone(), vec![], vec![], TyAttr::default()),
                origin: InterfaceImplOrigin::InBodyClass { class_qtn: dog_qtn },
                source_span: None,
            },
            InterfaceImplRule {
                generic_params: vec![Name::new("T")],
                generic_param_bounds: vec![Some(Ty::Interface(
                    named_qtn,
                    vec![],
                    vec![],
                    TyAttr::default(),
                ))],
                for_ty_pattern: Ty::Class(box_qtn.clone(), vec![type_var("T")], TyAttr::default()),
                interface_ty: Ty::Interface(
                    printable_qtn.clone(),
                    vec![],
                    vec![],
                    TyAttr::default(),
                ),
                origin: InterfaceImplOrigin::InBodyClass {
                    class_qtn: box_qtn.clone(),
                },
                source_span: None,
            },
        ];
        let views = derive_compatibility_views(&rules, std::slice::from_ref(&box_qtn));
        let mut registry = ImplementsRegistry {
            interface_impl_rule_index: InterfaceImplRuleIndex::from_rules(&rules),
            interface_impl_rules: rules,
            class_implements: views.class_implements,
            type_implements: views.type_implements,
            blanket_class_implements: views.blanket_class_implements,
            implements_type_args: views.implements_type_args,
            type_implements_type_args: views.type_implements_type_args,
            interface_requires: FxHashMap::default(),
        };

        assert!(!registry.implements(&box_qtn, &printable_qtn));
        assert!(registry.type_implements(
            &Ty::Class(
                box_qtn.clone(),
                vec![Ty::Class(qtn(&[], "Dog"), vec![], TyAttr::default())],
                TyAttr::default(),
            ),
            &printable_qtn,
        ));
        assert!(!registry.type_implements(
            &Ty::Class(box_qtn, vec![int()], TyAttr::default()),
            &printable_qtn,
        ));

        registry.class_implements.clear();
        assert!(!registry.type_implements(
            &Ty::Class(qtn(&[], "Box"), vec![int()], TyAttr::default()),
            &printable_qtn,
        ));
    }

    #[test]
    fn contains_bound_typevar_checks_interface_associated_bindings() {
        let ty = Ty::Interface(
            qtn(&[], "Source"),
            vec![],
            vec![(
                Name::new("Item"),
                Ty::List(Box::new(type_var("T")), TyAttr::default()),
            )],
            TyAttr::default(),
        );

        assert!(contains_bound_typevar(&ty, &[Name::new("T")]));
        assert!(!contains_bound_typevar(&ty, &[Name::new("U")]));
    }

    #[test]
    fn contains_generic_function_binders_checks_interface_associated_bindings() {
        let ty = Ty::Interface(
            qtn(&[], "Source"),
            vec![],
            vec![(
                Name::new("Item"),
                Ty::List(
                    Box::new(function(vec!["T"], vec![None], vec![type_var("T")], int())),
                    TyAttr::default(),
                ),
            )],
            TyAttr::default(),
        );

        assert!(contains_generic_function_binders(&ty));
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
        let pattern = Ty::Union(vec![type_var("T"), string()], TyAttr::default());
        let actual = Ty::Union(vec![string(), int()], TyAttr::default());
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
            interface_impl_rule_index: InterfaceImplRuleIndex::default(),
            class_implements: FxHashMap::default(),
            type_implements: FxHashMap::default(),
            blanket_class_implements: Vec::new(),
            implements_type_args: FxHashMap::default(),
            type_implements_type_args: FxHashMap::default(),
            interface_requires: FxHashMap::default(),
        };
        let rule = InterfaceImplRule {
            generic_params: vec![Name::new("T")],
            generic_param_bounds: vec![None],
            for_ty_pattern: class(&[], "Wrapper", vec![type_var("T")]),
            interface_ty: interface("Container", vec![type_var("T")]),
            origin: InterfaceImplOrigin::OutOfBody,
            source_span: None,
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

    #[test]
    fn rule_matches_actual_accepts_projection_binding_when_subtype_proves_equivalent() {
        let registry = ImplementsRegistry {
            interface_impl_rules: Vec::new(),
            interface_impl_rule_index: InterfaceImplRuleIndex::default(),
            class_implements: FxHashMap::default(),
            type_implements: FxHashMap::default(),
            blanket_class_implements: Vec::new(),
            implements_type_args: FxHashMap::default(),
            type_implements_type_args: FxHashMap::default(),
            interface_requires: FxHashMap::default(),
        };
        let source = interface("Source", vec![]);
        let projected_item = associated_projection(type_var("T"), source, "Item");
        let rule = InterfaceImplRule {
            generic_params: vec![Name::new("T")],
            generic_param_bounds: vec![None],
            for_ty_pattern: class(&[], "Wrapped", vec![type_var("T")]),
            interface_ty: interface_with_assoc("Renderable", vec![("Output", projected_item)]),
            origin: InterfaceImplOrigin::OutOfBody,
            source_span: None,
        };
        let actual = class(&[], "Wrapped", vec![class(&[], "TextSource", vec![])]);
        let requested = interface_with_assoc("Renderable", vec![("Output", string())]);

        assert!(
            registry
                .rule_matches_actual(
                    &rule,
                    &actual,
                    &requested,
                    &std::collections::HashMap::default(),
                    |lhs, rhs| {
                        matches!(lhs, Ty::AssociatedTypeProjection { member, .. } if member.as_str() == "Item")
                            && normalize::is_same_normalized_type(
                                rhs,
                                &string(),
                                &std::collections::HashMap::default(),
                            )
                            || matches!(rhs, Ty::AssociatedTypeProjection { member, .. } if member.as_str() == "Item")
                                && normalize::is_same_normalized_type(
                                    lhs,
                                    &string(),
                                    &std::collections::HashMap::default(),
                                )
                    },
                )
                .is_some()
        );
    }

    #[test]
    fn union_overlap_both_bare_vars_is_yes() {
        // Each bare variable can absorb the other side, so a common instance
        // always exists.
        let vars = vec![Name::new("T"), Name::new("V")];
        let aliases = std::collections::HashMap::default();
        let mut bindings = TypeBindings::default();
        let xs = vec![int(), type_var("T")];
        let ys = vec![string(), type_var("V")];
        assert_eq!(
            unify_union_members(&xs, &ys, &vars, &aliases, &mut bindings),
            Overlap::Yes
        );
    }

    #[test]
    fn union_overlap_variable_absorbs_extra_members_is_yes() {
        // `{int, T}` vs `{int, string, Foo}`: instantiating `T = string | Foo`
        // makes them the same union — a *provable* overlap, not indeterminate.
        let vars = vec![Name::new("T")];
        let aliases = std::collections::HashMap::default();
        let mut bindings = TypeBindings::default();
        let xs = vec![int(), type_var("T")];
        let ys = vec![int(), string(), class(&[], "Foo", vec![])];
        assert_eq!(
            unify_union_members(&xs, &ys, &vars, &aliases, &mut bindings),
            Overlap::Yes
        );
    }

    #[test]
    fn union_overlap_unmatchable_rigid_member_is_no() {
        // `{int, T}` vs `{string, Foo}`: `int` matches no member on the right and
        // `T` cannot make it appear there, so there is no common instance.
        let vars = vec![Name::new("T")];
        let aliases = std::collections::HashMap::default();
        let mut bindings = TypeBindings::default();
        let xs = vec![int(), type_var("T")];
        let ys = vec![string(), class(&[], "Foo", vec![])];
        assert_eq!(
            unify_union_members(&xs, &ys, &vars, &aliases, &mut bindings),
            Overlap::No
        );
    }

    #[test]
    fn union_overlap_shared_rigid_members_extracted_is_yes() {
        // `{A1, A2, T}` vs `{A1, ..., A9}`: A1 and A2 each have a unique candidate,
        // so unit propagation peels them with no search and `T` absorbs the rest —
        // a proven overlap even though the candidate set is large.
        let vars = vec![Name::new("T")];
        let aliases = std::collections::HashMap::default();
        let mut bindings = TypeBindings::default();
        let xs = vec![
            class(&[], "A1", vec![]),
            class(&[], "A2", vec![]),
            type_var("T"),
        ];
        let ys: Vec<Ty> = (1..=9)
            .map(|i| class(&[], &format!("A{i}"), vec![]))
            .collect();
        assert_eq!(
            unify_union_members(&xs, &ys, &vars, &aliases, &mut bindings),
            Overlap::Yes
        );
    }

    #[test]
    fn union_overlap_linear_large_residual_is_yes() {
        // `{List<T>, List<U>, V}` vs `{List<A1>, ..., List<A9>}`: `T` and `U` are
        // independent (each in one member), so covering is many-to-one and a witness
        // exists (e.g. `T=U=A1`, with `V` absorbing the rest) — a *provable* overlap,
        // not an NP-hard cap. The search finds it in a few steps.
        let vars = vec![Name::new("T"), Name::new("U"), Name::new("V")];
        let aliases = std::collections::HashMap::default();
        let mut bindings = TypeBindings::default();
        let list = |inner: Ty| Ty::List(Box::new(inner), TyAttr::default());
        let xs = vec![list(type_var("T")), list(type_var("U")), type_var("V")];
        let ys: Vec<Ty> = (1..=9)
            .map(|i| list(class(&[], &format!("A{i}"), vec![])))
            .collect();
        assert_eq!(
            unify_union_members(&xs, &ys, &vars, &aliases, &mut bindings),
            Overlap::Yes
        );
    }

    #[test]
    fn union_overlap_collapsing_members_is_yes() {
        // `{List<T>, List<U>, List<W>}` vs `{List<A1>, List<A2>}` (no bare var):
        // idempotency lets two members collapse, so `T=A1, U=A2, W=A1` makes the unions
        // equal. An injective matcher (the old model) wrongly rejected this; covering
        // (many-to-one, mutual) accepts it.
        let vars = vec![Name::new("T"), Name::new("U"), Name::new("W")];
        let aliases = std::collections::HashMap::default();
        let mut bindings = TypeBindings::default();
        let list = |inner: Ty| Ty::List(Box::new(inner), TyAttr::default());
        let xs = vec![
            list(type_var("T")),
            list(type_var("U")),
            list(type_var("W")),
        ];
        let ys = vec![
            list(class(&[], "A1", vec![])),
            list(class(&[], "A2", vec![])),
        ];
        assert_eq!(
            unify_union_members(&xs, &ys, &vars, &aliases, &mut bindings),
            Overlap::Yes
        );
    }

    #[test]
    fn union_overlap_oversized_search_is_unknown() {
        // Unknown via the *breadth* of the candidate set (contrast the pigeonhole test,
        // which is Unknown via search *depth*): `{Pair<T,A1>, Pair<T,A2>}` share `T`, and
        // the candidates pair `A1`/`A2` with disjoint left classes so no single `T`
        // works — but just scanning the huge candidate list exhausts the step budget
        // before the search can prove it ⇒ Unknown.
        let vars = vec![Name::new("T")];
        let aliases = std::collections::HashMap::default();
        let mut bindings = TypeBindings::default();
        let pair = |a: Ty, b: Ty| class(&[], "Pair", vec![a, b]);
        let a1 = || class(&[], "A1", vec![]);
        let a2 = || class(&[], "A2", vec![]);
        let xs = vec![pair(type_var("T"), a1()), pair(type_var("T"), a2())];
        let mut ys: Vec<Ty> = Vec::new();
        for i in 0..2050 {
            ys.push(pair(class(&[], &format!("L{i}"), vec![]), a1()));
            ys.push(pair(class(&[], &format!("R{i}"), vec![]), a2()));
        }
        assert_eq!(
            unify_union_members(&xs, &ys, &vars, &aliases, &mut bindings),
            Overlap::Unknown
        );
    }

    #[test]
    fn union_overlap_literal_covered_by_base_is_yes() {
        // `{1, T}` vs `{int, string}`: the literal `1` is a *subtype* of `int`, so it is
        // covered (covering uses subtype, not equality), and `T` absorbs the rest.
        let vars = vec![Name::new("T")];
        let aliases = std::collections::HashMap::default();
        let mut bindings = TypeBindings::default();
        let xs = vec![int_literal(1), type_var("T")];
        let ys = vec![int(), string()];
        assert_eq!(
            unify_union_members(&xs, &ys, &vars, &aliases, &mut bindings),
            Overlap::Yes
        );
    }

    #[test]
    fn union_overlap_base_not_covered_by_literal_is_no() {
        // `{int, T}` vs `{1, string}`: subtyping is directional — `int` is *not* a
        // subtype of the literal `1`, and `T` is on the left, so nothing covers `int`.
        let vars = vec![Name::new("T")];
        let aliases = std::collections::HashMap::default();
        let mut bindings = TypeBindings::default();
        let xs = vec![int(), type_var("T")];
        let ys = vec![int_literal(1), string()];
        assert_eq!(
            unify_union_members(&xs, &ys, &vars, &aliases, &mut bindings),
            Overlap::No
        );
    }

    #[test]
    fn interface_distinct_associated_binding_is_disjoint() {
        // `I<Item=int>` and `I<Item=string>` are distinct existential types — the
        // associated binding is part of the type's identity (one `impl I` per concrete
        // type ⇒ one `Item`), so they are provably disjoint.
        let aliases = std::collections::HashMap::default();
        let mut bindings = TypeBindings::default();
        let a = interface_with_assoc("I", vec![("Item", int())]);
        let b = interface_with_assoc("I", vec![("Item", string())]);
        assert_eq!(
            unify_into(&a, &b, &[], &aliases, &mut bindings),
            Overlap::No
        );
    }

    #[test]
    fn interface_associated_binding_unifies_variable() {
        // `I<Item=int>` unifies with `I<Item=T>` by binding `T = int`.
        let vars = vec![Name::new("T")];
        let aliases = std::collections::HashMap::default();
        let mut bindings = TypeBindings::default();
        let a = interface_with_assoc("I", vec![("Item", int())]);
        let b = interface_with_assoc("I", vec![("Item", type_var("T"))]);
        assert_eq!(
            unify_into(&a, &b, &vars, &aliases, &mut bindings),
            Overlap::Yes
        );
        assert_eq!(bindings.get(&Name::new("T")), Some(&int()));
    }

    #[test]
    fn cover_distinct_interface_binding_is_not_conservative() {
        // Same-name interfaces are decided precisely by `unify_into`, so `cover` does not
        // fall back to the conservative `Yes` — `I<Item=int>` does not cover `I<Item=string>`.
        let aliases = std::collections::HashMap::default();
        let mut bindings = TypeBindings::default();
        let a = interface_with_assoc("I", vec![("Item", int())]);
        let b = interface_with_assoc("I", vec![("Item", string())]);
        assert_eq!(cover(&a, &b, &[], &aliases, &mut bindings), Overlap::No);
    }

    #[test]
    fn cover_class_vs_interface_is_conservative_yes() {
        // Whether a concrete class implements an interface needs the impl registry, which
        // the solver does not yet consult here, so `cover` conservatively reports a
        // possible overlap (`Yes`) — never a wrong `No`.
        let aliases = std::collections::HashMap::default();
        let mut bindings = TypeBindings::default();
        let a = class(&[], "A", vec![]);
        let b = interface("I", vec![]);
        assert_eq!(cover(&a, &b, &[], &aliases, &mut bindings), Overlap::Yes);
    }

    #[test]
    fn nf_drops_never_and_collapses() {
        assert_eq!(nf(&union(vec![int(), never()]), &stub_enum_variants), int());
    }

    #[test]
    fn nf_subsumes_literal_by_base() {
        assert_eq!(
            nf(&union(vec![int_literal(1), int()]), &stub_enum_variants),
            int()
        );
    }

    #[test]
    fn nf_folds_complete_bool() {
        assert_eq!(
            nf(
                &union(vec![bool_literal(true), bool_literal(false)]),
                &stub_enum_variants
            ),
            bool_ty()
        );
    }

    #[test]
    fn nf_folds_complete_enum() {
        let all = union(vec![
            enum_variant("Cmp", "Less"),
            enum_variant("Cmp", "Equal"),
            enum_variant("Cmp", "More"),
        ]);
        assert_eq!(nf(&all, &stub_enum_variants), enum_ty("Cmp"));
    }

    #[test]
    fn nf_keeps_partial_enum() {
        // Two of `Cmp`'s three variants — not a complete base, so not folded to `Cmp`.
        // `nf` canonicalizes member order, so the result lists the variants sorted
        // (`Equal` before `Less`), independent of the input order.
        let partial = union(vec![
            enum_variant("Cmp", "Less"),
            enum_variant("Cmp", "Equal"),
        ]);
        let canonical = union(vec![
            enum_variant("Cmp", "Equal"),
            enum_variant("Cmp", "Less"),
        ]);
        assert_eq!(nf(&partial, &stub_enum_variants), canonical);
    }

    #[test]
    fn nf_absorbs_unknown() {
        assert_eq!(
            nf(&union(vec![int(), builtin_unknown()]), &stub_enum_variants),
            builtin_unknown()
        );
    }

    #[test]
    fn nf_recurses_into_arguments() {
        let wrapped = class(&[], "Wrap", vec![union(vec![int(), never()])]);
        assert_eq!(
            nf(&wrapped, &stub_enum_variants),
            class(&[], "Wrap", vec![int()])
        );
    }

    #[test]
    fn union_with_var_conservatively_overlaps_enum() {
        // `Cmp.Less | T` opposite `Cmp`: the var could complete the enum
        // (`T = Cmp.Equal | Cmp.More`), so it is conservatively a possible overlap.
        let vars = vec![Name::new("T")];
        let aliases = std::collections::HashMap::default();
        let mut bindings = TypeBindings::default();
        let u = union(vec![enum_variant("Cmp", "Less"), type_var("T")]);
        assert_eq!(
            unify_into(&u, &enum_ty("Cmp"), &vars, &aliases, &mut bindings),
            Overlap::Yes
        );
    }

    #[test]
    fn ground_partial_enum_union_is_disjoint_from_enum() {
        // No bare variable to complete the enum, and the partial variant set is a strict
        // subset of `Cmp`, so the two are disjoint.
        let aliases = std::collections::HashMap::default();
        let mut bindings = TypeBindings::default();
        let u = union(vec![
            enum_variant("Cmp", "Less"),
            enum_variant("Cmp", "Equal"),
        ]);
        assert_eq!(
            unify_into(&u, &enum_ty("Cmp"), &[], &aliases, &mut bindings),
            Overlap::No
        );
    }

    #[test]
    fn union_with_var_vs_single_class_overlaps_via_collapse() {
        // `C | T` opposite the single type `C`: at `T = C` idempotency collapses the
        // union to `C`, so they share the instance `C`. This is the union-vs-non-union
        // analogue of `union_overlap_collapsing_members_is_yes`; routing the non-union
        // operand through covering (as the singleton `{C}`) is what catches it.
        let vars = vec![Name::new("T")];
        let aliases = std::collections::HashMap::default();
        let mut bindings = TypeBindings::default();
        let c = || class(&[], "C", vec![]);
        let u = union(vec![c(), type_var("T")]);
        assert_eq!(
            unify_into(&u, &c(), &vars, &aliases, &mut bindings),
            Overlap::Yes
        );
    }

    #[test]
    fn union_with_literal_and_var_vs_base_overlaps_via_collapse() {
        // `1 | T` opposite `int`: at `T = int` the literal `1 <: int` collapses, so the
        // union equals `int` — decided precisely by `cover`'s `literal <: base` oracle.
        let vars = vec![Name::new("T")];
        let aliases = std::collections::HashMap::default();
        let mut bindings = TypeBindings::default();
        let u = union(vec![int_literal(1), type_var("T")]);
        assert_eq!(
            unify_into(&u, &int(), &vars, &aliases, &mut bindings),
            Overlap::Yes
        );
    }

    #[test]
    fn union_with_var_vs_unrelated_single_is_disjoint() {
        // `D | T` opposite `C` (`C ≠ D`): the union always contains `D`, which no
        // instantiation of `T` removes, so it can never equal `C`. Routing through
        // covering keeps this precise (`No`), not a conservative over-reject.
        let vars = vec![Name::new("T")];
        let aliases = std::collections::HashMap::default();
        let mut bindings = TypeBindings::default();
        let u = union(vec![class(&[], "D", vec![]), type_var("T")]);
        let c = class(&[], "C", vec![]);
        assert_eq!(
            unify_into(&u, &c, &vars, &aliases, &mut bindings),
            Overlap::No
        );
    }

    #[test]
    fn variable_binds_to_builtin_unknown() {
        // `unknown` is the inhabited top type, so a unification variable binds to it: a
        // blanket `Box<T>` overlaps `Box<unknown>` at `T = unknown`. The old bail that
        // lumped `BuiltinUnknown` in with the error sentinel wrongly rejected this.
        let vars = vec![Name::new("T")];
        let aliases = std::collections::HashMap::default();
        let mut bindings = TypeBindings::default();
        assert_eq!(
            unify_into(
                &type_var("T"),
                &builtin_unknown(),
                &vars,
                &aliases,
                &mut bindings
            ),
            Overlap::Yes
        );
    }

    #[test]
    fn builtin_unknown_is_disjoint_from_distinct_concrete() {
        // `unknown` is a distinct atomic type under invariance, compared by equality:
        // `Box<unknown>` and `Box<int>` do not overlap, matching how the runtime
        // resolver matches `unknown` (only an `unknown` value inhabits `Box<unknown>`).
        let aliases = std::collections::HashMap::default();
        let mut bindings = TypeBindings::default();
        assert_eq!(
            unify_into(&builtin_unknown(), &int(), &[], &aliases, &mut bindings),
            Overlap::No
        );
    }

    // Probe helper for the pigeonhole experiments below.
    fn pigeonhole_overlap(holes: usize, pigeons: usize) -> Overlap {
        let vars: Vec<Name> = (0..holes).map(|i| Name::new(format!("T{i}"))).collect();
        let aliases = std::collections::HashMap::default();
        let mut bindings = TypeBindings::default();
        let pair = |a: Ty, b: Ty| class(&[], "Pair", vec![a, b]);
        let xs: Vec<Ty> = (0..holes)
            .map(|i| {
                let t = type_var(&format!("T{i}"));
                pair(t.clone(), t)
            })
            .collect();
        let ys: Vec<Ty> = (0..pigeons)
            .map(|i| {
                let a = class(&[], &format!("A{i}"), vec![]);
                pair(a.clone(), a)
            })
            .collect();
        unify_union_members(&xs, &ys, &vars, &aliases, &mut bindings)
    }

    #[test]
    fn union_overlap_small_pigeonhole_is_decided() {
        // 4 distinct variables (holes) cannot realize 5 distinct ground members
        // (pigeons), so the unions are disjoint — and at this size the search proves it
        // within budget, returning a definite `No` (contrast with the next test).
        assert_eq!(pigeonhole_overlap(4, 5), Overlap::No);
    }

    #[test]
    fn union_overlap_pigeonhole_is_unknown() {
        // The NP-hard core in miniature: 5 distinct variables (holes) cannot realize 6
        // distinct ground members (pigeons), so the unions are *provably disjoint* — but
        // ruling out every one of the ~5! arrangements overruns the step budget, so the
        // solver conservatively returns `Unknown` ("simplify your type"). An 11-member
        // union that is exponential to decide — the search's *depth*, not breadth.
        assert_eq!(pigeonhole_overlap(5, 6), Overlap::Unknown);
    }

    /// 3-SAT → union ACI-matching (adapted from the ACI paper's Lemma 5). Each SAT
    /// variable `i` is a type var `Vi` pinned to `Pos`/`Neg`; clause `j` over vars
    /// `(p,q,r)` with polarities is the member `Cl<Cj, Vp, Vq, Vr>`, whose candidates are
    /// the (≤7) ground `Pos`/`Neg` combinations that satisfy it; a bare var absorbs the
    /// candidate pool. The unions overlap iff the formula is satisfiable.
    /// `clauses[j] = [(var, positive?); 3]`.
    fn three_sat_overlap(num_vars: usize, clauses: &[[(usize, bool); 3]]) -> Overlap {
        let aliases = std::collections::HashMap::default();
        let mut bindings = TypeBindings::default();
        let v = |i: usize| type_var(&format!("V{i}"));
        let val = |b: bool| class(&[], if b { "Pos" } else { "Neg" }, vec![]);
        let mut xs: Vec<Ty> = Vec::new();
        let mut ys: Vec<Ty> = Vec::new();
        for (j, clause) in clauses.iter().enumerate() {
            let tag = class(&[], &format!("C{j}"), vec![]);
            xs.push(class(
                &[],
                "Cl",
                vec![tag.clone(), v(clause[0].0), v(clause[1].0), v(clause[2].0)],
            ));
            for sp in [true, false] {
                for sq in [true, false] {
                    for sr in [true, false] {
                        let satisfied =
                            (sp == clause[0].1) || (sq == clause[1].1) || (sr == clause[2].1);
                        if satisfied {
                            ys.push(class(
                                &[],
                                "Cl",
                                vec![tag.clone(), val(sp), val(sq), val(sr)],
                            ));
                        }
                    }
                }
            }
        }
        xs.push(type_var("ABSORB"));
        let mut vars: Vec<Name> = (0..num_vars).map(|i| Name::new(format!("V{i}"))).collect();
        vars.push(Name::new("ABSORB"));
        unify_union_members(&xs, &ys, &vars, &aliases, &mut bindings)
    }

    // All 2^n exclusion clauses over the first `n` vars ⇒ unsatisfiable.
    fn unsat_exclusion_clauses(n: usize) -> Vec<[(usize, bool); 3]> {
        assert!(n >= 3);
        let mut clauses = Vec::new();
        for mask in 0..(1usize << n) {
            // Exclude assignment `mask`; use the first 3 vars' bits as the 3 literals.
            let lit = |i: usize| (i, (mask >> i) & 1 == 0);
            clauses.push([lit(0), lit(1), lit(2)]);
        }
        clauses
    }

    #[test]
    fn union_overlap_three_sat_satisfiable_is_yes() {
        // 3-SAT reduces to coherence (the ACI-unification paper's Lemma 5, adapted to
        // unions): two `implement` blocks overlap iff the encoded formula is satisfiable.
        // `(x ∨ y ∨ z)` is satisfiable, so the blocks overlap (`Yes`) — the witness is
        // any non-all-false assignment.
        assert_eq!(
            three_sat_overlap(3, &[[(0, true), (1, true), (2, true)]]),
            Overlap::Yes,
        );
    }

    #[test]
    fn union_overlap_three_sat_unsatisfiable_is_no() {
        // The same reduction on an unsatisfiable formula (all 8 assignments of x,y,z
        // excluded) ⇒ the blocks are disjoint (`No`). At three shared variables the
        // search decides it within budget; larger spread instances are what make the
        // problem exponential.
        let clauses = unsat_exclusion_clauses(3);
        assert_eq!(three_sat_overlap(3, &clauses), Overlap::No);
    }
}
