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
//! in `baml_compiler2_hir_ty::interfaces`), run on `bex_vm_types::RealizedTy`: unify the rule's
//! `for_ty_pattern` against the concrete type (binding the impl's generic
//! params), then discharge each param's declared bound as a nested obligation.
//!
//! The three entry points ([`ImplResolver::resolve_implements_rule`],
//! [`ImplResolver::resolve_implementation`] and
//! [`ImplResolver::type_implements`]) answer from the VM's
//! [`ImplCache`](crate::impl_cache::ImplCache) when they can; see that module
//! for why a cached answer stays exact.

use std::borrow::Cow;

use baml_type::{Literal, Name, StructuralInterface, normalize::TypeContext};
use bex_vm_types::{
    HeapPtr, RealizedTy, TyTemplate, TypeHead, errors::VmInternalError, types::RuntimeImplRule,
};

use crate::{
    BexVm,
    impl_cache::{Answer, CachedImplementation, CachedRule, Query, World},
    type_context::StructuralEquivCtx,
    vec_ext::VecExt,
};

/// A resolver candidate borrows an immutable rule from the heap. Every rule —
/// compiled into the static image, owned by a runtime package, or registered
/// as an anonymous class's witness — is an `Object::ImplRule`, so no candidate
/// ever copies rule metadata. The two variants differ only in cacheability:
/// a `Static` rule lives in the never-moving compile-time region.
pub(crate) enum RuntimeImplRuleCandidate<'vm> {
    Static(&'vm RuntimeImplRule),
    Borrowed(&'vm RuntimeImplRule),
}

/// An implementation selected without adding synthetic rules to the registry.
/// Structural defaults carry the exact `Self` type for native and shared-body frames.
pub(crate) enum ResolvedInterfaceImpl<'vm> {
    Explicit {
        rule: RuntimeImplRuleCandidate<'vm>,
        bindings: Vec<RealizedTy>,
    },
    Structural {
        kind: StructuralInterface,
        interface: TypeHead,
        self_ty: RealizedTy,
    },
}

impl ResolvedInterfaceImpl<'_> {
    pub(crate) fn is_static(&self) -> bool {
        match self {
            Self::Explicit { rule, .. } => rule.is_static(),
            // A dynamic explicit implementation can replace this fallback.
            Self::Structural { .. } => false,
        }
    }
}

impl RuntimeImplRuleCandidate<'_> {
    pub(crate) fn is_static(&self) -> bool {
        matches!(self, Self::Static(_))
    }
}

impl std::ops::Deref for RuntimeImplRuleCandidate<'_> {
    type Target = RuntimeImplRule;

    fn deref(&self) -> &Self::Target {
        match self {
            Self::Static(rule) | Self::Borrowed(rule) => rule,
        }
    }
}

/// One resolved method of a [`RuntimeImplRule`]: the callee row plus whether
/// the rule ADOPTED the interface's default body rather than providing its
/// own. See [`ImplResolver::rule_method_impl`].
pub(crate) struct RuleMethodImpl<'r> {
    pub method: Cow<'r, bex_vm_types::types::MethodImpl>,
    /// The rule has no provided row for this method: the interface's default
    /// body serves (Rust-trait adoption, not inheritance).
    pub is_default: bool,
}

/// The runtime impl resolver over a running VM. Holding the `&BexVm` here —
/// rather than threading it through every helper — keeps the whole selection
/// machinery able to reach VM facts at any depth: candidate collection reads the
/// package tables, and every leaf type comparison builds the alias-expanding
/// [`StructuralEquivCtx`], so recursive aliases fold correctly (see its docs for
/// the fact profile and why the re-entrant facts stay opaque).
#[derive(Clone, Copy)]
pub(crate) struct ImplResolver<'vm> {
    vm: &'vm BexVm,
    root_package: Option<bex_vm_types::HeapPtr>,
    /// A value-owned dynamic world is not always the whole lexical world: a
    /// runtime package may legally declare a local interface impl for a class
    /// imported from one of its dependencies. Dispatch on that foreign class
    /// must therefore search both the receiver owner's graph and the calling
    /// package's graph. Explicit `for_package` reflection keeps this empty so
    /// inspecting one package cannot accidentally see the caller's impls.
    additional_package: Option<bex_vm_types::HeapPtr>,
    /// Rules a registration proposes but has not published. They take part
    /// in every lookup this resolver makes and are visible nowhere else.
    staged_rules: &'vm [RuntimeImplRule],
}

impl<'vm> ImplResolver<'vm> {
    pub(crate) fn new(vm: &'vm BexVm) -> Self {
        Self {
            vm,
            root_package: None,
            additional_package: None,
            staged_rules: &[],
        }
    }

    /// Resolve in the world a registration would create, without publishing
    /// it: `rules` join the candidates of every lookup, including the nested
    /// obligations a blanket rule's bounds raise. That is what lets a batch be
    /// judged as a whole — a `Gate` witness in the batch activates
    /// `implements<T extends Gate> Pick for T` for the receiver, and a `Pick`
    /// witness in the same batch is then an overlap.
    pub(crate) fn with_staged_rules(self, rules: &'vm [RuntimeImplRule]) -> Self {
        Self {
            staged_rules: rules,
            ..self
        }
    }

    /// Coherence for one `concrete: I<args>` goal: exactly one rule in this
    /// resolver's world (staged rules included) applies. Associated bindings
    /// are outputs of a match, so two rows differing only there still overlap.
    pub(crate) fn check_sole_implementation(
        self,
        concrete: &RealizedTy,
        interface: bex_vm_types::HeapPtr,
        args: &[RealizedTy],
    ) -> Result<(), String> {
        let bex_vm_types::Object::Interface(declaration) = self.vm.get_object(interface) else {
            return Err("registration target is not an interface".into());
        };
        let head = TypeHead::new(interface, declaration.type_tag);
        let applicable = self
            .rules_for(head)
            .iter()
            .filter(|(_, rule)| self.requested_rule_args(rule, concrete, args).is_some())
            .count();
        if applicable == 1 {
            Ok(())
        } else {
            Err(format!(
                "overlapping implementations of `{}` for `{concrete}`",
                declaration.name().display_name()
            ))
        }
    }

    /// Resolve using an explicitly selected runtime package as the dynamic
    /// world root. Reflection needs this when checking a package value from
    /// code lexically owned by a different package.
    pub(crate) fn for_package(vm: &'vm BexVm, package: bex_vm_types::HeapPtr) -> Self {
        Self {
            vm,
            root_package: Some(package),
            additional_package: None,
            staged_rules: &[],
        }
    }

    /// Resolve in every dynamic world relevant to a call on `value`: its
    /// owning package (where runtime-created witnesses live) plus the lexical
    /// frame's package (which may own an orphan-legal impl for an imported
    /// receiver). Static values and primitives need only the lexical world.
    pub(crate) fn for_value(vm: &'vm BexVm, value: bex_vm_types::Value) -> Self {
        let package = vm.value_runtime_package(value);
        if package.is_null() {
            Self::new(vm)
        } else {
            let lexical = vm.current_runtime_package();
            Self {
                vm,
                root_package: Some(package),
                additional_package: (!lexical.is_null() && lexical != package).then_some(lexical),
                staged_rules: &[],
            }
        }
    }

