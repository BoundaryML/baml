//! Independently owned capture graphs. No VM pointers, user-code execution or filesystem dependencies.
//!
//! Values are inline. Only identity-bearing objects require graph IDs. Containers
//! are sampled independently; this is not an atomic snapshot of the entire graph.
use std::{
    marker::PhantomData,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};

use baml_type::{DeclarationName, MediaKind, typetag::TypeTag};
/// The string type the builder takes, so callers need not depend on `bex_str`.
pub use bex_str::BexStr;
use hash::Absorb as _;
use num_bigint::BigInt;

pub mod context;
mod decode;
pub use decode::{
    BlobError, ChildIndex, DecodeLimits, DecodedMedia, DecodedMediaSource, DecodedName,
    DecodedObject, DecodedRoot, DecodedSnapshot, DecodedValue, Entries, MediaPayload, NodeId,
    SHALLOW_TYPE_BYTES, SharedSnapshot, TypeDescription, decode_blob,
};
mod encoding;
pub use encoding::{BLOB_MAGIC, BLOB_VERSION, BlobScratch};
mod hash;
pub use hash::CasId;
mod memory;
mod shape;
pub use shape::{REFERENCE_WEIGHT, ShapePolicy, Shaper};
mod tags;
mod walk;

macro_rules! index {
    ($name:ident) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        #[repr(transparent)]
        pub struct $name(pub u32);
    };
}
index!(ObjectId);
index!(StringId);
index!(BigintId);
index!(TypeId);

/// An index range in one typed arena. Relocation cannot invalidate it.
#[repr(C)]
pub struct Range<T> {
    start: u32,
    len: u32,
    kind: PhantomData<fn() -> T>,
}
impl<T> Copy for Range<T> {}
impl<T> Clone for Range<T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T> std::fmt::Debug for Range<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Range")
            .field("start", &self.start)
            .field("len", &self.len)
            .finish_non_exhaustive()
    }
}
impl<T> Range<T> {
    pub fn empty() -> Self {
        Self {
            start: 0,
            len: 0,
            kind: PhantomData,
        }
    }
    pub fn len(self) -> usize {
        self.len as usize
    }
    pub fn is_empty(self) -> bool {
        self.len == 0
    }
    fn new(start: usize, len: usize) -> Self {
        Self {
            start: u32::try_from(start).expect("arena exhausted"),
            len: u32::try_from(len).expect("arena exhausted"),
            kind: PhantomData,
        }
    }
    fn indexes(self) -> std::ops::Range<usize> {
        self.start as usize..self.start as usize + self.len as usize
    }
}
/// Copied bytes and the content digest computed while copying them.
#[derive(Clone, Copy, Debug)]
pub struct Uint8ArrayData {
    range: Range<u8>,
    digest: hash::Digest,
}
impl Uint8ArrayData {
    pub fn len(self) -> usize {
        self.range.len()
    }
    pub fn is_empty(self) -> bool {
        self.range.is_empty()
    }
}
/// Owned nominal identity, including its tag; names alone are not identity.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TypeIdentity {
    Resolved(baml_type::TaggedTypeName),
    Unresolved(TypeTag),
}
pub type OwnedType = baml_type::RealizedTy<TypeIdentity>;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Limit {
    Values,
    Objects,
    Bytes,
    Depth,
}

/// Copyable values carry only snapshot-local indexes, never owning Rust handles
/// or VM pointers. String/bigint indexes are storage references, not object IDs.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SnapshotValue {
    Null,
    OmittedArg,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(StringId),
    Bigint(BigintId),
    Object(ObjectId),
    Type(TypeId),
    Enum {
        declaration: ObjectId,
        variant: u32,
        name: StringId,
    },
    Truncated(Limit),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Description {
    Function,
    Closure,
    BoundMethod,
    GenericFunction,
    HostFunction,
    Future,
    UnscheduledFuture,
    Package,
    Interface,
    Implementation,
    TypeAlias,
    Sentinel,
}
#[derive(Debug)]
pub struct MapEntry {
    pub key: BexStr,
    pub value: SnapshotValue,
}
#[derive(Debug)]
pub enum SnapshotObject {
    Uint8Array {
        data: Uint8ArrayData,
        original_len: usize,
    },
    List {
        element_type: TypeId,
        items: Range<SnapshotValue>,
        original_len: usize,
    },
    Map {
        key_type: TypeId,
        value_type: TypeId,
        entries: Range<MapEntry>,
        original_len: usize,
    },
    Instance {
        type_arguments: Range<OwnedType>,
        declaration: ObjectId,
        fields: Range<MapEntry>,
        original_len: usize,
    },
    Declaration {
        name: DeclarationName,
        tag: TypeTag,
        is_enum: bool,
    },
    Cell(SnapshotValue),
    /// Identity within this snapshot, with neither content nor a source handle.
    NonSnapshotableValue {},
    Descriptive {
        kind: Description,
        name: Option<StringId>,
    },
    /// A media value: its kind and where its content comes from. Its bytes
    /// are captured, as base64 text, only when the value holds them.
    Media {
        kind: MediaKind,
        mime_type: Option<StringId>,
        source: MediaSource,
    },
    Truncated(Limit),
}
/// Where a media value's content comes from, as the runtime holds it. `data`
/// is the base64 text of content already loaded from the URL or file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MediaSource {
    Url {
        url: StringId,
        data: Option<StringId>,
    },
    File {
        path: StringId,
        data: Option<StringId>,
    },
    Base64 {
        data: StringId,
    },
}
const _: () = assert!(std::mem::size_of::<SnapshotValue>() == 16);
#[cfg(target_pointer_width = "64")]
const _: () = assert!(std::mem::size_of::<MapEntry>() == 72);

