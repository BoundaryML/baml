//! `T[]` operations shared by interpreted and compiled BAML.
//!
//! A BAML array is a reference-semantics heap object: [`Shared<Vec<T>>`].
//! Subscripts follow the JavaScript/Python convention — a negative index counts
//! back from the end, so `-1` is the last element — and an index still outside
//! the array after that raises `baml.panics.IndexOutOfBounds` carrying the
//! index *as written* and the array's length. Mirrors `bex_vm`'s
//! `array_index::resolve_index` and the `LoadArrayElement` /
//! `StoreArrayElement` opcodes.

use std::cmp::Ordering;

use crate::{
    Int63, Panic, Str, Thrown, float,
    handle::{Shared, shared},
    int_from_usize,
};

/// Allocate an array from its elements (`[a, b, c]`).
#[inline]
pub fn new<T>(items: Vec<T>) -> Shared<Vec<T>> {
    shared(items)
}

/// `arr.length()`.
#[inline]
pub fn len<T>(arr: &Shared<Vec<T>>) -> Int63 {
    int_from_usize(arr.borrow().len())
}

/// `arr[index]`, copying the element out. A negative index counts from the
/// end; out of range raises `IndexOutOfBounds { index, length }`.
#[inline]
pub fn get<T: Clone>(arr: &Shared<Vec<T>>, index: Int63) -> Result<T, Thrown> {
    let items = arr.borrow();
    match resolve_index(index.get(), items.len()) {
        Some(offset) => Ok(items[offset].clone()),
        None => Err(index_out_of_bounds(index, items.len())),
    }
}

/// `arr[index] = value`. Same index rules as [`get`]; assignment never grows
/// the array.
#[inline]
pub fn set<T>(arr: &Shared<Vec<T>>, index: Int63, value: T) -> Result<(), Thrown> {
    let mut items = arr.borrow_mut();
    match resolve_index(index.get(), items.len()) {
        Some(offset) => {
            items[offset] = value;
            Ok(())
        }
        None => Err(index_out_of_bounds(index, items.len())),
    }
}

/// `arr.push(value)`.
#[inline]
pub fn push<T>(arr: &Shared<Vec<T>>, value: T) {
    arr.borrow_mut().push(value);
}

/// `int[].sort()` in place, ascending. Stable, like the VM's `_rust_sort`.
pub fn sort_int(arr: &Shared<Vec<Int63>>) {
    arr.borrow_mut().sort_by(Int63::cmp);
}

/// `float[].sort()` in place in BAML's total float order ([`float::cmp`]):
/// NaN last. Stable, so `-0.0` and `0.0` keep their relative order.
pub fn sort_float(arr: &Shared<Vec<f64>>) {
    arr.borrow_mut().sort_by(|a, b| float::cmp(*a, *b));
}

/// `string[].sort()` in place in UTF-8 byte order. Stable.
pub fn sort_str(arr: &Shared<Vec<Str>>) {
    arr.borrow_mut().sort_by(Str::cmp);
}

/// `arr.sort_by(cmp)` for any element type, stable, with a fallible
/// comparator (a BAML lambda can throw).
pub fn sort_by<T, F>(arr: &Shared<Vec<T>>, mut compare: F) -> Result<(), Thrown>
where
    F: FnMut(&T, &T) -> Result<Ordering, Thrown>,
{
    let mut error = None;
    arr.borrow_mut().sort_by(|a, b| {
        if error.is_some() {
            return Ordering::Equal;
        }
        compare(a, b).unwrap_or_else(|thrown| {
            error = Some(thrown);
            Ordering::Equal
        })
    });
    error.map_or(Ok(()), Err)
}

/// The state of a `for (x in arr)` loop: the VM's `ArrayIterator<T> { arr,
/// idx }`. Holds the array by reference and re-reads its length on every
/// step, so elements pushed during iteration are visited and elements removed
/// are not.
#[derive(Debug)]
pub struct Iter<T> {
    arr: Shared<Vec<T>>,
    idx: usize,
}

