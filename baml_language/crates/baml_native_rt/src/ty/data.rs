//! The shapes the copied types are made of, as `design/memory.md` gives
//! them: children that are in the executable's data or owned, and the
//! objects a type names, by address.
//!
//! Not a copy: nothing in `baml_type` has these. The compiler includes this
//! file by path, with the copies of the types, to learn their layouts.

use std::{fmt, ptr::NonNull};

/// The header word every object starts with. See `layout.rs`.
#[repr(transparent)]
pub struct ObjectHeader(pub usize);

/// Tag `CLASS`. Also describes an enum. Read through the offsets in
/// `layout.rs`, never as a Rust value: this names only its address.
pub enum ClassObject {}

/// Tag `INTERFACE`. Named only by its address.
pub enum InterfaceObject {}

/// Tag `ENUM_VARIANT`. Named only by its address.
pub enum EnumVariantObject {}

/// Tag `PACKAGE`. Named only by its address.
pub enum PackageObject {}

/// Tag `RAW_FUNCTION`. Named only by its address.
pub enum RawFunctionObject {}

/// Tag `TYPE`. A recursive alias is the address of the type object of what
/// it stands for, so the alias's definition is read from here.
#[repr(C, align(16))]
pub struct TypeObject {
    pub header: ObjectHeader,
    pub ty: super::RealizedTy,
}

/// An associated type's position in its interface's declaration.
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AssocIndex(pub(crate) u32);

/// A field's position among an instance's slots, type parameters included.
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SlotIndex(pub(crate) u32);

/// An associated type pinned to a type: where the original has the tuple
/// `(Name, T)`, whose layout Rust does not fix.
#[repr(C)]
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AssocBinding<T> {
    pub member: AssocIndex,
    pub ty: T,
}

/// What `Cow<'static, [T]>` is, with a layout the compiler can write: the
/// items are in the executable's data, or in memory this owns and frees
/// when it is dropped.
#[repr(C)]
pub struct CowSlice<T> {
    pub ptr: NonNull<T>,
    pub len: u32,
    pub owned: bool,
}

/// The same for one item.
#[repr(C)]
pub struct CowBox<T> {
    pub ptr: NonNull<T>,
    pub owned: bool,
}

/// UTF-8 text that is not a BAML string: a parameter's name, a literal
/// type's text.
pub type Text = CowSlice<u8>;

impl<T> CowSlice<T> {
    /// Items that are never freed: the executable's data.
    pub fn from_static(items: &'static [T]) -> Self {
        Self {
            ptr: NonNull::from(items).cast(),
            len: u32::try_from(items.len()).expect("a list of a type is shorter than 2^32"),
            owned: false,
        }
    }
}

impl<T> From<Vec<T>> for CowSlice<T> {
    /// Items this owns.
    fn from(items: Vec<T>) -> Self {
        let len = u32::try_from(items.len()).expect("a list of a type is shorter than 2^32");
        let items = Box::into_raw(items.into_boxed_slice());
        Self {
            // SAFETY: `Box::into_raw` is never null.
            ptr: unsafe { NonNull::new_unchecked(items.cast()) },
            len,
            owned: true,
        }
    }
}

impl<T> FromIterator<T> for CowSlice<T> {
    fn from_iter<I: IntoIterator<Item = T>>(items: I) -> Self {
        items.into_iter().collect::<Vec<T>>().into()
    }
}

impl<T> std::ops::Deref for CowSlice<T> {
    type Target = [T];

    fn deref(&self) -> &[T] {
        // SAFETY: `len` items at `ptr`, static or owned by `self`.
        unsafe { std::slice::from_raw_parts(self.ptr.as_ptr(), self.len as usize) }
    }
}

impl<T> Drop for CowSlice<T> {
    fn drop(&mut self) {
        if self.owned {
            // SAFETY: an owned slice was made by `From<Vec<T>>`, from a box
            // of exactly `len` items.
            unsafe {
                drop(Box::from_raw(std::ptr::slice_from_raw_parts_mut(
                    self.ptr.as_ptr(),
                    self.len as usize,
                )));
            }
        }
    }
}

impl<T: Clone> Clone for CowSlice<T> {
    /// Static items stay where they are; owned ones are copied.
    fn clone(&self) -> Self {
        if self.owned {
            self.to_vec().into()
        } else {
            Self {
                ptr: self.ptr,
                len: self.len,
                owned: false,
            }
        }
    }
}

impl<T> CowBox<T> {
    /// An item this owns.
    pub fn new(item: T) -> Self {
        Self {
            // SAFETY: `Box::into_raw` is never null.
            ptr: unsafe { NonNull::new_unchecked(Box::into_raw(Box::new(item))) },
            owned: true,
        }
    }

    /// An item that is never freed: the executable's data.
    pub fn from_static(item: &'static T) -> Self {
        Self {
            ptr: NonNull::from(item),
            owned: false,
        }
    }
}

impl<T> std::ops::Deref for CowBox<T> {
    type Target = T;

    fn deref(&self) -> &T {
        // SAFETY: the item at `ptr`, static or owned by `self`.
        unsafe { self.ptr.as_ref() }
    }
}

impl<T> Drop for CowBox<T> {
    fn drop(&mut self) {
        if self.owned {
            // SAFETY: an owned item was made by `new`, from a box.
            unsafe { drop(Box::from_raw(self.ptr.as_ptr())) }
        }
    }
}

impl<T: Clone> Clone for CowBox<T> {
    /// A static item stays where it is; an owned one is copied.
    fn clone(&self) -> Self {
        if self.owned {
            Self::new((**self).clone())
        } else {
            Self {
                ptr: self.ptr,
                owned: false,
            }
        }
    }
}

macro_rules! compared_as_items {
    ($cow:ident, $items:ty) => {
        impl<T: PartialEq> PartialEq for $cow<T> {
            fn eq(&self, other: &Self) -> bool {
                <$items as PartialEq>::eq(self, other)
            }
        }
        impl<T: Eq> Eq for $cow<T> {}
        impl<T: PartialOrd> PartialOrd for $cow<T> {
            fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
                <$items as PartialOrd>::partial_cmp(self, other)
            }
        }
        impl<T: Ord> Ord for $cow<T> {
            fn cmp(&self, other: &Self) -> std::cmp::Ordering {
                <$items as Ord>::cmp(self, other)
            }
        }
        impl<T: std::hash::Hash> std::hash::Hash for $cow<T> {
            fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
                <$items as std::hash::Hash>::hash(self, state);
            }
        }
        impl<T: fmt::Debug> fmt::Debug for $cow<T> {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                <$items as fmt::Debug>::fmt(self, f)
            }
        }
    };
}

compared_as_items!(CowSlice, [T]);
compared_as_items!(CowBox, T);

const _: () = assert!(size_of::<CowSlice<u64>>() == 16);
const _: () = assert!(size_of::<CowBox<u64>>() == 16);
// `None` is a null `ptr`: what the compiler writes for a parameter with no
// name or a method a rule does not provide.
const _: () = assert!(size_of::<Option<Text>>() == size_of::<Text>());