/// Optional capture policies, separate from pooling. Byte limits count logical
/// leaf content; value limits apply to each value/entry arena. None has no policy
/// limit, subject to the checked u32 index range. Object traversal is iterative.
#[derive(Clone, Copy, Debug, Default)]
pub struct Limits {
    pub max_values: Option<usize>,
    pub max_objects: Option<usize>,
    pub max_bytes: Option<usize>,
    pub max_depth: Option<usize>,
}
#[derive(Clone, Copy, Debug)]
pub struct PoolConfig {
    pub initial_values: usize,
    pub initial_objects: usize,
    pub initial_entries: usize,
    pub initial_bytes: usize,
    pub retention_budget_bytes: usize,
    pub max_retained_allocation_bytes: usize,
}
impl Default for PoolConfig {
    fn default() -> Self {
        use btel_settings::snapshot as settings;
        Self {
            initial_values: settings::INITIAL_VALUES,
            initial_objects: settings::INITIAL_OBJECTS,
            initial_entries: settings::INITIAL_ENTRIES,
            initial_bytes: settings::INITIAL_BYTES,
            retention_budget_bytes: settings::RETENTION_BUDGET_BYTES,
            max_retained_allocation_bytes: settings::MAX_RETAINED_ALLOCATION_BYTES,
        }
    }
}
/// Callee parameter-slot order, with omission distinct from null and truncation.
#[derive(Clone, Copy, Debug)]
pub struct FunctionArgs {
    pub parameter_count: usize,
    slots: Range<SnapshotValue>,
}
#[derive(Clone, Copy, Debug)]
pub enum SnapshotRoot {
    Value(SnapshotValue),
    FunctionArgs(FunctionArgs),
}
#[derive(Clone, Copy, Debug, Default)]
pub struct CaptureStats {
    /// Logical leaf bytes copied into the snapshot arenas. This excludes string
    /// internals: first hashing a rope can also materialize its shared backing.
    pub copied_bytes: usize,
    /// Logical leaf bytes retained by owning handles, not physical allocation size.
    pub shared_bytes: usize,
    pub limited: bool,
}
/// Structural capacity across free and checked-out owners. Shared leaf backing
/// is excluded. Peak includes conservative old + replacement overlap on growth.
#[derive(Clone, Copy, Debug, Default)]
pub struct PoolStats {
    pub allocated: usize,
    pub in_use: usize,
    pub allocation_misses: usize,
    pub allocated_bytes: usize,
    pub idle_bytes: usize,
    pub peak_accounted_bytes: usize,
    pub growths: usize,
}
#[expect(
    clippy::vec_box,
    reason = "recycle the pointer-sized owner descriptor itself"
)]
struct State {
    free: Vec<Box<Storage>>,
    allocated: usize,
    in_use: usize,
    misses: usize,
    idle_bytes: usize,
}
struct Pool {
    state: Mutex<State>,
    max: usize,
    limits: Limits,
    config: PoolConfig,
    bytes: AtomicUsize,
    peak: AtomicUsize,
    growths: AtomicUsize,
}
/// Runtime-owned pool; outstanding snapshots keep the pool alive after teardown.
#[derive(Clone)]
pub struct SnapshotPool(Arc<Pool>);
struct Storage {
    owner: Option<Arc<Pool>>,
    values: Vec<SnapshotValue>,
    objects: Vec<SnapshotObject>,
    entries: Vec<MapEntry>,
    bytes: Vec<u8>,
    strings: Vec<BexStr>,
    bigints: Vec<Arc<BigInt>>,
    types: Vec<OwnedType>,
    stats: CaptureStats,
    charged: usize,
    content: usize,
    /// Encoded bytes beyond the leaf content in `content`: every value's and
    /// object's tags, numbers and lengths; map keys and field names;
    /// declaration names; type descriptions at each use; and each use of a
    /// string or bigint after its first. Together with the header they bound
    /// the capture's size as one blob.
    value_bytes: usize,
    object_bytes: usize,
    /// Reserved objects not set yet; each writes as a truncation marker.
    unset_objects: usize,
    key_bytes: usize,
    declaration_bytes: usize,
    type_bytes: usize,
    reuse_bytes: usize,
    /// The largest leaf as the leaf size measures it: a string's or byte
    /// array's length, a bigint's limb bytes.
    largest_leaf: usize,
    /// Per object, string and bigint: whether a value has used it yet.
    referenced: Vec<bool>,
    string_used: Vec<bool>,
    bigint_used: Vec<bool>,
    /// Whether a value or cell references some object a second time: the
    /// capture shares or cycles through it. References to a declaration
    /// through the values of its type never count; every blob carries the
    /// declarations it names.
    shared: bool,
    string_hashes: Vec<hash::Digest>,
    bigint_hashes: Vec<hash::Digest>,
    type_leaves: Vec<hash::TypeLeaf>,
    /// Shaped blobs, children before parents; the last is the capture root.
    /// Empty until the capture is finished.
    blobs: Vec<BlobEntry>,
    /// Each blob's objects in blob-local order, concatenated.
    members: Vec<ObjectId>,
    /// Each blob's children in first-use order, concatenated.
    blob_children: Vec<BlobIndex>,
    /// Per object, string and bigint: the blob other blobs find it in. Empty
    /// when the capture is one blob.
    object_homes: Vec<Option<Home>>,
    string_homes: Vec<Option<BlobIndex>>,
    bigint_homes: Vec<Option<BlobIndex>>,
}
/// The header, the object and child counts, the root tag and an argument
/// root's parameter and slot counts.
const ROOT_BOUND: u64 = 8 + 4 + 16 + 4 + 4 + 1 + 8 + 4;
/// A reserved object holds `Truncated(Limit::Objects)` until it is set, and
/// writes as that if it never is.
const RESERVED_OBJECT_BYTES: u64 = 2;

