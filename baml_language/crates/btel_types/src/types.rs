//! Definitions of the classes and enums captured values name.
//!
//! A captured value names a declaration by its type tag: the engine-scoped,
//! never-reused identity a type head carries (static: the declaration's
//! compile-time object index; runtime: a process-wide counter). The tag is
//! distinct from the name (two runtime classes may share one) and from the
//! content (two classes may hold the same fields). The runtime registers each
//! declaration the first time a recorded capture names it; a resolver later
//! copies its definition out of the heap, off the VM thread, and the recorder
//! publishes it. The lookup is weak: it never keeps a declaration alive, and
//! the runtime's collector repairs or prunes it, extracting the definition of
//! a declaration that dies before it was resolved.
//!
//! Static declarations never move or die, so they live outside the map the
//! collector walks: a bitmap records which were registered, and their
//! pointers wait in their own queue. Dynamic declarations live in a sharded
//! weak map. Work beyond the queue's bound is deferred, not dropped: a
//! deferred declaration is queued again as the queue drains.

use std::{
    collections::VecDeque,
    sync::{
        Mutex, MutexGuard, OnceLock, PoisonError, RwLock, RwLockReadGuard, RwLockWriteGuard,
        atomic::{AtomicU64, AtomicUsize, Ordering},
    },
};

use baml_type::{DeclarationName, TyTemplate, typetag::TypeTag};
use borsh::{BorshDeserialize, BorshSerialize};
use btel_settings::{
    identity::{HASH_MULTIPLIER, SHARD_BITS, SHARD_COUNT},
    layout::Padded,
};
use rustc_hash::FxHashMap;

/// A class or enum a definition's field type names: its identity and, when
/// the runtime resolved it, its name.
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub struct DefinitionHead {
    pub tag: TypeTag,
    pub name: Option<DeclarationName>,
}

/// A field's type. Generic parameters are positional (`TypeArgRef(N)`): a
/// runtime class keeps only how many it has.
pub type FieldType = TyTemplate<DefinitionHead>;

/// What a class or enum declares, copied out of the heap.
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub struct TypeDeclaration {
    pub is_enum: bool,
    pub name: DeclarationName,
    pub type_params: u32,
    pub description: Option<String>,
    pub alias: Option<String>,
    pub docstring: Option<String>,
    pub attributes: Vec<(String, String)>,
    pub stream_done: bool,
    pub fields: Vec<TypeField>,
    pub variants: Vec<TypeVariant>,
}

#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub struct TypeField {
    pub name: String,
    pub schema: FieldType,
    pub description: Option<String>,
    pub alias: Option<String>,
    pub docstring: Option<String>,
    pub attributes: Vec<(String, String)>,
    pub skip: bool,
    pub stream_done: bool,
    pub must_exist: bool,
}

#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub struct TypeVariant {
    pub name: String,
    pub description: Option<String>,
    pub alias: Option<String>,
    pub docstring: Option<String>,
    pub attributes: Vec<(String, String)>,
    pub skip: bool,
}

/// One declaration's definition, or the observation that it could not be
/// copied (pending work overflowed). A later definition supersedes an
/// unavailable observation; nothing erases a known definition.
#[derive(Clone, Debug)]
pub struct TypeDefinition {
    pub tag: TypeTag,
    pub declaration: Option<TypeDeclaration>,
}

/// What a resolver offers the recorder on one poll.
#[derive(Debug)]
pub enum TypeResolution {
    /// Heap access is not available right now; pending work is kept.
    Busy,
    /// Definitions copied or settled since the last poll. `more` is true when
    /// declarations are still pending.
    Ready {
        definitions: Vec<TypeDefinition>,
        more: bool,
    },
}

/// The runtime side of definition resolution, polled by the recorder on the
/// processor thread. Never blocks: when heap access would wait, it answers
/// `Busy` and keeps its work.
pub trait TypeDefinitionSource: Send + Sync {
    fn try_take(&self, max: usize) -> TypeResolution;
}

/// A source with no definitions, for recordings no runtime heap backs.
pub struct NoTypeDefinitions;

