//! Runtime interface-method resolver — rustc-style trait selection over the
//! program-wide baked impl-rule table
//! ([`PackageIndex`](crate::package_load::PackageIndex), via `vm.packages`).
//!
//! Given a value's concrete runtime type plus an interface and method name, it
//! returns the applicable impl's method — a concrete callee and the impl's bound
//! type args (de Bruijn order, to seed the callee's frame) — or `None` when no
//! impl applies (the caller decides the fallback).
//!
//! This mirrors the compiler's selection (`match_ty_pattern` + bound validation
//! in `baml_compiler2_tir::interfaces`), decided by the shared goal solver
//! ([`SolverSession`]) over the VM's baked clauses: match a rule's
//! `for_ty_pattern` against the concrete type (binding the impl's generic
//! params), then discharge each param's declared bound as a nested goal.

use std::ops::ControlFlow;

use baml_type::{
    ClauseId, ImplClause, MediaKind, Name, RealizedTy, TyAttr, TyTemplate, TypeName,
    normalize::{SolverSession, TypeContext},
};
use bex_vm_types::{
    errors::VmInternalError,
    types::{Object, RuntimeImplRule},
};

use crate::{BexVm, type_context::DispatchProfile};

/// The runtime impl resolver over a running VM. Holding the `&BexVm` here —
/// rather than threading it through every helper — keeps the whole selection
/// machinery able to reach VM facts at any depth: each resolution's session runs
/// over the [`DispatchProfile`], so recursive aliases fold correctly in pattern
/// comparisons while the re-entrant facts stay severed (see its docs for the
/// fact profile as data).
#[derive(Clone, Copy)]
pub(crate) struct ImplResolver<'vm> {
    vm: &'vm BexVm,
}

impl<'vm> ImplResolver<'vm> {
    pub(crate) fn new(vm: &'vm BexVm) -> Self {
        Self { vm }
    }
    /// Resolve `(Self, Iface<Args>)` to the single applicable `implements` rule, plus
    /// the impl's bound type args — its generics realized by matching `concrete_ty`
    /// against the rule's `for` pattern. That rule is the canonical handle: read the
    /// concrete method off `rule.methods` (defaults are merged in at bake time,
    /// overrides winning), the associated bindings off `rule.interface_assoc`, etc.;
    /// the returned type args are the realization env for them.
    ///
    /// Selection is keyed on `Self` (`concrete_ty`, including its own type args) and
    /// the interface's **input** args only. Associated types are *outputs*
    /// (functionally determined by the impl), so they never affect which impl is
    /// selected — coherence is per `(Self, Iface<Args>)`. A caller holding an
    /// *expected* associated projection can validate it against the resolved rule via
    /// [`Self::type_implements`]; that is checking, not selection.
    ///
    /// The match is **exact — no near-miss fallback**. An empty `iface_args` request
    /// matches any instantiation, the legitimate case for a *non-generic* interface
    /// (`Equals`/`Compare`), which has nothing to specify. A *generic* interface
    /// always carries its full concrete instantiation (BAML generics are specified,
    /// never truly omitted), and the type checker already proved
    /// `concrete_ty : Iface<those args>` — so `None` means no rule matches a
    /// fully-specified request, i.e. the proof and the runtime registry disagree: an
    /// invariant violation the caller surfaces (a virtual call as an internal error,
    /// a projection as opaque). Returning the first applicable rule instead would
    /// silently dispatch a different instantiation (`Converter<string>` through
    /// `Converter<int>`), reading operands and members at the wrong type.
    pub(crate) fn resolve_implements_rule(
        self,
        concrete_ty: &RealizedTy,
        iface: &TypeName,
        iface_args: &[RealizedTy],
    ) -> Option<(&'vm RuntimeImplRule, Vec<RealizedTy>)> {
        let profile = DispatchProfile(self.vm);
        let (clause, bindings) =
            SolverSession::new(&profile).select(concrete_ty, iface, iface_args)?;
        // The clause id is the rule's heap *object* pointer (what the supplier
        // minted it from), so recovery walks the same index and compares the same
        // pointer — never the address of the `RuntimeImplRule` borrowed out of
        // the object, which is a different address entirely.
        let rule = self
            .vm
            .lookup_interface(iface)
            .into_iter()
            .flat_map(|ptr| self.vm.packages.impl_rules_of(ptr))
            .find(|&&rule_ptr| ClauseId(rule_ptr.as_ptr() as u64) == clause)
            .and_then(|&rule_ptr| self.vm.get_object(rule_ptr).as_impl_rule())?;
        Some((rule, bindings))
    }

