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

// Legacy L2 interface registry, slated for wholesale deletion in favour of the
// `impl_rules` substrate (`impl_data` / `get_implements_block`). Its own items are
// `#[deprecated]` and reference each other densely, so silence the lint for this
// file's internal self-use; the kept child modules (`coherence`, `impl_rules`)
// re-enable it so their remaining legacy uses still surface as migration work.
#![allow(deprecated)]

mod coherence;
mod impl_rules;

use baml_base::{Literal, Name, Span};
use baml_compiler2_hir::{contributions::Definition, package::PackageId};
pub use coherence::*;
pub use impl_rules::*;
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
    /// The requiring interface as a constraint (its associated types pinned to the realized
    /// bindings), so an explicit binding `Item = Self.Item` resolves `Self.Item` onto it —
    /// collapsing to the realized value (or a symbolic projection when unpinned). `None` only if
    /// that interface's qtn can't be resolved.
    self_bound: Option<baml_type::Interface>,
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
#[deprecated = "L2 per-impl rule. Use the canonical `ImplData` + `get_implements_block` \
    from `impl_rules` (keyed per `ImplLoc`)."]
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
#[deprecated = "L2 match result. Use `ResolvedImpl { impl_loc, bindings }` from `impl_rules`."]
pub struct InterfaceImplInstantiation {
    pub bindings: TypeBindings,
    pub for_ty: Ty,
    pub interface_ty: Ty,
}

/// Compatibility view for old callers while rule-based matching is being
/// plumbed through the compiler.
#[derive(Debug, Clone, PartialEq, Eq)]
#[deprecated = "L2 compatibility view. Use `impl_data` / `get_implements_block` from `impl_rules`."]
pub struct BlanketClassImpl {
    pub class_qtn: QualifiedTypeName,
    pub generic_params: Vec<Name>,
    pub generic_param_bounds: Vec<Option<Ty>>,
    pub interface_qtn: QualifiedTypeName,
    pub interface_type_args: Vec<Ty>,
    pub for_target_ty: Ty,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[deprecated = "L2 lookup index. `impl_data` is keyed per `ImplLoc`; `get_implements_block` resolves."]
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
#[deprecated = "L2 per-package registry. Use `package_impl_locs` + `impl_data` / \
    `get_implements_block` from `impl_rules`."]
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
#[deprecated = "L2 resolution helper. Resolve via `impl_data` / `interface_loc_qtn` from `impl_rules`."]
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

    #[deprecated = "L2 matcher. Realized `C <: I` now resolves through L1 \
        `get_implements_block` via `NormalizeCtx::implements_interface`; the only \
        remaining callers are the non-realized symbolic fallback to migrate."]
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
    ctx: &InterfaceTypeAssocLowering<'_, '_>,
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
                // The binding value may project `Self.Assoc` onto the requiring interface (a
                // `requires I<Item = Self.Item>` clause), so lower it through a context that
                // resolves `Self`, then substitute the realized generics / associated types.
                let ty = if let Some(self_bound) = &ctx.self_bound {
                    let bounds: FxHashMap<Name, baml_type::Interface> =
                        std::iter::once((Name::new("Self"), self_bound.clone())).collect();
                    let generic_params: Vec<Name> = bindings
                        .keys()
                        .cloned()
                        .chain(std::iter::once(Name::new("Self")))
                        .collect();
                    let scope = crate::lower_type_expr::ScopeCtx {
                        db: ctx.db,
                        package_items: ctx.binding_pkg_items,
                        ns_context: ctx.binding_namespace_path,
                        generic_params: &generic_params,
                        bounds: &bounds,
                        self_ty: Some(Ty::TypeVar(Name::new("Self"), TyAttr::default())),
                    };
                    generics::substitute_ty(
                        &crate::lower_type_expr::lower_type_expr(&binding.ty, &scope, diagnostics),
                        &bindings,
                    )
                } else {
                    generics::lower_type_expr_with_generics(
                        ctx.db,
                        &binding.ty,
                        ctx.binding_pkg_items,
                        ctx.binding_namespace_path,
                        &bindings,
                        diagnostics,
                    )
                };
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
            params,
            ret,
            throws,
            ..
        } => {
            params
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

/// Match several `(pattern, concrete)` pairs into one consistent set of
/// bindings, threading them across every pair — a `generic_param` that occurs
/// in more than one pattern must unify to the same type in all of them. Returns
/// `None` if any pair fails or the bindings conflict. Used by the canonical
/// resolver to bind an impl's generics from its for-type pattern *and* its
/// interface input args simultaneously (a param may appear in either).
pub fn match_ty_patterns(
    pairs: &[(&Ty, &Ty)],
    generic_params: &[Name],
    aliases: &std::collections::HashMap<QualifiedTypeName, Ty>,
) -> Option<TypeBindings> {
    let mut bindings = TypeBindings::default();
    for (pattern, concrete) in pairs {
        match_ty_pattern_into(pattern, concrete, generic_params, aliases, &mut bindings)?;
    }
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
                params: p_params,
                ret: p_ret,
                throws: p_throws,
                ..
            },
            Ty::Function {
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
            // Function values are realized: neither type carries generic binders,
            // so match the param/ret/throws components directly.
            for (p, c) in p_params.iter().zip(c_params.iter()) {
                match_ty_pattern_into(&p.ty, &c.ty, generic_params, aliases, bindings)?;
            }
            match_ty_pattern_into(p_ret, c_ret, generic_params, aliases, bindings)?;
            match_ty_pattern_into(p_throws, c_throws, generic_params, aliases, bindings)
        }
        _ if normalize::is_same_normalized_type(pattern, concrete, aliases) => Some(()),
        _ => None,
    }
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
            params,
            ret,
            throws,
            ..
        } => {
            params
                .iter()
                .any(|FunctionParamTy { ty, .. }| contains_bound_typevar(ty, generic_params))
                || contains_bound_typevar(ret, generic_params)
                || contains_bound_typevar(throws, generic_params)
        }
        _ => false,
    }
}

