#![allow(unsafe_code)]
//! The VM has 3 different "indexable" pools.
//!
//! One of them is the object pool, the other one is the globals pool, and
//! finally we have the evaluation stack (not a "pool" but behaves the same).
//!
//! Problem is that different bytecode instructions can contain parameters that
//! point to one of these 3 "pools" or vectors. If instructions used usize to
//! index into the pools, then it would be very easy to mistakenly use a
//! "global" index to access the "objects" vec and viceversa.
//!
//! This module provides a vector wrapper that needs specific types to index
//! into it, thus solving the problem mentioned above at compile time.

use std::{cell::UnsafeCell, collections::HashMap, marker::PhantomData, sync::Arc};

use borsh::{BorshDeserialize, BorshSerialize};

use crate::{HeapPtr, Object, PermitProof, RootHaver, Value};

// Marker types for different pool kinds

/// Evaluation stack index type.
#[derive(Copy, Clone, Debug, Default, BorshSerialize, BorshDeserialize)]
pub struct StackKind;

/// Global pool index type.
#[derive(Copy, Clone, Debug, Default, BorshSerialize, BorshDeserialize)]
pub struct GlobalKind;

/// Object pool index type.
#[derive(Copy, Clone, Debug, Default, BorshSerialize, BorshDeserialize)]
pub struct ObjectKind;

/// Generic index type that forces a subtype during compilation.
#[derive(Clone, Copy)]
pub struct Index<K>(pub(crate) usize, PhantomData<K>);

impl<K> BorshSerialize for Index<K> {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        BorshSerialize::serialize(&self.0, writer)
    }
}

impl<K> BorshDeserialize for Index<K> {
    fn deserialize_reader<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        usize::deserialize_reader(reader).map(|v| Self(v, PhantomData))
    }
}

impl<K> Index<K> {
    pub fn into_raw(self) -> usize {
        self.0
    }
}

impl<K> std::fmt::Debug for Index<K> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}({})",
            std::any::type_name::<K>().split("::").last().unwrap_or(""),
            self.0
        )
    }
}

impl<K> Index<K> {
    /// Create an index from a raw usize value.
    pub fn from_raw(raw: usize) -> Self {
        Self(raw, PhantomData)
    }

    /// Get the raw usize value.
    pub fn raw(self) -> usize {
        self.0
    }

    /// Helper method to convert [`Index<K>`] range bounds to usize ranges.
    fn usize_range<R>(range: R) -> (std::ops::Bound<usize>, std::ops::Bound<usize>)
    where
        R: std::ops::RangeBounds<Index<K>>,
    {
        use std::ops::Bound::{Excluded, Included, Unbounded};

        let start = match range.start_bound() {
            Unbounded => Unbounded,
            Included(idx) => Included(idx.0),
            Excluded(idx) => Excluded(idx.0),
        };

        let end = match range.end_bound() {
            Unbounded => Unbounded,
            Included(idx) => Included(idx.0),
            Excluded(idx) => Excluded(idx.0),
        };

        (start, end)
    }
}

impl<K> std::ops::Add<usize> for Index<K> {
    type Output = Self;
    fn add(self, rhs: usize) -> Self {
        Self(self.0 + rhs, PhantomData)
    }
}

impl<K> PartialEq for Index<K> {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl<K> PartialOrd for Index<K> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<K> Ord for Index<K> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0.cmp(&other.0)
    }
}

impl<K> Eq for Index<K> {}

impl<K> std::hash::Hash for Index<K> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.0.hash(state);
    }
}

impl<K> std::fmt::Display for Index<K> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.0, f)
    }
}

/// Generic pool type that uses a concrete [`Index`] for indexing.
#[derive(Clone)]
#[repr(transparent)]
pub struct Pool<T, K>(pub Vec<T>, PhantomData<K>);

impl<T: BorshSerialize, K> BorshSerialize for Pool<T, K> {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        BorshSerialize::serialize(&self.0, writer)
    }
}

