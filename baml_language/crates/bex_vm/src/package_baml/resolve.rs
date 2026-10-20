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

use std::{
    borrow::Cow,
    hash::{Hash, Hasher},
    sync::Arc,
};

use baml_type::{Literal, Name, StructuralInterface, normalize::TypeContext};
use bex_vm_types::{
    HeapPtr, RealizedTy, TyTemplate, TypeHead, errors::VmInternalError, types::RuntimeImplRule,
};
use indexmap::{Equivalent, IndexMap};

use crate::{BexVm, type_context::StructuralEquivCtx, vec_ext::VecExt};

/// An impl's bound generic args (de Bruijn order), as
/// [`ImplResolver::resolve_implements_rule`] returns them. Shared rather than
/// owned so a cached resolution hands out the same allocation on every hit
/// instead of cloning a `Vec<RealizedTy>`; callers only ever read it as a
/// slice. Empty for the common non-generic impl.
pub(crate) type BoundTypeArgs = Arc<[RealizedTy]>;

/// Per-VM memo of [`ImplResolver::resolve_implements_rule`].
///
/// Dispatch asks the same `(Self, Iface<Args>)` question over and over with
/// the same answer — one per decoded class value, per `==`, per
/// `String.from` — and each answer is a pattern unification plus bound
/// discharge over every candidate rule. The answer is a pure function of
/// the question, the resolver's *world* (which runtime packages it searches,
/// see [`ResolutionWorld`]), and the rule-world epoch
/// ([`DynDispatchTables::rule_world_epoch`](crate::package_load::DynDispatchTables::rule_world_epoch)),
/// so it is keyed on exactly those. Negative answers are cached too: a rule
/// can only *appear* through a witness registration, a runtime package
/// publish, or a collection, and each of those moves the epoch, which
/// empties the cache on the next lookup.
///
/// The cache lives on the VM (behind a `RefCell`, since the resolver holds
/// `&BexVm`) rather than on the shared tables: a VM runs on one thread at a
/// time, so no lock is needed, and the hit rate is high within one decode or
/// loop. It stores heap *pointers* (the rule, the world's packages) and
/// head-carrying types, so a moving collection also clears it
/// (`BexVm::forward_roots`); the epoch bump in
/// [`DynDispatchTables::sweep_and_forward`](crate::package_load::DynDispatchTables::sweep_and_forward)
/// covers a VM the collection did not visit. Lookups never hold the borrow
/// across resolution, which may re-enter the resolver through projection
/// reduction.
#[derive(Default)]
pub(crate) struct ImplRuleCache {
    epoch: u64,
    entries: IndexMap<ImplRuleKey, Option<CachedImplRule>>,
}

impl ImplRuleCache {
    /// Drop every memoized answer (a collection moved what they point at).
    pub(crate) fn clear(&mut self) {
        self.entries.clear();
    }

    /// Keep a runaway VM (one minting types forever) from growing the memo
    /// without bound; a realistic program has far fewer distinct questions.
    const CAPACITY: usize = 4096;
}

/// The runtime packages a resolver searches besides the static index: the
/// root (the receiver's owner, an explicitly inspected package, or the
/// lexical frame's) and the lexical frame's when it differs. Null for a
/// static-only world.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct ResolutionWorld {
    root: HeapPtr,
    additional: HeapPtr,
}

struct ImplRuleKey {
    concrete: RealizedTy,
    iface: TypeHead,
    iface_args: Box<[RealizedTy]>,
    world: ResolutionWorld,
}

/// The borrowed form of [`ImplRuleKey`] a lookup probes with, so a hit never
/// clones the concrete type or the interface args. Hashes and compares
/// field-for-field like the owned key (both are written out by hand so they
/// cannot drift apart).
struct ImplRuleKeyRef<'a> {
    concrete: &'a RealizedTy,
    iface: TypeHead,
    iface_args: &'a [RealizedTy],
    world: ResolutionWorld,
}

impl Hash for ImplRuleKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.concrete.hash(state);
        self.iface.hash(state);
        self.iface_args[..].hash(state);
        self.world.hash(state);
    }
}

impl PartialEq for ImplRuleKey {
    fn eq(&self, other: &Self) -> bool {
        self.iface == other.iface
            && self.world == other.world
            && self.concrete == other.concrete
            && self.iface_args == other.iface_args
    }
}

impl Eq for ImplRuleKey {}

impl Hash for ImplRuleKeyRef<'_> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.concrete.hash(state);
        self.iface.hash(state);
        self.iface_args.hash(state);
        self.world.hash(state);
    }
}