#[deprecated = "L2 index helper; removed with `ImplementsRegistry`."]
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
#[deprecated = "use `implements_interface` instead"]
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
#[deprecated = "L2 registry. Being replaced by `impl_data`-derived rules; \
    coherence and the non-realized interface-membership fallback are the \
    remaining callers to migrate before deletion."]
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
            let class_file = class_loc.file(db);
            // In-body impls (and merged simple `implement I for C`) are recorded
            // under `class_to_impls` in source order; each resolves through the
            // shared `impl_data` query. Bounds stay on the legacy single-bound
            // path (`lower_generic_param_bounds`) until the deferred enforcement.
            for impl_id in hir_tree
                .class_to_impls
                .get(&class_loc.id(db))
                .into_iter()
                .flatten()
            {
                let impl_loc = baml_compiler2_hir::loc::ImplLoc::new(db, class_file, *impl_id);
                let Ok(data) = impl_data(db, impl_loc).as_ref() else {
                    continue;
                };
                let Some(iface_qtn) = interface_loc_qtn(db, data.interface) else {
                    continue;
                };
                let mut diags = Vec::new();
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
                    for_ty_pattern: data.for_ty_pattern.clone(),
                    interface_ty: Ty::Interface(
                        iface_qtn,
                        data.interface_args.clone(),
                        data.associated_types.clone(),
                        TyAttr::default(),
                    ),
                    origin: data.origin.clone(),
                    source_span: impl_data_source_map(db, impl_loc)
                        .as_ref()
                        .map(|sm| sm.impl_span),
                });
            }
        }
    }

    for file in baml_compiler2_hir::compiler2_all_files(db) {
        let pkg_info = baml_compiler2_hir::file_package::file_package(db, file);
        if pkg_info.package != *pkg_id.name(db) {
            continue;
        }
        let item_tree = baml_compiler2_hir::file_item_tree(db, file);
        // Out-of-body impls in source order via `free_impls`; each resolves
        // through `impl_data`. Bounds stay on the legacy single-bound path.
        for impl_id in &item_tree.free_impls {
            let impl_loc = baml_compiler2_hir::loc::ImplLoc::new(db, file, *impl_id);
            let Ok(data) = impl_data(db, impl_loc).as_ref() else {
                continue;
            };
            let Some(iface_qtn) = interface_loc_qtn(db, data.interface) else {
                continue;
            };
            let Some(block) = item_tree.impls.get(impl_id) else {
                continue;
            };
            let baml_compiler2_hir::item_tree::ImplSubject::Free { generics, .. } = &block.subject
            else {
                continue;
            };
            let names: Vec<Name> = generics.iter().map(|g| g.name.clone()).collect();
            let mut diags = Vec::new();
            let bounds: Vec<Option<Ty>> = generics
                .iter()
                .map(|g| {
                    g.bounds.first().map(|te| {
                        crate::lower_type_expr::lower_type_expr_in_ns(
                            db,
                            te,
                            pkg_items,
                            &pkg_info.namespace_path,
                            &names,
                            &mut diags,
                        )
                    })
                })
                .collect();
            interface_impl_rules.push(InterfaceImplRule {
                generic_params: names,
                generic_param_bounds: bounds,
                for_ty_pattern: data.for_ty_pattern.clone(),
                interface_ty: Ty::Interface(
                    iface_qtn,
                    data.interface_args.clone(),
                    data.associated_types.clone(),
                    TyAttr::default(),
                ),
                origin: data.origin.clone(),
                source_span: impl_data_source_map(db, impl_loc)
                    .as_ref()
                    .map(|sm| sm.impl_span),
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

        // This interface as a constraint (its associated types pinned to the realized
        // bindings) — so a required interface's `Item = Self.Item` resolves `Self.Item` here.
        let self_bound = interface_loc_qtn(db, loc)
            .map(|qtn| baml_type::Interface::new(qtn, args.clone(), associated_bindings.clone()));

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
                &InterfaceTypeAssocLowering {
                    db,
                    iface: parent_iface,
                    interface_args: &parent_args,
                    explicit_associated_bindings: parent_explicit_assoc,
                    iface_pkg_items: parent_iface_pkg_items,
                    binding_pkg_items: parent_pkg_items,
                    iface_namespace_path: &parent_pkg.namespace_path,
                    binding_namespace_path: parent_binding_ns,
                    outer_bindings: &bindings,
                    self_bound: self_bound.clone(),
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

/// Does interface constraint `sub` transitively (and *properly*) require `sup`?
///
/// Walks `sub`'s `requires` closure instantiated at `sub`'s generic arguments and
/// associated-type pins, and looks for an entry matching `sup` by qualified name,
/// argument list, and every associated-type pin `sup` specifies. Argument and pin
/// equality is delegated to `equivalent` (the caller's type-equality oracle) so
/// the walk stays independent of the normalization backend.
///
/// *Proper* requirement: an interface requiring itself is not a requirement, so
/// an identical qualified name short-circuits to `false` — structural reflexivity
/// is the normalizer's job, and the closure walk includes `sub` itself. Returns
/// `false` when `sub` does not resolve to an interface in an accessible package.
///
/// The global counterpart to the per-scope requirement check: a pure function of
/// the program's declarations, the resolution context that bounds package
/// visibility, and the supplied equality oracle.
pub fn interface_requires<'db>(
    db: &'db dyn crate::Db,
    res_ctx: &'db crate::package_interface::PackageResolutionContext<'db>,
    sub: &baml_type::Interface,
    sup: &baml_type::Interface,
    mut equivalent: impl FnMut(&Ty, &Ty) -> bool,
) -> bool {
    if sub.name == sup.name {
        return false;
    }
    let Some(pkg_items) = res_ctx.items_for_package(db, sub.name.package()) else {
        return false;
    };
    let Some(Definition::Interface(sub_loc)) =
        pkg_items.lookup_type(sub.name.namespace(), sub.name.name())
    else {
        return false;
    };
    for (iface_loc, iface_args, iface_assoc) in interface_closure_locs_with_args_and_assoc(
        db,
        sub_loc,
        &sub.generics,
        &sub.associated_types,
        pkg_items,
        sub.name.namespace(),
    ) {
        let iface_tree = baml_compiler2_hir::file_item_tree(db, iface_loc.file(db));
        let Some(iface_data) = iface_tree.interfaces.get(&iface_loc.id(db)) else {
            continue;
        };
        let iface_qtn = qualify_def(db, Definition::Interface(iface_loc), &iface_data.name);
        if iface_qtn == sup.name
            && iface_args.len() == sup.generics.len()
            && iface_args
                .iter()
                .zip(sup.generics.iter())
                .all(|(a, b)| equivalent(a, b))
            && sup.associated_types.iter().all(|(sup_name, sup_ty)| {
                iface_assoc
                    .iter()
                    .find(|(iface_name, _)| iface_name == sup_name)
                    .is_some_and(|(_, iface_ty)| equivalent(iface_ty, sup_ty))
            })
        {
            return true;
        }
    }
    false
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

    fn associated_projection(base: Ty, interface: &Ty, member: &str) -> Ty {
        Ty::AssociatedTypeProjection {
            base: Box::new(base),
            interface: Some(Box::new(interface.as_interface().unwrap_or_else(|| {
                unreachable!("associated_projection requires an interface")
            }))),
            member: Name::new(member),
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
            match_ty_patterns(
                &[(&pattern, &good)],
                &params,
                &std::collections::HashMap::default()
            )
            .is_some()
        );
        assert!(
            match_ty_patterns(
                &[(&pattern, &bad)],
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

        let bindings = match_ty_patterns(
            &[(&pattern, &actual)],
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
    fn match_ty_pattern_uses_full_qualified_type_names() {
        let pattern = class(&["alpha"], "Thing", vec![]);
        let same_short_name = class(&["beta"], "Thing", vec![]);

        assert!(
            match_ty_patterns(
                &[(&pattern, &same_short_name)],
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

        let bindings = match_ty_patterns(
            &[(&pattern, &actual)],
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
        let projected_item = associated_projection(type_var("T"), &source, "Item");
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
}