    /// Every impl rule of `iface` in the program, in package-load order, each
    /// beside its heap pointer (null for a staged rule, which has none).
    ///
    /// The program-wide [`PackageIndex`](crate::package_load::PackageIndex) is
    /// keyed by the interface's canonical `Object::Interface` pointer, so this is
    /// one O(1) lookup over a table that already spans every package — see that
    /// type's docs for why a per-package search cannot be narrowed correctly. An
    /// unknown interface (not loaded) has no impls anywhere.
    fn rules_for(self, iface: TypeHead) -> Vec<(HeapPtr, RuntimeImplRuleCandidate<'vm>)> {
        // The head *is* the canonical `Object::Interface` pointer that keys
        // every package's `impl_rules` — no name lookup on the dispatch path.
        //
        // This is also why a rooted resolver needs no viewpoint choice here. A
        // goal can name an interface the inspected package declares itself or
        // one it borrowed from a mounted dependency; resolving that *name*
        // would have to pick which of two same-named interfaces to key on. The
        // head carries the identity the goal was built from, so there is
        // nothing left to disambiguate.
        let iface_ptr = iface.ptr();
        let mut pointers = Vec::new();
        pointers.extend(self.vm.packages.impl_rules_of(iface_ptr));
        let mut packages = vec![
            self.root_package
                .unwrap_or_else(|| self.vm.current_runtime_package()),
        ];
        packages.extend(self.additional_package);
        let mut seen = std::collections::HashSet::new();
        while let Some(package_ptr) = packages.pop() {
            if package_ptr.is_null() || !seen.insert(package_ptr) {
                continue;
            }
            let Some(package) = self.vm.get_object(package_ptr).as_package() else {
                continue;
            };
            if let Some(rules) = package.impl_rules.get(&iface_ptr) {
                pointers.extend(rules);
            }
            // The chain is the ordinary packages reached from here; the
            // language packages' rules are in the static index already.
            packages.extend(
                package
                    .dependencies()
                    .filter(|&dependency| !self.vm.is_language_package(dependency)),
            );
        }
        // A rule can be reached more than once — through the static index
        // and the package that owns it, or through two packages of one
        // chain. Each rule is one candidate, so a coherence count over the
        // candidates is exact.
        let mut seen_rules = std::collections::HashSet::new();
        let mut rules = pointers
            .into_iter()
            .chain(self.vm.dynamic_dispatch.rules_of(iface_ptr))
            .filter(|rule_ptr| seen_rules.insert(*rule_ptr))
            .filter_map(|rule_ptr| {
                self.vm
                    .get_object(rule_ptr)
                    .as_impl_rule()
                    .map(|rule| (rule_ptr, RuntimeImplRuleCandidate::Borrowed(rule)))
            })
            .collect::<Vec<_>>();
        rules.extend(
            self.staged_rules
                .iter()
                .filter(|rule| rule.interface_head == iface_ptr)
                .map(|rule| (HeapPtr::null(), RuntimeImplRuleCandidate::Borrowed(rule))),
        );
        rules
    }

    /// The world this resolver's answers depend on, as the cache keys it:
    /// the packages [`Self::rules_for`] walks, plus the running code's
    /// package, where nested projections reduce.
    fn world(self) -> World {
        let lexical = self.vm.current_runtime_package();
        World {
            root: self.root_package.unwrap_or(lexical),
            additional: self.additional_package,
            lexical,
        }
    }

    /// The question this resolver would ask the cache, or `None` when its
    /// answer must not be shared: staged rules make a world no other lookup
    /// sees.
    fn query<'q>(
        self,
        self_ty: &'q RealizedTy,
        interface: TypeHead,
        args: &'q [RealizedTy],
        assoc: &'q [(Name, RealizedTy)],
    ) -> Option<Query<'q>> {
        self.staged_rules.is_empty().then(|| Query {
            world: self.world(),
            self_ty,
            interface,
            args,
            assoc,
        })
    }

    /// This VM's cached answer to `query`, or the generation a computed
    /// answer must be stored under.
    fn lookup<V: Answer>(self, query: &Query<'_>) -> Result<V, u64> {
        let generation = self.vm.dynamic_dispatch.generation();
        match self.vm.impl_cache.borrow_mut().get::<V>(generation, query) {
            Some(answer) => Ok(answer.clone()),
            None => Err(generation),
        }
    }

    /// Store an answer computed since generation `started`.
    fn store<V: Answer>(self, started: u64, query: &Query<'_>, answer: V) {
        let now = self.vm.dynamic_dispatch.generation();
        self.vm
            .impl_cache
            .borrow_mut()
            .insert(started, now, query, answer);
    }

    /// Borrow the rule a cached answer names.
    fn cached_candidate(self, cached: &CachedRule) -> RuntimeImplRuleCandidate<'vm> {
        let rule = self
            .vm
            .get_object(cached.rule)
            .as_impl_rule()
            .expect("a cached impl rule is an Object::ImplRule");
        if cached.is_static {
            RuntimeImplRuleCandidate::Static(rule)
        } else {
            RuntimeImplRuleCandidate::Borrowed(rule)
        }
    }
}

impl CachedRule {
    fn new(
        rule: HeapPtr,
        candidate: &RuntimeImplRuleCandidate<'_>,
        bindings: &[RealizedTy],
    ) -> Self {
        Self {
            rule,
            is_static: candidate.is_static(),
            bindings: bindings.to_vec(),
        }
    }
}

/// Overflow backstop for the obligation stack. Cycle detection (in
/// [`ImplResolver::prove`]) already rejects goals that *repeat*; this guards the
/// other non-terminating
/// shape — goals that *grow* without ever repeating (`T: I` ⇒ `Container<T>: I`
/// ⇒ `Container<Container<T>>: I` ⇒ …), which a cycle check cannot see. rustc
/// keeps a fixed `recursion_limit` for exactly this reason. Realistic chains are
/// 1–3 deep (each normal step shrinks the type), so only pathological bounds ever
/// reach this.
const MAX_OBLIGATION_DEPTH: usize = 128;

/// An in-progress membership goal — does `RealizedTy` implement the interface `TypeName`
/// at these args / associated bindings? Tracked on a stack so a goal that
/// recurses back to itself (an inductive cycle, with no concrete-impl base case)
/// is detected and rejected rather than spun on until the depth backstop.
type Obligation = (
    RealizedTy,
    TypeHead,
    Vec<RealizedTy>,
    Vec<(Name, RealizedTy)>,
);

/// A literal or enum-variant type behaves as its underlying concrete type for
/// impl resolution: `1` uses `int`'s impls and `Color.Red` uses `Color`'s (a
/// literal "uses its concrete type's methods"). Normalize the *top-level* type
/// to that base before consulting the registry, which is keyed by concrete
/// types; every other type passes through untouched. Only the top level is
/// normalized — nested type args keep their literal form so invariance holds
/// (`Box<1>` is not `Box<int>`).
fn concrete_base(ty: &RealizedTy) -> Cow<'_, RealizedTy> {
    match ty {
        // Persist the type's attr onto the base (consistent with the enum-variant
        // arm below), so a literal carrying a non-default attr normalizes to its
        // base with that attr intact rather than silently dropping it.
        RealizedTy::Literal(lit, _) => Cow::Owned(match lit {
            Literal::Int(_) => RealizedTy::Int,
            Literal::Bigint(_) => RealizedTy::Bigint,
            Literal::Float(_) => RealizedTy::Float,
            Literal::String(_) => RealizedTy::String,
            Literal::Bool(_) => RealizedTy::Bool,
        }),
        RealizedTy::EnumVariant(name, _) => Cow::Owned(RealizedTy::Enum(*name)),
        _ => Cow::Borrowed(ty),
    }
}

