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

use baml_type::{DeclarationName, typetag::TypeTag};
use bex_str::BexStr;
use num_bigint::BigInt;

mod encoding;
pub use encoding::{BLOB_MAGIC, BLOB_VERSION};
mod hash;
pub use hash::SnapshotId;
mod memory;

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
    hash: hash::Digest,
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
            hash: hash::Hasher::new(32).finish(),
        }
    }
    pub fn len(self) -> usize {
        self.len as usize
    }
    pub fn is_empty(self) -> bool {
        self.len == 0
    }
    fn new(start: usize, len: usize, hash: hash::Digest) -> Self {
        Self {
            start: u32::try_from(start).expect("arena exhausted"),
            len: u32::try_from(len).expect("arena exhausted"),
            kind: PhantomData,
            hash,
        }
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
    Bytes {
        data: Range<u8>,
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
    Truncated(Limit),
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
    root: Option<SnapshotRoot>,
    stats: CaptureStats,
    charged: usize,
    content: usize,
    string_hashes: Vec<hash::Digest>,
    bigint_hashes: Vec<hash::Digest>,
    type_hashes: Vec<hash::Digest>,
    object_hashes: Vec<hash::Digest>,
    value_hash: hash::Hasher,
    entry_hash: hash::Hasher,
    type_hash: hash::Hasher,
    id: Option<SnapshotId>,
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
impl Snapshot {
    pub fn id(&self) -> SnapshotId {
        self.0.id.expect("frozen hash")
    }
    pub fn root(&self) -> SnapshotRoot {
        self.0.root.expect("frozen root")
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
        match self.0.root.as_ref().unwrap() {
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
    pub fn bytes(&self, range: Range<u8>) -> &[u8] {
        &self.0.bytes[range.start as usize..][..range.len as usize]
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
            + std::mem::size_of_val(self.0.type_hashes.as_slice())
            + std::mem::size_of_val(self.0.object_hashes.as_slice())
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
                root: None,
                stats: CaptureStats::default(),
                charged: std::mem::size_of::<Storage>(),
                content: 0,
                string_hashes: Vec::new(),
                bigint_hashes: Vec::new(),
                type_hashes: Vec::new(),
                object_hashes: Vec::new(),
                value_hash: hash::Hasher::new(32),
                entry_hash: hash::Hasher::new(32),
                type_hash: hash::Hasher::new(32),
                id: None,
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
        grow(
            &mut s.object_hashes,
            1,
            s.owner.as_ref().unwrap(),
            &mut s.charged,
        );
        s.object_hashes
            .push(hash::object(&SnapshotObject::Truncated(Limit::Objects), s));
        Some(id)
    }
    pub fn set_object(&mut self, id: ObjectId, object: SnapshotObject) {
        let s = self.storage();
        s.object_hashes[id.0 as usize] = hash::object(&object, s);
        s.objects[id.0 as usize] = object;
    }
    pub fn value_start(&mut self) -> usize {
        let s = self.storage();
        s.value_hash = hash::Hasher::new(32);
        s.values.len()
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
        grow(&mut s.values, 1, s.owner.as_ref().unwrap(), &mut s.charged);
        hash::value(
            &mut s.value_hash,
            value,
            &s.string_hashes,
            &s.bigint_hashes,
            &s.type_hashes,
        );
        s.values.push(value);
    }
    pub fn value_range(&mut self, start: usize) -> Range<SnapshotValue> {
        {
            let s = self.storage();
            Range::new(start, s.values.len() - start, s.value_hash.finish())
        }
    }
    pub fn entry_start(&mut self) -> usize {
        let s = self.storage();
        s.entry_hash = hash::Hasher::new(32);
        s.entries.len()
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
        grow(&mut s.entries, 1, s.owner.as_ref().unwrap(), &mut s.charged);
        s.entry_hash.string(key);
        hash::value(
            &mut s.entry_hash,
            value,
            &s.string_hashes,
            &s.bigint_hashes,
            &s.type_hashes,
        );
        s.entries.push(MapEntry {
            key: key.clone(),
            value,
        });
    }
    pub fn entry_range(&mut self, start: usize) -> Range<MapEntry> {
        {
            let s = self.storage();
            Range::new(start, s.entries.len() - start, s.entry_hash.finish())
        }
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
        s.bigint_hashes.push(hash::bigint(value));
        s.bigints.push(value.clone());
        Some(id)
    }
    pub fn type_start(&mut self) -> usize {
        let s = self.storage();
        s.type_hash = hash::Hasher::new(32);
        s.types.len()
    }
    pub fn push_type(&mut self, ty: OwnedType) -> TypeId {
        let s = self.storage();
        let id = TypeId(u32::try_from(s.types.len()).expect("type arena exhausted"));
        grow(&mut s.types, 1, s.owner.as_ref().unwrap(), &mut s.charged);
        grow(
            &mut s.type_hashes,
            1,
            s.owner.as_ref().unwrap(),
            &mut s.charged,
        );
        let hash = hash::ty(&ty);
        s.type_hash.digest(hash);
        s.type_hashes.push(hash);
        s.types.push(ty);
        id
    }
    pub fn type_range(&mut self, start: usize) -> Range<OwnedType> {
        {
            let s = self.storage();
            Range::new(start, s.types.len() - start, s.type_hash.finish())
        }
    }
    pub fn copy_bytes(&mut self, bytes: &[u8]) -> Range<u8> {
        let count = bytes.len().min(self.remaining_bytes());
        if count < bytes.len() {
            self.limited(Limit::Bytes);
        }
        self.content(count, false);
        let s = self.storage();
        let start = s.bytes.len();
        grow(
            &mut s.bytes,
            count,
            s.owner.as_ref().unwrap(),
            &mut s.charged,
        );
        let mut hash = hash::Hasher::new(32);
        for chunk in bytes[..count].chunks(btel_settings::snapshot::COPY_HASH_BATCH_BYTES) {
            hash.raw(chunk);
            s.bytes.extend_from_slice(chunk);
        }
        Range::new(start, count, hash.finish())
    }
    pub fn finish_value(mut self, value: SnapshotValue) -> Snapshot {
        self.storage().root = Some(SnapshotRoot::Value(value));
        self.finish()
    }
    /// Argument slots are written before processing deferred object children.
    pub fn finish_args(mut self, parameter_count: usize, slots: Range<SnapshotValue>) -> Snapshot {
        self.storage().root = Some(SnapshotRoot::FunctionArgs(FunctionArgs {
            parameter_count,
            slots,
        }));
        self.finish()
    }
    fn finish(mut self) -> Snapshot {
        let s = self.storage();
        s.id = Some(hash::snapshot(s));
        Snapshot(std::mem::ManuallyDrop::new(self.0.take().unwrap()))
    }
}
fn recycle(mut storage: Box<Storage>) {
    let pool = storage.owner.take().unwrap();
    storage.objects.clear();
    storage.entries.clear();
    storage.bytes.clear();
    storage.values.clear();
    storage.strings.clear();
    storage.bigints.clear();
    storage.types.clear();
    storage.string_hashes.clear();
    storage.bigint_hashes.clear();
    storage.type_hashes.clear();
    storage.object_hashes.clear();
    storage.id = None;
    storage.value_hash = hash::Hasher::new(32);
    storage.entry_hash = hash::Hasher::new(32);
    storage.type_hash = hash::Hasher::new(32);
    storage.root = None;
    storage.stats = CaptureStats::default();
    storage.content = 0;
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