impl TypeDefinitionSource for NoTypeDefinitions {
    fn try_take(&self, _: usize) -> TypeResolution {
        TypeResolution::Ready {
            definitions: Vec::new(),
            more: false,
        }
    }
}

/// Dynamic declarations queued for resolution at once. Registrations beyond
/// it are deferred and queued again as the queue drains, so a burst never
/// loses a definition and the queue never grows past this.
pub const MAX_PENDING_DECLARATIONS: usize = 16_384;
/// Definitions extracted at collection and not yet taken by the recorder.
/// Beyond this, an extracted definition is dropped (and counted): its
/// references keep their ids without a definition.
pub const MAX_SETTLED_DEFINITIONS: usize = 4_096;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum State {
    /// In the pending queue.
    Queued,
    /// Registered while the queue was full; queued again as it drains.
    Deferred,
    /// Copied: by the resolver, or by the collector before reclaiming it.
    Resolved,
}

struct Entry<R> {
    reference: R,
    state: State,
}

type Shard<R> = Padded<RwLock<FxHashMap<TypeTag, Entry<R>>>>;

// Locks here are taken inside the collector's safepoint, where a panic would
// abort the process: a poisoned lock is recovered, never unwrapped.
fn read<T>(lock: &RwLock<T>) -> RwLockReadGuard<'_, T> {
    lock.read().unwrap_or_else(PoisonError::into_inner)
}
fn write<T>(lock: &RwLock<T>) -> RwLockWriteGuard<'_, T> {
    lock.write().unwrap_or_else(PoisonError::into_inner)
}
fn lock<T>(lock: &Mutex<T>) -> MutexGuard<'_, T> {
    lock.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Weak map from a registered declaration's tag to its current reference,
/// with the queues of declarations still to resolve. Engine-owned, like
/// [`crate::FunctionLookup`].
pub struct DeclarationLookup<R> {
    /// One bit per compile-time object: registered. Sized on first use.
    static_registered: OnceLock<Box<[AtomicU64]>>,
    /// Registered static declarations not yet resolved. Their references
    /// never move, and the program bounds how many there are.
    static_pending: Mutex<Vec<(TypeTag, R)>>,
    shards: [Shard<R>; SHARD_COUNT],
    pending: Mutex<VecDeque<TypeTag>>,
    /// Work counters, so the resolver's "anything to do?" takes no lock.
    static_pending_len: AtomicUsize,
    pending_len: AtomicUsize,
    deferred: AtomicUsize,
    settled_len: AtomicUsize,
    /// Definitions dropped because no consumer took them in time.
    lost: AtomicUsize,
    /// Owned definitions waiting for the recorder, extracted before their
    /// declaration was reclaimed. At most [`MAX_SETTLED_DEFINITIONS`].
    settled: Mutex<Vec<TypeDefinition>>,
}

impl<R> Default for DeclarationLookup<R> {
    fn default() -> Self {
        Self {
            static_registered: OnceLock::new(),
            static_pending: Mutex::new(Vec::new()),
            shards: std::array::from_fn(|_| Padded(RwLock::new(FxHashMap::default()))),
            pending: Mutex::new(VecDeque::new()),
            static_pending_len: AtomicUsize::new(0),
            pending_len: AtomicUsize::new(0),
            deferred: AtomicUsize::new(0),
            settled_len: AtomicUsize::new(0),
            lost: AtomicUsize::new(0),
            settled: Mutex::new(Vec::new()),
        }
    }
}

impl<R: Copy> DeclarationLookup<R> {
    #[inline]
    fn shard(&self, tag: TypeTag) -> &RwLock<FxHashMap<TypeTag, Entry<R>>> {
        #[expect(clippy::cast_sign_loss, reason = "hashing the tag's bits")]
        let bits = tag.as_i64() as u64;
        let index = bits.wrapping_mul(HASH_MULTIPLIER) >> (u64::BITS - SHARD_BITS);
        &self.shards[index as usize].0
    }