impl<'vm> ImplResolver<'vm> {
    pub(crate) fn resolve_implementation(
        self,
        self_ty: &RealizedTy,
        interface: TypeHead,
        args: &[RealizedTy],
    ) -> Option<ResolvedInterfaceImpl<'vm>> {
        let base = concrete_base(self_ty);
        let Some(query) = self.query(&base, interface, args, &[]) else {
            return self
                .select_implementation(self_ty, interface, args)
                .map(|(_, resolved)| resolved);
        };
        let started = match self.lookup::<Option<CachedImplementation>>(&query) {
            Ok(answer) => {
                return answer.map(|answer| match answer {
                    CachedImplementation::Explicit(rule) => ResolvedInterfaceImpl::Explicit {
                        rule: self.cached_candidate(&rule),
                        bindings: rule.bindings,
                    },
                    CachedImplementation::Structural(kind) => ResolvedInterfaceImpl::Structural {
                        kind,
                        interface,
                        self_ty: self_ty.clone(),
                    },
                });
            }
            Err(started) => started,
        };
        let selected = self.select_implementation(self_ty, interface, args);
        self.store(
            started,
            &query,
            selected.as_ref().map(|(answer, _)| answer.clone()),
        );
        selected.map(|(_, resolved)| resolved)
    }

    /// [`Self::resolve_implementation`] without the cache, with the answer
    /// the cache would store.
    fn select_implementation(
        self,
        self_ty: &RealizedTy,
        interface: TypeHead,
        args: &[RealizedTy],
    ) -> Option<(CachedImplementation, ResolvedInterfaceImpl<'vm>)> {
        if let Some((ptr, rule, bindings)) = self.select_implements_rule(self_ty, interface, args) {
            return Some((
                CachedImplementation::Explicit(CachedRule::new(ptr, &rule, &bindings)),
                ResolvedInterfaceImpl::Explicit { rule, bindings },
            ));
        }
        let kind = self.structural_default(interface, args)?;
        if kind == StructuralInterface::Hash
            && !self.hash_eligible(self_ty, interface, &mut Vec::new(), &mut Vec::new())
        {
            return None;
        }
        Some((
            CachedImplementation::Structural(kind),
            ResolvedInterfaceImpl::Structural {
                kind,
                interface,
                self_ty: self_ty.clone(),
            },
        ))
    }

    fn structural_default(
        self,
        interface: TypeHead,
        args: &[RealizedTy],
    ) -> Option<StructuralInterface> {
        if !args.is_empty() {
            return None;
        }
        let bex_vm_types::Object::Interface(declaration) = self.vm.get_object(interface.ptr())
        else {
            return None;
        };
        declaration
            .structural_default
            .as_ref()
            .map(|default| default.kind)
    }

    fn hash_eligible(
        self,
        ty: &RealizedTy,
        interface: TypeHead,
        active: &mut Vec<RealizedTy>,
        obligations: &mut Vec<Obligation>,
    ) -> bool {
        if self
            .rules_for(interface)
            .iter()
            .any(|(_, rule)| self.rule_applies(rule, ty, &[], obligations).is_some())
        {
            return true;
        }
        let ty = concrete_base(ty).into_owned();
        if active.contains(&ty) {
            return true;
        }
        if active.len() >= 64 {
            return false;
        }
        // `Equals` is the `baml` package's, which is required at load.
        let equals = self
            .vm
            .lang_head(crate::lang_items::BAML_OPS_EQUALS)
            .unwrap_or_else(|| {
                unreachable!(
                    "the required `baml` package declares `{}`",
                    crate::lang_items::BAML_OPS_EQUALS
                )
            });
        if matches!(ty, RealizedTy::Class(..) | RealizedTy::Enum(_))
            && self.rules_for(equals).iter().any(|(_, rule)| {
                // Unproven bounds cannot establish the absence of custom
                // equality: those bounds may depend on Hash itself.
                rule.methods.contains_key("eq")
                    && self.match_template(
                        &rule.for_ty_pattern,
                        &ty,
                        &mut vec![None; rule.generic_param_bounds.len()],
                    )
            })
        {
            return false;
        }
        active.push(ty.clone());
        let eligible = match &ty {
            RealizedTy::Int
            | RealizedTy::Bigint
            | RealizedTy::Float
            | RealizedTy::Bool
            | RealizedTy::String
            | RealizedTy::Null
            | RealizedTy::Uint8Array
            | RealizedTy::Enum(_)
            | RealizedTy::Never => true,
            RealizedTy::Interface(head, _, _) => self.interface_requires_hash(*head, interface),
            RealizedTy::List(element) => {
                self.hash_eligible(element, interface, active, obligations)
            }
            RealizedTy::Map { key, value } => {
                self.hash_eligible(key, interface, active, obligations)
                    && self.hash_eligible(value, interface, active, obligations)
            }
            RealizedTy::Union(members) => members
                .iter()
                .all(|member| self.hash_eligible(member, interface, active, obligations)),
            RealizedTy::Class(head, args) => match self.vm.get_object(head.ptr()) {
                bex_vm_types::Object::Class(class) => class.fields.iter().all(|field| {
                    field
                        .field_template
                        .substitute(args, self.vm)
                        .is_ok_and(|field_ty| {
                            self.hash_eligible(&field_ty, interface, active, obligations)
                        })
                }),
                _ => false,
            },
            RealizedTy::TypeAlias(head) => match self.vm.get_object(head.ptr()) {
                bex_vm_types::Object::TypeAlias(alias) => {
                    self.hash_eligible(&alias.definition, interface, active, obligations)
                }
                _ => false,
            },
            _ => false,
        };
        active.pop();
        eligible
    }

    fn interface_requires_hash(self, head: TypeHead, hash: TypeHead) -> bool {
        let mut pending = vec![head];
        let mut seen = std::collections::HashSet::new();
        while let Some(head) = pending.pop() {
            if head == hash {
                return true;
            }
            if !seen.insert(head) {
                continue;
            }
            if let bex_vm_types::Object::Interface(interface) = self.vm.get_object(head.ptr()) {
                pending.extend(interface.requires.iter().map(|required| required.name));
            }
        }
        false
    }

    pub(crate) fn implementation_method(
        self,
        implementation: &ResolvedInterfaceImpl<'_>,
        method: &str,
    ) -> Result<(bex_vm_types::HeapPtr, Vec<RealizedTy>), VmInternalError> {
        match implementation {
            ResolvedInterfaceImpl::Explicit { rule, bindings } => {
                let resolved = self.rule_method_impl(rule, method)?;
                Ok((
                    resolved.method.fqn,
                    self.realize_frame(&resolved.method.frame, bindings)?,
                ))
            }
            ResolvedInterfaceImpl::Structural {
                kind,
                interface,
                self_ty,
            } => {
                debug_assert_eq!(self.structural_default(*interface, &[]), Some(*kind));
                if method == kind.primary_method() {
                    let bex_vm_types::Object::Interface(declaration) =
                        self.vm.get_object(interface.ptr())
                    else {
                        unreachable!()
                    };
                    let default = declaration
                        .structural_default
                        .as_ref()
                        .expect("selected structural default");
                    if default.function_ptr.is_null() {
                        return Err(VmInternalError::UnboundInterfaceDefault {
                            interface: declaration.name().display_name().to_string(),
                            method: method.to_string(),
                        });
                    }
                    let frame = if kind.native_type_arg_count() == 0 {
                        Vec::new()
                    } else {
                        vec![self_ty.clone()]
                    };
                    return Ok((default.function_ptr, frame));
                }
                let default_fn = self.interface_default(interface.ptr(), method)?;
                Ok((default_fn, vec![self_ty.clone()]))
            }
        }
    }

    /// Resolve `(Self, Iface<Args>)` to the single applicable `implements` rule, plus
    /// the impl's bound type args — its generics realized by matching `concrete_ty`
    /// against the rule's `for` pattern. That rule is the canonical handle: read the
    /// concrete method via [`Self::rule_method_impl`] (`rule.methods` carries
    /// only what the impl block provides; a miss adopts the interface's
    /// default body), the associated bindings off `rule.interface_assoc`,
    /// etc.; the returned type args are the realization env for them.
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
        iface: TypeHead,
        iface_args: &[RealizedTy],
    ) -> Option<(RuntimeImplRuleCandidate<'vm>, Vec<RealizedTy>)> {
        let base = concrete_base(concrete_ty);
        let Some(query) = self.query(&base, iface, iface_args, &[]) else {
            return self
                .select_implements_rule(concrete_ty, iface, iface_args)
                .map(|(_, rule, bindings)| (rule, bindings));
        };
        let started = match self.lookup::<Option<CachedRule>>(&query) {
            Ok(answer) => {
                return answer.map(|rule| (self.cached_candidate(&rule), rule.bindings));
            }
            Err(started) => started,
        };
        let selected = self.select_implements_rule(concrete_ty, iface, iface_args);
        self.store(
            started,
            &query,
            selected
                .as_ref()
                .map(|(ptr, rule, bindings)| CachedRule::new(*ptr, rule, bindings)),
        );
        selected.map(|(_, rule, bindings)| (rule, bindings))
    }

    /// [`Self::resolve_implements_rule`] without the cache, with the selected
    /// rule's heap pointer.
    fn select_implements_rule(
        self,
        concrete_ty: &RealizedTy,
        iface: TypeHead,
        iface_args: &[RealizedTy],
    ) -> Option<(HeapPtr, RuntimeImplRuleCandidate<'vm>, Vec<RealizedTy>)> {
        // Static/package-image rules are immutable and already indexed by the
        // canonical interface pointer. Try that slice directly: this is the
        // overwhelmingly common virtual-call path and avoids candidate Vecs,
        // runtime-package graph walks, and dynamic-table locking.
        {
            let iface_ptr = iface.ptr();
            for &rule_ptr in self.vm.packages.impl_rules_of(iface_ptr) {
                let Some(rule) = self.vm.get_object(rule_ptr).as_impl_rule() else {
                    continue;
                };
                if let Some(type_args) = self.requested_rule_args(rule, concrete_ty, iface_args) {
                    return Some((rule_ptr, RuntimeImplRuleCandidate::Static(rule), type_args));
                }
            }
        }

        for (rule_ptr, rule) in self.rules_for(iface) {
            if let Some(type_args) = self.requested_rule_args(&rule, concrete_ty, iface_args) {
                return Some((rule_ptr, rule, type_args));
            }
        }
        None
    }

    fn requested_rule_args(
        self,
        rule: &RuntimeImplRule,
        concrete_ty: &RealizedTy,
        iface_args: &[RealizedTy],
    ) -> Option<Vec<RealizedTy>> {
        let type_args = self.rule_applies(rule, concrete_ty, iface_args, &mut Vec::new())?;
        // Select on the interface's input args only (associated types are outputs).
        let rule_args: Vec<RealizedTy> = rule
            .interface_args
            .iter()
            .map(|t| self.substitute_checked(t, &type_args))
            .collect();
        (iface_args.is_empty() || self.ty_args_equivalent(&rule_args, iface_args))
            .then_some(type_args)
    }

    /// Resolve `method` on `rule`: the provided row, or the adopted interface
    /// default constructed on the fly.
    ///
    /// A rule's method table carries ONLY what its impl block provides. A miss
    /// ADOPTS the interface's default body (Rust-trait adoption, not
    /// inheritance): the callee is the interface's bound
    /// [`default_fn`](bex_vm_types::types::InterfaceMethodDef::default_fn) and
    /// the frame is the adopted-default convention `[Self, interface args..]`,
    /// spelled over the rule's own templates (`for_ty_pattern` ++
    /// `interface_args`) so it realizes against the match exactly like a
    /// provided row's frame (see the frame law on
    /// [`MethodImpl`](bex_vm_types::types::MethodImpl)).
    ///
    /// TOTAL for an accepted program. The compiler checks every virtual call
    /// against the interface's declaration and proves every required method
    /// provided, so each way this can fail to name a callee is an invariant
    /// break and an `Err`, distinguished by kind: the rule's head is not an
    /// interface ([`VmInternalError::ImplRuleHeadNotInterface`]), the
    /// interface declares no such method
    /// ([`VmInternalError::UndeclaredInterfaceMethod`]), a required method is
    /// unprovided ([`VmInternalError::UnprovidedRequiredMethod`]), or a
    /// declared default was never bound
    /// ([`VmInternalError::UnboundInterfaceDefault`]). There is deliberately
    /// no "not found" answer: collapsing these into one let the shim lanes
    /// render structurally over a broken image. A reflective probe ("does
    /// this impl have `m`?") must consult the interface's declaration first
    /// and only then resolve here.
    pub(crate) fn rule_method_impl<'r>(
        self,
        rule: &'r RuntimeImplRule,
        method: &str,
    ) -> Result<RuleMethodImpl<'r>, VmInternalError> {
        use bex_vm_types::types::MethodImpl;
        if let Some(provided) = rule.methods.get(method) {
            return Ok(RuleMethodImpl {
                method: Cow::Borrowed(provided),
                is_default: false,
            });
        }
        let default_fn = self.interface_default(rule.interface_head, method)?;
        let mut frame = Vec::with_capacity(1 + rule.interface_args.len());
        frame.push(rule.for_ty_pattern.clone());
        frame.extend(rule.interface_args.iter().cloned());
        Ok(RuleMethodImpl {
            method: Cow::Owned(MethodImpl {
                fqn: default_fn,
                frame,
            }),
            is_default: true,
        })
    }

    fn interface_default(
        self,
        interface: bex_vm_types::HeapPtr,
        method: &str,
    ) -> Result<bex_vm_types::HeapPtr, VmInternalError> {
        use bex_vm_types::types::ObjectType;
        let head_object = self.vm.get_object(interface);
        let bex_vm_types::Object::Interface(iface) = head_object else {
            return Err(VmInternalError::ImplRuleHeadNotInterface {
                found: ObjectType::of(head_object),
                method: method.to_string(),
            });
        };
        let interface = || iface.name().to_string();
        let Some(method_def) = iface.methods.iter().find(|m| m.name.as_str() == method) else {
            return Err(VmInternalError::UndeclaredInterfaceMethod {
                interface: interface(),
                method: method.to_string(),
            });
        };
        let default_fn = method_def.default_fn;
        if default_fn.is_null() {
            return Err(match method_def.default {
                // A wire-declared default must have been bound to its pointer
                // at load/graft; a null alongside `default: Some(..)` is a
                // binding bug, not an absent default.
                Some(_) => VmInternalError::UnboundInterfaceDefault {
                    interface: interface(),
                    method: method.to_string(),
                },
                // No default and no provided row: the compiler rejects such
                // an impl, so this rule table is corrupt or stale.
                None => VmInternalError::UnprovidedRequiredMethod {
                    interface: interface(),
                    method: method.to_string(),
                },
            });
        }
        Ok(default_fn)
    }

    /// Realize a [`MethodImpl`](bex_vm_types::types::MethodImpl) frame template (De
    /// Bruijn over the impl's generic params) against the impl's bound type args
    /// (from [`Self::resolve_implements_rule`]). The result is the `frame.type_args`
    /// to seed the resolved callee with — the owner half of the frame convention
    /// (see `MethodImpl`): the impl's own generics for a provided method,
    /// `[Self, interface args..]` for an adopted default (associated types are
    /// not frame slots — a default body's `Self.Assoc` reduces as a projection).
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
        iface: TypeHead,
        requested_args: &[RealizedTy],
        requested_assoc: &[(Name, RealizedTy)],
    ) -> bool {
        let base = concrete_base(concrete_ty);
        let Some(query) = self.query(&base, iface, requested_args, requested_assoc) else {
            return self.prove(
                concrete_ty,
                iface,
                requested_args,
                requested_assoc,
                &mut Vec::new(),
            );
        };
        let started = match self.lookup::<bool>(&query) {
            Ok(proven) => return proven,
            Err(started) => started,
        };
        let proven = self.prove(
            concrete_ty,
            iface,
            requested_args,
            requested_assoc,
            &mut Vec::new(),
        );
        self.store(started, &query, proven);
        proven
    }

    /// Discharge one membership goal against the in-progress obligation `stack`.
    /// A goal already on the stack is an inductive cycle — a BAML interface is only
    /// satisfied by a concrete impl, so a goal that depends on itself has no base
    /// case and is unprovable. An over-budget stack (a goal that grows without
    /// repeating) is likewise rejected.
    fn prove(
        self,
        concrete_ty: &RealizedTy,
        iface: TypeHead,
        requested_args: &[RealizedTy],
        requested_assoc: &[(Name, RealizedTy)],
        stack: &mut Vec<Obligation>,
    ) -> bool {
        // The blanket stdlib impl exists to supply AnyClass's default-method
        // dispatch. Membership is narrower: class values only. Keep the
        // narrowing at the recursive proof seam so nested bounds cannot
        // observe the blanket rule's broader receiver.
        let any_class = self
            .vm
            .lang_head(baml_type::normalize::WellKnownHead::AnyClass.item());
        if any_class.is_some_and(|head| head == iface) {
            return matches!(concrete_ty, RealizedTy::Class(..));
        }

        // Key on the normalized (literal/enum-variant → base) type so `1` and `int`
        // are the same goal for cycle purposes.
        let goal: Obligation = (
            concrete_base(concrete_ty).into_owned(),
            iface,
            requested_args.to_vec(),
            requested_assoc.to_vec(),
        );
        if stack.contains(&goal) || stack.len() >= MAX_OBLIGATION_DEPTH {
            return false;
        }
        stack.push(goal);
        // `rules_for` borrows `self.vm` immutably while `stack` is borrowed
        // mutably inside the predicate, so the candidates are collected first.
        let candidates = self.rules_for(iface);
        let selected = candidates.into_iter().find_map(|(_, rule)| {
            let bindings = self.rule_applies(&rule, concrete_ty, requested_args, stack)?;
            self.interface_request_matches(&rule, &bindings, requested_args, &[])
                .then_some((rule, bindings))
        });
        let proven = match selected {
            Some((rule, bindings)) => {
                self.interface_request_matches(&rule, &bindings, requested_args, requested_assoc)
            }
            None => {
                requested_assoc.is_empty()
                    && self
                        .structural_default(iface, requested_args)
                        .is_some_and(|kind| {
                            kind != StructuralInterface::Hash
                                || self.hash_eligible(concrete_ty, iface, &mut Vec::new(), stack)
                        })
            }
        };
        // SAFETY: this call pushed its goal, and recursive calls remove only theirs.
        #[allow(unsafe_code)]
        unsafe {
            stack.no_return_pop();
        };
        proven
    }

    /// Match a rule's `for_ty_pattern` against `concrete_ty`, bind whatever
    /// generics that leaves open from the request's interface args, then
    /// discharge the rule's bounds. On success returns the bound generic args
    /// in de Bruijn order.
    fn rule_applies(
        self,
        rule: &RuntimeImplRule,
        concrete_ty: &RealizedTy,
        requested_args: &[RealizedTy],
        stack: &mut Vec<Obligation>,
    ) -> Option<Vec<RealizedTy>> {
        let base = concrete_base(concrete_ty);
        let concrete_ty = &*base;
        let mut bindings: Vec<Option<RealizedTy>> = vec![None; rule.generic_param_bounds.len()];
        if !self.match_template(&rule.for_ty_pattern, concrete_ty, &mut bindings) {
            return None;
        }
        // A generic the for-type pattern never mentions is bound from the
        // interface arguments instead (`implements<T, E> Modifier<T, E> for
        // Limit`): unify the rule's interface args with the requested ones,
        // into the same bindings. A slot that stays open cannot be inferred
        // from this request, so the rule does not apply. A fully constrained
        // rule takes the identical path it always has: this unification only
        // runs when the pattern left something open, and an empty request (a
        // non-generic interface) has nothing to bind.
        if bindings.iter().any(Option::is_none)
            && !requested_args.is_empty()
            && !self.all_match(&rule.interface_args, requested_args, &mut bindings)
        {
            return None;
        }
        let type_args: Vec<RealizedTy> = bindings.into_iter().collect::<Option<_>>()?;

        // Bounds as nested obligations (rustc winnowing): every interface in a param's
        // bound set must be implemented by that param's bound type arg, at the bound's
        // substituted args/assoc. `prove` guards against cycles and runaway depth.
        for (param, bounds) in rule.generic_param_bounds.iter().enumerate() {
            for bound in bounds {
                // The bound's args/assoc may reference the impl's params (`T extends
                // Container<U>`), so substitute them with the bindings, then require
                // the arg to implement the interface at that exact instantiation.
                let req_args: Vec<RealizedTy> = bound
                    .args
                    .iter()
                    .map(|t| self.substitute_checked(t, &type_args))
                    .collect();
                let req_assoc: Vec<(Name, RealizedTy)> = bound
                    .assoc
                    .iter()
                    .map(|(n, t)| (n.clone(), self.substitute_checked(t, &type_args)))
                    .collect();
                // An interface-existential type arg satisfies a bound that names the
                // same interface directly: the type checker only forms `Iterable<Item
                // = int>` for a value that already implements `Iterable`, and there is
                // no concrete impl rule to find for an interface *type*. This is the
                // only way `T extends Iterable` is discharged when `T` is realized to
                // an interface-existential (e.g. `Flatten<I extends Iterable>`
                // instantiated with `I = Iterable<int>`). It is bound-discharge only,
                // *not* general membership — an interface type does not *implement* the
                // interface (reflection's "does X implement I"), so it stays out of
                // `prove`/`type_implements`. (Super-interface bounds — an `Iterator`
                // existential under an `Iterable` bound — would additionally consult
                // the interface's `requires` closure; not handled here.)
                if self.interface_existential_satisfies_bound(
                    &type_args[param],
                    bound.interface,
                    &req_args,
                    &req_assoc,
                ) {
                    continue;
                }
                if !self.prove(
                    &type_args[param],
                    bound.interface,
                    &req_args,
                    &req_assoc,
                    stack,
                ) {
                    return None;
                }
            }
        }
        Some(type_args)
    }

    /// Whether `concrete_ty` is an interface-existential that discharges a bound
    /// naming `iface` directly (same interface, equivalent args, matching assoc).
    ///
    /// This is *bound discharge*, not membership: an interface type does not
    /// *implement* an interface, so this must not be folded into `prove` /
    /// `type_implements` (reflection relies on `Iterable` not implementing itself).
    fn interface_existential_satisfies_bound(
        self,
        concrete_ty: &RealizedTy,
        iface: TypeHead,
        requested_args: &[RealizedTy],
        requested_assoc: &[(Name, RealizedTy)],
    ) -> bool {
        let base = concrete_base(concrete_ty);
        let RealizedTy::Interface(ex_qtn, ex_args, ex_assoc) = base.as_ref() else {
            return false;
        };
        *ex_qtn == iface
            && (requested_args.is_empty() || self.ty_args_equivalent(ex_args, requested_args))
            && self.associated_bindings_equivalent(ex_assoc, requested_assoc)
    }

    /// Unify a `TyTemplate` pattern against a concrete `RealizedTy`, binding each
    /// `TypeArgRef(n)` into `bindings[n]`. A repeated param must bind consistently.
    fn match_template(
        self,
        pattern: &TyTemplate,
        concrete: &RealizedTy,
        bindings: &mut [Option<RealizedTy>],
    ) -> bool {
        // A fully-realized pattern carries no frame refs or holes: compare it to the
        // concrete type semantically (union-order-insensitive, matching the type
        // checker) through the canonical fact-opaque `StructuralEquivCtx`. The
        // flattened successor to the old `Concrete(t)` arm.
        if let Ok(realized) = <&RealizedTy>::try_from(pattern) {
            return StructuralEquivCtx(self.vm).equivalent(realized.as_ty(), concrete.as_ty());
        }

        match pattern {
            TyTemplate::TypeArgRef(n) => {
                match bindings.get_mut(*n as usize) {
                    Some(slot @ None) => {
                        *slot = Some(concrete.clone());
                        true
                    }
                    // A repeated param must bind consistently; compare semantically so a
                    // union binds the same regardless of member order.
                    Some(Some(bound)) => {
                        StructuralEquivCtx(self.vm).equivalent(bound.as_ty(), concrete.as_ty())
                    }
                    None => false,
                }
            }
            TyTemplate::List(inner) => match concrete {
                RealizedTy::List(elem) => self.match_template(inner, elem, bindings),
                _ => false,
            },
            TyTemplate::Map { key, value, .. } => match concrete {
                RealizedTy::Map {
                    key: ckey,
                    value: cvalue,
                    ..
                } => {
                    self.match_template(key, ckey, bindings)
                        && self.match_template(value, cvalue, bindings)
                }
                _ => false,
            },
            TyTemplate::Class(name, args) => match concrete {
                RealizedTy::Class(cname, cargs) => {
                    name == cname && self.all_match(args, cargs, bindings)
                }
                _ => false,
            },
            TyTemplate::Interface(name, args, assoc) => match concrete {
                RealizedTy::Interface(cname, cargs, cassoc) => {
                    // Each *concrete* binding must match a same-named pattern binding, found
                    // order-insensitively; extra pattern bindings don't constrain. This
                    // direction mirrors the compiler's selection matcher
                    // (`match_ty_pattern_into`'s `Interface` arm, which iterates the concrete
                    // bindings and requires each in the pattern) so runtime dispatch never
                    // selects an impl compile-time selection would reject. A positional,
                    // length-locked `zip` would instead diverge if the two declaration orders
                    // differed. (A top-level interface for-type is rejected by
                    // `is_valid_impl_subject`; this is only reached for a nested interface
                    // argument, where the binding sets coincide in well-formed code.)
                    name == cname
                        && self.all_match(args, cargs, bindings)
                        && cassoc.iter().all(|(cn, ct)| {
                            assoc
                                .iter()
                                .find(|(an, _)| an == cn)
                                .is_some_and(|(_, at)| self.match_template(at, ct, bindings))
                        })
                }
                _ => false,
            },
            // A symbolic projection whose witness type was not resolved at compile
            // time — never a match: a runtime value's concrete type carries no
            // unresolved projection to unify with.
            TyTemplate::AssociatedTypeProjection { .. } => false,
            TyTemplate::Function {
                params,
                ret,
                throws,
                ..
            } => match concrete {
                RealizedTy::Function {
                    params: cparams,
                    ret: cret,
                    throws: cthrows,
                    ..
                } => {
                    params.len() == cparams.len()
                        && params.iter().zip(cparams).all(|(p, cp)| {
                            p.name == cp.name
                                && p.mode == cp.mode
                                && self.match_template(&p.ty, &cp.ty, bindings)
                        })
                        && self.match_template(ret, cret, bindings)
                        && self.match_template(throws, cthrows, bindings)
                }
                _ => false,
            },
            TyTemplate::Future(value, error) => match concrete {
                RealizedTy::Future(cvalue, cerror) => {
                    self.match_template(value, cvalue, bindings)
                        && self.match_template(error, cerror, bindings)
                }
                _ => false,
            },
            // Order-insensitive union match: each pattern member must pair with a
            // distinct concrete member, with type-var bindings consistent across the
            // chosen pairing (so `Box<T | int>` matches a value `Box<int | string>`).
            TyTemplate::Union(parts) => match concrete {
                RealizedTy::Union(cparts) => self.match_union(parts, cparts, bindings),
                _ => false,
            },
            // Realized leaves are handled by the fast path above.
            _ => false,
        }
    }

    /// Pairwise-unify positional template args against concrete args (same arity).
    fn all_match(
        self,
        patterns: &[TyTemplate],
        concretes: &[RealizedTy],
        bindings: &mut [Option<RealizedTy>],
    ) -> bool {
        patterns.len() == concretes.len()
            && patterns
                .iter()
                .zip(concretes)
                .all(|(p, c)| self.match_template(p, c, bindings))
    }

    /// Match union members order-insensitively: every pattern member must pair with a
    /// *distinct* concrete member (a perfect matching), with type-var bindings
    /// consistent across the pairing. Backtracks — restoring `bindings` after a failed
    /// branch — because a greedy pairing can bind a type var to the wrong member
    /// (e.g. `[T, int]` against `[int, string]` must pick `T = string`, not `T = int`).
    fn match_union(
        self,
        patterns: &[TyTemplate],
        concretes: &[RealizedTy],
        bindings: &mut [Option<RealizedTy>],
    ) -> bool {
        if patterns.len() != concretes.len() {
            return false;
        }
        self.match_union_rec(
            patterns,
            concretes,
            &mut vec![false; concretes.len()],
            bindings,
        )
    }

    fn match_union_rec(
        self,
        patterns: &[TyTemplate],
        concretes: &[RealizedTy],
        used: &mut [bool],
        bindings: &mut [Option<RealizedTy>],
    ) -> bool {
        let Some((first, rest)) = patterns.split_first() else {
            return true;
        };
        for (i, concrete) in concretes.iter().enumerate() {
            if used[i] {
                continue;
            }
            let snapshot = bindings.to_vec();
            if self.match_template(first, concrete, bindings) {
                used[i] = true;
                if self.match_union_rec(rest, concretes, used, bindings) {
                    return true;
                }
                used[i] = false;
            }
            // Restore bindings mutated by the (failed) trial before trying the next.
            bindings.clone_from_slice(&snapshot);
        }
        false
    }

    /// Whether a matched impl's implemented-interface instantiation satisfies the
    /// request. The rule's interface args / bindings are concretised with the
    /// `for_ty_pattern` bindings, then compared; an empty request matches any.
    fn interface_request_matches(
        self,
        rule: &RuntimeImplRule,
        bindings: &[RealizedTy],
        requested_args: &[RealizedTy],
        requested_assoc: &[(Name, RealizedTy)],
    ) -> bool {
        let rule_args: Vec<RealizedTy> = rule
            .interface_args
            .iter()
            .map(|t| self.substitute_checked(t, bindings))
            .collect();
        let rule_assoc: Vec<(Name, RealizedTy)> = rule
            .interface_assoc
            .iter()
            .map(|(n, t)| (n.clone(), self.substitute_checked(t, bindings)))
            .collect();
        (requested_args.is_empty() || self.ty_args_equivalent(&rule_args, requested_args))
            && self.associated_bindings_equivalent(&rule_assoc, requested_assoc)
    }

    /// Compare two generic-argument lists for *semantic* equivalence, position-wise.
    /// Each pair is related by the canonical set-theoretic algebra through the
    /// fact-opaque [`StructuralEquivCtx`]: union order is normalized away (`Box<int |
    /// string>` ≡ `Box<string | int>`), `never` and literal members collapse
    /// (`int | never == int`), recursive aliases fold (`type A = int | A[]` ≡
    /// `type B = int | B[]`), and interface bindings compare as an order-insensitive
    /// set — while the re-entrant nominal facts stay opaque, so this never re-enters
    /// the resolver (see [`StructuralEquivCtx`]).
    pub(super) fn ty_args_equivalent(self, a: &[RealizedTy], b: &[RealizedTy]) -> bool {
        a.len() == b.len()
            && a.iter()
                .zip(b.iter())
                .all(|(x, y)| StructuralEquivCtx(self.vm).equivalent(x.as_ty(), y.as_ty()))
    }

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
                            && StructuralEquivCtx(self.vm)
                                .equivalent(impl_ty.as_ty(), requested_ty.as_ty())
                    })
                })
    }
}

