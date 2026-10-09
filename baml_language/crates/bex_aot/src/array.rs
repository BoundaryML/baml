//! `T[]` operations for compiled BAML.
//!
//! A BAML array is a reference-semantics heap object: [`Shared<Vec<T>>`].
//! Subscripts follow the JavaScript/Python convention — a negative index counts
//! back from the end, so `-1` is the last element — and an index still outside
//! the array after that raises `baml.panics.IndexOutOfBounds` carrying the
//! index *as written* and the array's length, resolved by the same
//! [`bex_lang::index`] the VM's `LoadArrayElement` / `StoreArrayElement` use.

use bex_lang::index::resolve_index;

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

// ── Methods that call back into a function value ─────────────────────────────
//
// Every one of these walks a snapshot of the array taken before the first
// callback, as the VM's continuations do (`package_baml/array.rs` copies the
// elements into the continuation at dispatch), so a callback that pushes to or
// removes from the array is not observed by the walk, and no `RefCell` borrow
// is held while a callback runs. A callback's throw unwinds the method.

/// `arr.map(f)`: every result, in order.
pub fn map<T: Clone, U>(
    arr: &Shared<Vec<T>>,
    f: &dyn Fn(T) -> Result<U, Thrown>,
) -> Result<Shared<Vec<U>>, Thrown> {
    let items = snapshot(arr);
    let mut out = Vec::with_capacity(items.len());
    for item in items {
        out.push(f(item)?);
    }
    Ok(new(out))
}

/// `arr.filter(predicate)`: the elements the predicate accepts, in order.
pub fn filter<T: Clone>(
    arr: &Shared<Vec<T>>,
    predicate: &dyn Fn(T) -> Result<bool, Thrown>,
) -> Result<Shared<Vec<T>>, Thrown> {
    let items = snapshot(arr);
    let mut out = Vec::with_capacity(items.len());
    for item in items {
        if predicate(item.clone())? {
            out.push(item);
        }
    }
    Ok(new(out))
}

/// `arr.filter_map(f)`: the non-`null` results, in order.
pub fn filter_map<T: Clone, U>(
    arr: &Shared<Vec<T>>,
    f: &dyn Fn(T) -> Result<Option<U>, Thrown>,
) -> Result<Shared<Vec<U>>, Thrown> {
    let items = snapshot(arr);
    let mut out = Vec::new();
    for item in items {
        if let Some(value) = f(item)? {
            out.push(value);
        }
    }
    Ok(new(out))
}

/// `arr.for_each(f)`: `f` on every element, left to right.
pub fn for_each<T: Clone>(
    arr: &Shared<Vec<T>>,
    f: &dyn Fn(T) -> Result<(), Thrown>,
) -> Result<(), Thrown> {
    for item in snapshot(arr) {
        f(item)?;
    }
    Ok(())
}

/// `arr.some(predicate)`: whether any element is accepted; stops at the
/// first. `false` on an empty array.
pub fn some<T: Clone>(
    arr: &Shared<Vec<T>>,
    predicate: &dyn Fn(T) -> Result<bool, Thrown>,
) -> Result<bool, Thrown> {
    for item in snapshot(arr) {
        if predicate(item)? {
            return Ok(true);
        }
    }
    Ok(false)
}

/// `arr.every(predicate)`: whether every element is accepted; stops at the
/// first rejected. `true` on an empty array.
pub fn every<T: Clone>(
    arr: &Shared<Vec<T>>,
    predicate: &dyn Fn(T) -> Result<bool, Thrown>,
) -> Result<bool, Thrown> {
    for item in snapshot(arr) {
        if !predicate(item)? {
            return Ok(false);
        }
    }
    Ok(true)
}

/// `arr.find(predicate)`: the first element accepted, or `null`.
pub fn find<T: Clone>(
    arr: &Shared<Vec<T>>,
    predicate: &dyn Fn(T) -> Result<bool, Thrown>,
) -> Result<Option<T>, Thrown> {
    for item in snapshot(arr) {
        if predicate(item.clone())? {
            return Ok(Some(item));
        }
    }
    Ok(None)
}

/// `arr.find_index(predicate)`: the index of the first element accepted,
/// or `null`.
pub fn find_index<T: Clone>(
    arr: &Shared<Vec<T>>,
    predicate: &dyn Fn(T) -> Result<bool, Thrown>,
) -> Result<Option<Int63>, Thrown> {
    for (index, item) in snapshot(arr).into_iter().enumerate() {
        if predicate(item)? {
            return Ok(Some(int_from_usize(index)));
        }
    }
    Ok(None)
}

/// `arr.find_last(predicate)`: the last element accepted, or `null`.
pub fn find_last<T: Clone>(
    arr: &Shared<Vec<T>>,
    predicate: &dyn Fn(T) -> Result<bool, Thrown>,
) -> Result<Option<T>, Thrown> {
    for item in snapshot(arr).into_iter().rev() {
        if predicate(item.clone())? {
            return Ok(Some(item));
        }
    }
    Ok(None)
}