impl Storage {
    /// An upper bound on the capture's encoded size as one blob, kept as the
    /// capture is built. Shaping checks it against what it writes.
    fn encoded_bound(&self) -> u64 {
        (self.content as u64)
            .saturating_add(self.value_bytes as u64)
            .saturating_add(self.object_bytes as u64)
            .saturating_add((self.unset_objects as u64).saturating_mul(RESERVED_OBJECT_BYTES))
            .saturating_add(self.key_bytes as u64)
            .saturating_add((self.entries.len() as u64).saturating_mul(4))
            .saturating_add(self.declaration_bytes as u64)
            .saturating_add(self.type_bytes as u64)
            .saturating_add(self.reuse_bytes as u64)
            .saturating_add(ROOT_BOUND)
    }
    /// A value or cell references `id`.
    fn reference(&mut self, id: ObjectId) {
        let referenced = &mut self.referenced[id.0 as usize];
        if *referenced {
            self.shared = true;
        } else {
            *referenced = true;
        }
    }
    /// A value is written: note what it references and what its encoding
    /// adds beyond the leaf content already counted (its tag, numbers and
    /// lengths, as `encoding` writes them).
    fn use_value(&mut self, value: SnapshotValue) {
        self.value_bytes += match value {
            SnapshotValue::Null | SnapshotValue::OmittedArg => 1,
            SnapshotValue::Bool(_) | SnapshotValue::Truncated(_) => 2,
            SnapshotValue::Int(_) | SnapshotValue::Float(_) => 9,
            SnapshotValue::Object(id) => {
                self.reference(id);
                5
            }
            SnapshotValue::String(id) => {
                self.use_string(id);
                5
            }
            SnapshotValue::Bigint(id) => {
                // Sign and bit length, then whole limbs: the content counted
                // the bytes, and a first use pads them to a limb.
                let limbs = walk::bigint_limb_bytes(&self.bigints[id.0 as usize]);
                let used = &mut self.bigint_used[id.0 as usize];
                if *used {
                    self.reuse_bytes += limbs;
                } else {
                    *used = true;
                    self.reuse_bytes += limbs.saturating_sub(
                        usize::try_from(self.bigints[id.0 as usize].bits().div_ceil(8))
                            .unwrap_or(usize::MAX),
                    );
                }
                1 + 1 + 8
            }
            SnapshotValue::Type(id) => {
                self.use_type(id);
                1
            }
            SnapshotValue::Enum { name, .. } => {
                self.use_string(name);
                1 + 4 + 4 + 4
            }
        };
    }
    /// A string is written in place; its content counts once at capture and
    /// again at every later use.
    fn use_string(&mut self, id: StringId) {
        let used = &mut self.string_used[id.0 as usize];
        if *used {
            self.reuse_bytes += self.strings[id.0 as usize].len();
        } else {
            *used = true;
        }
    }
    fn use_type(&mut self, id: TypeId) {
        self.type_bytes += self.type_leaves[id.0 as usize].encoded_len as usize;
    }
    fn object_home(&self, id: ObjectId) -> Option<Home> {
        self.object_homes.get(id.0 as usize).copied().flatten()
    }
    fn string_home(&self, id: StringId) -> Option<BlobIndex> {
        self.string_homes.get(id.0 as usize).copied().flatten()
    }
    fn bigint_home(&self, id: BigintId) -> Option<BlobIndex> {
        self.bigint_homes.get(id.0 as usize).copied().flatten()
    }
}
/// Position of a blob in its snapshot's blob table. Meaningless for any other
/// snapshot.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BlobIndex(u32);
#[derive(Clone, Copy, Debug)]
struct BlobEntry {
    id: CasId,
    root: SnapshotRoot,
    members: Range<ObjectId>,
    children: Range<BlobIndex>,
    encoded_len: u64,
}
/// Where other blobs find an object stored in a blob of its own.
#[derive(Clone, Copy, Debug)]
struct Home {
    blob: BlobIndex,
    /// The object's local number there; zero is that blob's root.
    node: u32,
}
/// Exclusive pointer-sized owner, frozen before publication.
/// ```compile_fail
/// fn cloneable<T: Clone>() {}
/// cloneable::<btel_snapshot::Snapshot>();
/// ```
pub struct Snapshot(std::mem::ManuallyDrop<Box<Storage>>);
impl std::fmt::Debug for Snapshot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Snapshot")
            .field("root", &self.root())
            .field("values", &self.0.values)
            .field("objects", &self.0.objects)
            .field("entries", &self.0.entries)
            .field("bytes", &self.0.bytes)
            .field("strings", &self.0.strings)
            .field("bigints", &self.0.bigints)
            .field("types", &self.0.types)
            .finish()
    }
}
/// One content-addressed blob of a shaped snapshot.
#[derive(Clone, Copy)]
pub struct Blob<'s> {
    snapshot: &'s Snapshot,
    index: BlobIndex,
}
impl std::fmt::Debug for Blob<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Blob").field("id", &self.id()).finish()
    }
}
impl<'s> Blob<'s> {
    pub fn id(&self) -> CasId {
        self.entry().id
    }
    pub fn index(&self) -> BlobIndex {
        self.index
    }
    /// Exact length of what [`Self::write`] writes.
    pub fn encoded_len(&self) -> u64 {
        self.entry().encoded_len
    }
    /// The blobs this one names, in the order of its child table.
    pub fn children(&self) -> impl ExactSizeIterator<Item = Blob<'s>> + use<'s> {
        let snapshot = self.snapshot;
        snapshot.0.blob_children[self.entry().children.indexes()]
            .iter()
            .map(move |index| Blob {
                snapshot,
                index: *index,
            })
    }
    fn entry(&self) -> &'s BlobEntry {
        &self.snapshot.0.blobs[self.index.0 as usize]
    }
}
impl Snapshot {
    /// The blob a recording references for this capture.
    pub fn root_blob(&self) -> Blob<'_> {
        let last = self
            .0
            .blobs
            .len()
            .checked_sub(1)
            .unwrap_or_else(|| unreachable!("a finished snapshot has a root blob"));
        Blob {
            snapshot: self,
            index: BlobIndex(u32::try_from(last).expect("bounded blob count")),
        }
    }
    pub fn root_id(&self) -> CasId {
        self.root_blob().id()
    }
    /// The blob at `index`, which must come from this snapshot.
    pub fn blob(&self, index: BlobIndex) -> Blob<'_> {
        assert!(
            (index.0 as usize) < self.0.blobs.len(),
            "blob index from another snapshot"
        );
        Blob {
            snapshot: self,
            index,
        }
    }
    /// Every blob, each after the blobs it references; the root is last.
    pub fn blobs(&self) -> impl ExactSizeIterator<Item = Blob<'_>> + '_ {
        (0..self.0.blobs.len()).map(|index| Blob {
            snapshot: self,
            index: BlobIndex(u32::try_from(index).expect("bounded blob count")),
        })
    }
    pub fn root(&self) -> SnapshotRoot {
        self.root_blob().entry().root
    }
    pub fn value(&self) -> Option<SnapshotValue> {
        match self.root() {
            SnapshotRoot::Value(value) => Some(value),
            SnapshotRoot::FunctionArgs(_) => None,
        }
    }
    pub fn arguments(&self) -> Option<&[SnapshotValue]> {
        match self.root() {
            SnapshotRoot::FunctionArgs(args) => Some(self.values(args.slots)),
            SnapshotRoot::Value(_) => None,
        }
    }
    pub fn roots(&self) -> &[SnapshotValue] {
        match &self.root_blob().entry().root {
            SnapshotRoot::Value(value) => std::slice::from_ref(value),
            SnapshotRoot::FunctionArgs(args) => self.values(args.slots),
        }
    }
    pub fn object(&self, id: ObjectId) -> &SnapshotObject {
        &self.0.objects[id.0 as usize]
    }
    pub fn values(&self, range: Range<SnapshotValue>) -> &[SnapshotValue] {
        &self.0.values[range.start as usize..][..range.len as usize]
    }
    pub fn entries(&self, range: Range<MapEntry>) -> &[MapEntry] {
        &self.0.entries[range.start as usize..][..range.len as usize]
    }
    pub fn string(&self, id: StringId) -> &BexStr {
        &self.0.strings[id.0 as usize]
    }
    pub fn bigint(&self, id: BigintId) -> &Arc<BigInt> {
        &self.0.bigints[id.0 as usize]
    }
    pub fn bytes(&self, data: Uint8ArrayData) -> &[u8] {
        &self.0.bytes[data.range.indexes()]
    }
    pub fn ty(&self, id: TypeId) -> &OwnedType {
        &self.0.types[id.0 as usize]
    }
    pub fn type_arguments(&self, range: Range<OwnedType>) -> &[OwnedType] {
        &self.0.types[range.start as usize..][..range.len as usize]
    }
    pub fn stats(&self) -> CaptureStats {
        self.0.stats
    }
    pub fn object_count(&self) -> usize {
        self.0.objects.len()
    }
    /// Initialized arena bytes only; excludes spare capacity and external backing.
    pub fn live_arena_bytes(&self) -> usize {
        std::mem::size_of_val(self.0.values.as_slice())
            + std::mem::size_of_val(self.0.objects.as_slice())
            + std::mem::size_of_val(self.0.entries.as_slice())
            + self.0.bytes.len()
            + std::mem::size_of_val(self.0.strings.as_slice())
            + std::mem::size_of_val(self.0.bigints.as_slice())
            + std::mem::size_of_val(self.0.types.as_slice())
            + std::mem::size_of_val(self.0.string_hashes.as_slice())
            + std::mem::size_of_val(self.0.bigint_hashes.as_slice())
            + std::mem::size_of_val(self.0.type_leaves.as_slice())
            + std::mem::size_of_val(self.0.referenced.as_slice())
            + std::mem::size_of_val(self.0.string_used.as_slice())
            + std::mem::size_of_val(self.0.bigint_used.as_slice())
            + std::mem::size_of_val(self.0.blobs.as_slice())
            + std::mem::size_of_val(self.0.members.as_slice())
            + std::mem::size_of_val(self.0.blob_children.as_slice())
            + std::mem::size_of_val(self.0.object_homes.as_slice())
            + std::mem::size_of_val(self.0.string_homes.as_slice())
            + std::mem::size_of_val(self.0.bigint_homes.as_slice())
    }
}
const _: () = assert!(std::mem::size_of::<Snapshot>() == std::mem::size_of::<usize>());
const _: () = assert!(std::mem::size_of::<Option<Snapshot>>() == std::mem::size_of::<usize>());
impl SnapshotPool {
    pub fn new(max: usize, limits: Limits) -> Self {
        Self::with_config(max, limits, PoolConfig::default())
    }
    pub fn with_config(max: usize, limits: Limits, config: PoolConfig) -> Self {
        assert!(max > 0);
        assert!(
            [limits.max_values, limits.max_objects, limits.max_bytes]
                .into_iter()
                .flatten()
                .all(|n| n < u32::MAX as usize)
        );
        Self(Arc::new(Pool {
            state: Mutex::new(State {
                free: Vec::new(),
                allocated: 0,
                in_use: 0,
                misses: 0,
                idle_bytes: 0,
            }),
            max,
            limits,
            config,
            bytes: AtomicUsize::new(0),
            peak: AtomicUsize::new(0),
            growths: AtomicUsize::new(0),
        }))
    }
    /// Admission can wait for owners held elsewhere, but growth never waits for
    /// the idle budget. Publish private records before retrying an exhausted pool.
    pub fn try_acquire(&self) -> Option<Builder> {
        let mut state = self.0.state.try_lock().ok()?;
        let mut storage = if let Some(storage) = state.free.pop() {
            state.idle_bytes -= storage.charged;
            storage
        } else {
            if state.allocated == self.0.max {
                return None;
            }
            let storage = Box::new(Storage {
                owner: None,
                objects: Vec::new(),
                entries: Vec::new(),
                bytes: Vec::new(),
                values: Vec::new(),
                strings: Vec::new(),
                bigints: Vec::new(),
                types: Vec::new(),
                stats: CaptureStats::default(),
                charged: std::mem::size_of::<Storage>(),
                content: 0,
                value_bytes: 0,
                object_bytes: 0,
                unset_objects: 0,
                key_bytes: 0,
                declaration_bytes: 0,
                type_bytes: 0,
                reuse_bytes: 0,
                largest_leaf: 0,
                referenced: Vec::new(),
                string_used: Vec::new(),
                bigint_used: Vec::new(),
                shared: false,
                string_hashes: Vec::new(),
                bigint_hashes: Vec::new(),
                type_leaves: Vec::new(),
                blobs: Vec::new(),
                members: Vec::new(),
                blob_children: Vec::new(),
                object_homes: Vec::new(),
                string_homes: Vec::new(),
                bigint_homes: Vec::new(),
            });
            self.0.add_bytes(storage.charged);
            state.allocated += 1;
            state.misses += 1;
            storage
        };
        state.in_use += 1;
        drop(state);
        storage.owner = Some(Arc::clone(&self.0));
        // Builder owns cleanup even if growth fails partway through initialization.
        let mut builder = Builder(Some(storage));
        let s = builder.storage();
        let pool = s.owner.as_ref().unwrap();
        grow(
            &mut s.values,
            pool.config.initial_values,
            pool,
            &mut s.charged,
        );
        grow(
            &mut s.objects,
            pool.config.initial_objects,
            pool,
            &mut s.charged,
        );
        grow(
            &mut s.entries,
            pool.config.initial_entries,
            pool,
            &mut s.charged,
        );
        grow(
            &mut s.bytes,
            pool.config.initial_bytes,
            pool,
            &mut s.charged,
        );
        Some(builder)
    }
    pub fn stats(&self) -> PoolStats {
        let s = self
            .0
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        PoolStats {
            allocated: s.allocated,
            in_use: s.in_use,
            allocation_misses: s.misses,
            idle_bytes: s.idle_bytes,
            allocated_bytes: self.0.bytes.load(Ordering::Relaxed),
            peak_accounted_bytes: self.0.peak.load(Ordering::Relaxed),
            growths: self.0.growths.load(Ordering::Relaxed),
        }
    }
}
impl Pool {
    fn add_bytes(&self, bytes: usize) {
        let total = self.bytes.fetch_add(bytes, Ordering::Relaxed) + bytes;
        self.peak.fetch_max(total, Ordering::Relaxed);
    }
}
/// Account both old and replacement capacity while Vec owns the move. Vec never
/// drops the old owners after moving them. There is no capacity wait here.
fn grow<T>(values: &mut Vec<T>, additional: usize, pool: &Pool, charged: &mut usize) {
    struct Pending<'a> {
        pool: &'a Pool,
        bytes: usize,
    }
    impl Drop for Pending<'_> {
        fn drop(&mut self) {
            self.pool.bytes.fetch_sub(self.bytes, Ordering::Relaxed);
        }
    }
    let required = values
        .len()
        .checked_add(additional)
        .expect("snapshot capacity overflow");
    if required <= values.capacity() {
        return;
    }
    let capacity = required.max(values.capacity().saturating_mul(2));
    let replacement = capacity
        .checked_mul(std::mem::size_of::<T>())
        .expect("snapshot byte capacity overflow");
    let old = values.capacity() * std::mem::size_of::<T>();
    pool.add_bytes(replacement);
    let mut pending = Pending {
        pool,
        bytes: replacement,
    };
    values
        .try_reserve_exact(capacity - values.len())
        .expect("snapshot allocation failed");
    let actual = values.capacity() * std::mem::size_of::<T>();
    // reserve_exact may receive more space than requested from an allocator.
    if actual > replacement {
        pool.add_bytes(actual - replacement);
    }
    pool.bytes.fetch_sub(old, Ordering::Relaxed);
    *charged = *charged - old + actual;
    pending.bytes = 0;
    pool.growths.fetch_add(1, Ordering::Relaxed);
}
/// Structural arenas grow using Vec's ownership-preserving relocation.
pub struct Builder(Option<Box<Storage>>);
impl Builder {
    fn storage(&mut self) -> &mut Storage {
        self.0.as_mut().unwrap()
    }
    pub fn limits(&self) -> Limits {
        self.0.as_ref().unwrap().owner.as_ref().unwrap().limits
    }
    pub fn remaining_values(&self) -> usize {
        self.limits()
            .max_values
            .unwrap_or(u32::MAX as usize - 1)
            .saturating_sub(self.0.as_ref().unwrap().values.len())
    }
    pub fn remaining_entries(&self) -> usize {
        self.limits()
            .max_values
            .unwrap_or(u32::MAX as usize - 1)
            .saturating_sub(self.0.as_ref().unwrap().entries.len())
    }
    pub fn remaining_bytes(&self) -> usize {
        self.limits()
            .max_bytes
            .unwrap_or(u32::MAX as usize - 1)
            .saturating_sub(self.0.as_ref().unwrap().content)
    }
    pub fn limited(&mut self, reason: Limit) -> SnapshotValue {
        self.storage().stats.limited = true;
        SnapshotValue::Truncated(reason)
    }
    pub fn reserve_object(&mut self) -> Option<ObjectId> {
        if self.0.as_ref().unwrap().objects.len()
            >= self.limits().max_objects.unwrap_or(u32::MAX as usize - 1)
        {
            self.limited(Limit::Objects);
            return None;
        }
        let s = self.storage();
        let id = ObjectId(u32::try_from(s.objects.len()).expect("bounded objects"));
        grow(&mut s.objects, 1, s.owner.as_ref().unwrap(), &mut s.charged);
        s.objects.push(SnapshotObject::Truncated(Limit::Objects));
        s.unset_objects += 1;
        grow(
            &mut s.referenced,
            1,
            s.owner.as_ref().unwrap(),
            &mut s.charged,
        );
        s.referenced.push(false);
        Some(id)
    }
    /// The object's encoding beyond its content, keys, types and values is
    /// added to the size bound as `encoding` writes it.
    pub fn set_object(&mut self, id: ObjectId, object: SnapshotObject) {
        let s = self.storage();
        if matches!(
            s.objects[id.0 as usize],
            SnapshotObject::Truncated(Limit::Objects)
        ) {
            s.unset_objects -= 1;
        }
        s.object_bytes += match &object {
            SnapshotObject::Cell(value) => {
                s.use_value(*value);
                1
            }
            SnapshotObject::Declaration { name, tag, .. } => {
                let mut counter = hash::Counter(0);
                borsh::BorshSerialize::serialize(tag, &mut counter).expect("counting cannot fail");
                borsh::BorshSerialize::serialize(name, &mut counter).expect("counting cannot fail");
                s.declaration_bytes += counter.0;
                1 + 1
            }
            SnapshotObject::Uint8Array { .. } => 1 + 8 + 4,
            SnapshotObject::List { element_type, .. } => {
                s.use_type(*element_type);
                1 + 8 + 4
            }
            SnapshotObject::Map {
                key_type,
                value_type,
                ..
            } => {
                s.use_type(*key_type);
                s.use_type(*value_type);
                1 + 8 + 4
            }
            SnapshotObject::Instance { type_arguments, .. } => {
                for index in type_arguments.indexes() {
                    s.use_type(TypeId(u32::try_from(index).expect("bounded types")));
                }
                1 + 4 + 4 + 8 + 4
            }
            SnapshotObject::Descriptive { name, .. } => {
                if let Some(name) = name {
                    s.use_string(*name);
                }
                1 + 1 + 1 + 4
            }
            SnapshotObject::NonSnapshotableValue {} => 1,
            SnapshotObject::Media {
                mime_type, source, ..
            } => {
                // Tag, kind, whether a MIME type follows, and the source's tag.
                let mut bytes = 1 + 1 + 1 + 1;
                if let Some(mime_type) = mime_type {
                    s.use_string(*mime_type);
                    bytes += 4;
                }
                let data = match *source {
                    MediaSource::Url { url: text, data }
                    | MediaSource::File { path: text, data } => {
                        s.use_string(text);
                        // The text's length, and whether content follows.
                        bytes += 4 + 1;
                        data
                    }
                    MediaSource::Base64 { data } => Some(data),
                };
                if let Some(data) = data {
                    // The content's length, then the content as a value.
                    s.use_value(SnapshotValue::String(data));
                    bytes += 8;
                }
                bytes
            }
            SnapshotObject::Truncated(_) => 2,
        };
        s.objects[id.0 as usize] = object;
    }
    pub fn value_start(&mut self) -> usize {
        self.storage().values.len()
    }
    /// Reserve capacity once for a container, without initializing unused slots.
    pub fn reserve_values(&mut self, count: usize) {
        assert!(count <= self.remaining_values());
        let s = self.storage();
        grow(
            &mut s.values,
            count,
            s.owner.as_ref().unwrap(),
            &mut s.charged,
        );
    }
    pub fn push_value(&mut self, value: SnapshotValue) {
        assert!(self.remaining_values() > 0);
        let s = self.storage();
        s.use_value(value);
        grow(&mut s.values, 1, s.owner.as_ref().unwrap(), &mut s.charged);
        s.values.push(value);
    }
    pub fn value_range(&mut self, start: usize) -> Range<SnapshotValue> {
        Range::new(start, self.storage().values.len() - start)
    }
    pub fn entry_start(&mut self) -> usize {
        self.storage().entries.len()
    }
    pub fn reserve_entries(&mut self, count: usize) {
        assert!(count <= self.remaining_entries());
        let s = self.storage();
        grow(
            &mut s.entries,
            count,
            s.owner.as_ref().unwrap(),
            &mut s.charged,
        );
    }
    pub fn entry(&mut self, key: &BexStr, value: SnapshotValue) {
        assert!(self.remaining_entries() > 0);
        let s = self.storage();
        s.use_value(value);
        s.key_bytes += key.len();
        grow(&mut s.entries, 1, s.owner.as_ref().unwrap(), &mut s.charged);
        s.entries.push(MapEntry {
            key: key.clone(),
            value,
        });
    }
    pub fn entry_range(&mut self, start: usize) -> Range<MapEntry> {
        Range::new(start, self.storage().entries.len() - start)
    }
    /// Logical leaf bytes; not total retained backing (a slice can own a larger string).
    pub fn content(&mut self, bytes: usize, shared: bool) -> bool {
        if bytes > self.remaining_bytes() {
            self.limited(Limit::Bytes);
            return false;
        }
        let s = self.storage();
        s.content += bytes;
        if shared {
            s.stats.shared_bytes += bytes;
        } else {
            s.stats.copied_bytes += bytes;
        }
        true
    }
    pub fn string(&mut self, value: &BexStr) -> Option<StringId> {
        if !self.content(value.len(), !matches!(value, BexStr::Inline { .. })) {
            return None;
        }
        let s = self.storage();
        let id = StringId(u32::try_from(s.strings.len()).expect("string arena exhausted"));
        grow(&mut s.strings, 1, s.owner.as_ref().unwrap(), &mut s.charged);
        grow(
            &mut s.string_hashes,
            1,
            s.owner.as_ref().unwrap(),
            &mut s.charged,
        );
        grow(
            &mut s.string_used,
            1,
            s.owner.as_ref().unwrap(),
            &mut s.charged,
        );
        s.string_used.push(false);
        s.largest_leaf = s.largest_leaf.max(value.len());
        s.string_hashes.push(hash::string(value));
        s.strings.push(value.clone());
        Some(id)
    }
    pub fn bigint(&mut self, value: &Arc<BigInt>) -> Option<BigintId> {
        if !self.content(
            usize::try_from(value.bits().div_ceil(8)).unwrap_or(usize::MAX),
            true,
        ) {
            return None;
        }
        let s = self.storage();
        let id = BigintId(u32::try_from(s.bigints.len()).expect("bigint arena exhausted"));
        grow(&mut s.bigints, 1, s.owner.as_ref().unwrap(), &mut s.charged);
        grow(
            &mut s.bigint_hashes,
            1,
            s.owner.as_ref().unwrap(),
            &mut s.charged,
        );
        grow(
            &mut s.bigint_used,
            1,
            s.owner.as_ref().unwrap(),
            &mut s.charged,
        );
        s.bigint_used.push(false);
        s.largest_leaf = s.largest_leaf.max(walk::bigint_limb_bytes(value));
        s.bigint_hashes.push(hash::bigint(value));
        s.bigints.push(value.clone());
        Some(id)
    }
    pub fn type_start(&mut self) -> usize {
        self.storage().types.len()
    }
    pub fn push_type(&mut self, ty: OwnedType) -> TypeId {
        let s = self.storage();
        let id = TypeId(u32::try_from(s.types.len()).expect("type arena exhausted"));
        grow(&mut s.types, 1, s.owner.as_ref().unwrap(), &mut s.charged);
        grow(
            &mut s.type_leaves,
            1,
            s.owner.as_ref().unwrap(),
            &mut s.charged,
        );
        s.type_leaves.push(hash::ty(&ty));
        s.types.push(ty);
        id
    }
    pub fn type_range(&mut self, start: usize) -> Range<OwnedType> {
        Range::new(start, self.storage().types.len() - start)
    }
    pub fn copy_bytes(&mut self, bytes: &[u8]) -> Uint8ArrayData {
        let count = bytes.len().min(self.remaining_bytes());
        if count < bytes.len() {
            self.limited(Limit::Bytes);
        }
        self.content(count, false);
        let s = self.storage();
        s.largest_leaf = s.largest_leaf.max(count);
        let start = s.bytes.len();
        grow(
            &mut s.bytes,
            count,
            s.owner.as_ref().unwrap(),
            &mut s.charged,
        );
        let mut digest = hash::Hasher::new(tags::HashDomain::Uint8Array);
        for chunk in bytes[..count].chunks(btel_settings::snapshot::COPY_HASH_BATCH_BYTES) {
            digest.absorb(chunk);
            s.bytes.extend_from_slice(chunk);
        }
        Uint8ArrayData {
            range: Range::new(start, count),
            digest: digest.finish(),
        }
    }
    pub fn finish_value(self, value: SnapshotValue, shaper: &mut Shaper) -> Snapshot {
        self.finish(SnapshotRoot::Value(value), shaper)
    }
    /// Argument slots are written before processing deferred object children.
    pub fn finish_args(
        self,
        parameter_count: usize,
        slots: Range<SnapshotValue>,
        shaper: &mut Shaper,
    ) -> Snapshot {
        self.finish(
            SnapshotRoot::FunctionArgs(FunctionArgs {
                parameter_count,
                slots,
            }),
            shaper,
        )
    }
    /// Shape the capture into blobs; the snapshot is immutable afterwards.
    fn finish(mut self, root: SnapshotRoot, shaper: &mut Shaper) -> Snapshot {
        if let SnapshotRoot::Value(value) = root {
            self.storage().use_value(value);
        }
        shaper.shape(self.storage(), root);
        Snapshot(std::mem::ManuallyDrop::new(self.0.take().unwrap()))
    }
}
fn recycle(mut storage: Box<Storage>) {
    let pool = storage.owner.take().unwrap();
    // Every field is named, so none can keep a previous capture's state.
    let Storage {
        owner: _,
        values,
        objects,
        entries,
        bytes,
        strings,
        bigints,
        types,
        stats,
        charged: _,
        content,
        value_bytes,
        object_bytes,
        unset_objects,
        key_bytes,
        declaration_bytes,
        type_bytes,
        reuse_bytes,
        largest_leaf,
        referenced,
        string_used,
        bigint_used,
        shared,
        string_hashes,
        bigint_hashes,
        type_leaves,
        blobs,
        members,
        blob_children,
        object_homes,
        string_homes,
        bigint_homes,
    } = &mut *storage;
    values.clear();
    objects.clear();
    entries.clear();
    bytes.clear();
    strings.clear();
    bigints.clear();
    types.clear();
    *stats = CaptureStats::default();
    *content = 0;
    *value_bytes = 0;
    *object_bytes = 0;
    *unset_objects = 0;
    *key_bytes = 0;
    *declaration_bytes = 0;
    *type_bytes = 0;
    *reuse_bytes = 0;
    *largest_leaf = 0;
    referenced.clear();
    string_used.clear();
    bigint_used.clear();
    *shared = false;
    string_hashes.clear();
    bigint_hashes.clear();
    type_leaves.clear();
    blobs.clear();
    members.clear();
    blob_children.clear();
    object_homes.clear();
    string_homes.clear();
    bigint_homes.clear();
    let mut state = pool
        .state
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    state.in_use -= 1;
    if storage.charged > pool.config.max_retained_allocation_bytes
        || storage.charged
            > pool
                .config
                .retention_budget_bytes
                .saturating_sub(state.idle_bytes)
    {
        pool.bytes.fetch_sub(storage.charged, Ordering::Relaxed);
        state.allocated -= 1;
        drop(state);
        drop(storage);
    } else {
        state.idle_bytes += storage.charged;
        state.free.push(storage);
    }
}
impl Drop for Builder {
    fn drop(&mut self) {
        if let Some(storage) = self.0.take() {
            recycle(storage);
        }
    }
}
impl Drop for Snapshot {
    #[allow(unsafe_code)]
    fn drop(&mut self) {
        // SAFETY: this is the sole owner; ManuallyDrop prevents a second Box drop.
        recycle(unsafe { std::mem::ManuallyDrop::take(&mut self.0) });
    }
}
#[cfg(test)]
mod tests;

