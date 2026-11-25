//! Rust state behind a `$rust_type` field: a heap object of one of two
//! forms, which the field refers to.
//!
//! - **Plain**, for a `T: Copy`: the value's bytes are in the object, which
//!   the collector copies like any other that holds no reference. Nothing
//!   is dropped and nothing lists the object. The value is read and
//!   written in place through a [`Cell`], borrowed from a view, so the
//!   borrow ends before the next allocation can move the object.
//! - **Owned**, for a `T: Send + Sync`: the object holds an `Arc<T>` in its
//!   raw form. The Rust value never moves, so a native borrows `&T` from a
//!   view, and one that stops clones the `Arc` into its job. The heap
//!   lists every such object, and the collection that finds one dead
//!   drops its `Arc`; what is left when the program ends is dropped then.
//!
//! Each object names its value's type by the address of a [`RustKind`],
//! which a native compares with the one of the type it expects: one
//! compare when they are the same, as they are unless the runtime itself
//! is wrong.

use std::{any::TypeId, cell::Cell, marker::PhantomData, sync::Arc};

use super::{Managed, Root, Scope, View};
use crate::{
    heap::{RustKind, load, store},
    layout::{
        OWNED_KIND, OWNED_VALUE, PLAIN_KIND, PLAIN_VALUE, PLAIN_WORDS, TAG_OWNED, TAG_PLAIN,
        TYPE_ID_MASK, WORD, plain_size,
    },
};

/// The Rust state a `$rust_type` field refers to.
pub struct Opaque;

impl Managed for Opaque {
    type View<'a> = OpaqueRef<'a>;
}

/// A view of the Rust state a `$rust_type` field refers to.
#[derive(Clone, Copy)]
pub struct OpaqueRef<'a>(usize, PhantomData<&'a ()>);

impl<'a> View<'a> for OpaqueRef<'a> {
    unsafe fn from_raw(raw: usize) -> Self {
        Self(raw, PhantomData)
    }
    fn raw(self) -> usize {
        self.0
    }
}

/// The kinds of the type `T`, one for each form.
struct Kinds<T>(PhantomData<T>);

impl<T: Send + Sync + 'static> Kinds<T> {
    const OWNED: &'static RustKind = &RustKind {
        drop: drop_arc::<T>,
        id: TypeId::of::<T>,
        name: std::any::type_name::<T>,
    };
}

impl<T: Copy + 'static> Kinds<T> {
    const PLAIN: &'static RustKind = &RustKind {
        drop: drop_nothing,
        id: TypeId::of::<T>,
        name: std::any::type_name::<T>,
    };
    /// The words of the value in a plain object.
    const WORDS: usize = {
        assert!(
            align_of::<T>() <= WORD,
            "plain data is aligned as a word is"
        );
        size_of::<T>().div_ceil(WORD)
    };
}

/// Drop the `Arc<T>` whose raw form is `value`.
unsafe fn drop_arc<T>(value: usize) {
    // SAFETY: the heap calls this once, with what `Arc::into_raw` returned
    // when the object was made.
    unsafe { drop(Arc::from_raw(value as *const T)) };
}

unsafe fn drop_nothing(_value: usize) {}

/// End the program: the state at `object` is not what a native took it
/// for. Only a fault of the runtime's own gets here.
#[cold]
fn mismatched(object: usize, expected: &RustKind, form: &str) -> ! {
    // SAFETY: a live object, whose header says whether it has a kind.
    let found = unsafe {
        match load(object) & TYPE_ID_MASK {
            TAG_OWNED => ((*(load(object + OWNED_KIND) as *const RustKind)).name)(),
            TAG_PLAIN => ((*(load(object + PLAIN_KIND) as *const RustKind)).name)(),
            _ => "no Rust state at all",
        }
    };
    panic!(
        "a native expected {form} Rust state of type `{}` and found `{found}`",
        (expected.name)()
    );
}