impl Equivalent<ImplRuleKey> for ImplRuleKeyRef<'_> {
    fn equivalent(&self, key: &ImplRuleKey) -> bool {
        self.iface == key.iface
            && self.world == key.world
            && *self.concrete == key.concrete
            && self.iface_args == &key.iface_args[..]
    }
}

impl ImplRuleKeyRef<'_> {
    fn to_owned(&self) -> ImplRuleKey {
        ImplRuleKey {
            concrete: self.concrete.clone(),
            iface: self.iface,
            iface_args: self.iface_args.into(),
            world: self.world,
        }
    }
}

/// A memoized positive answer: the selected rule by heap pointer (re-borrowed
/// through the VM on every hit, exactly as the uncached path borrows it) and
/// its bound type args.
struct CachedImplRule {
    rule: HeapPtr,
    is_static: bool,
    bound_args: BoundTypeArgs,
}

/// A resolver candidate borrows an immutable rule from the heap. Every rule —
/// compiled into the static image, owned by a runtime package, or registered
/// as an anonymous class's witness — is an `Object::ImplRule`, so no candidate
/// ever copies rule metadata. Candidates differ only in cacheability: a
/// static rule lives in the never-moving compile-time region.
#[derive(Clone, Copy)]
pub(crate) struct RuntimeImplRuleCandidate<'vm> {
    rule: &'vm RuntimeImplRule,
    /// The rule's heap object, what the [`ImplRuleCache`] remembers it by.
    /// Null for a staged rule, which has no heap home (and is never cached).
    ptr: HeapPtr,
    is_static: bool,
}