/// `arr.find_last_index(predicate)`: the index of the last element
/// accepted, or `null`.
pub fn find_last_index<T: Clone>(
    arr: &Shared<Vec<T>>,
    predicate: &dyn Fn(T) -> Result<bool, Thrown>,
) -> Result<Option<Int63>, Thrown> {
    for (index, item) in snapshot(arr).into_iter().enumerate().rev() {
        if predicate(item)? {
            return Ok(Some(int_from_usize(index)));
        }
    }
    Ok(None)
}

/// `arr.reduce(reducer, initial)`: the accumulator after every element,
/// left to right; `initial` on an empty array.
pub fn reduce<T: Clone, A>(
    arr: &Shared<Vec<T>>,
    reducer: &dyn Fn(A, T) -> Result<A, Thrown>,
    initial: A,
) -> Result<A, Thrown> {
    let mut accumulator = initial;
    for item in snapshot(arr) {
        accumulator = reducer(accumulator, item)?;
    }
    Ok(accumulator)
}

/// `arr.flat_map(f)`: the results' elements, concatenated. Each result is
/// read once `f` has returned it, so a later change to it is not seen.
pub fn flat_map<T: Clone, U: Clone>(
    arr: &Shared<Vec<T>>,
    f: &dyn Fn(T) -> Result<Shared<Vec<U>>, Thrown>,
) -> Result<Shared<Vec<U>>, Thrown> {
    let mut out = Vec::new();
    for item in snapshot(arr) {
        let inner = f(item)?;
        out.extend(inner.borrow().iter().cloned());
    }
    Ok(new(out))
}

/// `arr.sort_by(compare)` in place: the VM's bottom-up merge sort over a
/// copy (`SortByContinuation`), comparison for comparison, so a comparator
/// with side effects sees the same sequence of calls. `greater` is whether
/// the comparator put its first argument after its second
/// (`Ordering.Greater`); an equal pair keeps the left element first, which
/// is what makes the sort stable. The copy is written back only once every
/// comparison has returned: a comparator that throws leaves the array as it
/// was, and one that mutates the array has its changes overwritten.
pub fn sort_by<T: Clone>(
    arr: &Shared<Vec<T>>,
    greater: &dyn Fn(T, T) -> Result<bool, Thrown>,
) -> Result<(), Thrown> {
    let items = snapshot(arr);
    let sorted = merge_sort(items, &|left, right| greater(left.clone(), right.clone()))?;
    *arr.borrow_mut() = sorted;
    Ok(())
}

/// `arr.sort_by_key(key)` in place: every key computed once, left to right,
/// then the elements ordered by their keys through `compare` with the same
/// stable sort as [`sort_by`]. Written back only once every key has been
/// computed.
pub fn sort_by_key<T: Clone, K>(
    arr: &Shared<Vec<T>>,
    key: &dyn Fn(T) -> Result<K, Thrown>,
    compare: &dyn Fn(&K, &K) -> std::cmp::Ordering,
) -> Result<(), Thrown> {
    let items = snapshot(arr);
    if items.len() <= 1 {
        return Ok(());
    }
    let mut keys = Vec::with_capacity(items.len());
    for item in &items {
        keys.push(key(item.clone())?);
    }
    let order: Vec<usize> = (0..items.len()).collect();
    let order = merge_sort(order, &|left, right| {
        Ok(compare(&keys[*left], &keys[*right]).is_gt())
    })?;
    let sorted: Vec<T> = order
        .into_iter()
        .map(|index| items[index].clone())
        .collect();
    *arr.borrow_mut() = sorted;
    Ok(())
}

/// The elements as they are now, with the borrow released before any
/// callback runs.
#[inline]
fn snapshot<T: Clone>(arr: &Shared<Vec<T>>) -> Vec<T> {
    arr.borrow().clone()
}