    /// Realize a [`MethodImpl`](bex_vm_types::types::MethodImpl) frame template (De
    /// Bruijn over the impl's generic params) against the impl's bound type args
    /// (from [`Self::resolve_implements_rule`]). The result is the `frame.type_args`
    /// to seed the resolved callee with: the impl's own generics for an impl method,
    /// or `Self` + the interface's args + associated types for an inherited default.
    pub(crate) fn realize_frame(
        self,
        template: &[TyTemplate],
        bound_args: &[RealizedTy],
    ) -> Result<Vec<RealizedTy>, VmInternalError> {
        // Materialization boundary: seeding a bytecode frame's realized type args.
        // The impl's frame templates realize fully against the (realized) bound args —
        // every projection reduced through the impl registry — or it is an internal
        // error, never a silent `unknown`.
        template
            .iter()
            .map(|t| {
                t.substitute(bound_args, self.vm)
                    .map_err(|e| VmInternalError::TypeSubstitution {
                        message: e.to_string(),
                    })
            })
            .collect()
    }

    /// Whether `concrete_ty` implements `iface` at the requested args / associated
    /// bindings — an empty request matches any instantiation for that dimension.
    /// The runtime twin of proving a `T: Iface<Args, Assoc = …>` obligation: select
    /// an impl (match its for-type, discharge its own bounds), then confirm the
    /// impl's interface args/assoc — concretised with its bindings — equal the
    /// request. Used both by reflection's membership queries and to discharge a
    /// bounded impl's nested obligations.
    pub(crate) fn type_implements(
        self,
        concrete_ty: &RealizedTy,
        iface: &TypeName,
        requested_args: &[RealizedTy],
        requested_assoc: &[(Name, RealizedTy)],
    ) -> bool {
        // The runtime has no channel for an open verdict: dispatch either finds
        // an impl or it does not, so a limit-cut search collapses fail-closed
        // (no membership claimed) — the sanctioned `holds` direction here.
        let profile = DispatchProfile(self.vm);
        SolverSession::new(&profile)
            .implements(concrete_ty, iface, requested_args, requested_assoc)
            .holds()
    }

    /// Compare two generic-argument lists for *semantic* equivalence, position-wise.
    /// Each pair is related by the canonical set-theoretic algebra under the
    /// [`DispatchProfile`]: union order is normalized away (`Box<int | string>` ≡
    /// `Box<string | int>`), `never` and literal members collapse
    /// (`int | never == int`), recursive aliases fold (`type A = int | A[]` ≡
    /// `type B = int | B[]`), and interface bindings compare as an
    /// order-insensitive set — while the re-entrant nominal facts stay severed,
    /// so this never re-enters the resolver.
    pub(super) fn ty_args_equivalent(self, a: &[RealizedTy], b: &[RealizedTy]) -> bool {
        a.len() == b.len()
            && a.iter().zip(b.iter()).all(|(x, y)| {
                DispatchProfile(self.vm)
                    .equivalent(x.as_ty(), y.as_ty())
                    .holds()
            })
    }

    /// Name-keyed and asymmetric: every *requested* binding must be provided, and
    /// the provider may be wider (bindings are outputs; asking about fewer is
    /// asking less). Trivially satisfied by an empty request.
    pub(super) fn associated_bindings_equivalent(
        self,
        impl_bindings: &[(Name, RealizedTy)],
        requested_bindings: &[(Name, RealizedTy)],
    ) -> bool {
        requested_bindings.is_empty()
            || requested_bindings
                .iter()
                .all(|(requested_name, requested_ty)| {
                    impl_bindings.iter().any(|(impl_name, impl_ty)| {
                        impl_name == requested_name
                            && DispatchProfile(self.vm)
                                .equivalent(impl_ty.as_ty(), requested_ty.as_ty())
                                .holds()
                    })
                })
    }
}