/// Whether the kind at `found` is `expected`. The same type can have its
/// kind at two addresses, so two that differ are compared by type.
fn is_kind(found: usize, expected: &'static RustKind) -> bool {
    // SAFETY: the kind of an object is the address of a `RustKind` in the
    // executable's data.
    found == std::ptr::from_ref(expected) as usize
        || unsafe { ((*(found as *const RustKind)).id)() == (expected.id)() }
}

impl<'a> OpaqueRef<'a> {
    /// The address of the owned `T`, checked to be one.
    fn owned_value<T: Send + Sync + 'static>(self) -> *const T {
        let expected = Kinds::<T>::OWNED;
        // SAFETY: a view is of a live object, which has the two words of
        // an owned value when its header says it is one.
        unsafe {
            if load(self.0) & TYPE_ID_MASK != TAG_OWNED
                || !is_kind(load(self.0 + OWNED_KIND), expected)
            {
                mismatched(self.0, expected, "owned");
            }
            load(self.0 + OWNED_VALUE) as *const T
        }
    }

    /// The owned `T`. The value does not move when its object does, and
    /// the borrow ends with the view, while the object is alive.
    pub fn owned<T: Send + Sync + 'static>(self) -> &'a T {
        // SAFETY: the object holds an `Arc<T>` for as long as it lives.
        unsafe { &*self.owned_value::<T>() }
    }

    /// Another `Arc` of the owned `T`, for what outlives the view: the job
    /// of `scope.io`, or the rest of a native after it allocates.
    pub fn share<T: Send + Sync + 'static>(self) -> Arc<T> {
        let value = self.owned_value::<T>();
        // SAFETY: the address is the raw form of an `Arc<T>` the object
        // holds, which stays the object's.
        unsafe {
            Arc::increment_strong_count(value);
            Arc::from_raw(value)
        }
    }

    /// The plain `T`, to read and to write in place. The borrow ends with
    /// the view, so before anything can move the object.
    pub fn plain<T: Copy + 'static>(self) -> &'a Cell<T> {
        let expected = Kinds::<T>::PLAIN;
        // SAFETY: a view is of a live object, which holds a `T` after its
        // kind when its header and its kind say so. A cell may be written
        // through a shared reference, and compiled code reads none of it.
        unsafe {
            if load(self.0) & TYPE_ID_MASK != TAG_PLAIN
                || !is_kind(load(self.0 + PLAIN_KIND), expected)
            {
                mismatched(self.0, expected, "plain");
            }
            &*((self.0 + PLAIN_VALUE) as *const Cell<T>)
        }
    }
}

impl<'s> Scope<'s> {
    /// A new object that owns `value`. The heap drops its `Arc` when the
    /// object is dead, at a collection, or when the program ends. May
    /// collect.
    pub fn alloc_owned<T: Send + Sync + 'static>(&mut self, value: Arc<T>) -> Root<'s, Opaque> {
        let value = Arc::into_raw(value) as usize;
        let object = self.mutator().allocate_owned(value, Kinds::<T>::OWNED);
        // SAFETY: just made.
        unsafe { self.root_raw(object) }
    }

    /// A new object holding `value` in place. May collect.
    pub fn alloc_plain<T: Copy + 'static>(&mut self, value: T) -> Root<'s, Opaque> {
        let count = Kinds::<T>::WORDS;
        let size = plain_size(count);
        let object = self.mutator().allocate_object(TAG_PLAIN, size);
        // SAFETY: the object has its count, its kind and `count` words for
        // the value, all zeroed first for the bytes `T` does not fill.
        unsafe {
            std::ptr::write_bytes((object + PLAIN_WORDS) as *mut u8, 0, size - PLAIN_WORDS);
            store(object + PLAIN_WORDS, count);
            store(
                object + PLAIN_KIND,
                std::ptr::from_ref(Kinds::<T>::PLAIN) as usize,
            );
            ((object + PLAIN_VALUE) as *mut T).write(value);
            self.root_raw(object)
        }
    }
}
