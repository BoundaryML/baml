//! `baml.Array`.
//!
//! Every native here is generic: an element is the slot the array holds it
//! in, moved without being looked at, tag and payload. What tag a value
//! passed on its own has is said by the type id of the call's type
//! argument.

use super::{
    Arr, ArrRef, Elem, NativeBamlArray, Natives, Root, Scope, Slot, Str, StrRef, Thrown, TypeId,
    View, array_size, string::text_size,
};
use crate::{
    array::FIRST_CAPACITY,
    heap::{Value, load, store},
    layout::{ARRAY_LENGTH, TAG_BOX_BOOL, TAG_BOX_FLOAT, TAG_BOX_INT, TAG_STRING},
    string::{INT_DIGITS, float_text, int_digits},
};

/// `index` as a position in a sequence of `length`, a negative one counting
/// from the end.
fn from_end(index: i64, length: usize) -> Option<usize> {
    let length = i64::try_from(length).ok()?;
    let position = if index < 0 {
        index.checked_add(length)?
    } else {
        index
    };
    usize::try_from(position).ok()
}

fn element_index(index: i64, length: usize) -> Option<usize> {
    from_end(index, length).filter(|&position| position < length)
}

fn insert_index(index: i64, length: usize) -> Option<usize> {
    from_end(index, length).filter(|&position| position <= length)
}

/// A slice bound: as an index, but one out of range is the nearest end.
fn slice_bound(bound: i64, length: usize) -> usize {
    let wide = i64::try_from(length).unwrap_or(i64::MAX);
    let resolved = if bound < 0 { bound + wide } else { bound };
    usize::try_from(resolved.clamp(0, wide)).unwrap_or(length)
}

/// Drop the elements from `length` on. The slots given up are made `null`:
/// the collector scans storage to its capacity.
fn truncate(array: ArrRef<'_, Slot>, length: usize) {
    // SAFETY: a view is of a live array, and no native runs beside this.
    unsafe {
        if length < array.len() {
            crate::array::clear(array.raw(), length, array.len());
            store(array.raw() + ARRAY_LENGTH, length);
        }
    }
}

/// Take the element at `index` out, closing the gap, and return its
/// payload.
fn remove(array: ArrRef<'_, Slot>, index: usize) -> usize {
    // SAFETY: as in `truncate`.
    let removed = unsafe {
        let removed = Value::load(crate::array::element(array.raw(), index));
        crate::array::shift(array.raw(), index + 1, index, array.len() - index - 1);
        removed
    };
    truncate(array, array.len() - 1);
    removed.payload
}

/// A new array holding `count` values of the type a call names by
/// `element`, each given by its index.
fn build<'s>(
    scope: &mut Scope<'s>,
    count: usize,
    element: TypeId,
    value: impl Fn(&super::Reservation<'_, 's>, usize) -> Value,
) -> Root<'s, Arr<Slot>> {
    let reservation = scope.reserve(array_size(count));
    let array = reservation.elements(count, element);
    for index in 0..count {
        array.set_value(index, value(&reservation, index));
    }
    let built = array.keep();
    reservation.finish(built)
}

/// Give `array` storage for `needed` elements if it has less. May collect.
fn ensure_capacity<'s>(scope: &mut Scope<'s>, array: Root<'s, Arr<Slot>>, needed: usize) {
    let length = array.get(scope).len();
    // SAFETY: the root is of a live array, and its slot is where the
    // collector keeps the array's address current.
    unsafe {
        if needed <= crate::array::capacity(array.raw(scope)) {
            return;
        }
        let capacity = needed.max(length * 2).max(FIRST_CAPACITY);
        crate::array::grow(scope.mutator(), array.0.as_ptr(), capacity);
    }
}

/// Replace elements `start..end` of `array` by `replacement`. The slots
/// the array gives up, when it gets shorter, are made `null`.
///
/// # Safety
///
/// `array` is a live array whose storage holds the new length, and
/// `start <= end <= ` its length.
unsafe fn replace(array: usize, start: usize, end: usize, replacement: &[Value]) {
    // SAFETY: the caller's contract: the storage holds the longer of the
    // array's two lengths.
    unsafe {
        let length = load(array + ARRAY_LENGTH);
        let after = length - (end - start) + replacement.len();
        crate::array::shift(array, end, start + replacement.len(), length - end);
        crate::array::put(array, start, replacement);
        crate::array::clear(array, after, length.max(after));
        store(array + ARRAY_LENGTH, after);
    }
}

