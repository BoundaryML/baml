//! Roundtrip coverage for `baml_sdk::class_refs` — class composition.

use std::sync::Arc;

use baml_sdk::class_refs::{
    Inner, Outer, make_outer, round_trip_inner, round_trip_inner_async, round_trip_outer,
    round_trip_outer_async,
};

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

// SDK_PARITY_LINT(skip): Rust class parameters take `impl Borrow<Class>`, so callers pass a value or a reference
#[test]
fn test_class_refs_round_trip_by_value_or_reference_rust_only() {
    let i = Inner { value: 3 };
    // By reference: `i` stays with the caller, so repeat calls need no clone.
    assert_eq!(round_trip_inner(&i).unwrap(), i);
    assert_eq!(round_trip_inner(&i).unwrap(), i);
    // By value, as before.
    assert_eq!(round_trip_inner(i.clone()).unwrap(), i);
    // Any other `Borrow<Inner>`, such as a shared `Arc`.
    assert_eq!(round_trip_inner(Arc::new(i.clone())).unwrap(), i);

    let o = Outer { inner: i };
    assert_eq!(round_trip_outer(&o).unwrap(), o);
    assert_eq!(round_trip_outer(o.clone()).unwrap(), o);
}

// SDK_PARITY_LINT(skip): Rust class parameters take `impl Borrow<Class>`, so callers pass a value or a reference
#[tokio::test(flavor = "multi_thread")]
async fn test_class_refs_round_trip_async_by_value_or_reference_rust_only() {
    let i = Inner { value: 4 };
    // A borrowed argument lives as long as the future that is awaited here.
    assert_eq!(round_trip_inner_async(&i).await.unwrap(), i);
    assert_eq!(round_trip_inner_async(&i).await.unwrap(), i);
    assert_eq!(round_trip_inner_async(i.clone()).await.unwrap(), i);
    // An owned argument (a value or an `Arc`) keeps the future `'static`,
    // so it can be spawned.
    let spawned = tokio::spawn(round_trip_inner_async(Arc::new(i.clone())))
        .await
        .unwrap();
    assert_eq!(spawned.unwrap(), i);

    let o = Outer { inner: i };
    assert_eq!(round_trip_outer_async(&o).await.unwrap(), o);
    assert_eq!(round_trip_outer_async(o.clone()).await.unwrap(), o);
}