impl<T: BorshDeserialize, K> BorshDeserialize for Pool<T, K> {
    fn deserialize_reader<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        Vec::<T>::deserialize_reader(reader).map(|v| Self(v, PhantomData))
    }
}

impl<T, K> Default for Pool<T, K> {
    fn default() -> Self {
        Self(Vec::new(), PhantomData)
    }
}

impl<T, K> Pool<T, K> {
    /// Creates a new empty vec.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a new pool from a [`Vec`].
    pub fn from_vec(vec: Vec<T>) -> Self {
        Self(vec, PhantomData)
    }
}

impl<T, K> std::fmt::Debug for Pool<T, K>
where
    T: std::fmt::Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(&self.0, f)
    }
}

impl<T, K> std::ops::Deref for Pool<T, K> {
    type Target = Vec<T>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T, K> std::ops::DerefMut for Pool<T, K> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<T, K> std::ops::Index<Index<K>> for Pool<T, K> {
    type Output = T;

    fn index(&self, index: Index<K>) -> &Self::Output {
        &self.0[index.0]
    }
}

impl<T, K> std::ops::IndexMut<Index<K>> for Pool<T, K> {
    fn index_mut(&mut self, index: Index<K>) -> &mut Self::Output {
        &mut self.0[index.0]
    }
}

impl<T, K, R> std::ops::Index<R> for Pool<T, K>
where
    R: std::ops::RangeBounds<Index<K>>,
{
    type Output = [T];

    fn index(&self, range: R) -> &Self::Output {
        self.0.index(Index::usize_range(range))
    }
}

impl<T, K, R> std::ops::IndexMut<R> for Pool<T, K>
where
    R: std::ops::RangeBounds<Index<K>>,
{
    fn index_mut(&mut self, range: R) -> &mut Self::Output {
        self.0.index_mut(Index::usize_range(range))
    }
}

impl<T, K> Pool<T, K> {
    pub fn drain<R>(&mut self, range: R) -> std::vec::Drain<'_, T>
    where
        R: std::ops::RangeBounds<Index<K>>,
    {
        self.0.drain(Index::usize_range(range))
    }
}

impl<T, K> std::iter::IntoIterator for Pool<T, K> {
    type Item = T;
    type IntoIter = <Vec<T> as IntoIterator>::IntoIter;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<'a, T, K> std::iter::IntoIterator for &'a Pool<T, K> {
    type Item = &'a T;
    type IntoIter = std::slice::Iter<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

impl<'a, T, K> std::iter::IntoIterator for &'a mut Pool<T, K> {
    type Item = &'a mut T;
    type IntoIter = std::slice::IterMut<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter_mut()
    }
}

pub type GlobalPool = Pool<Value, GlobalKind>;
pub type ObjectPool = Pool<Object, ObjectKind>;

/// The frozen globals pool shared by every post-`$init` VM.
///
/// Wraps an `Arc<UnsafeCell<Box<[Value]>>>` so every VM holds a cheap
/// refcounted view, and so the GC can rewrite `Value::object(HeapPtr)`
/// entries in place after an object move (via `RootHaver::forward_roots`)
/// without re-broadcasting a new `Arc` to every VM.
///
/// # Why this exists
///
/// Pre-fix, the engine stored `globals: Arc<[Value]>`. The frozen `Arc<[Value]>`
/// was *not* registered with [`HeapPermitManager`](crate)
/// (it isn't a `RootHaver`), so any `Value::object(HeapPtr)` populated during
/// `$init` (e.g. a top-level `let` bound to a heap-allocated literal) was
/// invisible to GC — the object could be reclaimed and the stale pointer left
/// in the globals pool. `SharedGlobals` fixes this: the engine wraps the
/// frozen pool, registers a clone with the permit manager, and the GC
/// traces / forwards the entries.
///
/// # Concurrency
///
/// - Reads ([`Self::get`], [`Self::as_slice`]) require the caller to hold an
///   active heap permit. While any permit is active, GC cannot park, so
///   `forward_roots` cannot be running.
/// - [`RootHaver::collect_roots`] / [`RootHaver::forward_roots`] are only
///   called by the GC while every permit is parked — so the `&mut` view they
///   take through the [`UnsafeCell`] cannot alias any reader.
///
/// The "caller holds a permit for the slice's lifetime" obligation is
/// **not** enforced by the type system: `SharedGlobals` is `Send + Sync +
/// Clone`, so the lifetime of `&Self` (and any `&[Value]` projected from
/// it via [`Self::as_slice`]) is decoupled from any specific
/// `ActiveHeapPermit`'s lifetime. Misuse causes UB. Where
/// possible, prefer [`Self::get`] (returns `Value` by copy) to avoid
/// holding the slice borrow across any operation that could end the
/// caller's permit.
pub struct SharedGlobals {
    inner: Arc<SharedGlobalsInner>,
}

struct SharedGlobalsInner {
    /// Heap-allocated `Box<[Value]>` whose contents are read concurrently
    /// by VMs (under their permits) and rewritten by GC `forward_roots`
    /// (with all permits parked). `UnsafeCell` is the right primitive
    /// because the `Send`/`Sync` invariant is enforced by the permit
    /// manager, not by the type system.
    values: UnsafeCell<Box<[Value]>>,
}

// SAFETY: concurrent access to `values` is gated externally by the
// `HeapPermitManager` semaphore + holders mutex. See module docs.
unsafe impl Send for SharedGlobalsInner {}
unsafe impl Sync for SharedGlobalsInner {}

impl Clone for SharedGlobals {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl std::fmt::Debug for SharedGlobals {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // SAFETY: same as `Self::as_slice` — read-only access via UnsafeCell;
        // the caller of `Debug::fmt` is expected to be holding a permit (or
        // running outside the GC). In practice this is only used for
        // diagnostics.
        let slice = unsafe { &*self.inner.values.get() };
        f.debug_struct("SharedGlobals")
            .field("len", &slice.len())
            .finish_non_exhaustive()
    }
}

impl SharedGlobals {
    /// Build a `SharedGlobals` from the freshly-frozen globals vector
    /// produced by `$init`.
    pub fn from_vec(values: Vec<Value>) -> Self {
        Self {
            inner: Arc::new(SharedGlobalsInner {
                values: UnsafeCell::new(values.into_boxed_slice()),
            }),
        }
    }