/// Put `item` at `index` of `array`, moving what follows up by one, and
/// return the new length.
fn insert<'s>(
    scope: &mut Scope<'s>,
    array: Root<'s, Arr<Slot>>,
    index: usize,
    item: Elem<'s>,
) -> i64 {
    let length = array.get(scope).len();
    ensure_capacity(scope, array, length + 1);
    // SAFETY: the array has room for one more, and nothing allocates
    // between reading the item and storing it.
    unsafe { replace(array.raw(scope), index, index, &[item.value(scope)]) };
    i64::try_from(length + 1).unwrap_or(i64::MAX)
}

impl NativeBamlArray for Natives {
    fn at(_types: [TypeId; 1], array: ArrRef<'_, Slot>, index: i64) -> Option<usize> {
        element_index(index, array.len()).and_then(|index| array.get(index))
    }

    fn pop(_types: [TypeId; 1], array: ArrRef<'_, Slot>) -> Option<usize> {
        let last = array.len().checked_sub(1)?;
        Some(remove(array, last))
    }

    fn shift(_types: [TypeId; 1], array: ArrRef<'_, Slot>) -> Option<usize> {
        (!array.is_empty()).then(|| remove(array, 0))
    }

    fn remove_at(_types: [TypeId; 1], array: ArrRef<'_, Slot>, index: i64) -> Option<usize> {
        element_index(index, array.len()).map(|index| remove(array, index))
    }

    fn clear(_types: [TypeId; 1], array: ArrRef<'_, Slot>) {
        truncate(array, 0);
    }

    fn _truncate(_types: [TypeId; 1], array: ArrRef<'_, Slot>, length: i64) {
        truncate(array, usize::try_from(length).unwrap_or(0));
    }

