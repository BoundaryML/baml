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

use std::marker::PhantomData;

use baml_types::BamlMedia;

use crate::{types::Type, InternalError, Object, ObjectType, Value};

// Marker types for different pool kinds

/// Evaluation stack index type.
#[derive(Copy, Clone, Debug)]
pub struct StackKind;

/// Global pool index type.
#[derive(Copy, Clone, Debug)]
pub struct GlobalKind;

/// Object pool index type.
#[derive(Copy, Clone, Debug)]
pub struct ObjectKind;

/// Generic index type that forces a subtype during compilation.
#[derive(Clone, Copy)]
pub struct Index<K>(pub(crate) usize, PhantomData<K>);

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
        use std::ops::Bound::*;

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
#[derive(Clone, Default)]
#[repr(transparent)]
pub struct Pool<T, K>(pub(crate) Vec<T>, PhantomData<K>);

impl<T, K> Pool<T, K> {
    /// Creates a new empty vec.
    pub fn new() -> Self {
        Self(Vec::new(), PhantomData)
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

// Type aliases for specific pools and indices

pub type EvalStack = Pool<Value, StackKind>;
pub type GlobalPool = Pool<Value, GlobalKind>;
pub type ObjectPool = Pool<Object, ObjectKind>;

pub type StackIndex = Index<StackKind>;
pub type GlobalIndex = Index<GlobalKind>;
pub type ObjectIndex = Index<ObjectKind>;

impl ObjectPool {
    /// If `value` is an object, returns a reference to the object.
    /// - If `value` is not an object, throws [`InternalError::TypeError`].
    /// - If `value` is an object but reference is not accessible, throws
    ///   [`InternalError::InvalidObjectRef`].
    pub fn as_object(
        &self,
        value: &Value,
        object_type: ObjectType,
    ) -> Result<ObjectIndex, InternalError> {
        let Value::Object(index) = value else {
            return Err(InternalError::TypeError {
                expected: object_type.into(),
                got: self.type_of(value),
            });
        };

        Ok(*index)
    }

    pub fn as_string(&self, value: &Value) -> Result<&String, InternalError> {
        let index = self.as_object(value, ObjectType::String)?;
        self[index].as_string()
    }

    pub fn as_media(&self, value: &Value) -> Result<&BamlMedia, InternalError> {
        let object_index = self.as_object(value, ObjectType::Media)?;

        let Object::Media(media) = &self[object_index] else {
            return Err(InternalError::TypeError {
                expected: ObjectType::Media.into(),
                got: ObjectType::of(&self[object_index]).into(),
            });
        };

        Ok(media)
    }

    /// Inspects the type of a value, including the [`ObjectType`] if the object
    /// reference is valid.
    pub fn type_of(&self, value: &Value) -> Type {
        Type::of(value, |index| ObjectType::of(&self[index]))
    }

    pub fn insert(&mut self, value: Object) -> ObjectIndex {
        self.push(value);
        ObjectIndex::from_raw(self.0.len() - 1)
    }
}

impl EvalStack {
    pub fn ensure_pop(&mut self) -> Result<Value, InternalError> {
        self.pop().ok_or(InternalError::UnexpectedEmptyStack)
    }

    pub fn ensure_stack_top(&self) -> Result<StackIndex, InternalError> {
        self.ensure_slot_from_top(0)
    }

    pub fn ensure_slot_from_top(&self, index_from_top: usize) -> Result<StackIndex, InternalError> {
        self.len()
            .checked_sub(index_from_top + 1)
            .ok_or(InternalError::NotEnoughItemsOnStack(index_from_top))
            .map(StackIndex::from_raw)
    }
}
