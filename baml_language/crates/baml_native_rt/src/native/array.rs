//! `baml.Array`.
//!
//! Every native here is generic: an element is the slot the array holds it
//! in, moved without being looked at, tag and payload. A value passed on
//! its own is one too.

use super::{
    Arr, ArrRef, Elem, NativeBamlArray, Natives, Root, Scope, Slot, Str, StrRef, Thrown, TypeRef,
    View, array_size, string::text_size,
};
use crate::{
    array::FIRST_CAPACITY,
    heap::{Value, load, store},
    layout::{ARRAY_LENGTH, STRING_TAGS_END, TAG_BOOL, TAG_FLOAT, TAG_INT, TYPE_TAG},
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

/// Take the element at `index` out, closing the gap, and return it.
fn remove(array: ArrRef<'_, Slot>, index: usize) -> Value {
    // SAFETY: as in `truncate`.
    let removed = unsafe {
        let removed = Value::load(crate::array::element(array.raw(), index));
        crate::array::shift(array.raw(), index + 1, index, array.len() - index - 1);
        removed
    };
    truncate(array, array.len() - 1);
    removed
}

/// A new array holding `count` values, each given by its index.
fn build<'s>(
    scope: &mut Scope<'s>,
    count: usize,
    value: impl Fn(&super::Reservation<'_, 's>, usize) -> Value,
) -> Root<'s, Arr<Slot>> {
    let reservation = scope.reserve(array_size(count));
    let array = reservation.array::<Slot>(count);
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
        crate::array::grow(scope.mutator(), array.address(), capacity);
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

/// The text `join` writes for `element`, when it is not a string: an
/// `int`, a `float` or a `bool`, which its tag says.
fn scalar_text(element: Value, digits: &mut [u8; INT_DIGITS]) -> std::borrow::Cow<'_, str> {
    use std::borrow::Cow;
    match element.id() {
        TAG_INT => Cow::Borrowed(int_digits(element.payload.cast_signed() as i64, digits)),
        TAG_FLOAT => Cow::Owned(float_text(f64::from_bits(element.payload as u64))),
        TAG_BOOL => Cow::Borrowed(if element.payload == 0 {
            "false"
        } else {
            "true"
        }),
        other => unreachable!("join is compiled for no element with the tag {other}"),
    }
}

impl NativeBamlArray for Natives {
    fn at(array: ArrRef<'_, Slot>, index: i64) -> Option<Value> {
        element_index(index, array.len()).and_then(|index| array.get(index))
    }

    fn pop(array: ArrRef<'_, Slot>) -> Option<Value> {
        let last = array.len().checked_sub(1)?;
        Some(remove(array, last))
    }

    fn shift(array: ArrRef<'_, Slot>) -> Option<Value> {
        (!array.is_empty()).then(|| remove(array, 0))
    }

    fn remove_at(array: ArrRef<'_, Slot>, index: i64) -> Option<Value> {
        element_index(index, array.len()).map(|index| remove(array, index))
    }

    fn clear(array: ArrRef<'_, Slot>) {
        truncate(array, 0);
    }

    fn _truncate(array: ArrRef<'_, Slot>, length: i64) {
        truncate(array, usize::try_from(length).unwrap_or(0));
    }

    fn reverse<'s>(scope: &mut Scope<'s>, array: Root<'s, Arr<Slot>>) -> Root<'s, Arr<Slot>> {
        let length = array.get(scope).len();
        build(scope, length, |reservation, index| {
            (reservation.get(array).value(length - 1 - index)).unwrap_or(Value::NULL)
        })
    }

