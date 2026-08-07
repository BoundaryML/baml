//! The payload of a runtime `type` value: a described type plus its minted
//! identity (BEP-066 slice 1).
//!
//! Equality and hashing on a [`TypeValue`] are **exactly the mint** — never
//! the heap pointer (the GC is copying, so a pointer can never be an identity
//! token — I-4) and never a fresh structural walk of `ty` (the three
//! historically divergent equality paths this replaces). The mint is plain
//! data carried inline in the object, so GC copies and `baml.deep_copy`
//! preserve identity by construction (I-1: a copy *is* the same type value).

use baml_type::{QualifiedTypeName, RealizedTy, normalize::TypeContext};
use indexmap::IndexMap;

use crate::HeapPtr;

/// Runtime schema definitions carried by a minted `type` value.
///
/// The map is a per-value overlay, never a process/global registry. Heap
/// pointers keep the ordinary `Object::Class` and `Object::Enum` definitions
/// authoritative for reflection and parsed values; the owning `Object::Type`
/// traces them.
#[derive(Debug, Clone, Default)]
pub struct DynTypeDefs {
    pub classes: IndexMap<QualifiedTypeName, HeapPtr>,
    pub enums: IndexMap<QualifiedTypeName, HeapPtr>,
    /// Runtime interface witnesses owned by this mint.  Unlike source impls,
    /// these rules are value-scoped: keeping the type value alive keeps the
    /// witness (and its method handles) alive; dropping it makes the rule
    /// eligible for the same sweep as the runtime class definition.
    pub impl_rules: Vec<HeapPtr>,
}

impl DynTypeDefs {
    pub fn with_class(name: QualifiedTypeName, ptr: HeapPtr) -> Self {
        Self {
            classes: IndexMap::from([(name, ptr)]),
            enums: IndexMap::new(),
            impl_rules: Vec::new(),
        }
    }

    pub fn with_enum(name: QualifiedTypeName, ptr: HeapPtr) -> Self {
        Self {
            classes: IndexMap::new(),
            enums: IndexMap::from([(name, ptr)]),
            impl_rules: Vec::new(),
        }
    }

    pub fn merge_from(&mut self, other: &Self) {
        for (name, ptr) in &other.classes {
            self.classes.entry(name.clone()).or_insert(*ptr);
        }
        for (name, ptr) in &other.enums {
            self.enums.entry(name.clone()).or_insert(*ptr);
        }
        self.impl_rules.extend(other.impl_rules.iter().copied());
    }

    pub fn is_empty(&self) -> bool {
        self.classes.is_empty() && self.enums.is_empty() && self.impl_rules.is_empty()
    }
}

/// A minted identity token for a runtime `type` value.
///
/// The enum discriminant participates in derived equality, so a `Static` and
/// a `Runtime` mint never compare equal — even on a raw `u64` collision — and
/// a constructed type can never alias a static declaration (I-1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MintId {
    /// A static spelling (`type.of<T>()`, a reflected signature, a wire-named
    /// type): the deterministic canonical-form digest computed by
    /// `baml_type::normalize::canonical_digest`. Equivalent static spellings
    /// (`string?` vs `string | null`, permuted unions) share a digest, so
    /// every reference to a static declaration is the same value with no
    /// intern table (I-2), and re-materializations (a second `type.of<T>()`,
    /// a sys-op round trip) rebuild the same identity.
    Static(u64),
    /// One per constructor evaluation (I-1): allocated from the monotonic
    /// engine-wide counter on `BexHeap` (`mint_runtime_id`), shared by
    /// spawned VMs. No producer exists yet in slice 1 — the structured
    /// constructors land in slice 2 — but the variant is part of the
    /// equality/hash contract now so the semantics cannot drift.
    Runtime(u64),
}

/// Provenance retained by a runtime-created nominal definition.
///
/// The definition itself cannot include its own pointer, so `defs` contains
/// only dependencies. `type.of_value` adds the instance/variant's definition
/// pointer back when reconstructing the original minted type value.
#[derive(Debug, Clone)]
pub struct RuntimeTypeProvenance {
    pub mint: MintId,
    pub defs: DynTypeDefs,
}

/// What an `Object::Type` wraps: the described type and its identity.
///
/// `==`/`Hash` are mint-only (see [`MintId`]); `ty` is carried data that the
/// VM reads for rendering, parsing, dispatch, and reflection. Two values with
/// different `ty` payloads and equal mints cannot arise from the constructors
/// below: a `Static` mint is a function of `ty`'s canonical form, and a
/// `Runtime` mint is globally unique.
#[derive(Debug, Clone)]
pub struct TypeValue {
    /// The type this value denotes.
    pub ty: RealizedTy,
    mint: MintId,
    defs: DynTypeDefs,
}

impl TypeValue {
    /// Mint a type value for a **statically spelled** type: the mint is the
    /// canonical-form digest of `ty` under `ctx`.
    ///
    /// `ctx` decides which facts the canonical form folds in (alias
    /// expansion, union absorption); every site materializing types for the
    /// same program must supply the same fact source — inside the VM that is
    /// the VM itself (use `BexVm::alloc_static_type`, which also memoizes the
    /// walk).
    pub fn static_new<C: TypeContext>(ty: RealizedTy, ctx: &C) -> Self {
        let digest = baml_type::normalize::canonical_digest(ty.as_ty(), ctx);
        Self {
            ty,
            mint: MintId::Static(digest),
            defs: DynTypeDefs::default(),
        }
    }

