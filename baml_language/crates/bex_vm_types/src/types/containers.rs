use std::{cell::UnsafeCell, collections::HashMap};

use indexmap::IndexMap;

use crate::{Value, lazy_biased_mutex::LazyBiasedMutex};

/// Heap-mutable structural container. Pairs a dynamic backing store with a
/// [`LazyBiasedMutex`] so cross-fiber `spawn`-racing mutations don't corrupt
/// internal container state such as a `Vec`'s `(ptr, len, cap)` triple or an
/// `IndexMap`'s hash table.
///
/// # Soundness
///
/// The inner value is wrapped in [`UnsafeCell`] so that both
/// [`Self::lock`] and [`Self::lock_mut`] can take `&self`. Without this,
/// the mutator-side `BexVm::as_array_mut` / `as_map_mut` would have to call
/// `get_object_mut`, which fabricates `&'static mut Object` for slots
/// that — by design — are shared across `spawn` fibers, violating
/// Rust's aliasing rules even though the [`LazyBiasedMutex`] provides
/// actual mutual exclusion at the memory level.
///
/// All mutator access to `data` happens through the lock guards, which is the
/// only place we materialize shared or mutable references to the backing store.
#[derive(Debug)]
pub struct LockedContainer<T> {
    mutex: LazyBiasedMutex,
    data: UnsafeCell<T>,
}

// SAFETY: cross-thread access is serialized by `mutex`. The `UnsafeCell` is
// necessary so callers can take the lock via `&self` (the only sound option
// when the container is reachable through aliased `&Object` from the shared
// heap). `T: Send` is required because the protected backing store can move
// between threads behind the lock.
unsafe impl<T: Send> Sync for LockedContainer<T> {}

impl<T> LockedContainer<T> {
    pub fn new(data: T) -> Self {
        Self {
            mutex: LazyBiasedMutex::new(),
            data: UnsafeCell::new(data),
        }
    }

    /// Acquire the container's mutex and return a read guard. The lock is
    /// released when the guard is dropped.
    pub fn lock(&self) -> LockedReadGuard<'_, T> {
        let access = self.mutex.enter();
        // SAFETY: we just acquired the lock; no other thread can hold a
        // `&mut` to `data` (the only place `&mut data` is materialized
        // is `lock_mut`, which also takes the lock). Lifetime is tied
        // to `&self`, which is tied to the access guard.
        let data = unsafe { &*self.data.get() };
        LockedReadGuard {
            data,
            _access: access,
        }
    }

    /// Acquire the container's mutex and return a write guard. The lock
    /// is released when the guard is dropped. Takes `&self` (not
    /// `&mut self`) so callers can lock through a shared reference
    /// obtained from the shared heap (`get_object`, not the unsound
    /// `get_object_mut`).
    pub fn lock_mut(&self) -> LockedWriteGuard<'_, T> {
        let access = self.mutex.enter();
        // SAFETY: the access guard provides mutual exclusion against
        // all other lock holders for this container. The returned
        // `&mut T` lifetime is bounded by the guard's lifetime.
        let data = unsafe { &mut *self.data.get() };
        LockedWriteGuard {
            data,
            _access: access,
        }
    }

    /// Get a reference to the underlying `Vec` WITHOUT acquiring the lock.
    ///
    /// # Safety
    ///
    /// The caller must ensure no other thread is concurrently mutating
    /// this container. Safe contexts:
    ///
    /// - GC traversal while the stop-the-world barrier is engaged
    ///   (all mutator threads are parked).
    /// - Single-threaded engine setup / init.
    /// - Other code that has independently stopped all VM mutators.
    ///
    /// For any path where a `spawn`ed fiber may be running, use
    /// [`Self::lock`] instead.
    #[allow(clippy::missing_safety_doc)]
    pub unsafe fn data_unchecked(&self) -> &T {
        // SAFETY: caller upholds the no-concurrent-writer contract.
        unsafe { &*self.data.get() }
    }

    /// Mutable counterpart of [`Self::data_unchecked`]. Same safety
    /// contract.
    ///
    /// # Safety
    ///
    /// In addition to the no-concurrent-mutator contract, the caller
    /// must hold the only `&mut ArrayContainer` (or otherwise
    /// guarantee no other readers).
    #[allow(clippy::missing_safety_doc, clippy::mut_from_ref)]
    pub unsafe fn data_unchecked_mut(&self) -> &mut T {
        // SAFETY: caller upholds the contract.
        unsafe { &mut *self.data.get() }
    }

    /// The backing store, through exclusive access to the container. No lock
    /// is taken: `&mut self` already proves nothing else can reach it.
    pub fn get_mut(&mut self) -> &mut T {
        self.data.get_mut()
    }
}

