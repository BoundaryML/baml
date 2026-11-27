//! Growable arrays. Compiled code reads the length, indexes and appends in
//! place inline; the runtime builds arrays and grows their storage.

use crate::{
    heap::{Mutator, Value, load, store},
    layout::{
        ARRAY_LENGTH, ARRAY_PTR, ARRAY_SIZE, ARRAY_TY, STORAGE_CAPACITY, STORAGE_ELEMENTS,
        TAG_ARRAY, TAG_ARRAY_STORAGE, VALUE, storage_size,
    },
};

/// The smallest storage an array grows into.
pub(crate) const FIRST_CAPACITY: usize = 4;

/// The bytes an array of `length` elements takes when it is made, storage
/// included.
pub(crate) const fn size(length: usize) -> usize {
    if length == 0 {
        ARRAY_SIZE
    } else {
        ARRAY_SIZE + storage_size(length)
    }
}

/// Lay out an array of `length` elements, each `null`, at `array`, with
/// its storage after it.
///
/// # Safety
///
/// `array` is [`size`] of `length` fresh bytes.
pub(crate) unsafe fn format(array: usize, length: usize) {
    // SAFETY: the caller's contract.
    unsafe {
        store(array, TAG_ARRAY);
        store(array + ARRAY_LENGTH, length);
        store(array + ARRAY_TY, 0);
        if length == 0 {
            store(array + ARRAY_PTR, 0);
        } else {
            let storage = array + ARRAY_SIZE;
            format_storage(storage, length, 0);
            store(array + ARRAY_PTR, storage);
        }
    }
}

/// Lay out storage for `capacity` elements at `storage`, those from
/// `used` on `null`.
///
/// # Safety
///
/// `storage` is `storage_size(capacity)` fresh bytes.
pub(crate) unsafe fn format_storage(storage: usize, capacity: usize, used: usize) {
    // SAFETY: the caller's contract.
    unsafe {
        store(storage, TAG_ARRAY_STORAGE);
        store(storage + STORAGE_CAPACITY, capacity);
        let elements = (storage + STORAGE_ELEMENTS) as *mut Value;
        std::ptr::write_bytes(elements.add(used), 0, capacity - used);
    }
}

/// How many elements the storage of the array at `array` has room for.
///
/// # Safety
///
/// `array` is a live array.
pub(crate) unsafe fn capacity(array: usize) -> usize {
    // SAFETY: the caller's contract.
    unsafe {
        match load(array + ARRAY_PTR) {
            0 => 0,
            storage => load(storage + STORAGE_CAPACITY),
        }
    }
}

/// The address of element `index` of the array at `array`: a slot.
///
/// # Safety
///
/// `array` is a live array with storage for that element.
pub(crate) unsafe fn element(array: usize, index: usize) -> usize {
    // SAFETY: the caller's contract.
    unsafe { load(array + ARRAY_PTR) + STORAGE_ELEMENTS + index * VALUE }
}

/// A copy of the elements of the array at `array`.
///
/// # Safety
///
/// `array` is a live array.
pub(crate) unsafe fn values(array: usize) -> Vec<Value> {
    // SAFETY: an array has storage holding its length of elements.
    unsafe {
        (0..load(array + ARRAY_LENGTH))
            .map(|index| Value::load(element(array, index)))
            .collect()
    }
}

/// Write `values` over the elements of the array at `array` from `at` on.
///
/// # Safety
///
/// `array` is a live array with storage for those elements, each of which
/// is a value of its element type.
pub(crate) unsafe fn put(array: usize, at: usize, values: &[Value]) {
    for (index, value) in values.iter().enumerate() {
        // SAFETY: the caller's contract.
        unsafe { Value::store(element(array, at + index), *value) };
    }
}

/// Give the elements of the array at `array`, which nothing else can
/// reach yet, the values `values`, from the first on.
///
/// # Safety
///
/// `array` is an array just laid out with storage for that many elements.
pub(crate) unsafe fn fill(array: usize, values: impl Iterator<Item = Value>) {
    for (index, value) in values.enumerate() {
        // SAFETY: the caller's contract.
        unsafe { Value::init(element(array, index), value) };
    }
}

/// Copy the `count` elements from `from` on of the array at `array` to
/// `to` on. The two runs may overlap.
///
/// # Safety
///
/// `array` is a live array with storage for both runs.
pub(crate) unsafe fn shift(array: usize, from: usize, to: usize, count: usize) {
    let copy = |index: usize| {
        // SAFETY: the caller's contract.
        unsafe {
            Value::store(
                element(array, to + index),
                Value::load(element(array, from + index)),
            );
        }
    };
    // Whichever end is not yet overwritten first.
    if to <= from {
        (0..count).for_each(copy);
    } else {
        (0..count).rev().for_each(copy);
    }
}

