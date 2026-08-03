//! Roundtrip coverage for `baml_sdk::generics` — generic classes (over `<int>`).
//!
//! The generic *instance method* path (`WrapperMethods::get_value` /
//! `get_value_or_marker`) is covered separately in
//! `customizable/test_generic.rs`; here we cover the
//! concretely-instantiated generic class round trips.

// Generic classes emit as `BamlValue`-bounded structs with pub fields,
// instantiated with struct-path turbofish (`Wrapper::<i64> { .. }`), and
// recursive fields box (`next: Option<std::boxed::Box<GenericLinkedList<T>>>`).
// The generated `Box` (the fixture's `Box<T>` class) shadows the prelude's
// `std::boxed::Box`, so the recursion boxes below are spelled in full.
use baml_sdk::generics::{
    Box, DifferingInstantiation, GenericBinaryTree, GenericLinkedList, NestedGenerics, Wrapper,
    round_trip_box_int, round_trip_differing_instantiation, round_trip_generic_binary_tree_int,
    round_trip_generic_linked_list_int, round_trip_nested_generics, round_trip_wrapper_int,
};

#[test]
fn test_generics_round_trip_wrapper_int() {
    let w = Wrapper::<i64> { value: 5 };
    assert_eq!(round_trip_wrapper_int(w.clone()).unwrap(), w);
}

#[test]
fn test_generics_round_trip_generic_linked_list_int() {
    let ll = GenericLinkedList::<i64> {
        value: 1,
        next: Some(std::boxed::Box::new(GenericLinkedList::<i64> {
            value: 2,
            next: None,
        })),
    };
    assert_eq!(round_trip_generic_linked_list_int(ll.clone()).unwrap(), ll);
}

#[test]
fn test_generics_round_trip_generic_binary_tree_int() {
    let t = GenericBinaryTree::<i64> {
        value: 1,
        left: None,
        right: None,
    };
    assert_eq!(round_trip_generic_binary_tree_int(t.clone()).unwrap(), t);
}

#[test]
fn test_generics_round_trip_box_int() {
    let b = Box::<i64> {
        value: 3,
        wrapped: Wrapper::<i64> { value: 4 },
    };
    assert_eq!(round_trip_box_int(b.clone()).unwrap(), b);
}

#[test]
fn test_generics_round_trip_nested_generics() {
    let n = NestedGenerics {
        ww: Wrapper::<Wrapper<i64>> {
            value: Wrapper::<i64> { value: 1 },
        },
        // python spells this `Wrapper[list]`; the field type is
        // `Wrapper<int[]>`, i.e. `Wrapper<Vec<i64>>`.
        wl: Wrapper::<Vec<i64>> { value: vec![1, 2] },
        wr: Wrapper::<GenericLinkedList<i64>> {
            value: GenericLinkedList::<i64> {
                value: 9,
                next: None,
            },
        },
    };
    assert_eq!(round_trip_nested_generics(n.clone()).unwrap(), n);
}

#[test]
fn test_generics_round_trip_differing_instantiation() {
    let d = DifferingInstantiation {
        list: GenericLinkedList::<Wrapper<i64>> {
            value: Wrapper::<i64> { value: 1 },
            next: None,
        },
    };
    assert_eq!(round_trip_differing_instantiation(d.clone()).unwrap(), d);
}