impl<T> From<T> for LockedContainer<T> {
    fn from(data: T) -> Self {
        Self::new(data)
    }
}

/// The bytes a value's backing storage occupies in its own allocations.
///
/// This is capacity, not length: it is what the allocator handed out, so a
/// container that grew and was then emptied still reports the buffer it holds.
pub trait Footprint {
    fn footprint(&self) -> usize;
}

impl<T> Footprint for Vec<T> {
    fn footprint(&self) -> usize {
        self.capacity().saturating_mul(size_of::<T>())
    }
}

impl Footprint for Box<MapData> {
    fn footprint(&self) -> usize {
        size_of::<MapData>().saturating_add(self.backing_bytes())
    }
}

// Cloning takes the lock so a concurrent writer can't tear `data` mid-clone.
// The contention state (the in-flight access counter) is not part of the
// logical value of the source.
impl<T: Clone> Clone for LockedContainer<T> {
    fn clone(&self) -> Self {
        let guard = self.lock();
        Self::new(guard.clone())
    }
}

impl<T> LockedContainer<Vec<T>> {
    /// Locked convenience: number of elements.
    pub fn len(&self) -> usize {
        self.lock().len()
    }

    /// Locked convenience: whether the container is empty.
    pub fn is_empty(&self) -> bool {
        self.lock().is_empty()
    }
}

impl<T: Copy> LockedContainer<Vec<T>> {
    /// Locked convenience: copy the element at `idx`, or `None` if out of bounds.
    pub fn get(&self, idx: usize) -> Option<T> {
        self.lock().get(idx).copied()
    }
}

impl<T: Clone> LockedContainer<Vec<T>> {
    /// Locked convenience: snapshot the underlying `Vec<T>`.
    pub fn to_vec(&self) -> Vec<T> {
        self.lock().clone()
    }
}

/// Read guard for a [`LockedContainer`]. Holds the container's
/// [`LazyBiasedMutex`] for the duration of the guard's lifetime.
pub struct LockedReadGuard<'a, T> {
    data: &'a T,
    _access: crate::lazy_biased_mutex::AccessGuard<'a>,
}

impl<T> std::ops::Deref for LockedReadGuard<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        self.data
    }
}

/// Write guard for a [`LockedContainer`]. Holds the container's
/// [`LazyBiasedMutex`] for the duration of the guard's lifetime.
pub struct LockedWriteGuard<'a, T> {
    data: &'a mut T,
    _access: crate::lazy_biased_mutex::AccessGuard<'a>,
}

impl<T> std::ops::Deref for LockedWriteGuard<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        self.data
    }
}

impl<T> std::ops::DerefMut for LockedWriteGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        self.data
    }
}

#[derive(Clone, Debug)]
pub struct Array {
    pub element_ty: Box<crate::RealizedTy>,
    pub data: ArrayContainer,
}

impl Array {
    /// Build an array of `element_ty` from its backing values.
    pub fn new(element_ty: crate::RealizedTy, data: Vec<Value>) -> Self {
        Self {
            element_ty: Box::new(element_ty),
            data: ArrayContainer::new(data),
        }
    }

