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

use std::{marker::PhantomData, sync::Arc};

use crate::{Object, Value};

// Marker types for different pool kinds

/// Evaluation stack index type.
#[derive(Copy, Clone, Debug, Default)]
pub struct StackKind;

/// Global pool index type.
#[derive(Copy, Clone, Debug, Default)]
pub struct GlobalKind;

/// Object pool index type.
#[derive(Copy, Clone, Debug, Default)]
pub struct ObjectKind;

/// Generic index type that forces a subtype during compilation.
#[derive(Clone, Copy)]
pub struct Index<K>(pub(crate) usize, PhantomData<K>);

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
/// - [`VmGlobals::Shared`]: a frozen `Arc<[Value]>` shared by every post-`$init` VM.
///   Cloning is a refcount bump; reads are direct slice indexing.
#[derive(Clone, Debug)]
pub enum VmGlobals {
    /// Mutable globals pool, used during `$init`.
    Owned(GlobalPool),
    /// Frozen globals shared across all post-`$init` VMs.
    Shared(Arc<[Value]>),
}

impl VmGlobals {
    /// Number of globals in the view.
    pub fn len(&self) -> usize {
        match self {
            Self::Owned(pool) => pool.len(),
            Self::Shared(slice) => slice.len(),
        }
    }

    /// Whether the view is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Read a global by index. `Value` is `Copy` so this returns by value.
    ///
    /// # Panics
    /// Panics if `index` is out of bounds — same contract as slice indexing.
    pub fn get(&self, index: GlobalIndex) -> Value {
        match self {
            Self::Owned(pool) => pool[index],
            Self::Shared(slice) => slice[index.into_raw()],
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

    /// Freeze this view into a shared `Arc<[Value]>`. For [`VmGlobals::Owned`],
    /// this consumes the underlying `Vec`; for [`VmGlobals::Shared`], it clones
    /// the existing `Arc`.
    pub fn freeze(self) -> Arc<[Value]> {
        match self {
            Self::Owned(pool) => Arc::from(pool.0),
            Self::Shared(slice) => slice,
        }
    }

    /// View the globals as a slice. Both variants support O(1) slice access.
    pub fn as_slice(&self) -> &[Value] {
        match self {
            Self::Owned(pool) => &pool.0,
            Self::Shared(slice) => slice,
        }
    }
}

impl std::ops::Index<GlobalIndex> for VmGlobals {
    type Output = Value;

    fn index(&self, index: GlobalIndex) -> &Self::Output {
        match self {
            Self::Owned(pool) => &pool[index],
            Self::Shared(slice) => &slice[index.into_raw()],
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

#[cfg(feature = "heap_debug")]
impl ObjectIndex {
    pub fn from_raw(raw: usize) -> Self {
        Self { raw, epoch: 0 }
    }

    pub fn from_raw_epoch(raw: usize, epoch: u32) -> Self {
        Self { raw, epoch }
    }

    pub fn into_raw(self) -> usize {
        self.raw
    }

    pub fn raw(self) -> usize {
        self.raw
    }

    pub fn epoch(self) -> u32 {
        self.epoch
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
impl Index<ObjectKind> {
    pub fn from_raw_epoch(raw: usize, _epoch: u32) -> Self {
        Self::from_raw(raw)
    }

    pub fn epoch(self) -> u32 {
        0
    }
}