    /// Number of globals in the pool.
    ///
    /// Requires a [`PermitProof`] witnessing that an active heap permit
    /// is held — this excludes GC's `forward_roots` (the only writer
    /// against `inner.values`) for the duration of the read.
    pub fn len(&self, _proof: PermitProof<'_>) -> usize {
        // SAFETY: `_proof` is the runtime witness that no `forward_roots`
        // can run concurrently. Explicit `&*` produces a `&Box<[Value]>`
        // so `.len()` doesn't autoref a raw pointer.
        #[allow(unsafe_code, reason = "UnsafeCell projection gated by `_proof`")]
        unsafe {
            (&*self.inner.values.get()).len()
        }
    }

    /// Read a global by index.
    ///
    /// Requires a [`PermitProof`]; see [`Self::len`].
    ///
    /// # Panics
    ///
    /// Panics if `index` is out of bounds — same contract as slice indexing.
    pub fn get(&self, _proof: PermitProof<'_>, index: GlobalIndex) -> Value {
        // SAFETY: `_proof` excludes concurrent `forward_roots`. `Value`
        // is `Copy`, so the returned value is decoupled from any
        // aliasing of the underlying `UnsafeCell` after this returns.
        // Explicit `&*` produces a `&[Value]` (no autoref through the
        // raw-pointer indexing path).
        #[allow(unsafe_code, reason = "UnsafeCell projection gated by `_proof`")]
        unsafe {
            (&*self.inner.values.get())[index.into_raw()]
        }
    }

