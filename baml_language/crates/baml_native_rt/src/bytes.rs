//! `uint8array`: an array of bytes whose length never changes, laid out by
//! [`crate::layout::BYTES_LENGTH`]. Compiled code reads the length and
//! reads and writes a byte in place inline; the runtime makes the arrays
//! and compares them.

use crate::{
    heap::{Mutator, store},
    layout::{
        BYTES_DATA, BYTES_LENGTH, BYTES_MAX, BYTES_SIZE, STORAGE_CAPACITY, STORAGE_ELEMENTS,
        TYPE_BYTES, TYPE_VALUES, WORD, words,
    },
    native::{BytesRef, View},
};

/// The bytes a `uint8array` of `length` bytes takes when it is made,
/// storage included.
pub(crate) const fn size(length: usize) -> usize {
    if length == 0 {
        BYTES_SIZE
    } else {
        BYTES_SIZE + STORAGE_ELEMENTS + words(length)
    }
}

/// Lay out a `uint8array` of `length` bytes at `object`, and return the
/// address of its first byte. The bytes are left to be written, but for
/// those of the last word, which are zero.
///
/// # Safety
///
/// `object` is [`size`] of `length` fresh bytes.
pub(crate) unsafe fn init(object: usize, length: usize) -> *mut u8 {
    assert!(
        length <= BYTES_MAX,
        "a uint8array cannot be longer than 4 GiB"
    );
    // SAFETY: the caller's contract.
    unsafe {
        store(object, TYPE_BYTES);
        store(object + BYTES_LENGTH, length);
        if length == 0 {
            store(object + BYTES_DATA, 0);
            return std::ptr::null_mut();
        }
        let storage = object + BYTES_SIZE;
        let data = storage + STORAGE_ELEMENTS;
        store(storage, TYPE_VALUES);
        // What the collector sizes the storage by, as it does any array's.
        store(storage + STORAGE_CAPACITY, words(length) / WORD);
        store(data + words(length) - WORD, 0);
        store(object + BYTES_DATA, data);
        data as *mut u8
    }
}

/// A new `uint8array` holding a copy of `bytes`. May collect.
pub(crate) fn make(mutator: &mut Mutator, bytes: &[u8]) -> usize {
    // One allocation for both objects, so that neither needs rooting while
    // the other is made.
    let object = mutator.allocate(size(bytes.len()));
    // SAFETY: the allocation has room for the array and its storage.
    unsafe {
        let data = init(object, bytes.len());
        if !bytes.is_empty() {
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), data, bytes.len());
        }
    }
    object
}

/// A new `uint8array` of `length` zero bytes. May collect.
pub(crate) fn zeroed(mutator: &mut Mutator, length: usize) -> usize {
    let object = mutator.allocate(size(length));
    // SAFETY: the allocation has room for the array and its storage.
    unsafe {
        let data = init(object, length);
        if length != 0 {
            std::ptr::write_bytes(data, 0, length);
        }
    }
    object
}

crate::stubs::anchored! {
    /// The symbol of [`bytes_new`].
    baml_rt_bytes_new => bytes_new
}

/// A new `uint8array` holding the `length` bytes at `data`: what a byte
/// string literal evaluates to, each time anew, since the array can be
/// written.
///
/// # Safety
///
/// Called from compiled code with its mutator; `data` is the address of
/// `length` bytes outside the heap.
pub(crate) unsafe extern "C" fn bytes_new(
    mutator: &mut Mutator,
    data: *const u8,
    length: usize,
) -> usize {
    // SAFETY: the caller's contract.
    make(mutator, unsafe { std::slice::from_raw_parts(data, length) })
}

/// Whether two `uint8array`s hold the same bytes.
///
/// # Safety
///
/// Both are live `uint8array`s.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baml_rt_bytes_eq(left: usize, right: usize) -> u8 {
    // SAFETY: the caller's contract, and nothing allocates meanwhile.
    let (left, right) = unsafe { (BytesRef::from_raw(left), BytesRef::from_raw(right)) };
    u8::from(left.as_slice() == right.as_slice())
}