/// One implementor of an interface: the concrete implementor type plus the
/// interface args / associated bindings that impl pins (empty = "matches any
/// instantiation", for blanket impls and typevar-bearing dimensions).
type ImplementorEntry = (RealizedTy, Vec<RealizedTy>, Vec<(Name, RealizedTy)>);

impl ImplResolver<'_> {
    /// The concrete types that nominally implement `iface`, each paired with the
    /// interface instantiation its impl fixes — the inverse of
    /// [`Self::type_implements`]. Reflection (`type.implementors()`) filters these by
    /// the requested args/assoc.
    ///
    /// Every impl of `iface` in the program is considered — the same clause view
    /// membership selection consumes ([`TypeContext::for_each_clause`] on
    /// [`BexVm`]), so the two can never disagree about which impls exist. A
    /// non-generic impl contributes its concrete for-type; a generic class
    /// contributes its base (instantiations can't be enumerated, so
    /// typevar-bearing interface args are erased to "match any"); a blanket
    /// `for T` contributes every loaded concrete class whose bounds it
    /// satisfies. Container/union for-types have no nominal implementor to list.
    pub(super) fn implementor_entries(self, iface: &TypeName) -> Vec<ImplementorEntry> {
        // The walk borrows the clause list out of the heap while `concrete_types`
        // and the session also read the VM, so collect the clauses first (shared
        // borrows throughout; the clause lifetime is tied to the VM, not the walk).
        let mut clauses: Vec<ImplClause<'_>> = Vec::new();
        self.vm.for_each_clause(iface, &mut |clause| {
            clauses.push(clause);
            ControlFlow::Continue(())
        });
        // One session spans the whole enumeration, so a blanket impl's bound
        // proofs memoize across the candidate types instead of recomputing per
        // candidate (each `applies` is its own root; the store persists).
        let profile = DispatchProfile(self.vm);
        let mut session = SolverSession::new(&profile);
        let mut out: Vec<ImplementorEntry> = Vec::new();
        for clause in &clauses {
            match clause.self_pattern {
                TyTemplate::TypeArgRef(_) => {
                    // Blanket impl: its bounds decide membership; every concrete
                    // type satisfying them is an implementor, at the interface
                    // instantiation the blanket pins (typevar dimensions erased).
                    let (args, assoc) = self.pinned_interface_instantiation(clause);
                    for ty in self.concrete_types() {
                        if session.applies(clause, &ty).is_some() {
                            push_unique(&mut out, (ty, args.clone(), assoc.clone()));
                        }
                    }
                }
                // A concrete for-type (`int`, a monomorphic class, `int[]`, …)
                // narrows to a realized type — the implementor is that type.
                other if <&RealizedTy>::try_from(other).is_ok() => {
                    let realized = <&RealizedTy>::try_from(other)
                        .unwrap_or_else(|_| unreachable!("guarded by the `is_ok` above"));
                    let (args, assoc) = self.pinned_interface_instantiation(clause);
                    push_unique(&mut out, (realized.clone(), args, assoc));
                }
                // A generic class for-type (`Foo<T>`) is reported by its base.
                TyTemplate::Class(name, _, _) => {
                    let (args, assoc) = self.pinned_interface_instantiation(clause);
                    let base = RealizedTy::Class(name.clone(), Vec::new(), TyAttr::default());
                    push_unique(&mut out, (base, args, assoc));
                }
                _ => {}
            }
        }
        out.sort_by_cached_key(|(ty, _, _)| ty.to_string());
        out
    }

    /// The interface args / associated bindings a non-blanket impl pins, with any
    /// dimension that references the impl's generic params erased to empty (a
    /// generic class is reported by its base, so a per-instantiation arg can't be
    /// named — empty then matches any requested instantiation).
    fn pinned_interface_instantiation(
        self,
        clause: &ImplClause<'_>,
    ) -> (Vec<RealizedTy>, Vec<(Name, RealizedTy)>) {
        let args = if clause.iface_args.iter().any(template_has_type_arg_ref) {
            Vec::new()
        } else {
            clause
                .iface_args
                .iter()
                .map(|t| self.substitute_checked(t, &[]))
                .collect()
        };
        let assoc = if clause
            .iface_assoc
            .iter()
            .any(|(_, t)| template_has_type_arg_ref(t))
        {
            Vec::new()
        } else {
            clause
                .iface_assoc
                .iter()
                .map(|(n, t)| (n.clone(), self.substitute_checked(t, &[])))
                .collect()
        };
        (args, assoc)
    }

    /// Realize an impl clause's interface-arg [`TyTemplate`] against a *complete*
    /// env (one binding per impl generic param) for reflection's
    /// pinned-instantiation view — not value materialization. An out-of-range
    /// `TypeArgRef` (a malformed rule) is reported by `substitute` itself and
    /// asserted here in debug; in release, any substitution failure falls back to
    /// the top type, keeping the view conservative without aborting the query.
    /// (Value materialization uses [`Self::realize_frame`], which is strict.)
    fn substitute_checked(self, template: &TyTemplate, env: &[RealizedTy]) -> RealizedTy {
        template.substitute(env, self.vm).unwrap_or_else(|e| {
            debug_assert!(
                !matches!(e, baml_type::SubstituteError::TypeArgRefOutOfRange { .. }),
                "impl rule template is malformed: {e}",
            );
            RealizedTy::unknown()
        })
    }
}