    /// Borrow the underlying globals as a slice.
    ///
    /// The returned slice's lifetime is tied to `proof`'s lifetime, not
    /// to `&self`: dropping the originating [`bex_heap::ActiveHeapPermit`]
    /// invalidates the proof and therefore the slice borrow. This is the
    /// type-system guarantee that makes the underlying `UnsafeCell`
    /// projection sound — GC cannot run `forward_roots` while any
    /// `PermitProof<'a>` is live, and the borrow checker forces the
    /// slice borrow to die with the proof.
    ///
    /// [`bex_heap::ActiveHeapPermit`]: <https://docs.rs/bex_heap>
    pub fn as_slice<'a>(&'a self, _proof: PermitProof<'a>) -> &'a [Value] {
        // SAFETY: `_proof` excludes concurrent `forward_roots` for `'a`,
        // and the returned slice borrow cannot outlive `'a` (and hence
        // cannot outlive the proof, which cannot outlive the permit).
        #[allow(unsafe_code, reason = "UnsafeCell projection gated by `_proof`")]
        unsafe {
            &*self.inner.values.get()
        }
    }
}

impl RootHaver for SharedGlobals {
    fn collect_roots(&self, roots: &mut Vec<HeapPtr>) {
        // SAFETY: GC parks every active permit before calling this, so no
        // reader is observing the slice concurrently.
        let slice = unsafe { &*self.inner.values.get() };
        for value in slice {
            if let Some(ptr) = value.as_object_ptr() {
                roots.push(ptr);
            }
        }
    }

    fn forward_roots(&mut self, roots: &HashMap<HeapPtr, HeapPtr>) {
        // SAFETY: GC parks every active permit before calling this; `&mut self`
        // additionally proves there is no concurrent reader of the
        // `SharedGlobals` permit-cell value on this thread.
        let slice = unsafe { &mut *self.inner.values.get() };
        for value in slice.iter_mut() {
            if let Some(ptr) = value.as_object_ptr()
                && let Some(&new) = roots.get(&ptr)
            {
                *value = Value::object(new);
            }
        }
    }
}

/// The view of globals available to a [`crate::Object`]-aware VM.
///
/// **Invariant**: only `$init` functions emit [`crate::Instruction::StoreGlobal`].
/// The compiler enforces this by emitting `StoreGlobal` solely from
/// `compile_init_function`. The runtime enforces it via the [`VmGlobals::Owned`]
/// vs [`VmGlobals::Shared`] split: post-`$init` VMs hold a [`VmGlobals::Shared`]
/// reference into the engine's frozen globals, and a `StoreGlobal` against a
/// `Shared` view must be turned into a `VmInternalError` by the VM.
///
/// # Variants
/// - [`VmGlobals::Owned`]: a mutable [`GlobalPool`] used during `$init` execution.
/// - [`VmGlobals::Shared`]: a frozen [`SharedGlobals`] shared by every
///   post-`$init` VM. The underlying `Arc` clone is a refcount bump; reads
///   require an active heap permit (which excludes the GC's `forward_roots`
///   mutation pass).
#[derive(Clone, Debug)]
pub enum VmGlobals {
    /// Mutable globals pool, used during `$init`.
    Owned(GlobalPool),
    /// Frozen globals shared across all post-`$init` VMs and rooted by the
    /// engine's [`SharedGlobals`] permit holder.
    Shared(SharedGlobals),
}

impl VmGlobals {
    /// Read a global by index. `Value` is `Copy` so this returns by value.
    ///
    /// # Panics
    /// Panics if `index` is out of bounds — same contract as slice indexing.
    pub fn get(&self, proof: PermitProof<'_>, index: GlobalIndex) -> Value {
        match self {
            Self::Owned(pool) => pool[index],
            Self::Shared(globals) => globals.get(proof, index),
        }
    }