/// The VM's bottom-up merge sort: runs of width 1, 2, 4, .. merged left to
/// right, taking the left element unless `greater(left, right)`.
fn merge_sort<T: Clone>(
    mut source: Vec<T>,
    greater: &dyn Fn(&T, &T) -> Result<bool, Thrown>,
) -> Result<Vec<T>, Thrown> {
    let len = source.len();
    if len <= 1 {
        return Ok(source);
    }
    let mut merged: Vec<T> = Vec::with_capacity(len);
    let mut width = 1;
    loop {
        let mut run_start = 0;
        while run_start < len {
            let middle = run_start.saturating_add(width).min(len);
            let run_end = middle.saturating_add(width).min(len);
            let (mut left, mut right) = (run_start, middle);
            while left < middle && right < run_end {
                if greater(&source[left], &source[right])? {
                    merged.push(source[right].clone());
                    right += 1;
                } else {
                    merged.push(source[left].clone());
                    left += 1;
                }
            }
            merged.extend_from_slice(&source[left..middle]);
            merged.extend_from_slice(&source[right..run_end]);
            run_start = run_end;
        }
        std::mem::swap(&mut source, &mut merged);
        merged.clear();
        width = width.saturating_mul(2);
        if width >= len {
            return Ok(source);
        }
    }
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
            "index -1 out of bounds for length 0"
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
    fn callbacks_walk_a_snapshot_and_unwind_a_throw() {
        let arr = ints(&[1, 2, 3]);
        let doubled = map(&arr, &|x| Ok(int(x.get() * 2))).unwrap();
        assert_eq!(*doubled.borrow(), vec![int(2), int(4), int(6)]);
        // A callback that pushes is not observed by the walk.
        let seen = std::cell::Cell::new(0);
        let pushing = &|x: Int63| {
            seen.set(seen.get() + 1);
            push(&arr, x);
            Ok(true)
        };
        assert_eq!(
            *filter(&arr, pushing).unwrap().borrow(),
            vec![int(1), int(2), int(3)]
        );
        assert_eq!(seen.get(), 3);
        assert_eq!(len(&arr), int(6));
        let arr = ints(&[1, 2, 3, 4]);
        assert_eq!(
            *filter_map(&arr, &|x| Ok((x.get() % 2 == 0).then_some(x)))
                .unwrap()
                .borrow(),
            vec![int(2), int(4)]
        );
        assert!(some(&arr, &|x| Ok(x.get() > 3)).unwrap());
        assert!(!every(&arr, &|x| Ok(x.get() > 3)).unwrap());
        assert!(!some(&ints(&[]), &|_| Ok(true)).unwrap());
        assert!(every(&ints(&[]), &|_| Ok(false)).unwrap());
        assert_eq!(find(&arr, &|x| Ok(x.get() > 1)).unwrap(), Some(int(2)));
        assert_eq!(
            find_index(&arr, &|x| Ok(x.get() > 1)).unwrap(),
            Some(int(1))
        );
        assert_eq!(find_last(&arr, &|x| Ok(x.get() < 3)).unwrap(), Some(int(2)));
        assert_eq!(
            find_last_index(&arr, &|x| Ok(x.get() < 3)).unwrap(),
            Some(int(1))
        );
        assert_eq!(find(&arr, &|x| Ok(x.get() > 9)).unwrap(), None);
        assert_eq!(
            reduce(&arr, &|acc: Int63, x| Ok(int(acc.get() + x.get())), int(0)).unwrap(),
            int(10)
        );
        assert_eq!(
            reduce(&ints(&[]), &|acc, _| Ok(acc), int(42)).unwrap(),
            int(42)
        );
        let flat = flat_map(&arr, &|x| Ok(ints(&[x.get(), x.get() * 10]))).unwrap();
        assert_eq!(flat.borrow().len(), 8);
        let total = std::cell::Cell::new(0);
        for_each(&arr, &|x| {
            total.set(total.get() + x.get());
            Ok(())
        })
        .unwrap();
        assert_eq!(total.get(), 10);
        let thrown = map(&arr, &|x| {
            if x.get() == 3 {
                Err(Thrown::from(Panic::AssertionFailed))
            } else {
                Ok(x)
            }
        });
        assert!(matches!(thrown, Err(Thrown::Panic(Panic::AssertionFailed))));
    }

    #[test]
    fn sort_by_is_the_vm_merge_sort_and_writes_back_on_success() {
        let arr = ints(&[5, 1, 4, 2, 3, 3]);
        let calls = std::cell::RefCell::new(Vec::new());
        sort_by(&arr, &|a, b| {
            calls.borrow_mut().push((a.get(), b.get()));
            Ok(a > b)
        })
        .unwrap();
        assert_eq!(
            *arr.borrow(),
            vec![int(1), int(2), int(3), int(3), int(4), int(5)]
        );
        // The VM's comparison sequence: width-1 runs, then 2, then 4.
        assert_eq!(
            *calls.borrow(),
            vec![
                (5, 1),
                (4, 2),
                (3, 3),
                (1, 2),
                (5, 2),
                (5, 4),
                (1, 3),
                (2, 3),
                (4, 3),
                (4, 3),
            ]
        );
        // A throwing comparator leaves the array as it was.
        let arr = ints(&[2, 1]);
        let thrown = sort_by(&arr, &|_, _| Err(Thrown::from(Panic::AssertionFailed)));
        assert!(thrown.is_err());
        assert_eq!(*arr.borrow(), vec![int(2), int(1)]);
        // A comparator that mutates the array has its changes overwritten.
        let arr = ints(&[2, 1]);
        sort_by(&arr, &|a, b| {
            push(&arr, int(9));
            Ok(a > b)
        })
        .unwrap();
        assert_eq!(*arr.borrow(), vec![int(1), int(2)]);
        // Stable: equal keys keep their order.
        let arr = new(vec![
            from_literal("bb"),
            from_literal("a"),
            from_literal("cc"),
            from_literal("d"),
        ]);
        sort_by_key(
            &arr,
            &|s: Str| Ok(int(i64::try_from(s.as_str().len()).unwrap())),
            &|a, b| Ord::cmp(a, b),
        )
        .unwrap();
        let names: Vec<String> = arr.borrow().iter().map(Str::to_string).collect();
        assert_eq!(names, ["a", "d", "bb", "cc"]);
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