    /// Register a declaration a recorded capture names, before the capture is
    /// published. The caller holds heap access and `reference` is the live
    /// declaration `tag` identifies. `statics` is how many compile-time
    /// objects there are, when `reference` is one of them: such a
    /// declaration never moves or dies, and its registration is one atomic
    /// read after the first. True when this call registered it.
    pub fn register(&self, tag: TypeTag, reference: R, statics: Option<usize>) -> bool {
        if let (Some(index), Some(count)) = (tag.static_index(), statics)
            && index < count
        {
            let bits = self
                .static_registered
                .get_or_init(|| (0..count.div_ceil(64)).map(|_| AtomicU64::new(0)).collect());
            let (word, mask) = (&bits[index / 64], 1u64 << (index % 64));
            if word.load(Ordering::Acquire) & mask != 0
                || word.fetch_or(mask, Ordering::AcqRel) & mask != 0
            {
                return false;
            }
            lock(&self.static_pending).push((tag, reference));
            self.static_pending_len.fetch_add(1, Ordering::AcqRel);
            return true;
        }
        if read(self.shard(tag)).contains_key(&tag) {
            return false;
        }
        let mut shard = write(self.shard(tag));
        if shard.contains_key(&tag) {
            return false;
        }
        let state = {
            let mut pending = lock(&self.pending);
            if pending.len() < MAX_PENDING_DECLARATIONS {
                pending.push_back(tag);
                self.pending_len.fetch_add(1, Ordering::AcqRel);
                State::Queued
            } else {
                self.deferred.fetch_add(1, Ordering::AcqRel);
                State::Deferred
            }
        };
        shard.insert(tag, Entry { reference, state });
        true
    }

    /// Whether a dynamic `tag` is registered. Static declarations are
    /// checked by [`Self::register`] itself, without a lock.
    pub fn contains(&self, tag: TypeTag) -> bool {
        read(self.shard(tag)).contains_key(&tag)
    }

    /// Whether anything waits: declarations to resolve, deferred ones, or
    /// settled definitions. Lock-free.
    pub fn has_work(&self) -> bool {
        self.static_pending_len.load(Ordering::Acquire)
            + self.pending_len.load(Ordering::Acquire)
            + self.deferred.load(Ordering::Acquire)
            + self.settled_len.load(Ordering::Acquire)
            > 0
    }

    /// Declarations queued or deferred, not yet resolved.
    pub fn pending_len(&self) -> usize {
        self.static_pending_len.load(Ordering::Acquire)
            + self.pending_len.load(Ordering::Acquire)
            + self.deferred.load(Ordering::Acquire)
    }

    /// Definitions dropped because the settled queue was full.
    pub fn lost(&self) -> usize {
        self.lost.load(Ordering::Acquire)
    }

    /// Up to `max` declarations to resolve, with their current references.
    /// The caller holds heap access, resolves each and `mark_resolved`s it.
    /// A declaration collected meanwhile was extracted by the collector and
    /// is skipped. Deferred declarations are queued as room frees up.
    pub fn take_pending(&self, max: usize) -> Vec<(TypeTag, R)> {
        let mut out = Vec::new();
        if self.static_pending_len.load(Ordering::Acquire) > 0 {
            let mut statics = lock(&self.static_pending);
            let take = statics.len().min(max);
            out.extend(statics.drain(..take));
            self.static_pending_len.fetch_sub(take, Ordering::AcqRel);
        }
        if self.deferred.load(Ordering::Acquire) > 0 {
            self.requeue_deferred();
        }
        let mut pending = lock(&self.pending);
        while out.len() < max {
            let Some(tag) = pending.pop_front() else {
                break;
            };
            self.pending_len.fetch_sub(1, Ordering::AcqRel);
            if let Some(entry) = read(self.shard(tag)).get(&tag)
                && entry.state == State::Queued
            {
                out.push((tag, entry.reference));
            }
        }
        out
    }

    /// Queue deferred declarations while the queue has room.
    fn requeue_deferred(&self) {
        let mut pending = lock(&self.pending);
        for shard in &self.shards {
            if pending.len() >= MAX_PENDING_DECLARATIONS
                || self.deferred.load(Ordering::Acquire) == 0
            {
                return;
            }
            let mut shard = write(&shard.0);
            for (tag, entry) in shard.iter_mut() {
                if entry.state != State::Deferred {
                    continue;
                }
                if pending.len() >= MAX_PENDING_DECLARATIONS {
                    return;
                }
                entry.state = State::Queued;
                pending.push_back(*tag);
                self.pending_len.fetch_add(1, Ordering::AcqRel);
                self.deferred.fetch_sub(1, Ordering::AcqRel);
            }
        }
    }