    /// Mutably set a global. Only succeeds for [`VmGlobals::Owned`]; returns the
    /// caller-supplied error for [`VmGlobals::Shared`] (post-`$init` writes
    /// violate the invariant and the VM should treat them as internal errors).
    pub fn set<E>(&mut self, index: GlobalIndex, value: Value, on_shared: E) -> Result<(), E> {
        match self {
            Self::Owned(pool) => {
                pool[index] = value;
                Ok(())
            }
            Self::Shared(_) => Err(on_shared),
        }
    }

    /// View the globals as a slice. Both variants support O(1) slice access.
    ///
    /// The returned slice is tied to `proof`'s lifetime: dropping the
    /// originating [`bex_heap::ActiveHeapPermit`] invalidates the proof
    /// and therefore the slice borrow. See [`SharedGlobals::as_slice`].
    ///
    /// [`bex_heap::ActiveHeapPermit`]: <https://docs.rs/bex_heap>
    pub fn as_slice<'a>(&'a self, proof: PermitProof<'a>) -> &'a [Value] {
        match self {
            Self::Owned(pool) => &pool.0,
            Self::Shared(globals) => globals.as_slice(proof),
        }
    }
}

pub type StackIndex = Index<StackKind>;
pub type GlobalIndex = Index<GlobalKind>;

#[cfg(feature = "heap_debug")]
#[derive(Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct ObjectIndex {
    raw: usize,
    epoch: u32,
}

// The borsh wire format must be identical with and without `heap_debug`, so
// a serialized `Program` (pack envelope, bytecode cache) written by one build
// is readable by the other — e.g. `baml pack` run from a heap_debug build
// embeds into a non-heap_debug `baml-pack-host`. `epoch` is a runtime GC-debug
// tag: every `ObjectIndex` that reaches a serialized payload is a compile-time
// pool index created via `from_raw` (epoch 0); live epochs exist only inside
// the heap debugger and never cross serialization. So serialize `raw` alone,
// exactly matching `Index<ObjectKind>`'s impl below, and rehydrate epoch 0.
#[cfg(feature = "heap_debug")]
impl BorshSerialize for ObjectIndex {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        BorshSerialize::serialize(&self.raw, writer)
    }
}

#[cfg(feature = "heap_debug")]
impl BorshDeserialize for ObjectIndex {
    fn deserialize_reader<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        usize::deserialize_reader(reader).map(|raw| Self { raw, epoch: 0 })
    }
}

#[cfg(feature = "heap_debug")]
impl ObjectIndex {
    pub fn from_raw(raw: usize) -> Self {
        Self { raw, epoch: 0 }
    }

    pub fn into_raw(self) -> usize {
        self.raw
    }

    pub fn raw(self) -> usize {
        self.raw
    }
}

#[cfg(feature = "heap_debug")]
impl std::fmt::Debug for ObjectIndex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ObjectIndex({}@{})", self.raw, self.epoch)
    }
}

#[cfg(feature = "heap_debug")]
impl std::fmt::Display for ObjectIndex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.raw, f)
    }
}

#[cfg(feature = "heap_debug")]
impl std::ops::Add<usize> for ObjectIndex {
    type Output = Self;
    fn add(self, rhs: usize) -> Self {
        Self {
            raw: self.raw + rhs,
            epoch: self.epoch,
        }
    }
}

#[cfg(feature = "heap_debug")]
impl<T> std::ops::Index<ObjectIndex> for Pool<T, ObjectKind> {
    type Output = T;

    fn index(&self, index: ObjectIndex) -> &Self::Output {
        &self.0[index.raw]
    }
}

#[cfg(feature = "heap_debug")]
impl<T> std::ops::IndexMut<ObjectIndex> for Pool<T, ObjectKind> {
    fn index_mut(&mut self, index: ObjectIndex) -> &mut Self::Output {
        &mut self.0[index.raw]
    }
}

#[cfg(not(feature = "heap_debug"))]
pub type ObjectIndex = Index<ObjectKind>;

