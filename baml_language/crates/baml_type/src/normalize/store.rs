//! The answer store: theorems of one world, separated from the search that finds them.
//!
//! A [`SolverSession`](super::solve::SolverSession) is ephemeral *search* state; this is
//! established *knowledge*. Only grounded, refusal-free answers are ever recorded, so
//! every entry is a pure function of its goal and its world (the facts and clauses it
//! was derived over) — nothing here can go stale while the world stands, which is what
//! makes a store shareable across sessions and lifetimes at all. The caller owns the
//! association between a store and its world: entries keyed under one world's canonical
//! forms mean nothing in another, so a store is never shared across fact/clause sources.
//!
//! # Weak canonical identity
//!
//! The store also owns the canonical interner: each distinct canonical form it has been
//! asked to intern gets a [`CanonId`], and the id **value space is never reused** — a
//! slot's generation is bumped on eviction, and a slot whose generation would wrap is
//! retired instead. That one property carries the whole identity contract:
//!
//! - `a == b` ⇒ same type, unconditionally — each id value was only ever minted for one
//!   canonical form, so equality holds even for ids whose entries have been evicted.
//! - `a != b` ⇒ different types only when **both are live**: the live map is injective
//!   (re-minting after eviction retires the old id first), so two simultaneously-live
//!   ids cannot name one type. Either side dead ⇒ the comparison says nothing.
//! - Resolution (id → form) requires liveness; a dead id degrades to a pure
//!   positive-comparison witness whose holder re-interns to refresh. No invalidation
//!   sweep is ever needed, which is what lets each embedding pick its own eviction
//!   policy (never within a compiler revision; capacity- and GC-driven in a
//!   long-running VM) without coordinating with id holders.
//!
//! Ids are deliberately not ordered ([`CanonId`] has no `Ord`): they are minted in
//! first-encounter order, so any ordering derived from them would leak query order into
//! observable output. They are cache keys and identity tokens, nothing else — never
//! serialized, never compared across stores.
//!
//! # Advisory answers
//!
//! The answer tables are advisory: a probe may miss for anything at any time (eviction
//! is always legal), because admission re-charges a hit's recorded cost — a hit is
//! observationally identical to the recompute it replaces, so losing an entry loses
//! only time. Entries keyed by dead ids become unreachable (new probes intern fresh
//! ids) and are reclaimed by whatever eviction policy the store's owner runs.

use rustc_hash::FxHashMap;

use super::NormalTy;

/// A weak handle to one canonical form in one store — the O(1) identity token.
///
/// See the module docs for the contract. `Copy` and 8 bytes, so it travels freely;
/// deliberately neither `Ord` nor serializable.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(in crate::normalize) struct CanonId {
    slot: u32,
    generation: u32,
}

/// One interner slot: the current generation, and the live form if any.
struct Slot {
    generation: u32,
    ty: Option<NormalTy>,
}

/// What a derivation consumed — measured at recording time, re-charged at admission,
/// so a cache hit spends exactly what the recompute it stands in for would.
#[derive(Clone, Copy)]
pub(in crate::normalize) struct Cost {
    /// Steps consumed by the whole derivation.
    pub(in crate::normalize) steps: u64,
}

/// One recorded verdict: the answer plus its [`Cost`], which gates admission.
#[derive(Clone, Copy)]
pub(in crate::normalize) struct Answer {
    pub(in crate::normalize) proven: bool,
    pub(in crate::normalize) cost: Cost,
}

/// Theorems of one world: the canonical interner plus the answer tables.
#[derive(Default)]
pub(in crate::normalize) struct Answers {
    slots: Vec<Slot>,
    /// Evicted slots available for re-mint (each already generation-bumped).
    free: Vec<u32>,
    /// The live map: canonical form → its current id. Injective by construction.
    ids: FxHashMap<NormalTy, CanonId>,
    /// Barriered subtype verdicts, keyed by the operands' ids.
    subtype: FxHashMap<(CanonId, CanonId), Answer>,
}

impl Answers {
    /// The id of `ty`'s canonical form, minting one if none is live. Clones the form
    /// only at first mint — the one deep operation, paid once per distinct type.
    pub(in crate::normalize) fn intern(&mut self, ty: &NormalTy) -> CanonId {
        if let Some(&id) = self.ids.get(ty) {
            return id;
        }
        let id = match self.free.pop() {
            Some(slot) => {
                let entry = &mut self.slots[slot as usize];
                entry.ty = Some(ty.clone());
                CanonId {
                    slot,
                    generation: entry.generation,
                }
            }
            None => {
                let slot = u32::try_from(self.slots.len())
                    .unwrap_or_else(|_| unreachable!("more than u32::MAX live canonical forms"));
                self.slots.push(Slot {
                    generation: 0,
                    ty: Some(ty.clone()),
                });
                CanonId {
                    slot,
                    generation: 0,
                }
            }
        };
        self.ids.insert(ty.clone(), id);
        id
    }

