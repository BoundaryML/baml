//! Roundtrip coverage for `baml_sdk::class_refs` — class composition.

use baml_sdk::class_refs::{Inner, Outer, make_outer, round_trip_inner, round_trip_outer};

#[test]
fn test_class_refs_make_outer() {
    let o = make_outer(5).unwrap();
    assert_eq!(o.inner.value, 5);
}

#[test]
fn test_class_refs_round_trip_inner() {
    let i = Inner { value: 3 };
    assert_eq!(round_trip_inner(i.clone()).unwrap(), i);
}

#[test]
fn test_class_refs_round_trip_outer() {
    let o = Outer {
        inner: Inner { value: 9 },
    };
    assert_eq!(round_trip_outer(o.clone()).unwrap(), o);
}
