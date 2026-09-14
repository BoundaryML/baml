use std::cell::UnsafeCell;

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
}

impl<T> From<T> for LockedContainer<T> {
    fn from(data: T) -> Self {
        Self::new(data)
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
/// Heap-mutable map container. Pairs a boxed `IndexMap<BexStr, Value>` with
/// the generic [`LockedContainer`] lock/guard machinery.
///
/// `IndexMap` is 72 bytes before the lock, so storing it inline would push
/// `Object` past its size cap. Storing only the backing map behind `Box<_>`
/// keeps the container itself small while avoiding an extra indirection around
/// the lock.
pub type MapContainer = LockedContainer<Box<IndexMap<bex_str::BexStr, Value>>>;
pub type MapReadGuard<'a> = LockedReadGuard<'a, Box<IndexMap<bex_str::BexStr, Value>>>;
pub type MapWriteGuard<'a> = LockedWriteGuard<'a, Box<IndexMap<bex_str::BexStr, Value>>>;

impl MapReadGuard<'_> {
    /// Snapshot the underlying `IndexMap`.
    pub fn to_index_map(&self) -> IndexMap<bex_str::BexStr, Value> {
        self.data.as_ref().clone()
    }
}

impl Map {
    /// Build a map of `key_ty`/`value_ty` from its backing entries.
    pub fn new(
        key_ty: crate::RealizedTy,
        value_ty: crate::RealizedTy,
        data: IndexMap<bex_str::BexStr, Value>,
    ) -> Self {
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
    pub unsafe fn data_unchecked(&self) -> &IndexMap<bex_str::BexStr, Value> {
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
    pub unsafe fn data_unchecked_mut(&self) -> &mut IndexMap<bex_str::BexStr, Value> {
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

    /// Locked convenience: copy the value at `key`, or `None` if absent.
    pub fn get(&self, key: &str) -> Option<Value> {
        self.data.lock().data.get(key).copied()
    }

    /// Locked convenience: snapshot the underlying `IndexMap`.
    pub fn to_index_map(&self) -> IndexMap<bex_str::BexStr, Value> {
        self.data.lock().to_index_map()
    }
}