impl ImplResolver<'_> {
    /// Realize an impl rule's interface-arg [`TyTemplate`] against a *complete* env
    /// (one binding per impl generic param) for the resolver's own arg *comparison* —
    /// not value materialization. An out-of-range `TypeArgRef` (a malformed rule) is
    /// reported by `substitute` itself and asserted here in debug; in release, any
    /// substitution failure — including a projection a non-matching candidate's
    /// bindings legitimately cannot reduce — falls back to the top type, keeping
    /// selection conservative without aborting the query. (Value materialization
    /// uses [`Self::realize_frame`], which is strict.)
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

impl ImplResolver<'_> {}

#[cfg(test)]
mod tests {
    use baml_type::{Freshness, Literal};

    use super::*;

    // Pins matcher invariance: a `Concrete` literal pattern matches only that exact
    // literal type, never its base primitive. The compiler applies `1 <: int` only at
    // the *top* of an overlap check (`cover`); under a constructor (`Box<1>` vs
    // `Box<int>`) both sides stay invariant, and this runtime matcher agrees — so a
    // `Box<1>` value does not dispatch an `implement … for Box<int>`. (Reached via
    // reflection today; this pins the behavior before the `==` dispatch wires the
    // resolver, where the literal-/attr-sensitivity would otherwise be untested.)
    #[test]
    fn concrete_literal_pattern_matches_only_the_literal() {
        let vm = crate::vm::tests::test_vm(Vec::new());
        let resolver = ImplResolver::new(&vm);
        let one = RealizedTy::Literal(Literal::Int(1), Freshness::Regular);
        let pattern = TyTemplate::from(RealizedTy::Literal(Literal::Int(1), Freshness::Regular));
        let mut binds: Vec<Option<RealizedTy>> = Vec::new();
        assert!(resolver.match_template(&pattern, &one, &mut binds));
        assert!(!resolver.match_template(&pattern, &RealizedTy::int(), &mut binds));
    }

