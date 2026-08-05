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
//! The store also owns the canonical interner: each distinct form it has been
//! asked to intern gets a [`CanonId`]. (Sessions intern goal keys for whatever
//! forms their walks produced — a limit-starved walk interns a *partial* form,
//! which is fine for cache keying since probes hit only on the identical form.
//! What upholds the public identity contract is the minting gate at the
//! boundary: only canonical-tier forms are ever handed out as identity tokens,
//! so two escaped ids compare form-equality *of canonical representatives*,
//! which is type identity.) The id **value space is never reused** — a
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
//! ids); sweeping them is a job for the first real eviction policy — until one
//! exists they simply persist for the store's life, sound but unreclaimed.

use rustc_hash::FxHashMap;

use super::NormalTy;
use crate::{Name, QualifiedTypeName};

/// A weak handle to one canonical form in one store — the O(1) identity token.
///
/// This is the external identity handle the algebra hands out (a
/// [`Normalized`](super::Normalized) carries one for a canonical-tier form):
/// `a == b` means the two ids name the same type, unconditionally; `a != b`
/// means different types only while both ids are live in their store — either
/// side evicted, the comparison says nothing, and the holder re-normalizes to
/// refresh. See the module docs for why that contract holds.
///
/// `Copy` and 8 bytes, so it travels freely; deliberately neither `Ord` (minted
/// in first-encounter order, so an ordering would leak query order into
/// observable output) nor serializable (meaningless outside its store).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CanonId {
    slot: u32,
    generation: u32,
}

/// One interner slot: the current generation, and the live form if any.
struct Slot {
    generation: u32,
    ty: Option<NormalTy>,
}

/// What a derivation consumed — measured at recording time, re-verified (and, for
/// steps, re-charged) at admission, so a cache hit spends exactly what the
/// recompute it stands in for would.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Cost {
    /// Membership-goal depth headroom the derivation needed below its entry point,
    /// its own goal included — the deepest extent explored, failed branches and
    /// all, which a recompute would explore again.
    pub(super) member_depth: usize,
    /// Steps consumed by the whole derivation.
    pub(super) steps: u64,
}

/// One recorded verdict: the answer plus its [`Cost`], which gates admission.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Answer {
    pub(super) proven: bool,
    pub(super) cost: Cost,
}

/// A membership goal in canonical form, keyed by identity: any respelling of the
/// subject, its arguments, or its bindings canonicalizes to the same forms and so
/// interns to the same ids. Ids are stable for as long as a session holds its
/// store (the exclusive borrow rules out mid-derivation eviction), which is what
/// makes the in-progress repeat scan exact.
#[derive(Debug, PartialEq, Eq, Hash)]
pub(super) struct MembershipGoal {
    pub(super) subject: CanonId,
    pub(super) interface: QualifiedTypeName,
    pub(super) args: Vec<CanonId>,
    /// Sorted by name, so declaration order never distinguishes two requests.
    pub(super) assoc: Vec<(Name, CanonId)>,
}

/// Theorems of one world: the canonical interner plus the answer tables.
#[derive(Default)]
pub(super) struct Answers {
    slots: Vec<Slot>,
    /// Evicted slots available for re-mint (each already generation-bumped).
    free: Vec<u32>,
    /// The live map: canonical form → its current id. Injective by construction.
    ids: FxHashMap<NormalTy, CanonId>,
    /// Barriered subtype verdicts, keyed by the operands' ids.
    subtype: FxHashMap<(CanonId, CanonId), Answer>,
    /// Membership verdicts, keyed by canonical goal. The session records every
    /// goal whose scope closed grounded; what never lands here is a cycle
    /// participant other than its head, or anything a limit touched.
    membership: FxHashMap<MembershipGoal, Answer>,
}

impl Answers {
    /// The id of `ty`'s canonical form, minting one if none is live. The deep
    /// clones (one into the slot for resolution, one as the live-map key) happen
    /// only at first mint, paid once per distinct type.
    pub(super) fn intern(&mut self, ty: &NormalTy) -> CanonId {
        if let Some(&id) = self.ids.get(ty) {
            return id;
        }
        let id = match self.free.pop() {
            Some(slot) => {
                let entry = &mut self.slots[slot as usize];
                debug_assert!(entry.ty.is_none(), "a free-listed slot was still live");
                entry.ty = Some(ty.clone());
                CanonId {
                    slot,
                    generation: entry.generation,
                }
            }
            None => {
                let slot = u32::try_from(self.slots.len())
                    .unwrap_or_else(|_| unreachable!("the interner's slot space is exhausted"));
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
    pub(super) fn lookup(&self, ty: &NormalTy) -> Option<CanonId> {
        self.ids.get(ty).copied()
    }

    /// The recorded verdict for `sub <: sup`, if any.
    pub(super) fn subtype_answer(&self, sub: CanonId, sup: CanonId) -> Option<Answer> {
        self.subtype.get(&(sub, sup)).copied()
    }

    /// Record a grounded subtype verdict. Entries are theorems of the world, so a
    /// re-recording may refresh the cost but can never flip the verdict.
    pub(super) fn record_subtype(&mut self, sub: CanonId, sup: CanonId, answer: Answer) {
        let previous = self.subtype.insert((sub, sup), answer);
        debug_assert!(
            previous.is_none_or(|p| p.proven == answer.proven),
            "a recorded subtype verdict flipped"
        );
    }

    /// The recorded verdict for a membership goal, if any.
    pub(super) fn membership_answer(&self, goal: &MembershipGoal) -> Option<Answer> {
        self.membership.get(goal).copied()
    }

    /// Record a grounded membership verdict; same theorem contract as
    /// [`Self::record_subtype`].
    pub(super) fn record_membership(&mut self, goal: MembershipGoal, answer: Answer) {
        let previous = self.membership.insert(goal, answer);
        debug_assert!(
            previous.is_none_or(|p| p.proven == answer.proven),
            "a recorded membership verdict flipped"
        );
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
    pub(super) fn evict(&mut self, id: CanonId) {
        let Some(entry) = self.slots.get_mut(id.slot as usize) else {
            debug_assert!(false, "an id from another store reached this one");
            return;
        };
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
    pub(super) fn live_ids(&self) -> impl Iterator<Item = CanonId> {
        self.ids.values().copied()
    }

    /// Resolve a live id back to its form; `None` for dead ids.
    #[cfg(test)]
    pub(super) fn resolve(&self, id: CanonId) -> Option<&NormalTy> {
        let entry = self.slots.get(id.slot as usize)?;
        if entry.generation != id.generation {
            return None;
        }
        entry.ty.as_ref()
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