/// Whether a template references any impl generic parameter (a `TypeArgRef`).
fn template_has_type_arg_ref(t: &TyTemplate) -> bool {
    match t {
        TyTemplate::TypeArgRef(_) => true,
        TyTemplate::List(inner, _) => template_has_type_arg_ref(inner),
        TyTemplate::Map { key, value, .. } | TyTemplate::Future(key, value, _) => {
            template_has_type_arg_ref(key) || template_has_type_arg_ref(value)
        }
        TyTemplate::Union(parts, _) => parts.iter().any(template_has_type_arg_ref),
        TyTemplate::Class(_, args, _) => args.iter().any(template_has_type_arg_ref),
        TyTemplate::Interface(_, args, assoc, _) => {
            args.iter().any(template_has_type_arg_ref)
                || assoc.iter().any(|(_, t)| template_has_type_arg_ref(t))
        }
        TyTemplate::Function {
            params,
            ret,
            throws,
            ..
        } => {
            params.iter().any(|p| template_has_type_arg_ref(&p.ty))
                || template_has_type_arg_ref(ret)
                || template_has_type_arg_ref(throws)
        }
        TyTemplate::AssociatedTypeProjection {
            base, interface, ..
        } => {
            template_has_type_arg_ref(base)
                || interface.generics.iter().any(template_has_type_arg_ref)
                || interface
                    .associated_types
                    .iter()
                    .any(|(_, t)| template_has_type_arg_ref(t))
        }
        // Realized leaves carry no frame ref.
        _ => false,
    }
}