    /// Lock the backing store for reading (see [`LockedContainer::lock`]).
    pub fn lock(&self) -> ArrayReadGuard<'_> {
        self.data.lock()
    }

    /// Lock the backing store for writing (see [`LockedContainer::lock_mut`]).
    pub fn lock_mut(&self) -> ArrayWriteGuard<'_> {
        self.data.lock_mut()
    }

    /// Locked convenience: number of elements.
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// Locked convenience: whether the array is empty.
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// Locked convenience: copy the element at `idx`, or `None` if out of bounds.
    pub fn get(&self, idx: usize) -> Option<Value> {
        self.data.get(idx)
    }

    /// Locked convenience: snapshot the backing `Vec<Value>`.
    pub fn to_vec(&self) -> Vec<Value> {
        self.data.to_vec()
    }

    /// Unlocked read of the backing store. See [`LockedContainer::data_unchecked`].
    ///
    /// # Safety
    ///
    /// The caller must uphold the no-concurrent-writer contract documented on
    /// [`LockedContainer::data_unchecked`].
    pub unsafe fn data_unchecked(&self) -> &Vec<Value> {
        // SAFETY: forwarded to the caller's obligation.
        unsafe { self.data.data_unchecked() }
    }

    /// Unlocked mutable access to the backing store. See
    /// [`LockedContainer::data_unchecked_mut`].
    ///
    /// # Safety
    ///
    /// The caller must uphold the contract documented on
    /// [`LockedContainer::data_unchecked_mut`].
    #[expect(clippy::mut_from_ref)]
    pub unsafe fn data_unchecked_mut(&self) -> &mut Vec<Value> {
        // SAFETY: forwarded to the caller's obligation.
        unsafe { self.data.data_unchecked_mut() }
    }
}

/// Heap-mutable array container.
///
/// Held inline by `Object::Array`. Size: 24 (Vec) + 1 (mutex) + padding = 32 bytes.
pub type ArrayContainer = LockedContainer<Vec<Value>>;
pub type ArrayReadGuard<'a> = LockedReadGuard<'a, Vec<Value>>;
pub type ArrayWriteGuard<'a> = LockedWriteGuard<'a, Vec<Value>>;

/// Heap-mutable byte-array container. Same synchronization strategy as
/// [`ArrayContainer`], but over a `Vec<u8>` backing store.
pub type Uint8ArrayContainer = LockedContainer<Vec<u8>>;
pub type Uint8ArrayReadGuard<'a> = LockedReadGuard<'a, Vec<u8>>;
pub type Uint8ArrayWriteGuard<'a> = LockedWriteGuard<'a, Vec<u8>>;

#[derive(Clone, Debug)]
pub struct Map {
    pub key_ty: Box<crate::RealizedTy>,
    pub value_ty: Box<crate::RealizedTy>,
    pub data: MapContainer,
}
/// Heap-mutable map container. Pairs a boxed [`MapData`] with
/// the generic [`LockedContainer`] lock/guard machinery.
///
/// `IndexMap` is 72 bytes before the lock, so storing it inline would push
/// `Object` past its size cap. Storing only the backing map behind `Box<_>`
/// keeps the container itself small while avoiding an extra indirection around
/// the lock.
pub type MapContainer = LockedContainer<Box<MapData>>;
pub type MapReadGuard<'a> = LockedReadGuard<'a, Box<MapData>>;
pub type MapWriteGuard<'a> = LockedWriteGuard<'a, Box<MapData>>;

pub type EntryId = u64;

/// Hash a native string key with the same framing as the VM's `Hash` driver.
pub fn map_string_hash(key: &str) -> u64 {
    use std::hash::Hasher;
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    hasher.write(&[5]);
    hasher.write(&(key.len() as u64).to_le_bytes());
    hasher.write(key.as_bytes());
    hasher.finish()
}

