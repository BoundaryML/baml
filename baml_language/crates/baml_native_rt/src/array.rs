//! Growable arrays. Compiled code reads the length, indexes and appends in
//! place inline; the runtime builds arrays and grows their storage.

use crate::{
    heap::{Mutator, load, store},
    layout::{
        ARRAY_DATA, ARRAY_LENGTH, ARRAY_SIZE, STORAGE_CAPACITY, STORAGE_ELEMENTS,
        TYPE_ARRAY_OF_REFS, WORD,
    },
};

/// The smallest storage an array grows into.
const FIRST_CAPACITY: usize = 4;

crate::stubs::anchored! {
    /// The symbol of [`array_new`].
    baml_rt_array_new => array_new
}

/// A new array of `length` zeroed elements, of the array type `array_type`.
///
/// # Safety
///
/// Called from compiled code with its mutator; `array_type` is the id of an
/// array type.
pub(crate) unsafe extern "C" fn array_new(
    mutator: &mut Mutator,
    array_type: usize,
    length: usize,
) -> usize {
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

crate::stubs::anchored! {
    /// The symbol of [`array_push_slow`].
    baml_rt_array_push_slow => array_push_slow
}

/// Append `value` to an array whose storage is full or missing.
///
/// # Safety
///
/// Called from compiled code with its mutator; `array` is a live array and
/// `value` an element of its type.
pub(crate) unsafe extern "C" fn array_push_slow(
    mutator: &mut Mutator,
    mut array: usize,
    mut value: usize,
) {
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

/// Sort the elements of `array` in place by their natural order, as the
/// standard library's `sort` does for the element types it orders in Rust:
/// `kind` is one of [`crate::layout::SORT_INTS`] and the kinds after it. The
/// sort is stable. A float orders as the language's total order has it:
/// every NaN above every number, equal to each other. Allocates nothing on
/// the heap.
///
/// # Safety
///
/// `array` is a live array of elements of that kind.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baml_rt_array_sort(array: usize, kind: usize) {
    use crate::layout::{SORT_BIGINTS, SORT_FLOATS, SORT_INTS, SORT_STRINGS};
    // SAFETY: the caller's contract: the array's storage holds its length
    // in elements, and nothing else reads it meanwhile.
    let elements = unsafe {
        std::slice::from_raw_parts_mut(
            load(array + ARRAY_DATA) as *mut usize,
            load(array + ARRAY_LENGTH),
        )
    };
    match kind {
        SORT_INTS => elements.sort_by_key(|word| word.cast_signed()),
        SORT_FLOATS => elements.sort_by(|left, right| {
            let (left, right) = (f64::from_bits(*left as u64), f64::from_bits(*right as u64));
            left.partial_cmp(&right)
                .unwrap_or_else(|| left.is_nan().cmp(&right.is_nan()))
        }),
        // SAFETY: the caller's contract: each element is a live string.
        SORT_STRINGS => elements.sort_by(|left, right| unsafe {
            crate::string::text(*left).cmp(crate::string::text(*right))
        }),
        // SAFETY: the caller's contract: each element is a live `bigint`.
        SORT_BIGINTS => elements.sort_by_cached_key(|word| unsafe { crate::bigint::value(*word) }),
        _ => {}
    }
}