    /// A runtime interface whose structural default is `ToString`'s, so every
    /// type without an explicit rule resolves to the default.
    fn defaulted_interface(vm: &mut BexVm, name: &str) -> TypeHead {
        use bex_heap::TlabHolder;
        use bex_vm_types::{Object, types::InterfaceDef};

        let type_tag = baml_type::typetag::TypeTag::fresh_dynamic();
        let interface = vm.alloc(Object::Interface(Box::new(InterfaceDef {
            path: bex_vm_types::types::LocalName::new(Vec::new(), Name::new(name)),
            structural_default: Some(bex_vm_types::types::StructuralDefault {
                kind: baml_type::StructuralInterface::ToString,
                function: bex_vm_types::ObjectIndex::from_raw(0),
                function_ptr: HeapPtr::null(),
            }),
            type_tag,
            args: Vec::new(),
            requires: Vec::new(),
            assoc: Vec::new(),
            fields: Vec::new(),
            methods: Vec::new(),
            owner: HeapPtr::null(),
        })));
        TypeHead::new(interface, type_tag)
    }

    /// An explicit rule implementing `interface` for `int`.
    fn int_rule(interface: HeapPtr) -> RuntimeImplRule {
        RuntimeImplRule {
            interface_head: interface,
            for_ty_pattern: TyTemplate::Int,
            generic_param_bounds: Vec::new(),
            interface_args: Vec::new(),
            interface_assoc: Vec::new(),
            methods: indexmap::IndexMap::new(),
            field_links: Box::default(),
        }
    }