#[derive(Clone, Copy, Debug)]
pub struct MapEntry {
    pub key: Value,
    pub value: Value,
    pub hash: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MapEpochChanged;

/// Ordered entries indexed by hashes computed by the VM, never by Rust's
/// pointer-based `Value` equality or hashing.
#[derive(Clone, Debug, Default)]
pub struct MapData {
    entries: IndexMap<EntryId, MapEntry>,
    buckets: HashMap<u64, Vec<EntryId>>,
    next_id: EntryId,
    // Only membership changes invalidate an in-flight key comparison.
    epoch: u64,
}

impl MapData {
    pub fn new() -> Self {
        Self::default()
    }

    /// Construct from distinct keys with their VM-computed hashes.
    pub fn from_hashed_entries(entries: impl IntoIterator<Item = (u64, Value, Value)>) -> Self {
        let mut data = Self::new();
        for (hash, key, value) in entries {
            data.insert_hashed(hash, key, value);
        }
        data
    }

    pub fn epoch(&self) -> u64 {
        self.epoch
    }

    /// Everything this map allocates: the entry storage, its index, and the
    /// per-hash buckets. Estimated from capacities without walking entries.
    fn backing_bytes(&self) -> usize {
        // An entry stores its id, the entry and its hash, plus an index slot.
        let entry = size_of::<EntryId>() + size_of::<MapEntry>() + 2 * size_of::<usize>();
        // A bucket-table slot stores the hash, the bucket and a control byte.
        let bucket_slot = size_of::<u64>() + size_of::<Vec<EntryId>>() + 1;
        // Each distinct hash has a bucket, whose first allocation holds four ids.
        let bucket = 4 * size_of::<EntryId>();
        self.entries
            .capacity()
            .saturating_mul(entry)
            .saturating_add(self.buckets.capacity().saturating_mul(bucket_slot))
            .saturating_add(self.buckets.len().saturating_mul(bucket))
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn clear(&mut self) {
        if !self.entries.is_empty() {
            self.advance_epoch();
            self.entries.clear();
            self.buckets.clear();
        }
    }

    pub fn iter(&self) -> impl ExactSizeIterator<Item = (&Value, &Value)> {
        self.entries
            .values()
            .map(|entry| (&entry.key, &entry.value))
    }

    pub fn entries(&self) -> impl ExactSizeIterator<Item = (&EntryId, &MapEntry)> {
        self.entries.iter()
    }

    pub fn keys(&self) -> impl ExactSizeIterator<Item = &Value> {
        self.entries.values().map(|entry| &entry.key)
    }

    pub fn values(&self) -> impl ExactSizeIterator<Item = &Value> {
        self.entries.values().map(|entry| &entry.value)
    }

    /// Only for GC forwarding: changing a key's identity or contents invalidates
    /// its cached hash. GC relocation preserves the VM hash.
    pub fn trace_values_mut(&mut self) -> impl Iterator<Item = &mut Value> {
        self.entries
            .values_mut()
            .flat_map(|entry| [&mut entry.key, &mut entry.value])
    }

    /// Capture candidates for equality checks performed after releasing the lock.
    /// The caller must root copied keys and values if a check can trigger GC.
    pub fn snapshot_bucket(&self, hash: u64) -> (u64, Vec<(EntryId, MapEntry)>) {
        (
            self.epoch,
            self.buckets
                .get(&hash)
                .into_iter()
                .flatten()
                .map(|id| (*id, self.entries[id]))
                .collect(),
        )
    }

    fn advance_epoch(&mut self) {
        self.epoch = self.epoch.checked_add(1).expect("map epoch exhausted");
    }

    /// Insert a key already known to be distinct by the caller.
    pub fn insert_hashed(&mut self, hash: u64, key: Value, value: Value) -> EntryId {
        let id = self.next_id;
        self.next_id = self
            .next_id
            .checked_add(1)
            .expect("map entry IDs exhausted");
        self.advance_epoch();
        self.entries.insert(id, MapEntry { key, value, hash });
        self.buckets.entry(hash).or_default().push(id);
        id
    }

    /// Validate a search against this map's snapshot epoch. The ID must come
    /// from that snapshot; `None` denotes a completed search with no match.
    pub fn get_if_epoch(
        &self,
        epoch: u64,
        id: Option<EntryId>,
    ) -> Result<Option<Value>, MapEpochChanged> {
        if epoch != self.epoch {
            return Err(MapEpochChanged);
        }
        Ok(id.map(|id| self.entries[&id].value))
    }

    pub fn set_if_epoch(
        &mut self,
        epoch: u64,
        id: Option<EntryId>,
        hash: u64,
        key: Value,
        value: Value,
    ) -> Result<Option<Value>, MapEpochChanged> {
        if epoch != self.epoch {
            return Err(MapEpochChanged);
        }
        if let Some(id) = id {
            Ok(Some(std::mem::replace(&mut self.entries[&id].value, value)))
        } else {
            self.insert_hashed(hash, key, value);
            Ok(None)
        }
    }

    pub fn remove_if_epoch(
        &mut self,
        epoch: u64,
        id: Option<EntryId>,
    ) -> Result<Option<Value>, MapEpochChanged> {
        if epoch != self.epoch {
            return Err(MapEpochChanged);
        }
        let Some(id) = id else {
            return Ok(None);
        };
        self.advance_epoch();
        let entry = self.entries.shift_remove(&id).expect("validated map entry");
        let bucket = self.buckets.get_mut(&entry.hash).expect("map bucket");
        bucket.retain(|candidate| *candidate != id);
        if bucket.is_empty() {
            self.buckets.remove(&entry.hash);
        }
        Ok(Some(entry.value))
    }
}

impl Map {
    /// Build a map of `key_ty`/`value_ty` from its backing entries.
    pub fn new(key_ty: crate::RealizedTy, value_ty: crate::RealizedTy, data: MapData) -> Self {
        Self {
            key_ty: Box::new(key_ty),
            value_ty: Box::new(value_ty),
            data: MapContainer::new(Box::new(data)),
        }
    }