/// An implementation selected without adding synthetic rules to the registry.
/// Structural defaults carry the exact `Self` type for native and shared-body frames.
pub(crate) enum ResolvedInterfaceImpl<'vm> {
    Explicit {
        rule: RuntimeImplRuleCandidate<'vm>,
        bindings: BoundTypeArgs,
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

impl<'vm> RuntimeImplRuleCandidate<'vm> {
    pub(crate) fn is_static(&self) -> bool {
        self.is_static
    }

    /// A staged rule: borrowed from the registration, not the heap.
    fn staged(rule: &'vm RuntimeImplRule) -> Self {
        Self {
            rule,
            ptr: HeapPtr::null(),
            is_static: false,
        }
    }
}

impl std::ops::Deref for RuntimeImplRuleCandidate<'_> {
    type Target = RuntimeImplRule;

    fn deref(&self) -> &Self::Target {
        self.rule
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
            .filter(|rule| self.requested_rule_args(rule, concrete, args).is_some())
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

    /// Every impl rule of `iface` in the program, in package-load order.
    ///
    /// The program-wide [`PackageIndex`](crate::package_load::PackageIndex) is
    /// keyed by the interface's canonical `Object::Interface` pointer, so this is
    /// one O(1) lookup over a table that already spans every package — see that
    /// type's docs for why a per-package search cannot be narrowed correctly. An
    /// unknown interface (not loaded) has no impls anywhere.
    fn rules_for(self, iface: TypeHead) -> Vec<RuntimeImplRuleCandidate<'vm>> {
        let iface_ptr = iface.ptr();
        let mut rules = self
            .rule_pointers_for(iface)
            .into_iter()
            .filter_map(|rule_ptr| self.heap_candidate(rule_ptr, false))
            .collect::<Vec<_>>();
        rules.extend(
            self.staged_rules
                .iter()
                .filter(|rule| rule.interface_head == iface_ptr)
                .map(RuntimeImplRuleCandidate::staged),
        );
        rules
    }

    /// Borrow the `Object::ImplRule` at `ptr` as a candidate, or `None` when
    /// the pointer names some other object.
    fn heap_candidate(
        self,
        ptr: HeapPtr,
        is_static: bool,
    ) -> Option<RuntimeImplRuleCandidate<'vm>> {
        let rule = self.vm.get_object(ptr).as_impl_rule()?;
        Some(RuntimeImplRuleCandidate {
            rule,
            ptr,
            is_static,
        })
    }

    /// The runtime packages this resolver searches, as a cache key: the root
    /// is the explicit one or the lexical frame's, exactly as
    /// [`Self::rule_pointers_for`] walks them.
    fn world(self) -> ResolutionWorld {
        ResolutionWorld {
            root: self
                .root_package
                .unwrap_or_else(|| self.vm.current_runtime_package()),
            additional: self.additional_package.unwrap_or_else(HeapPtr::null),
        }
    }

    /// Every published impl rule of `iface` in this resolver's world, by heap
    /// pointer and deduplicated, in package-load order: the static index,
    /// then the runtime package chain, then the dynamic witnesses. Staged
    /// rules are not here (they have no heap home); [`Self::rules_for`] adds
    /// them.
    fn rule_pointers_for(self, iface: TypeHead) -> Vec<HeapPtr> {
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
        let world = self.world();
        let mut packages = vec![world.root];
        if !world.additional.is_null() {
            packages.push(world.additional);
        }
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
        pointers
            .into_iter()
            .chain(self.vm.dynamic_dispatch.rules_of(iface_ptr))
            .filter(|rule_ptr| seen_rules.insert(*rule_ptr))
            .collect()
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
        if let Some((rule, bindings)) = self.resolve_implements_rule(self_ty, interface, args) {
            return Some(ResolvedInterfaceImpl::Explicit { rule, bindings });
        }
        let kind = self.structural_default(interface, args)?;
        if kind == StructuralInterface::Hash
            && !self.hash_eligible(self_ty, interface, &mut Vec::new(), &mut Vec::new())
        {
            return None;
        }
        Some(ResolvedInterfaceImpl::Structural {
            kind,
            interface,
            self_ty: self_ty.clone(),
        })
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
            .any(|rule| self.rule_applies(rule, ty, &[], obligations).is_some())
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
            && self.rules_for(equals).iter().any(|rule| {
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
    ///
    /// Memoized per VM in the [`ImplRuleCache`] (see its docs for the key and
    /// the invalidation rule). A resolver with staged rules answers in a
    /// world nobody else sees, so it bypasses the memo entirely.
    pub(crate) fn resolve_implements_rule(
        self,
        concrete_ty: &RealizedTy,
        iface: TypeHead,
        iface_args: &[RealizedTy],
    ) -> Option<(RuntimeImplRuleCandidate<'vm>, BoundTypeArgs)> {
        if !self.staged_rules.is_empty() {
            return self
                .resolve_implements_rule_uncached(concrete_ty, iface, iface_args)
                .map(|(rule, bound_args)| (rule, BoundTypeArgs::from(bound_args)));
        }

        let key = ImplRuleKeyRef {
            concrete: concrete_ty,
            iface,
            iface_args,
            world: self.world(),
        };
        let epoch = self.vm.dynamic_dispatch.rule_world_epoch();
        {
            // The borrow ends before any resolution work: unification can
            // re-enter this function through projection reduction.
            let mut cache = self.vm.impl_rule_cache.borrow_mut();
            if cache.epoch != epoch {
                cache.entries.clear();
                cache.epoch = epoch;
            }
            match cache.entries.get(&key) {
                Some(None) => return None,
                Some(Some(hit)) => {
                    // The pointer is as fresh as the epoch: a collection or a
                    // publish moves the epoch, which just emptied the memo.
                    if let Some(rule) = self.heap_candidate(hit.rule, hit.is_static) {
                        return Some((rule, Arc::clone(&hit.bound_args)));
                    }
                }
                None => {}
            }
        }

        let answer = self
            .resolve_implements_rule_uncached(concrete_ty, iface, iface_args)
            .map(|(rule, bound_args)| (rule, BoundTypeArgs::from(bound_args)));
        let mut cache = self.vm.impl_rule_cache.borrow_mut();
        // Only memoize an answer computed wholly within the epoch the memo is
        // filled at; a change mid-resolution leaves this one uncached.
        if cache.epoch == epoch {
            if cache.entries.len() >= ImplRuleCache::CAPACITY {
                cache.entries.clear();
            }
            cache.entries.insert(
                key.to_owned(),
                answer.as_ref().map(|(rule, bound_args)| CachedImplRule {
                    rule: rule.ptr,
                    is_static: rule.is_static,
                    bound_args: Arc::clone(bound_args),
                }),
            );
        }
        answer
    }

    /// The uncached selection: the static image first (immutable, indexed by
    /// the canonical interface pointer — the overwhelmingly common path,
    /// with no candidate `Vec`, package walk, or table lock), then every
    /// rule of this resolver's world in load order, staged rules last.
    fn resolve_implements_rule_uncached(
        self,
        concrete_ty: &RealizedTy,
        iface: TypeHead,
        iface_args: &[RealizedTy],
    ) -> Option<(RuntimeImplRuleCandidate<'vm>, Vec<RealizedTy>)> {
        for &rule_ptr in self.vm.packages.impl_rules_of(iface.ptr()) {
            let Some(rule) = self.heap_candidate(rule_ptr, true) else {
                continue;
            };
            if let Some(type_args) = self.requested_rule_args(&rule, concrete_ty, iface_args) {
                return Some((rule, type_args));
            }
        }
        for rule in self.rules_for(iface) {
            if let Some(type_args) = self.requested_rule_args(&rule, concrete_ty, iface_args) {
                return Some((rule, type_args));
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
        // An empty request matches any instantiation: nothing to compare, so
        // skip realizing the rule's interface args (a non-generic interface,
        // the common case, would otherwise pay a substitution per candidate).
        if iface_args.is_empty() {
            return Some(type_args);
        }
        // Select on the interface's input args only (associated types are outputs).
        let rule_args: Vec<RealizedTy> = rule
            .interface_args
            .iter()
            .map(|t| self.substitute_checked(t, &type_args))
            .collect();
        self.ty_args_equivalent(&rule_args, iface_args)
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
        self.prove(
            concrete_ty,
            iface,
            requested_args,
            requested_assoc,
            &mut Vec::new(),
        )
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
        let selected = candidates.into_iter().find_map(|rule| {
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

    #[test]
    fn explicit_rules_override_implicit_defaults_without_overlapping() {
        use bex_heap::TlabHolder;
        use bex_vm_types::{HeapPtr, Object, types::InterfaceDef};

        let mut vm = crate::vm::tests::test_vm(Vec::new());
        let type_tag = baml_type::typetag::TypeTag::fresh_dynamic();
        let interface = vm.alloc(Object::Interface(Box::new(InterfaceDef {
            path: bex_vm_types::types::LocalName::new(Vec::new(), Name::new("Defaulted")),
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
        let head = TypeHead::new(interface, type_tag);
        let explicit = RuntimeImplRule {
            interface_head: interface,
            for_ty_pattern: TyTemplate::Int,
            generic_param_bounds: Vec::new(),
            interface_args: Vec::new(),
            interface_assoc: Vec::new(),
            methods: indexmap::IndexMap::new(),
            field_links: Box::default(),
        };
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
    }

    // Pins the memo's invalidation rule: a negative answer is cached, and a
    // witness registered afterwards (which moves the rule-world epoch) is
    // found on the next lookup rather than shadowed by the stale `None`.
    #[test]
    fn memoized_negative_answer_is_invalidated_by_a_registration() {
        use bex_heap::TlabHolder;
        use bex_vm_types::{HeapPtr, Object, types::InterfaceDef};

        let mut vm = crate::vm::tests::test_vm(Vec::new());
        let type_tag = baml_type::typetag::TypeTag::fresh_dynamic();
        let interface = vm.alloc(Object::Interface(Box::new(InterfaceDef {
            path: bex_vm_types::types::LocalName::new(Vec::new(), Name::new("Marker")),
            structural_default: None,
            type_tag,
            args: Vec::new(),
            requires: Vec::new(),
            assoc: Vec::new(),
            fields: Vec::new(),
            methods: Vec::new(),
            owner: HeapPtr::null(),
        })));
        let head = TypeHead::new(interface, type_tag);
        let resolver = ImplResolver::new(&vm);
        assert!(
            resolver
                .resolve_implements_rule(&RealizedTy::Int, head, &[])
                .is_none()
        );
        // The second ask is served from the memo.
        assert!(
            resolver
                .resolve_implements_rule(&RealizedTy::Int, head, &[])
                .is_none()
        );
        let rule = vm.alloc(Object::ImplRule(Box::new(RuntimeImplRule {
            interface_head: interface,
            for_ty_pattern: TyTemplate::Int,
            generic_param_bounds: Vec::new(),
            interface_args: Vec::new(),
            interface_assoc: Vec::new(),
            methods: indexmap::IndexMap::new(),
            field_links: Box::default(),
        })));
        vm.dynamic_dispatch.register_rule(
            interface,
            crate::package_load::DynRuleEntry {
                class: HeapPtr::null(),
                rule,
            },
        );
        let resolver = ImplResolver::new(&vm);
        let (found, bound) = resolver
            .resolve_implements_rule(&RealizedTy::Int, head, &[])
            .expect("the registered witness is found");
        assert!(!found.is_static());
        assert!(bound.is_empty());
        // The positive answer is memoized too: the same heap rule comes back.
        let (again, _) = resolver
            .resolve_implements_rule(&RealizedTy::Int, head, &[])
            .expect("memoized");
        assert_eq!(found.ptr, again.ptr);
        assert!(
            resolver
                .resolve_implements_rule(&RealizedTy::String, head, &[])
                .is_none()
        );
    }
}
