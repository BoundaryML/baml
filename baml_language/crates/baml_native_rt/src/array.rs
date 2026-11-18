//! Growable arrays. Compiled code reads the length, indexes and appends in
//! place inline; the runtime builds arrays and grows their storage.

use crate::{
    heap::{Mutator, current_frame, load, store},
    layout::{
        ARRAY_DATA, ARRAY_LENGTH, ARRAY_SIZE, STORAGE_CAPACITY, STORAGE_ELEMENTS,
        TYPE_ARRAY_OF_REFS, WORD,
    },
};

/// The smallest storage an array grows into.
const FIRST_CAPACITY: usize = 4;

/// A new array of `length` zeroed elements, of the array type `array_type`.
///
/// # Safety
///
/// Called from compiled code with its mutator; `array_type` is the id of an
/// array type.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baml_rt_array_new(
    mutator: &mut Mutator,
    array_type: usize,
    length: usize,
) -> usize {
    mutator.enter(current_frame());
    // One allocation for both objects, so that neither needs rooting while
    // the other is made. An empty array has no storage.
    let storage_size = if length == 0 {
        0
    } else {
        STORAGE_ELEMENTS + length * WORD
    };
    let array = mutator.allocate(ARRAY_SIZE + storage_size);
    // SAFETY: the allocation has room for the array and its storage.
    unsafe {
        store(array, array_type);
        store(array + ARRAY_LENGTH, length);
        if length == 0 {
            store(array + ARRAY_DATA, 0);
        } else {
            let storage = array + ARRAY_SIZE;
            let data = storage + STORAGE_ELEMENTS;
            store(storage, mutator.storage_type(array_type));
            store(storage + STORAGE_CAPACITY, length);
            std::ptr::write_bytes(data as *mut usize, 0, length);
            store(array + ARRAY_DATA, data);
        }
    }
    array
}

/// Append `value` to an array whose storage is full or missing.
///
/// # Safety
///
/// Called from compiled code with its mutator; `array` is a live array and
/// `value` an element of its type.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baml_rt_array_push_slow(
    mutator: &mut Mutator,
    mut array: usize,
    mut value: usize,
) {
    mutator.enter(current_frame());
    // SAFETY: `array` is a live array, kept current across the allocation.
    unsafe {
        let array_type = load(array) & crate::layout::TYPE_ID_MASK;
        let length = load(array + ARRAY_LENGTH);
        let capacity = (length * 2).max(FIRST_CAPACITY);

        mutator.root(&mut array);
        let holds_refs = array_type == TYPE_ARRAY_OF_REFS;
        if holds_refs {
            mutator.root(&mut value);
        }
        let storage = mutator.allocate_object(
            mutator.storage_type(array_type),
            STORAGE_ELEMENTS + capacity * WORD,
        );
        mutator.unroot(if holds_refs { 2 } else { 1 });

        store(storage + STORAGE_CAPACITY, capacity);
        let elements = (storage + STORAGE_ELEMENTS) as *mut usize;
        let old = load(array + ARRAY_DATA);
        if old != 0 {
            std::ptr::copy_nonoverlapping(old as *const usize, elements, length);
        }
        *elements.add(length) = value;
        std::ptr::write_bytes(elements.add(length + 1), 0, capacity - length - 1);
        store(array + ARRAY_DATA, elements as usize);
        store(array + ARRAY_LENGTH, length + 1);
    }
}
