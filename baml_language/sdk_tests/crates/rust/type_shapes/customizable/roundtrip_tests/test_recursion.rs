//! Roundtrip coverage for `baml_sdk::recursion` — recursive classes / SCCs.
//!
//! All recursive child fields are optional, so finite values are built by
//! terminating recursion with `None`. Recursive fields are boxed at the
//! cycle site (`Option<Box<T>>`), so children are wrapped in
//! `Some(Box::new(…))`.

use baml_sdk::recursion::{
    A, B, IntBinaryTree, T1, T2, T3, T4, T5, T6, round_trip_a, round_trip_b,
    round_trip_int_binary_tree, round_trip_t1, round_trip_t2, round_trip_t3, round_trip_t4,
    round_trip_t5, round_trip_t6,
};

#[test]
fn test_recursion_round_trip_int_binary_tree() {
    let t = IntBinaryTree {
        value: 1,
        left: Some(Box::new(IntBinaryTree {
            value: 2,
            left: None,
            right: None,
        })),
        right: None,
    };
    assert_eq!(round_trip_int_binary_tree(t.clone()).unwrap(), t);
}

#[test]
fn test_recursion_round_trip_mutual_recursion() {
    let a = A {
        b: Some(Box::new(B { a: None })),
    };
    let b = B {
        a: Some(Box::new(A { b: None })),
    };
    assert_eq!(round_trip_a(a.clone()).unwrap(), a);
    assert_eq!(round_trip_b(b.clone()).unwrap(), b);
}

#[test]
fn test_recursion_round_trip_scc_t1_t2_t3() {
    let t1 = T1 {
        via2: Some(Box::new(T2 {
            via1: None,
            via3: None,
        })),
        via3: None,
    };
    let t2 = T2 {
        via1: None,
        via3: Some(Box::new(T3 {
            via1: None,
            via2: None,
        })),
    };
    let t3 = T3 {
        via1: None,
        via2: None,
    };
    assert_eq!(round_trip_t1(t1.clone()).unwrap(), t1);
    assert_eq!(round_trip_t2(t2.clone()).unwrap(), t2);
    assert_eq!(round_trip_t3(t3.clone()).unwrap(), t3);
}

#[test]
fn test_recursion_round_trip_scc_t4_t5_t6() {
    let t4 = T4 {
        via5: Some(Box::new(T5 {
            via4: None,
            via6: None,
        })),
        via6: None,
    };
    let t5 = T5 {
        via4: None,
        via6: Some(Box::new(T6 {
            via4: None,
            via5: None,
        })),
    };
    let t6 = T6 {
        via4: None,
        via5: None,
    };
    assert_eq!(round_trip_t4(t4.clone()).unwrap(), t4);
    assert_eq!(round_trip_t5(t5.clone()).unwrap(), t5);
    assert_eq!(round_trip_t6(t6.clone()).unwrap(), t6);
}
