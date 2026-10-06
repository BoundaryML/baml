//! Definitions of the classes and enums captured values name.
//!
//! A captured value names a declaration by its type tag: the engine-scoped,
//! never-reused identity a type head carries (static: assigned at link;
//! runtime: a process-wide counter). The tag is distinct from the name (two
//! runtime classes may share one) and from the content (two classes may hold
//! the same fields). The runtime registers each declaration the first time a
//! recorded capture names it; a resolver later copies its definition out of
//! the heap, off the VM thread, and the recorder publishes it once per
//! recording. The lookup here is weak: it never keeps a declaration alive, and
//! the runtime's collector repairs or prunes it, extracting the definition of
//! a declaration that dies before it was resolved.

use std::{
    collections::VecDeque,
    sync::{
        Mutex, RwLock,
        atomic::{AtomicUsize, Ordering},
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

/// Pending declarations beyond this are settled as unavailable at
/// registration, so a program minting classes faster than they resolve
/// cannot grow telemetry memory without bound.
pub const MAX_PENDING_DECLARATIONS: usize = 16_384;
/// Definitions extracted at collection and not yet taken by the recorder.
/// Beyond this, a dying declaration is settled as unavailable.
pub const MAX_SETTLED_DEFINITIONS: usize = 4_096;

struct Entry<R> {
    reference: R,
    resolved: bool,
}

type Shard<R> = Padded<RwLock<FxHashMap<TypeTag, Entry<R>>>>;

/// Weak map from a registered declaration's tag to its current reference,
/// with the queue of declarations still to resolve. Engine-owned, like
/// [`crate::FunctionLookup`].
pub struct DeclarationLookup<R> {
    shards: [Shard<R>; SHARD_COUNT],
    pending: Mutex<VecDeque<TypeTag>>,
    pending_len: AtomicUsize,
    /// Owned definitions waiting for the recorder: extracted before their
    /// declaration was reclaimed, or settled as unavailable.
    settled: Mutex<Vec<TypeDefinition>>,
}

impl<R> Default for DeclarationLookup<R> {
    fn default() -> Self {
        Self {
            shards: std::array::from_fn(|_| Padded(RwLock::new(FxHashMap::default()))),
            pending: Mutex::new(VecDeque::new()),
            pending_len: AtomicUsize::new(0),
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
    /// declaration `tag` identifies. True when this call registered it.
    pub fn register(&self, tag: TypeTag, reference: R) -> bool {
        if self
            .shard(tag)
            .read()
            .expect("declaration lookup poisoned")
            .contains_key(&tag)
        {
            return false;
        }
        let mut shard = self
            .shard(tag)
            .write()
            .expect("declaration lookup poisoned");
        if shard.contains_key(&tag) {
            return false;
        }
        let overflow = self.pending_len.load(Ordering::Acquire) >= MAX_PENDING_DECLARATIONS;
        shard.insert(
            tag,
            Entry {
                reference,
                resolved: overflow,
            },
        );
        drop(shard);
        if overflow {
            self.settle(TypeDefinition {
                tag,
                declaration: None,
            });
        } else {
            self.pending
                .lock()
                .expect("pending declarations")
                .push_back(tag);
            self.pending_len.fetch_add(1, Ordering::AcqRel);
        }
        true
    }

    /// Declarations registered and not yet resolved or extracted.
    pub fn pending_len(&self) -> usize {
        self.pending_len.load(Ordering::Acquire)
    }

    /// Up to `max` pending declarations with their current references. The
    /// caller holds heap access and resolves each, then `mark_resolved`s it.
    /// A declaration collected meanwhile was extracted by the collector and
    /// is skipped.
    pub fn take_pending(&self, max: usize) -> Vec<(TypeTag, R)> {
        let mut out = Vec::new();
        let mut pending = self.pending.lock().expect("pending declarations");
        while out.len() < max {
            let Some(tag) = pending.pop_front() else {
                break;
            };
            self.pending_len.fetch_sub(1, Ordering::AcqRel);
            let shard = self.shard(tag).read().expect("declaration lookup poisoned");
            if let Some(entry) = shard.get(&tag)
                && !entry.resolved
            {
                out.push((tag, entry.reference));
            }
        }
        out
    }

    pub fn mark_resolved(&self, tag: TypeTag) {
        if let Some(entry) = self
            .shard(tag)
            .write()
            .expect("declaration lookup poisoned")
            .get_mut(&tag)
        {
            entry.resolved = true;
        }
    }

    /// Whether `tag` is registered (resolved or not).
    pub fn contains(&self, tag: TypeTag) -> bool {
        self.shard(tag)
            .read()
            .expect("declaration lookup poisoned")
            .contains_key(&tag)
    }

    /// Hand a definition to the recorder without heap access. Never waits on
    /// telemetry: beyond [`MAX_SETTLED_DEFINITIONS`], a definition is settled
    /// as unavailable instead of kept.
    pub fn settle(&self, mut definition: TypeDefinition) {
        let mut settled = self.settled.lock().expect("settled definitions");
        if settled.len() >= MAX_SETTLED_DEFINITIONS {
            definition.declaration = None;
        }
        settled.push(definition);
    }

    pub fn take_settled(&self) -> Vec<TypeDefinition> {
        std::mem::take(&mut *self.settled.lock().expect("settled definitions"))
    }

    pub fn has_settled(&self) -> bool {
        !self.settled.lock().expect("settled definitions").is_empty()
    }

    /// At the collector's safepoint, with mutators and resolvers excluded:
    /// `keep` repairs a surviving reference in place and answers whether it
    /// survives. Returns the dying declarations that were never resolved, so
    /// the collector can extract their definitions before reclaiming them.
    pub fn retain(&self, mut keep: impl FnMut(&mut R) -> bool) -> Vec<(TypeTag, R)> {
        let mut dying = Vec::new();
        for shard in &self.shards {
            let mut shard = shard.0.write().expect("declaration lookup poisoned");
            shard.retain(|tag, entry| {
                if keep(&mut entry.reference) {
                    return true;
                }
                if !entry.resolved {
                    dying.push((*tag, entry.reference));
                }
                false
            });
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

    #[test]
    fn registration_is_once_and_pending_until_resolved() {
        let lookup = DeclarationLookup::default();
        assert!(lookup.register(tag(200), 1_usize));
        assert!(!lookup.register(tag(200), 1));
        assert!(lookup.register(tag(201), 2));
        assert_eq!(lookup.pending_len(), 2);
        let taken = lookup.take_pending(1);
        assert_eq!(
            taken
                .iter()
                .map(|(t, r)| (t.as_i64(), *r))
                .collect::<Vec<_>>(),
            [(200, 1)]
        );
        lookup.mark_resolved(tag(200));
        assert_eq!(lookup.pending_len(), 1);
        // A resolved declaration stays registered: no second resolution.
        assert!(!lookup.register(tag(200), 1));
        assert_eq!(lookup.take_pending(10).len(), 1);
        assert_eq!(lookup.pending_len(), 0);
    }

    #[test]
    fn collection_repairs_survivors_and_returns_unresolved_dying_declarations() {
        let lookup = DeclarationLookup::default();
        for n in 0..6 {
            lookup.register(tag(300 + n), usize::try_from(n).unwrap());
        }
        lookup.mark_resolved(tag(301));
        // Even references survive and move; odd ones die.
        let dying = lookup.retain(|reference| {
            if *reference % 2 == 0 {
                *reference += 100;
                true
            } else {
                false
            }
        });
        let mut dying: Vec<_> = dying.iter().map(|(t, _)| t.as_i64()).collect();
        dying.sort_unstable();
        // 301 was resolved already, so only 303 and 305 need extraction.
        assert_eq!(dying, [303, 305]);
        assert!(!lookup.contains(tag(301)));
        // Pending survivors resolve through their repaired references; the
        // dead ones are skipped.
        let mut taken: Vec<_> = lookup
            .take_pending(10)
            .into_iter()
            .map(|(t, r)| (t.as_i64(), r))
            .collect();
        taken.sort_unstable();
        assert_eq!(taken, [(300, 100), (302, 102), (304, 104)]);
    }

    #[test]
    fn pending_and_settled_work_is_bounded() {
        let lookup = DeclarationLookup::default();
        for n in 0..(i64::try_from(MAX_PENDING_DECLARATIONS).unwrap() + 3) {
            lookup.register(tag(1_000 + n), 0_usize);
        }
        assert_eq!(lookup.pending_len(), MAX_PENDING_DECLARATIONS);
        // The overflow is settled as unavailable, never queued.
        let settled = lookup.take_settled();
        assert_eq!(settled.len(), 3);
        assert!(settled.iter().all(|d| d.declaration.is_none()));

        for n in 0..(MAX_SETTLED_DEFINITIONS + 2) {
            lookup.settle(TypeDefinition {
                tag: tag(i64::try_from(n).unwrap()),
                declaration: Some(TypeDeclaration {
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
                }),
            });
        }
        let settled = lookup.take_settled();
        assert_eq!(settled.len(), MAX_SETTLED_DEFINITIONS + 2);
        assert_eq!(
            settled.iter().filter(|d| d.declaration.is_none()).count(),
            2
        );
    }

    #[test]
    fn concurrent_first_registration_queues_once() {
        let lookup = DeclarationLookup::default();
        let barrier = std::sync::Barrier::new(8);
        std::thread::scope(|scope| {
            for _ in 0..8 {
                scope.spawn(|| {
                    barrier.wait();
                    lookup.register(tag(500), 7_usize);
                });
            }
        });
        assert_eq!(lookup.pending_len(), 1);
        assert_eq!(lookup.take_pending(10).len(), 1);
    }
}