    fn slice<'s>(
        scope: &mut Scope<'s>,
        array: Root<'s, Arr<Slot>>,
        start: i64,
        end: i64,
    ) -> Root<'s, Arr<Slot>> {
        let length = array.get(scope).len();
        let start = slice_bound(start, length);
        let end = slice_bound(end, length).max(start);
        build(scope, end - start, |reservation, index| {
            (reservation.get(array).value(start + index)).unwrap_or(Value::NULL)
        })
    }

    fn concat<'s>(
        scope: &mut Scope<'s>,
        array: Root<'s, Arr<Slot>>,
        other: Root<'s, Arr<Slot>>,
    ) -> Root<'s, Arr<Slot>> {
        let first = array.get(scope).len();
        let second = other.get(scope).len();
        build(scope, first + second, |reservation, index| {
            let value = if index < first {
                reservation.get(array).value(index)
            } else {
                reservation.get(other).value(index - first)
            };
            value.unwrap_or(Value::NULL)
        })
    }

    fn _copy<'s>(scope: &mut Scope<'s>, array: Root<'s, Arr<Slot>>) -> Root<'s, Arr<Slot>> {
        let length = array.get(scope).len();
        build(scope, length, |reservation, index| {
            (reservation.get(array).value(index)).unwrap_or(Value::NULL)
        })
    }

    /// An array as long as `array` for elements of the second type the
    /// call names: the caller sets each before it reads any. An `int`, a
    /// `float` or a `bool` is zero, so that each slot has the tag compiled
    /// code leaves there when it stores such an element by its payload
    /// alone; anything else is `null`.
    fn _results<'s>(
        scope: &mut Scope<'s>,
        types: [TypeRef; 2],
        array: Root<'s, Arr<Slot>>,
    ) -> Root<'s, Arr<Slot>> {
        let length = array.get(scope).len();
        // SAFETY: a call names each type by its type object.
        let zero = match unsafe { load(types[1].0 + TYPE_TAG) } {
            tag @ (TAG_INT | TAG_FLOAT | TAG_BOOL) => Value::immediate(tag, 0),
            _ => Value::NULL,
        };
        build(scope, length, |_, _| zero)
    }

    fn filled<'s>(scope: &mut Scope<'s>, length: i64, value: Elem<'s>) -> Root<'s, Arr<Slot>> {
        let length = usize::try_from(length).unwrap_or(0);
        build(scope, length, |reservation, _| {
            value.value(&*reservation.scope)
        })
    }

    fn unshift<'s>(scope: &mut Scope<'s>, array: Root<'s, Arr<Slot>>, item: Elem<'s>) -> i64 {
        insert(scope, array, 0, item)
    }

    fn insert<'s>(
        scope: &mut Scope<'s>,
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
        array: Root<'s, Arr<Slot>>,
        separator: Root<'s, Str>,
    ) -> Root<'s, Str> {
        // Measure, reserve once, then write: the text is never anywhere but
        // in the string. What an element is its tag says: a string, an
        // `int`, a `float` or a `bool`.
        let is_string = |element: Value| element.is_pointer() && element.id() < STRING_TAGS_END;
        let (view, by) = (array.get(scope), separator.get(scope));
        let gaps = view.len().saturating_sub(1);
        let mut bytes = gaps.saturating_mul(by.byte_len());
        let mut chars = gaps.saturating_mul(by.char_count());
        let mut digits = [0; INT_DIGITS];
        for element in view.iter() {
            let (more_bytes, more_chars) = if is_string(element) {
                // SAFETY: the tag says it is a live string.
                let part = unsafe { StrRef::from_raw(element.payload) };
                (part.byte_len(), part.char_count())
            } else {
                let length = scalar_text(element, &mut digits).len();
                (length, length)
            };
            bytes = bytes.saturating_add(more_bytes);
            chars += more_chars;
        }
        let reservation = scope.reserve(text_size(bytes));
        let mut joined = reservation.new_str(bytes, chars);
        let by = reservation.get(separator).as_str();
        for (index, element) in reservation.get(array).iter().enumerate() {
            if index != 0 {
                joined.push(by);
            }
            if is_string(element) {
                // SAFETY: as above.
                joined.push(unsafe { StrRef::from_raw(element.payload) }.as_str());
            } else {
                joined.push(&scalar_text(element, &mut digits));
            }
        }
        let joined = joined.finish().keep();
        reservation.finish(joined)
    }
}