/// A `map<string, string>` snapshot built outside any VM, such as a
/// project's sources keyed by path. Entries past the pool's limits are
/// dropped and the snapshot says so. `None` when the pool has no slot.
pub fn string_map(pool: &SnapshotPool, entries: &[(String, String)]) -> Option<Snapshot> {
    let mut b = pool.try_acquire()?;
    let mut shaper = Shaper::default();
    let Some(map) = b.reserve_object() else {
        let truncated = b.limited(Limit::Objects);
        return Some(b.finish_value(truncated, &mut shaper));
    };
    let key_type = b.push_type(OwnedType::string());
    let value_type = b.push_type(OwnedType::string());
    let start = b.entry_start();
    let count = entries.len().min(b.remaining_entries());
    b.reserve_entries(count);
    for (key, value) in entries.iter().take(count) {
        let key = BexStr::from(key.as_str());
        if !b.content(key.len(), false) {
            break;
        }
        let value = match b.string(&BexStr::from(value.as_str())) {
            Some(id) => SnapshotValue::String(id),
            None => SnapshotValue::Truncated(Limit::Bytes),
        };
        b.entry(&key, value);
    }
    let entries_range = b.entry_range(start);
    if entries_range.len() < entries.len() {
        b.limited(Limit::Values);
    }
    b.set_object(
        map,
        SnapshotObject::Map {
            key_type,
            value_type,
            entries: entries_range,
            original_len: entries.len(),
        },
    );
    Some(b.finish_value(SnapshotValue::Object(map), &mut shaper))
}
