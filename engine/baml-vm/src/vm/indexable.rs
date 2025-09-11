use baml_types::BamlMedia;

use super::{InternalError, ObjectType, Type};
use crate::{Object, Value};

macro_rules! impl_indexable_wrapper {
    ($name:ident, $value_type:ident, $index_name:ident) => {
        #[derive(Clone, Default)]
        #[repr(transparent)]
        pub struct $name(pub(crate) Vec<$value_type>);

        // forward debug implementation
        impl std::fmt::Debug for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                std::fmt::Debug::fmt(&self.0, f)
            }
        }

        // deref(mut) for vec
        impl std::ops::Deref for $name {
            type Target = Vec<$value_type>;

            fn deref(&self) -> &Self::Target {
                &self.0
            }
        }

        impl std::ops::DerefMut for $name {
            fn deref_mut(&mut self) -> &mut Self::Target {
                &mut self.0
            }
        }

        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        pub struct $index_name(pub(crate) usize);

        impl $name {
            /// Create a new wrapper from a Vec.
            pub fn from_vec(vec: Vec<$value_type>) -> Self {
                Self(vec)
            }
        }

        impl $index_name {
            /// Pinky promise that the given index is safe to interpret.
            pub fn from_raw(raw: usize) -> Self {
                Self(raw)
            }
        }

        impl std::ops::Add<usize> for $index_name {
            type Output = Self;
            fn add(self, rhs: usize) -> Self {
                Self(self.0 + rhs)
            }
        }

        impl std::fmt::Display for $index_name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "{}", self.0)
            }
        }

        // index(mut)
        impl std::ops::Index<$index_name> for $name {
            type Output = $value_type;

            fn index(&self, index: $index_name) -> &Self::Output {
                &self.0[index.0]
            }
        }

        impl std::ops::IndexMut<$index_name> for $name {
            fn index_mut(&mut self, index: $index_name) -> &mut Self::Output {
                &mut self.0[index.0]
            }
        }

        // index(mut), range.
        // All methods are marked inline so that the match expression is simplified through
        // monomorphization.
        impl<R> std::ops::Index<R> for $name
        where
            R: std::ops::RangeBounds<$index_name>,
        {
            type Output = [$value_type];

            #[inline]
            fn index(&self, r: R) -> &Self::Output {
                let start = r.start_bound().map(|i| i.0);
                let end = r.end_bound().map(|i| i.0);

                use std::ops::Bound::*;
                match (start, end) {
                    (Unbounded, Unbounded) => &self.0,
                    (Unbounded, Included(end)) => &self.0[..=end],
                    (Unbounded, Excluded(end)) => &self.0[..end],
                    (Included(start), Unbounded) => &self.0[start..],
                    (Included(start), Included(end)) => &self.0[start..=end],
                    (Included(start), Excluded(end)) => &self.0[start..end],
                    (Excluded(start), Unbounded) => &self.0[start + 1..],
                    (Excluded(start), Included(end)) => &self.0[start + 1..=end],
                    (Excluded(start), Excluded(end)) => &self.0[start + 1..end],
                }
            }
        }

        impl<R> std::ops::IndexMut<R> for $name
        where
            R: std::ops::RangeBounds<$index_name>,
        {
            #[inline]
            fn index_mut(&mut self, r: R) -> &mut Self::Output {
                let start = r.start_bound().map(|i| i.0);
                let end = r.end_bound().map(|i| i.0);

                use std::ops::Bound::*;
                match (start, end) {
                    (Unbounded, Unbounded) => &mut self.0,
                    (Unbounded, Included(end)) => &mut self.0[..=end],
                    (Unbounded, Excluded(end)) => &mut self.0[..end],
                    (Included(start), Unbounded) => &mut self.0[start..],
                    (Included(start), Included(end)) => &mut self.0[start..=end],
                    (Included(start), Excluded(end)) => &mut self.0[start..end],
                    (Excluded(start), Unbounded) => &mut self.0[start + 1..],
                    (Excluded(start), Included(end)) => &mut self.0[start + 1..=end],
                    (Excluded(start), Excluded(end)) => &mut self.0[start + 1..end],
                }
            }
        }

        impl $name {
            #[inline]
            pub fn drain<R>(&mut self, range: R) -> std::vec::Drain<'_, $value_type>
            where
                R: std::ops::RangeBounds<$index_name>,
            {
                let start = range.start_bound().map(|i| i.0);
                let end = range.end_bound().map(|i| i.0);
                use std::ops::Bound::*;
                match (start, end) {
                    (Unbounded, Unbounded) => self.0.drain(..),
                    (Unbounded, Included(end)) => self.0.drain(..=end),
                    (Unbounded, Excluded(end)) => self.0.drain(..end),
                    (Included(start), Unbounded) => self.0.drain(start..),
                    (Included(start), Included(end)) => self.0.drain(start..=end),
                    (Included(start), Excluded(end)) => self.0.drain(start..end),
                    (Excluded(start), Unbounded) => self.0.drain(start + 1..),
                    (Excluded(start), Included(end)) => self.0.drain(start + 1..=end),
                    (Excluded(start), Excluded(end)) => self.0.drain(start + 1..end),
                }
            }
        }

        // IntoIterator since deref isn't enough
        impl std::iter::IntoIterator for $name {
            type Item = $value_type;
            type IntoIter = <Vec<$value_type> as IntoIterator>::IntoIter;

            fn into_iter(self) -> Self::IntoIter {
                self.0.into_iter()
            }
        }

        impl<'a> std::iter::IntoIterator for &'a $name {
            type Item = &'a $value_type;
            type IntoIter = std::slice::Iter<'a, $value_type>;

            fn into_iter(self) -> Self::IntoIter {
                self.0.iter()
            }
        }

        impl<'a> std::iter::IntoIterator for &'a mut $name {
            type Item = &'a mut $value_type;
            type IntoIter = std::slice::IterMut<'a, $value_type>;

            fn into_iter(self) -> Self::IntoIter {
                self.0.iter_mut()
            }
        }
    };
}

impl_indexable_wrapper!(EvalStack, Value, StackIndex);
impl_indexable_wrapper!(GlobalPool, Value, GlobalIndex);
impl_indexable_wrapper!(ObjectPool, Object, ObjectIndex);

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

    /// Inspects the type of a value, including the [`ObjectType`] if the object reference is
    /// valid.
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
            .map(StackIndex)
    }
}