    /// Register `rule` as a runtime witness of its interface, owned by a
    /// fresh heap object standing in for the minted class. Returns the
    /// rule's and the owner's pointers.
    fn register_witness(vm: &mut BexVm, rule: RuntimeImplRule) -> (HeapPtr, HeapPtr) {
        use bex_heap::TlabHolder;
        use bex_vm_types::Object;

        let interface = rule.interface_head;
        let rule = vm.alloc(Object::ImplRule(Box::new(rule)));
        let owner = vm.alloc(Object::String(bex_str::BexStr::from("witness owner")));
        vm.dynamic_dispatch.register_rule(
            interface,
            crate::package_load::DynRuleEntry { class: owner, rule },
        );
        (rule, owner)
    }

    fn is_explicit(resolved: Option<&ResolvedInterfaceImpl<'_>>) -> bool {
        matches!(resolved, Some(ResolvedInterfaceImpl::Explicit { .. }))
    }

    fn is_structural(resolved: Option<&ResolvedInterfaceImpl<'_>>) -> bool {
        matches!(resolved, Some(ResolvedInterfaceImpl::Structural { .. }))
    }

    #[test]
    fn explicit_rules_override_implicit_defaults_without_overlapping() {
        let mut vm = crate::vm::tests::test_vm(Vec::new());
        let head = defaulted_interface(&mut vm, "Defaulted");
        let interface = head.ptr();
        let explicit = int_rule(interface);
        {
            let rules = [explicit.clone()];
            let resolver = ImplResolver::new(&vm).with_staged_rules(&rules);
            let selected = resolver
                .resolve_implementation(&RealizedTy::Int, head, &[])
                .expect("explicit rule applies");
            assert!(matches!(selected, ResolvedInterfaceImpl::Explicit { .. }));
            assert!(
                resolver
                    .check_sole_implementation(&RealizedTy::Int, interface, &[])
                    .is_ok()
            );
            let selected = resolver
                .resolve_implementation(&RealizedTy::String, head, &[])
                .expect("default applies to other types");
            assert!(matches!(selected, ResolvedInterfaceImpl::Structural { .. }));
        }
        let overlapping = [explicit.clone(), explicit];
        assert!(
            ImplResolver::new(&vm)
                .with_staged_rules(&overlapping)
                .check_sole_implementation(&RealizedTy::Int, interface, &[])
                .is_err()
        );
        // Staged lookups describe a world nobody else sees: none is cached.
        assert_eq!(vm.impl_cache.borrow().len(), 0);
    }