#[cfg(not(feature = "heap_debug"))]
#[cfg(test)]
mod shared_globals_tests {
    use super::*;
    use crate::HeapPtr;

    fn fake_ptr(addr: usize) -> HeapPtr {
        // SAFETY: tests never deref the pointer; we only inspect it as a
        // raw `HeapPtr` through `RootHaver::collect_roots` /
        // `forward_roots`. The address need not be a valid heap slot.
        #[cfg(feature = "heap_debug")]
        unsafe {
            HeapPtr::from_ptr(addr as *mut crate::Object, 0)
        }
        #[cfg(not(feature = "heap_debug"))]
        unsafe {
            HeapPtr::from_ptr(addr as *mut crate::Object)
        }
    }

    #[test]
    fn collect_roots_returns_only_object_values() {
        let p1 = fake_ptr(0x1000);
        let p2 = fake_ptr(0x2000);
        // Float is now Object-boxed; substitute another non-object scalar (Int)
        // to keep the test's intent ("non-Object values are NOT collected as roots").
        let globals = SharedGlobals::from_vec(vec![
            Value::int(1),
            Value::object(p1),
            Value::int(99),
            Value::object(p2),
            Value::NULL,
        ]);

        let mut roots = Vec::new();
        globals.collect_roots(&mut roots);

        assert_eq!(roots.len(), 2);
        assert!(roots.contains(&p1));
        assert!(roots.contains(&p2));
    }

    #[test]
    fn forward_roots_rewrites_in_place_through_unsafecell() {
        let old1 = fake_ptr(0x1000);
        let new1 = fake_ptr(0x9000);
        let old2 = fake_ptr(0x2000);
        let new2 = fake_ptr(0xA000);
        // Pointer NOT in the forwarding map should pass through unchanged
        // (it represents a compile-time pointer or one that wasn't moved).
        let stable = fake_ptr(0x3000);

        let mut globals = SharedGlobals::from_vec(vec![
            Value::object(old1),
            Value::int(42),
            Value::object(old2),
            Value::object(stable),
        ]);

        let mut forwarding = HashMap::new();
        forwarding.insert(old1, new1);
        forwarding.insert(old2, new2);
        globals.forward_roots(&forwarding);

        // Take a fresh `as_slice()` view; the in-place rewrite must be
        // visible through the *same* `Arc<UnsafeCell<...>>` that any
        // existing `SharedGlobals` clone observes.
        //
        // SAFETY: single-threaded test, no concurrent `forward_roots`.
        #[allow(unsafe_code, reason = "test mock proof")]
        let proof = unsafe { PermitProof::new() };
        let slice = globals.as_slice(proof);
        assert_eq!(slice[0], Value::object(new1));
        assert_eq!(slice[1], Value::int(42));
        assert_eq!(slice[2], Value::object(new2));
        assert_eq!(slice[3], Value::object(stable));
    }

    #[test]
    fn clone_shares_underlying_storage_with_engine() {
        // The engine wires this up so that `SharedGlobals` registered with
        // the heap permit manager and `SharedGlobals` cloned into VMs
        // share the same `Arc<UnsafeCell<Box<[Value]>>>`. Verify the clone
        // observes a `forward_roots` mutation made through another clone.
        let old = fake_ptr(0x1000);
        let new = fake_ptr(0x9000);
        let original = SharedGlobals::from_vec(vec![Value::object(old)]);
        let mut held_by_holder = original.clone();

        let mut forwarding = HashMap::new();
        forwarding.insert(old, new);
        held_by_holder.forward_roots(&forwarding);

        // The engine-side clone (`original`) must see the rewrite.
        // SAFETY: single-threaded test; no concurrent `forward_roots`.
        #[allow(unsafe_code, reason = "test mock proof")]
        let proof = unsafe { PermitProof::new() };
        let v = original.get(proof, GlobalIndex::from_raw(0));
        assert_eq!(v, Value::object(new));
    }
}