impl<T> Clone for Iter<T> {
    fn clone(&self) -> Self {
        Self {
            arr: self.arr.clone(),
            idx: self.idx,
        }
    }
}

/// `arr.iter()`: a cursor at the first element.
#[inline]
pub fn iter<T>(arr: &Shared<Vec<T>>) -> Iter<T> {
    Iter {
        arr: arr.clone(),
        idx: 0,
    }
}

/// `iterator.next()`: `Some(element)` or `None` for the VM's `Done {}`.
#[inline]
pub fn next<T: Clone>(it: &mut Iter<T>) -> Option<T> {
    let items = it.arr.borrow();
    let element = items.get(it.idx)?.clone();
    it.idx += 1;
    Some(element)
}

impl<T: Clone> Iterator for Iter<T> {
    type Item = T;

    #[inline]
    fn next(&mut self) -> Option<T> {
        next(self)
    }
}

/// Resolve a possibly-negative element index against a sequence of length
/// `len`, counting negatives back from the end. `Some(offset)` when the index
/// lands in `[0, len)`; `None` when it falls outside even after counting from
/// the end.
#[inline]
pub fn resolve_index(index: i64, len: usize) -> Option<usize> {
    // `len` is a `Vec` length, so it fits in `i64`; a negative index moves
    // toward zero when `len` is added, so the sum cannot overflow.
    let len_i64 = i64::try_from(len).ok()?;
    let resolved = if index < 0 { index + len_i64 } else { index };
    usize::try_from(resolved).ok().filter(|&i| i < len)
}