    /// Lock the backing store for reading (see [`LockedContainer::lock`]).
    pub fn lock(&self) -> MapReadGuard<'_> {
        self.data.lock()
    }

    /// Lock the backing store for writing (see [`LockedContainer::lock_mut`]).
    pub fn lock_mut(&self) -> MapWriteGuard<'_> {
        self.data.lock_mut()
    }

    /// Unlocked read of the backing store. See [`LockedContainer::data_unchecked`].
    ///
    /// # Safety
    ///
    /// The caller must uphold the no-concurrent-writer contract documented on
    /// [`LockedContainer::data_unchecked`].
    pub unsafe fn data_unchecked(&self) -> &MapData {
        // SAFETY: forwarded to the caller's obligation.
        unsafe { self.data.data_unchecked() }
    }

    /// Unlocked mutable access to the backing store. See
    /// [`LockedContainer::data_unchecked_mut`].
    ///
    /// # Safety
    ///
    /// The caller must uphold the contract documented on
    /// [`LockedContainer::data_unchecked_mut`].
    #[expect(clippy::mut_from_ref)]
    pub unsafe fn data_unchecked_mut(&self) -> &mut MapData {
        // SAFETY: forwarded to the caller's obligation.
        unsafe { self.data.data_unchecked_mut() }
    }

    /// Locked convenience: number of entries.
    pub fn len(&self) -> usize {
        self.data.lock().data.len()
    }

    /// Locked convenience: whether the map is empty.
    pub fn is_empty(&self) -> bool {
        self.data.lock().data.is_empty()
    }

    pub fn epoch(&self) -> u64 {
        self.lock().epoch()
    }