    /// Assemble a type value from an already-derived mint.
    ///
    /// For the memoized static-digest path (`BexVm::alloc_static_type`), the
    /// slice-2 runtime constructors (a counter mint from
    /// `BexHeap::mint_runtime_id`), and tests pinning mint semantics. The
    /// caller owns the invariant that `mint` was produced for `ty` — a
    /// mismatched pair breaks type-value equality program-wide.
    pub fn from_parts(ty: RealizedTy, mint: MintId) -> Self {
        Self {
            ty,
            mint,
            defs: DynTypeDefs::default(),
        }
    }

    pub fn from_parts_with_defs(ty: RealizedTy, mint: MintId, defs: DynTypeDefs) -> Self {
        Self { ty, mint, defs }
    }

    /// This value's identity token.
    pub fn mint(&self) -> MintId {
        self.mint
    }

    pub fn defs(&self) -> &DynTypeDefs {
        &self.defs
    }

    pub fn defs_mut(&mut self) -> &mut DynTypeDefs {
        &mut self.defs
    }
}

/// Identity comparison (I-1/I-2): the mint, nothing else. `ty` is deliberately
/// excluded — see the struct doc.
impl PartialEq for TypeValue {
    fn eq(&self, other: &Self) -> bool {
        self.mint == other.mint
    }
}

impl Eq for TypeValue {}

/// Identity hashing (I-4): consistent with `==` by hashing exactly what `==`
/// compares.
impl std::hash::Hash for TypeValue {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.mint.hash(state);
    }
}

#[cfg(test)]
mod tests {
    use std::hash::{BuildHasher, RandomState};

    use super::*;

    /// Runtime mints are counter-identities: distinct counters are distinct
    /// types even for byte-identical `ty` payloads (I-1), and the variant
    /// discriminant keeps `Runtime` disjoint from `Static` even when the raw
    /// `u64`s collide.
    #[test]
    fn mint_distinctness() {
        assert_ne!(MintId::Runtime(0), MintId::Runtime(1));
        assert_ne!(MintId::Runtime(7), MintId::Static(7));
        assert_eq!(MintId::Runtime(3), MintId::Runtime(3));
        assert_eq!(MintId::Static(3), MintId::Static(3));

        let a = TypeValue::from_parts(RealizedTy::int(), MintId::Runtime(0));
        let b = TypeValue::from_parts(RealizedTy::int(), MintId::Runtime(1));
        let c = TypeValue::from_parts(RealizedTy::int(), MintId::Static(0));
        assert_ne!(a, b, "distinct runtime mints are distinct identities");
        assert_ne!(a, c, "a runtime mint never equals a static mint");
        assert_eq!(a, a.clone(), "a copy preserves identity (I-1)");
    }

    /// `Hash` must be consistent with `==` (I-4): equal values hash equal,
    /// and the `ty` payload is excluded from both.
    #[test]
    fn hash_consistent_with_eq() {
        let a = TypeValue::from_parts(RealizedTy::int(), MintId::Static(42));
        let b = TypeValue::from_parts(RealizedTy::string(), MintId::Static(42));
        assert_eq!(a, b, "mint-equal values are equal regardless of payload");
        // Same-RandomState comparison isn't available through `hash_one`
        // twice (each `RandomState::new()` is keyed); use one state for both.
        let state = RandomState::new();
        assert_eq!(state.hash_one(&a), state.hash_one(&b));
        // Unequal values need not hash differently, but hashing a different
        // mint directly must remain supported by the same implementation.
        let c = TypeValue::from_parts(RealizedTy::int(), MintId::Static(43));
        assert_ne!(a, c);
        let _ = state.hash_one(&c);
    }

    /// The static digest is a pure function of the canonical form: equivalent
    /// spellings share a mint, distinct types do not, and derivation is
    /// deterministic across independent derivations (two "processes" worth of
    /// state in one test — nothing session-local feeds the digest).
    #[test]
    fn static_digest_deterministic_and_canonical() {
        #[expect(
            deprecated,
            reason = "unit test of the fact-free digest itself; the VM paths under test \
                      elsewhere supply the real fact context"
        )]
        let ctx = baml_type::normalize::NoFacts;

        let optional = RealizedTy::Union(
            vec![RealizedTy::string(), RealizedTy::null()],
            baml_type::TyAttr::default(),
        );
        let reversed = RealizedTy::Union(
            vec![RealizedTy::null(), RealizedTy::string()],
            baml_type::TyAttr::default(),
        );
        let a = TypeValue::static_new(optional.clone(), &ctx);
        let b = TypeValue::static_new(reversed, &ctx);
        assert_eq!(a, b, "permuted union spellings share a static mint");

        let again = TypeValue::static_new(optional, &ctx);
        assert_eq!(a.mint(), again.mint(), "derivation is deterministic");

        let other = TypeValue::static_new(RealizedTy::int(), &ctx);
        assert_ne!(a, other, "distinct types get distinct static mints");
    }
}