impl ImplResolver<'_> {
    /// Every loaded concrete type — classes, enums, and the primitives — as a `RealizedTy` at
    /// its base (no type args). The candidate set a blanket impl's bounds are checked
    /// against: a blanket `for T` can be satisfied by any concrete type, not just
    /// classes. (`all_class_and_enum_ptrs` covers both classes and enums across every
    /// loaded package.) `$stream` companions are filtered later by
    /// `push_unique`.
    fn concrete_types(self) -> Vec<RealizedTy> {
        let mut types: Vec<RealizedTy> = self
            .vm
            .all_class_and_enum_ptrs()
            .filter_map(|ptr| match self.vm.get_object(ptr) {
                Object::Class(class) => Some(RealizedTy::Class(
                    class.name.clone(),
                    Vec::new(),
                    TyAttr::default(),
                )),
                Object::Enum(enum_def) => {
                    Some(RealizedTy::Enum(enum_def.name.clone(), TyAttr::default()))
                }
                _ => None,
            })
            .collect();
        // Primitives and the other arg-less builtin value types have no owning object, so
        // they aren't in any package's class/enum tables; a blanket `for T` (or one whose bound they
        // satisfy) admits them, so add the fixed set. This mirrors `Ty::is_valid_impl_subject`'s
        // arg-less accepted variants — keeping reflection's blanket-implementor enumeration
        // from silently dropping, e.g., `uint8array` (which has a builtin `Equals` impl).
        // Parameterized subjects (`List`/`Map`) and non-value types are intentionally omitted:
        // their instantiations can't be enumerated.
        types.extend([
            RealizedTy::Int {
                attr: TyAttr::default(),
            },
            RealizedTy::Bigint {
                attr: TyAttr::default(),
            },
            RealizedTy::Float {
                attr: TyAttr::default(),
            },
            RealizedTy::String {
                attr: TyAttr::default(),
            },
            RealizedTy::Bool {
                attr: TyAttr::default(),
            },
            RealizedTy::Null {
                attr: TyAttr::default(),
            },
            RealizedTy::Uint8Array {
                attr: TyAttr::default(),
            },
            RealizedTy::Type {
                attr: TyAttr::default(),
            },
            RealizedTy::Resource {
                attr: TyAttr::default(),
            },
            RealizedTy::PromptAst {
                attr: TyAttr::default(),
            },
        ]);
        // `Media` carries a kind, so a media value's concrete type pins one — enumerate each.
        types.extend(
            [
                MediaKind::Image,
                MediaKind::Audio,
                MediaKind::Video,
                MediaKind::Pdf,
                MediaKind::Generic,
            ]
            .map(|kind| RealizedTy::Media(kind, TyAttr::default())),
        );
        types
    }
}

fn push_unique(out: &mut Vec<ImplementorEntry>, entry: ImplementorEntry) {
    // Hide `…$stream` companion types: they implement their base's interfaces so
    // dispatch works on stream values, but they are compiler-synthesized and must
    // not surface in reflection's implementor lists.
    if is_stream_companion(&entry.0) || out.contains(&entry) {
        return;
    }
    out.push(entry);
}

/// Whether `ty` is an internal `…$stream` companion type.
fn is_stream_companion(ty: &RealizedTy) -> bool {
    match ty {
        RealizedTy::Class(tn, ..) | RealizedTy::Enum(tn, ..) => tn.name().ends_with("$stream"),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use baml_type::{Freshness, Literal, TyAttr};

    use super::*;

    /// The session's pattern-comparison strategy (its private compare over the
    /// dispatch profile), reconstructed for driving the matcher directly.
    struct ProfileCompare<'a>(DispatchProfile<'a>);

    impl baml_type::TemplateCompare for ProfileCompare<'_> {
        fn same_type(&mut self, pattern: &baml_type::Ty, concrete: &baml_type::Ty) -> bool {
            self.0.equivalent(pattern, concrete).holds()
        }
    }

    // Pins matcher invariance: a `Concrete` literal pattern matches only that exact
    // literal type, never its base primitive. The compiler applies `1 <: int` only at
    // the *top* of an overlap check (`cover`); under a constructor (`Box<1>` vs
    // `Box<int>`) both sides stay invariant, and this runtime matcher agrees — so a
    // `Box<1>` value does not dispatch an `implement … for Box<int>`. (Reached via
    // reflection today; this pins the behavior at the dispatch comparison the
    // session's clause matching uses, where the literal-/attr-sensitivity would
    // otherwise be untested.)
    #[test]
    fn concrete_literal_pattern_matches_only_the_literal() {
        let vm = crate::vm::tests::test_vm(Vec::new());
        let one = RealizedTy::Literal(Literal::Int(1), Freshness::Regular, TyAttr::default());
        let pattern = TyTemplate::from(RealizedTy::Literal(
            Literal::Int(1),
            Freshness::Regular,
            TyAttr::default(),
        ));
        let mut binds: Vec<Option<RealizedTy>> = Vec::new();
        let cmp = &mut ProfileCompare(DispatchProfile(&vm));
        assert!(pattern.match_against(&one, &mut binds, cmp));
        assert!(!pattern.match_against(&RealizedTy::int(), &mut binds, cmp));
    }
}