    #[test]
    fn a_registered_witness_replaces_warm_answers() {
        let mut vm = crate::vm::tests::test_vm(Vec::new());
        let head = defaulted_interface(&mut vm, "Defaulted");
        let int = RealizedTy::Int;

        // Warm every entry point: `int` has only the structural default.
        let resolver = ImplResolver::new(&vm);
        assert!(is_structural(
            resolver.resolve_implementation(&int, head, &[]).as_ref()
        ));
        assert!(resolver.resolve_implements_rule(&int, head, &[]).is_none());
        assert!(resolver.type_implements(&int, head, &[], &[]));
        assert_eq!(vm.impl_cache.borrow().len(), 3);
        // A hit answers the same.
        assert!(is_structural(
            resolver.resolve_implementation(&int, head, &[]).as_ref()
        ));
        assert_eq!(vm.impl_cache.borrow().len(), 3);

        let (rule, _) = register_witness(&mut vm, int_rule(head.ptr()));

        let resolver = ImplResolver::new(&vm);
        assert!(is_explicit(
            resolver.resolve_implementation(&int, head, &[]).as_ref()
        ));
        assert!(resolver.resolve_implements_rule(&int, head, &[]).is_some());
        assert!(resolver.type_implements(&int, head, &[], &[]));
        assert!(
            vm.impl_cache
                .borrow()
                .cached_rules()
                .all(|cached| cached == rule)
        );
    }