    pub fn mark_resolved(&self, tag: TypeTag) {
        if let Some(entry) = write(self.shard(tag)).get_mut(&tag) {
            entry.state = State::Resolved;
        }
    }

    /// Hand a definition to the recorder without heap access. Never waits on
    /// telemetry and never grows past [`MAX_SETTLED_DEFINITIONS`].
    pub fn settle(&self, definition: TypeDefinition) {
        let mut settled = lock(&self.settled);
        if settled.len() >= MAX_SETTLED_DEFINITIONS {
            self.lost.fetch_add(1, Ordering::AcqRel);
            return;
        }
        settled.push(definition);
        self.settled_len.fetch_add(1, Ordering::AcqRel);
    }

    pub fn take_settled(&self) -> Vec<TypeDefinition> {
        if self.settled_len.load(Ordering::Acquire) == 0 {
            return Vec::new();
        }
        let taken = std::mem::take(&mut *lock(&self.settled));
        self.settled_len.fetch_sub(taken.len(), Ordering::AcqRel);
        taken
    }

    /// At the collector's safepoint, with mutators and resolvers excluded:
    /// `keep` repairs a surviving dynamic reference in place and answers
    /// whether it survives. Returns the dying declarations that were never
    /// resolved, so the collector can extract their definitions before
    /// reclaiming them. Static declarations are never visited.
    pub fn retain(&self, mut keep: impl FnMut(&mut R) -> bool) -> Vec<(TypeTag, R)> {
        let mut dying = Vec::new();
        for shard in &self.shards {
            let mut shard = write(&shard.0);
            if shard.is_empty() {
                continue;
            }
            shard.retain(|tag, entry| {
                if keep(&mut entry.reference) {
                    return true;
                }
                match entry.state {
                    State::Resolved => {}
                    State::Deferred => {
                        self.deferred.fetch_sub(1, Ordering::AcqRel);
                        dying.push((*tag, entry.reference));
                    }
                    // Its tag stays queued; taking it skips the missing entry.
                    State::Queued => dying.push((*tag, entry.reference)),
                }
                false
            });
            if shard.is_empty() {
                *shard = FxHashMap::default();
            } else if shard.len()
                < shard.capacity() / btel_settings::identity::SHRINK_OCCUPANCY_DIVISOR
            {
                let target = shard
                    .len()
                    .saturating_mul(btel_settings::identity::RETAINED_CAPACITY_MULTIPLIER);
                shard.shrink_to(target);
            }
        }
        dying
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tag(n: i64) -> TypeTag {
        TypeTag::from_i64(n)
    }
    fn dynamic(n: i64) -> TypeTag {
        TypeTag::from_i64(baml_type::typetag::DYNAMIC_BASE + n)
    }
    fn declaration() -> TypeDeclaration {
        TypeDeclaration {
            is_enum: true,
            name: DeclarationName::Anonymous("E".into()),
            type_params: 0,
            description: None,
            alias: None,
            docstring: None,
            attributes: Vec::new(),
            stream_done: false,
            fields: Vec::new(),
            variants: Vec::new(),
        }
    }

    #[test]
    fn static_declarations_register_once_without_the_map() {
        let lookup = DeclarationLookup::default();
        // Static tag 100 + 3: object 3 of a 10-object image.
        assert!(lookup.register(tag(103), 3_usize, Some(10)));
        assert!(!lookup.register(tag(103), 3, Some(10)));
        assert!(
            !lookup.contains(tag(103)),
            "statics stay out of the weak map"
        );
        assert_eq!(lookup.pending_len(), 1);
        assert_eq!(lookup.take_pending(10), [(tag(103), 3)]);
        assert!(!lookup.has_work());
        // Never registered again, so never resolved twice.
        assert!(!lookup.register(tag(103), 3, Some(10)));
        assert!(
            lookup.retain(|_| false).is_empty(),
            "the collector never sees statics"
        );
    }

    #[test]
    fn dynamic_registration_is_once_and_pending_until_resolved() {
        let lookup = DeclarationLookup::default();
        assert!(lookup.register(dynamic(0), 1_usize, None));
        assert!(!lookup.register(dynamic(0), 1, None));
        assert!(lookup.register(dynamic(1), 2, None));
        assert_eq!(lookup.pending_len(), 2);
        let taken = lookup.take_pending(1);
        assert_eq!(taken, [(dynamic(0), 1)]);
        lookup.mark_resolved(dynamic(0));
        assert!(!lookup.register(dynamic(0), 1, None));
        assert_eq!(lookup.take_pending(10).len(), 1);
        assert_eq!(lookup.pending_len(), 0);
    }

    #[test]
    fn collection_repairs_survivors_and_returns_unresolved_dying_declarations() {
        let lookup = DeclarationLookup::default();
        for n in 0..6 {
            lookup.register(dynamic(n), usize::try_from(n).unwrap(), None);
        }
        lookup.mark_resolved(dynamic(1));
        let dying = lookup.retain(|reference| {
            if *reference % 2 == 0 {
                *reference += 100;
                true
            } else {
                false
            }
        });
        let mut dying: Vec<_> = dying.iter().map(|(_, r)| *r).collect();
        dying.sort_unstable();
        // 1 was resolved already, so only 3 and 5 need extraction.
        assert_eq!(dying, [3, 5]);
        let mut taken: Vec<_> = lookup
            .take_pending(10)
            .into_iter()
            .map(|(_, r)| r)
            .collect();
        taken.sort_unstable();
        assert_eq!(taken, [100, 102, 104]);
    }

    /// A burst beyond the queue is deferred, then queued as it drains: no
    /// registration is lost, and the queue never exceeds its bound.
    #[test]
    fn overflow_is_deferred_and_retried() {
        let lookup = DeclarationLookup::default();
        let total = MAX_PENDING_DECLARATIONS + 5;
        for n in 0..total {
            assert!(lookup.register(dynamic(i64::try_from(n).unwrap()), n, None));
        }
        assert_eq!(lookup.pending_len(), total);
        let mut resolved = 0;
        loop {
            let batch = lookup.take_pending(1_000);
            if batch.is_empty() {
                break;
            }
            assert!(lock(&lookup.pending).len() <= MAX_PENDING_DECLARATIONS);
            for (tag, _) in batch {
                lookup.mark_resolved(tag);
                resolved += 1;
            }
        }
        assert_eq!(resolved, total);
        assert!(!lookup.has_work());
    }

    /// Deferred declarations that die are extracted like queued ones.
    #[test]
    fn deferred_declarations_that_die_are_extracted() {
        let lookup = DeclarationLookup::default();
        let total = MAX_PENDING_DECLARATIONS + 3;
        for n in 0..total {
            lookup.register(dynamic(i64::try_from(n).unwrap()), n, None);
        }
        let dying = lookup.retain(|_| false);
        assert_eq!(dying.len(), total);
        // Their queued tags are skipped; nothing is left to resolve.
        assert!(lookup.take_pending(usize::MAX).is_empty());
        assert_eq!(lookup.pending_len(), 0);
    }

    #[test]
    fn settled_definitions_are_bounded_by_count() {
        let lookup = DeclarationLookup::<usize>::default();
        for n in 0..(MAX_SETTLED_DEFINITIONS + 2) {
            lookup.settle(TypeDefinition {
                tag: dynamic(i64::try_from(n).unwrap()),
                declaration: Some(declaration()),
            });
        }
        assert_eq!(lookup.lost(), 2);
        assert_eq!(lookup.take_settled().len(), MAX_SETTLED_DEFINITIONS);
        assert!(!lookup.has_work());
    }

    #[test]
    fn concurrent_first_registration_queues_once() {
        let lookup = DeclarationLookup::default();
        let barrier = std::sync::Barrier::new(8);
        std::thread::scope(|scope| {
            for _ in 0..8 {
                scope.spawn(|| {
                    barrier.wait();
                    lookup.register(dynamic(500), 7_usize, None);
                    lookup.register(tag(105), 5_usize, Some(10));
                });
            }
        });
        assert_eq!(lookup.pending_len(), 2);
        assert_eq!(lookup.take_pending(10).len(), 2);
    }
}