/// Make elements `from..to` of the array at `array` `null`: what an array
/// gives up it clears, since the collector scans storage to its capacity.
///
/// # Safety
///
/// `array` is a live array with storage for those elements.
pub(crate) unsafe fn clear(array: usize, from: usize, to: usize) {
    for index in from..to {
        // SAFETY: the caller's contract.
        unsafe { Value::store(element(array, index), Value::NULL) };
    }
}

/// Give the array in the root `array` storage for `capacity` elements, a
/// number no less than its length. May collect.
///
/// # Safety
///
/// `array` is a root holding a live array.
pub(crate) unsafe fn grow(mutator: &mut Mutator, array: *const usize, capacity: usize) {
    let storage = mutator.allocate(storage_size(capacity));
    // SAFETY: the caller's contract: the root is read after the
    // allocation, which can move the array.
    unsafe {
        let array = array.read_volatile();
        let length = load(array + ARRAY_LENGTH);
        format_storage(storage, capacity, length);
        for index in 0..length {
            Value::init(
                storage + STORAGE_ELEMENTS + index * VALUE,
                Value::load(element(array, index)),
            );
        }
        store(array + ARRAY_PTR, storage);
    }
}

crate::stubs::anchored! {
    /// The symbol of [`array_new`].
    baml_rt_array_new => array_new
}

/// A new array of `length` elements, each `null`.
///
/// # Safety
///
/// Called from compiled code with its mutator.
pub(crate) unsafe extern "C" fn array_new(mutator: &mut Mutator, length: usize) -> usize {
    // One allocation for both objects, so that neither needs rooting while
    // the other is made. An empty array has no storage.
    let array = mutator.allocate(size(length));
    // SAFETY: the allocation has room for the array and its storage.
    unsafe { format(array, length) };
    array
}

crate::stubs::anchored! {
    /// The symbol of [`array_push_slow`].
    baml_rt_array_push_slow => array_push_slow
}

/// Append the value of tag `tag` and payload `payload` to an array whose
/// storage is full or missing.
///
/// # Safety
///
/// Called from compiled code with its mutator; `array` is a live array and
/// the two words a value of its element type.
pub(crate) unsafe extern "C" fn array_push_slow(
    mutator: &mut Mutator,
    mut array: usize,
    tag: usize,
    payload: usize,
) {
    let mut value = Value { tag, payload };
    // SAFETY: `array` is a live array, kept current across the allocation,
    // as is what the value points at.
    unsafe {
        let length = load(array + ARRAY_LENGTH);
        mutator.root(&mut array);
        if value.is_pointer() {
            mutator.root(&mut value.payload);
        }
        grow(mutator, &raw const array, (length * 2).max(FIRST_CAPACITY));
        mutator.unroot(if value.is_pointer() { 2 } else { 1 });
        let payload = (&raw const value.payload).read_volatile();
        Value::store(element(array, length), value.kept(payload));
        store(array + ARRAY_LENGTH, length + 1);
    }
}

/// Sort the elements of `array` in place by their natural order, as the
/// standard library's `sort` does for the element types it orders in Rust:
/// `kind` is one of [`crate::layout::SORT_INTS`] and the kinds after it. The
/// sort is stable. A float orders as the language's total order has it:
/// every NaN above every number, equal to each other. Allocates nothing on
/// the heap: the elements are sorted in a copy and stored back.
///
/// # Safety
///
/// `array` is a live array of elements of that kind.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baml_rt_array_sort(array: usize, kind: usize) {
    use crate::layout::{SORT_BIGINTS, SORT_FLOATS, SORT_INTS, SORT_STRINGS};
    // SAFETY: the caller's contract: the array's storage holds its length
    // in elements.
    let mut elements = unsafe { values(array) };
    match kind {
        SORT_INTS => elements.sort_by_key(|element| element.payload.cast_signed()),
        SORT_FLOATS => elements.sort_by(|left, right| {
            let (left, right) = (
                f64::from_bits(left.payload as u64),
                f64::from_bits(right.payload as u64),
            );
            left.partial_cmp(&right)
                .unwrap_or_else(|| left.is_nan().cmp(&right.is_nan()))
        }),
        // SAFETY: the caller's contract: each element is a live string.
        SORT_STRINGS => elements.sort_by(|left, right| unsafe {
            crate::string::text(left.payload).cmp(crate::string::text(right.payload))
        }),
        // SAFETY: the caller's contract: each element is a live `bigint`.
        SORT_BIGINTS => {
            elements.sort_by_cached_key(|element| unsafe { crate::bigint::value(element.payload) });
        }
        _ => return,
    }
    // SAFETY: the array has the elements just read.
    unsafe { put(array, 0, &elements) };
}