    #[test]
    fn an_answer_computed_across_a_generation_bump_is_not_stored() {
        let mut vm = crate::vm::tests::test_vm(Vec::new());
        let head = defaulted_interface(&mut vm, "Defaulted");
        let query = Query {
            world: World {
                root: HeapPtr::null(),
                additional: None,
                lexical: HeapPtr::null(),
            },
            self_ty: &RealizedTy::Int,
            interface: head,
            args: &[],
            assoc: &[],
        };
        let mut cache = crate::impl_cache::ImplCache::default();
        assert!(cache.get::<bool>(0, &query).is_none());
        // The generation moved while the answer was computed.
        cache.insert(0, 1, &query, true);
        assert_eq!(cache.len(), 0);
        cache.insert(0, 0, &query, true);
        assert_eq!(cache.get::<bool>(0, &query), Some(&true));
        // A lookup under a newer generation drops what the old one stored.
        assert!(cache.get::<bool>(1, &query).is_none());
        assert_eq!(cache.len(), 0);
    }

    #[test]
    fn a_collection_forwards_live_answers_and_drops_dead_ones() {
        use bex_heap::CollectionLevel;
        use bex_vm_types::RootHaver as _;

        let mut vm = crate::vm::tests::test_vm(Vec::new());
        let live_head = defaulted_interface(&mut vm, "Live");
        let dead_head = defaulted_interface(&mut vm, "Dead");
        let (live_rule, live_owner) = register_witness(&mut vm, int_rule(live_head.ptr()));
        // Only the dynamic table, which holds nothing alive, refers to this
        // witness and its owner, so the collection frees both.
        register_witness(&mut vm, int_rule(dead_head.ptr()));

        let int = RealizedTy::Int;
        let resolver = ImplResolver::new(&vm);
        assert!(is_explicit(
            resolver
                .resolve_implementation(&int, live_head, &[])
                .as_ref()
        ));
        assert!(is_explicit(
            resolver
                .resolve_implementation(&int, dead_head, &[])
                .as_ref()
        ));
        assert_eq!(vm.impl_cache.borrow().len(), 2);

        let mut roots = Vec::new();
        vm.collect_roots(&mut roots);
        roots.extend([live_head.ptr(), dead_head.ptr(), live_rule, live_owner]);
        // SAFETY: the test VM is the heap's only mutator and holds no
        // pointer the root set misses.
        #[allow(unsafe_code, reason = "a test drives the collector directly")]
        let (_, _, forwarding) = unsafe {
            vm.heap
                .collect_garbage_generational(&roots, CollectionLevel::Major)
        };
        let moved_rule = forwarding[&live_rule];
        assert_ne!(
            moved_rule, live_rule,
            "the collection moves the live witness"
        );
        vm.forward_roots(&forwarding);
        crate::package_load::DynDispatchRoot::new(
            std::sync::Arc::clone(&vm.dynamic_dispatch),
            std::sync::Arc::clone(&vm.heap),
        )
        .forward_roots(&forwarding);

        // The live answer survived the collection at the rule's new address;
        // the answer naming the freed witness is gone.
        assert_eq!(vm.impl_cache.borrow().len(), 1);
        assert_eq!(
            vm.impl_cache.borrow().cached_rules().collect::<Vec<_>>(),
            [moved_rule]
        );
        let live_head = TypeHead::new(forwarding[&live_head.ptr()], live_head.tag());
        let dead_head = TypeHead::new(forwarding[&dead_head.ptr()], dead_head.tag());
        let resolver = ImplResolver::new(&vm);
        let Some(ResolvedInterfaceImpl::Explicit { rule, .. }) =
            resolver.resolve_implementation(&int, live_head, &[])
        else {
            panic!("the live witness still applies");
        };
        let moved = vm
            .get_object(moved_rule)
            .as_impl_rule()
            .expect("the moved witness is a rule");
        assert!(std::ptr::eq(&raw const *rule, moved));
        assert!(is_structural(
            resolver
                .resolve_implementation(&int, dead_head, &[])
                .as_ref()
        ));
    }

    #[test]
    fn a_collection_rekeys_answers_under_a_moved_world_root() {
        use bex_heap::{CollectionLevel, TlabHolder};
        use bex_vm_types::{Object, RootHaver as _};

        let mut vm = crate::vm::tests::test_vm(Vec::new());
        let head = defaulted_interface(&mut vm, "Defaulted");
        // Any heap object can stand for a runtime package here: the walk
        // skips a root that is not a package, and the answer is still keyed
        // under that root.
        let live_root = vm.alloc(Object::String(bex_str::BexStr::from("live package")));
        let dead_root = vm.alloc(Object::String(bex_str::BexStr::from("dead package")));
        let int = RealizedTy::Int;
        for root in [live_root, dead_root] {
            assert!(is_structural(
                ImplResolver::for_package(&vm, root)
                    .resolve_implementation(&int, head, &[])
                    .as_ref()
            ));
        }
        assert_eq!(vm.impl_cache.borrow().len(), 2);

        let mut roots = Vec::new();
        vm.collect_roots(&mut roots);
        roots.extend([head.ptr(), live_root]);
        // SAFETY: the test VM is the heap's only mutator and holds no
        // pointer the root set misses.
        #[allow(unsafe_code, reason = "a test drives the collector directly")]
        let (_, _, forwarding) = unsafe {
            vm.heap
                .collect_garbage_generational(&roots, CollectionLevel::Major)
        };
        let moved_root = forwarding[&live_root];
        assert_ne!(moved_root, live_root, "the collection moves the live root");
        vm.forward_roots(&forwarding);

        // The answer under the freed root is gone; the other one now lives
        // under the moved root, so asking there is a hit that stores nothing.
        assert_eq!(vm.impl_cache.borrow().len(), 1);
        let head = TypeHead::new(forwarding[&head.ptr()], head.tag());
        assert!(is_structural(
            ImplResolver::for_package(&vm, moved_root)
                .resolve_implementation(&int, head, &[])
                .as_ref()
        ));
        assert_eq!(vm.impl_cache.borrow().len(), 1);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn threads_with_warm_answers_see_a_witness_registered_mid_run() {
        use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

        const WORKERS: usize = 4;
        let mut vm = crate::vm::tests::test_vm(Vec::new());
        let head = defaulted_interface(&mut vm, "Defaulted");
        let workers: Vec<BexVm> = (0..WORKERS)
            .map(|_| crate::vm::tests::test_vm_sharing(&vm))
            .collect();
        let registered = AtomicBool::new(false);
        let warmed = AtomicUsize::new(0);
        std::thread::scope(|scope| {
            for worker in workers {
                let (registered, warmed) = (&registered, &warmed);
                scope.spawn(move || {
                    let mut warm = false;
                    loop {
                        // Read the flag first: a worker that saw the
                        // registration finish must find the witness.
                        let after = registered.load(Ordering::Acquire);
                        let explicit = is_explicit(
                            ImplResolver::new(&worker)
                                .resolve_implementation(&RealizedTy::Int, head, &[])
                                .as_ref(),
                        );
                        if after {
                            assert!(explicit, "a warm cache hid a registered witness");
                            break;
                        }
                        if !warm {
                            warm = true;
                            warmed.fetch_add(1, Ordering::SeqCst);
                        }
                    }
                });
            }
            // Register only once every worker holds a cached answer.
            while warmed.load(Ordering::SeqCst) < WORKERS {
                std::hint::spin_loop();
            }
            register_witness(&mut vm, int_rule(head.ptr()));
            registered.store(true, Ordering::Release);
        });
    }
}