    pub fn snapshot_bucket(&self, hash: u64) -> (u64, Vec<(EntryId, MapEntry)>) {
        self.lock().snapshot_bucket(hash)
    }

    pub fn snapshot_entries(&self) -> Vec<(Value, Value)> {
        self.lock()
            .iter()
            .map(|(key, value)| (*key, *value))
            .collect()
    }

    pub fn get_if_epoch(
        &self,
        epoch: u64,
        id: Option<EntryId>,
    ) -> Result<Option<Value>, MapEpochChanged> {
        self.lock().get_if_epoch(epoch, id)
    }

    pub fn set_if_epoch(
        &self,
        epoch: u64,
        id: Option<EntryId>,
        hash: u64,
        key: Value,
        value: Value,
    ) -> Result<Option<Value>, MapEpochChanged> {
        self.lock_mut().set_if_epoch(epoch, id, hash, key, value)
    }

    pub fn remove_if_epoch(
        &self,
        epoch: u64,
        id: Option<EntryId>,
    ) -> Result<Option<Value>, MapEpochChanged> {
        self.lock_mut().remove_if_epoch(epoch, id)
    }
}

#[cfg(test)]
mod map_tests {
    use super::*;

    #[test]
    fn collisions_updates_and_reinsertion_preserve_order() {
        let mut map = MapData::new();
        let first = map.insert_hashed(7, Value::int(1), Value::int(10));
        let second = map.insert_hashed(7, Value::int(2), Value::int(20));
        let (epoch, candidates) = map.snapshot_bucket(7);
        assert_eq!(candidates.len(), 2);
        assert_eq!(
            map.set_if_epoch(epoch, Some(first), 7, Value::int(99), Value::int(11)),
            Ok(Some(Value::int(10)))
        );
        assert_eq!(
            map.keys().copied().collect::<Vec<_>>(),
            vec![Value::int(1), Value::int(2)]
        );
        assert_eq!(
            map.remove_if_epoch(map.epoch(), Some(first)),
            Ok(Some(Value::int(11)))
        );
        let third = map.insert_hashed(7, Value::int(1), Value::int(12));
        assert!(third > second);
        assert_eq!(
            map.keys().copied().collect::<Vec<_>>(),
            vec![Value::int(2), Value::int(1)]
        );
        assert_eq!(
            map.snapshot_bucket(7)
                .1
                .iter()
                .map(|(id, _)| *id)
                .collect::<Vec<_>>(),
            vec![second, third]
        );
    }

    #[test]
    fn membership_changes_invalidate_searches_but_value_updates_do_not() {
        let mut map = MapData::new();
        let (epoch, _) = map.snapshot_bucket(0);
        let id = map.insert_hashed(0, Value::TRUE, Value::FALSE);
        assert_eq!(map.get_if_epoch(epoch, None), Err(MapEpochChanged));
        assert_eq!(
            map.set_if_epoch(epoch, None, 0, Value::NULL, Value::NULL),
            Err(MapEpochChanged)
        );
        assert_eq!(map.remove_if_epoch(epoch, Some(id)), Err(MapEpochChanged));
        assert_eq!(map.len(), 1);
        let epoch = map.epoch();
        map.set_if_epoch(epoch, Some(id), 0, Value::TRUE, Value::NULL)
            .unwrap();
        assert_eq!(map.get_if_epoch(epoch, Some(id)), Ok(Some(Value::NULL)));
    }

    #[test]
    fn forwarding_visits_keys_and_values_without_changing_hashes() {
        let mut map = MapData::from_hashed_entries([(123, Value::int(1), Value::int(2))]);
        for value in map.trace_values_mut() {
            *value = Value::int(value.as_int().unwrap() + 10);
        }
        let (_, entries) = map.snapshot_bucket(123);
        assert_eq!(entries[0].1.key, Value::int(11));
        assert_eq!(entries[0].1.value, Value::int(12));
        assert_eq!(entries[0].1.hash, 123);
    }
}