    fn reverse<'s>(
        scope: &mut Scope<'s>,
        types: [TypeId; 1],
        array: Root<'s, Arr<Slot>>,
    ) -> Root<'s, Arr<Slot>> {
        let length = array.get(scope).len();
        build(scope, length, types[0], |reservation, index| {
            (reservation.get(array).value(length - 1 - index)).unwrap_or(Value::NULL)
        })
    }

    fn slice<'s>(
        scope: &mut Scope<'s>,
        types: [TypeId; 1],
        array: Root<'s, Arr<Slot>>,
        start: i64,
        end: i64,
    ) -> Root<'s, Arr<Slot>> {
        let length = array.get(scope).len();
        let start = slice_bound(start, length);
        let end = slice_bound(end, length).max(start);
        build(scope, end - start, types[0], |reservation, index| {
            (reservation.get(array).value(start + index)).unwrap_or(Value::NULL)
        })
    }

    fn concat<'s>(
        scope: &mut Scope<'s>,
        types: [TypeId; 1],
        array: Root<'s, Arr<Slot>>,
        other: Root<'s, Arr<Slot>>,
    ) -> Root<'s, Arr<Slot>> {
        let first = array.get(scope).len();
        let second = other.get(scope).len();
        build(scope, first + second, types[0], |reservation, index| {
            let value = if index < first {
                reservation.get(array).value(index)
            } else {
                reservation.get(other).value(index - first)
            };
            value.unwrap_or(Value::NULL)
        })
    }

    fn _copy<'s>(
        scope: &mut Scope<'s>,
        types: [TypeId; 1],
        array: Root<'s, Arr<Slot>>,
    ) -> Root<'s, Arr<Slot>> {
        let length = array.get(scope).len();
        build(scope, length, types[0], |reservation, index| {
            (reservation.get(array).value(index)).unwrap_or(Value::NULL)
        })
    }

    fn _results<'s>(
        scope: &mut Scope<'s>,
        types: [TypeId; 2],
        array: Root<'s, Arr<Slot>>,
    ) -> Root<'s, Arr<Slot>> {
        let length = array.get(scope).len();
        let reservation = scope.reserve(array_size(length));
        let results = reservation.elements(length, types[1]).keep();
        reservation.finish(results)
    }

    fn filled<'s>(
        scope: &mut Scope<'s>,
        types: [TypeId; 1],
        length: i64,
        value: Elem<'s>,
    ) -> Root<'s, Arr<Slot>> {
        let length = usize::try_from(length).unwrap_or(0);
        build(scope, length, types[0], |reservation, _| {
            value.value(&*reservation.scope)
        })
    }

    fn unshift<'s>(
        scope: &mut Scope<'s>,
        _types: [TypeId; 1],
        array: Root<'s, Arr<Slot>>,
        item: Elem<'s>,
    ) -> i64 {
        insert(scope, array, 0, item)
    }

    fn insert<'s>(
        scope: &mut Scope<'s>,
        _types: [TypeId; 1],
        array: Root<'s, Arr<Slot>>,
        item: Elem<'s>,
        idx: i64,
    ) -> Result<i64, Thrown<'s>> {
        let length = array.get(scope).len();
        match insert_index(idx, length) {
            Some(index) => Ok(insert(scope, array, index, item)),
            None => Err(scope.invalid_argument(&format!(
                "array.insert: idx ({idx}) is out of range for length {length}"
            ))),
        }
    }

    fn splice<'s>(
        scope: &mut Scope<'s>,
        _types: [TypeId; 1],
        array: Root<'s, Arr<Slot>>,
        start: i64,
        count: i64,
        replace: Root<'s, Arr<Slot>>,
    ) -> Result<(), Thrown<'s>> {
        let length = array.get(scope).len();
        let Some(start) = insert_index(start, length) else {
            return Err(scope.invalid_argument(&format!(
                "array.splice: start ({start}) is out of range for length {length}"
            )));
        };
        let Ok(count) = usize::try_from(count) else {
            return Err(
                scope.invalid_argument(&format!("array.splice: count ({count}) is negative"))
            );
        };
        let end = start.saturating_add(count).min(length);
        let added = replace.get(scope).len();
        ensure_capacity(scope, array, length - (end - start) + added);
        // Copied out first: the replacement may be the array itself.
        let replace = replace.get(scope);
        let replacement: Vec<Value> = (0..added)
            .filter_map(|index| replace.value(index))
            .collect();
        // SAFETY: the array has room, the bounds are in range, and nothing
        // allocates after the replacement is read.
        unsafe { self::replace(array.raw(scope), start, end, &replacement) };
        Ok(())
    }

    fn join<'s>(
        scope: &mut Scope<'s>,
        types: [TypeId; 1],
        array: Root<'s, Arr<Slot>>,
        separator: Root<'s, Str>,
    ) -> Root<'s, Str> {
        // Measure, reserve once, then write: the text is never anywhere but
        // in the string.
        let kind = types[0].0 as usize;
        let (view, by) = (array.get(scope), separator.get(scope));
        let gaps = view.len().saturating_sub(1);
        let mut bytes = gaps.saturating_mul(by.byte_len());
        let mut chars = gaps.saturating_mul(by.char_count());
        let mut digits = [0; INT_DIGITS];
        for word in view.iter() {
            let (more_bytes, more_chars) = match kind {
                TAG_STRING => {
                    // SAFETY: the compiler passes this id for an array of
                    // strings only.
                    let part = unsafe { StrRef::from_raw(word) };
                    (part.byte_len(), part.char_count())
                }
                TAG_BOX_INT => {
                    let length = int_digits(word.cast_signed() as i64, &mut digits).len();
                    (length, length)
                }
                TAG_BOX_FLOAT => {
                    let length = float_text(f64::from_bits(word as u64)).len();
                    (length, length)
                }
                TAG_BOX_BOOL => {
                    let length = if word == 0 { 5 } else { 4 };
                    (length, length)
                }
                other => unreachable!("join is compiled for no element type with id {other}"),
            };
            bytes = bytes.saturating_add(more_bytes);
            chars += more_chars;
        }
        let reservation = scope.reserve(text_size(bytes));
        let mut joined = reservation.new_str(bytes, chars);
        let by = reservation.get(separator).as_str();
        for (index, word) in reservation.get(array).iter().enumerate() {
            if index != 0 {
                joined.push(by);
            }
            match kind {
                // SAFETY: as above.
                TAG_STRING => joined.push(unsafe { StrRef::from_raw(word) }.as_str()),
                TAG_BOX_INT => joined.push(int_digits(word.cast_signed() as i64, &mut digits)),
                TAG_BOX_FLOAT => joined.push(&float_text(f64::from_bits(word as u64))),
                _ => joined.push(if word == 0 { "false" } else { "true" }),
            }
        }
        let joined = joined.finish().keep();
        reservation.finish(joined)
    }
}