#[cold]
fn index_out_of_bounds(index: Int63, len: usize) -> Thrown {
    Thrown::Panic(Panic::IndexOutOfBounds {
        index,
        length: int_from_usize(len),
    })
}

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
    use super::*;
    use crate::string::from_literal;

    fn int(value: i64) -> Int63 {
        Int63::new(value).unwrap()
    }

    fn ints(values: &[i64]) -> Shared<Vec<Int63>> {
        new(values.iter().copied().map(int).collect())
    }

    fn rendered_panic<T: std::fmt::Debug>(result: Result<T, Thrown>) -> String {
        match result {
            Err(Thrown::Panic(panic)) => panic.render_readable(),
            other => panic!("expected a panic, got {other:?}"),
        }
    }

    #[test]
    fn resolve_index_matches_the_vm() {
        assert_eq!(resolve_index(0, 3), Some(0));
        assert_eq!(resolve_index(2, 3), Some(2));
        assert_eq!(resolve_index(-1, 3), Some(2));
        assert_eq!(resolve_index(-3, 3), Some(0));
        assert_eq!(resolve_index(3, 3), None);
        assert_eq!(resolve_index(-4, 3), None);
        assert_eq!(resolve_index(0, 0), None);
        assert_eq!(resolve_index(-1, 0), None);
        assert_eq!(resolve_index(i64::MAX, 3), None);
        assert_eq!(resolve_index(i64::MIN, 3), None);
    }

    #[test]
    fn get_and_set_count_negative_indexes_from_the_end() {
        let arr = ints(&[10, 20, 30]);
        assert_eq!(len(&arr), int(3));
        assert_eq!(get(&arr, int(0)).unwrap(), int(10));
        assert_eq!(get(&arr, int(-1)).unwrap(), int(30));
        assert_eq!(get(&arr, int(-3)).unwrap(), int(10));
        set(&arr, int(-1), int(31)).unwrap();
        set(&arr, int(1), int(21)).unwrap();
        assert_eq!(*arr.borrow(), vec![int(10), int(21), int(31)]);
        push(&arr, int(40));
        assert_eq!(len(&arr), int(4));
        assert_eq!(get(&arr, int(3)).unwrap(), int(40));
    }

    #[test]
    fn out_of_range_panics_report_the_written_index_and_length() {
        let arr = ints(&[1, 2, 3]);
        assert_eq!(
            rendered_panic(get(&arr, int(3))),
            "baml.panics.IndexOutOfBounds {index: 3, length: 3}"
        );
        assert_eq!(
            rendered_panic(get(&arr, int(-4))),
            "baml.panics.IndexOutOfBounds {index: -4, length: 3}"
        );
        assert_eq!(
            rendered_panic(set(&arr, int(5), int(0))),
            "baml.panics.IndexOutOfBounds {index: 5, length: 3}"
        );
        assert_eq!(
            rendered_panic(get(&arr, Int63::MIN)),
            "baml.panics.IndexOutOfBounds {index: -4611686018427387904, length: 3}"
        );
        let empty: Shared<Vec<Int63>> = new(Vec::new());
        assert_eq!(
            rendered_panic(get(&empty, int(0))),
            "baml.panics.IndexOutOfBounds {index: 0, length: 0}"
        );
        assert_eq!(
            get(&empty, int(-1)).unwrap_err().to_string(),
            "index out of bounds: -1 of 0"
        );
        // A failed store leaves the array untouched.
        assert_eq!(*arr.borrow(), vec![int(1), int(2), int(3)]);
    }

    #[test]
    fn sorts_are_stable_and_use_the_language_orders() {
        let i = ints(&[3, -1, 2, 0]);
        sort_int(&i);
        assert_eq!(*i.borrow(), vec![int(-1), int(0), int(2), int(3)]);

        let f = new(vec![2.0, f64::NAN, -0.0, f64::NEG_INFINITY, 0.0, 1.0]);
        sort_float(&f);
        let sorted = f.borrow();
        assert_eq!(sorted[0], f64::NEG_INFINITY);
        // `-0.0` and `0.0` are equal, so the stable sort keeps `-0.0` first.
        assert!(sorted[1].is_sign_negative() && sorted[1] == 0.0);
        assert!(sorted[2].is_sign_positive() && sorted[2] == 0.0);
        assert_eq!(sorted[3], 1.0);
        assert_eq!(sorted[4], 2.0);
        assert!(sorted[5].is_nan());
        drop(sorted);

        let s = new(vec![
            from_literal("b"),
            from_literal("é"),
            from_literal("a"),
            from_literal("Z"),
            from_literal(""),
        ]);
        sort_str(&s);
        let names: Vec<String> = s.borrow().iter().map(Str::to_string).collect();
        assert_eq!(names, ["", "Z", "a", "b", "é"]);
    }

    #[test]
    fn sort_by_propagates_the_first_thrown_error() {
        let arr = ints(&[2, 1, 3]);
        sort_by(&arr, |a, b| Ok(b.cmp(a))).unwrap();
        assert_eq!(*arr.borrow(), vec![int(3), int(2), int(1)]);
        let failed = sort_by(&arr, |_, _| Err(Thrown::Panic(Panic::AssertionFailed)));
        assert!(matches!(failed, Err(Thrown::Panic(Panic::AssertionFailed))));
    }

    #[test]
    fn iteration_observes_pushes_and_shares_the_array() {
        let arr = ints(&[1, 2]);
        let mut it = iter(&arr);
        let mut seen = Vec::new();
        while let Some(x) = next(&mut it) {
            seen.push(x);
            if x == int(1) {
                push(&arr, int(3));
            }
        }
        assert_eq!(seen, vec![int(1), int(2), int(3)]);
        assert_eq!(next(&mut it), None);
        // Pushing after `Done` resumes the cursor, as the VM's `idx` field does.
        push(&arr, int(4));
        assert_eq!(next(&mut it), Some(int(4)));

        let total: i64 = iter(&arr).map(Int63::get).sum();
        assert_eq!(total, 10);
        let empty: Shared<Vec<Int63>> = new(Vec::new());
        assert_eq!(iter(&empty).next(), None);
    }
}