    /// The live id of `ty`, if one has been minted and not evicted.
    pub(in crate::normalize) fn lookup(&self, ty: &NormalTy) -> Option<CanonId> {
        self.ids.get(ty).copied()
    }

    /// The recorded verdict for `sub <: sup`, if any.
    pub(in crate::normalize) fn subtype_answer(
        &self,
        sub: CanonId,
        sup: CanonId,
    ) -> Option<Answer> {
        self.subtype.get(&(sub, sup)).copied()
    }

    pub(in crate::normalize) fn record_subtype(
        &mut self,
        sub: CanonId,
        sup: CanonId,
        answer: Answer,
    ) {
        self.subtype.insert((sub, sup), answer);
    }

    /// Evict one live identity: the form loses its id (a later intern mints a fresh
    /// one), and the id becomes a dead witness — still valid for positive comparison,
    /// no longer resolvable. Answers keyed by it become unreachable, which is sound
    /// (a probe under fresh ids misses and recomputes) and reclaimed by policy.
    ///
    /// Test-only until a production eviction policy exists (the long-running VM
    /// tier); the *semantics* are load-bearing now because the identity contract —
    /// never-reused id values — is what every holder of a `CanonId` relies on.
    #[cfg(test)]
    pub(in crate::normalize) fn evict(&mut self, id: CanonId) {
        let entry = &mut self.slots[id.slot as usize];
        if entry.generation != id.generation {
            return; // Already dead: eviction is idempotent per id value.
        }
        let Some(ty) = entry.ty.take() else {
            return;
        };
        self.ids.remove(&ty);
        // The never-reuse guarantee: bump the generation so this id value can never
        // be minted again; a slot whose generation would wrap is retired outright.
        if let Some(next) = entry.generation.checked_add(1) {
            entry.generation = next;
            self.free.push(id.slot);
        }
    }

    /// Every currently-live id, for tests that exercise eviction.
    #[cfg(test)]
    pub(in crate::normalize) fn live_ids(&self) -> impl Iterator<Item = CanonId> + '_ {
        self.ids.values().copied()
    }

    /// Resolve a live id back to its form; `None` for dead ids.
    #[cfg(test)]
    pub(in crate::normalize) fn resolve(&self, id: CanonId) -> Option<&NormalTy> {
        let entry = &self.slots[id.slot as usize];
        (entry.generation == id.generation).then_some(entry.ty.as_ref())?
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interning_the_same_form_yields_the_same_id() {
        let mut store = Answers::default();
        let a = store.intern(&NormalTy::Int);
        let b = store.intern(&NormalTy::Int);
        assert_eq!(a, b);
        assert_eq!(store.resolve(a), Some(&NormalTy::Int));
    }

    #[test]
    fn distinct_forms_get_distinct_ids() {
        let mut store = Answers::default();
        let a = store.intern(&NormalTy::Int);
        let b = store.intern(&NormalTy::String);
        assert_ne!(a, b);
    }

    #[test]
    fn an_evicted_id_is_a_dead_witness_and_the_form_reminted_fresh() {
        let mut store = Answers::default();
        let first = store.intern(&NormalTy::Int);
        store.evict(first);
        // Dead: no longer resolvable, no longer returned by lookup…
        assert_eq!(store.resolve(first), None);
        assert_eq!(store.lookup(&NormalTy::Int), None);
        // …and the re-mint is a fresh id value: `first != second` even though both
        // were minted for `int`, which is exactly why inequality concludes nothing
        // unless both sides are live.
        let second = store.intern(&NormalTy::Int);
        assert_ne!(first, second);
        assert_eq!(store.resolve(second), Some(&NormalTy::Int));
    }

    #[test]
    fn a_reused_slot_can_never_reissue_a_dead_id() {
        let mut store = Answers::default();
        let first = store.intern(&NormalTy::Int);
        store.evict(first);
        // The slot is reused for a *different* form: the generation bump keeps the
        // new id distinct from every id the slot ever issued.
        let other = store.intern(&NormalTy::String);
        assert_ne!(first, other);
        assert_eq!(store.resolve(other), Some(&NormalTy::String));
        assert_eq!(store.resolve(first), None);
    }

    #[test]
    fn eviction_is_idempotent_per_id_value() {
        let mut store = Answers::default();
        let first = store.intern(&NormalTy::Int);
        store.evict(first);
        let second = store.intern(&NormalTy::Int);
        // Evicting the long-dead first id again must not disturb the live second.
        store.evict(first);
        assert_eq!(store.resolve(second), Some(&NormalTy::Int));
        assert_eq!(store.lookup(&NormalTy::Int), Some(second));
    }
}
