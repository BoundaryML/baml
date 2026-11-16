//! Roundtrip coverage for `baml_sdk::unions` — union normalization variants.

// PROVISIONAL: unions have no Rust SDK design yet. This port assumes one
// generated enum per distinct normalized union, named by joining the arm
// type names with `Or` (`IntOrString`, `TOrString`), with the normalized
// trailing `null` arm surfacing as `Option<..>` around the enum. Python
// passes and compares bare arm values; Rust wraps both sides in the enum.
use baml_sdk::unions::{
    IntOrString, StringListOrIntList, T, TOrString, UnionContainer, round_trip_dedup_sync,
    round_trip_null_to_end_sync, round_trip_optional_plus_null_sync,
    round_trip_singleton_unwrap_sync, round_trip_str_or_int_list_sync, round_trip_t_sync,
    round_trip_union_container_sync,
};

#[test]
fn test_unions_round_trip_null_to_end() {
    assert_eq!(
        round_trip_null_to_end_sync(Some(IntOrString::Int(1))).unwrap(),
        Some(IntOrString::Int(1))
    );
    assert_eq!(
        round_trip_null_to_end_sync(Some(IntOrString::String("s".to_string()))).unwrap(),
        Some(IntOrString::String("s".to_string()))
    );
    assert_eq!(round_trip_null_to_end_sync(None).unwrap(), None);
}

#[test]
fn test_unions_round_trip_dedup() {
    assert_eq!(
        round_trip_dedup_sync(IntOrString::Int(2)).unwrap(),
        IntOrString::Int(2)
    );
    assert_eq!(
        round_trip_dedup_sync(IntOrString::String("x".to_string())).unwrap(),
        IntOrString::String("x".to_string())
    );
}

#[test]
fn test_unions_round_trip_singleton_unwrap() {
    // `int | int` collapses to plain `int`.
    assert_eq!(round_trip_singleton_unwrap_sync(7).unwrap(), 7);
}

#[test]
fn test_unions_round_trip_optional_plus_null() {
    assert_eq!(
        round_trip_optional_plus_null_sync(Some(TOrString::T(T { v: 1 }))).unwrap(),
        Some(TOrString::T(T { v: 1 }))
    );
    assert_eq!(
        round_trip_optional_plus_null_sync(Some(TOrString::String("s".to_string()))).unwrap(),
        Some(TOrString::String("s".to_string()))
    );
    assert_eq!(round_trip_optional_plus_null_sync(None).unwrap(), None);
}

#[test]
fn test_unions_round_trip_str_or_int_list() {
    for value in [
        StringListOrIntList::StringList(vec!["hello".to_string()]),
        StringListOrIntList::IntList(vec![1, 2]),
        StringListOrIntList::StringList(vec![]),
        StringListOrIntList::IntList(vec![]),
    ] {
        assert_eq!(
            round_trip_str_or_int_list_sync(value.clone()).unwrap(),
            value
        );
    }
}

#[test]
fn test_unions_round_trip_t() {
    assert_eq!(round_trip_t_sync(T { v: 4 }).unwrap(), T { v: 4 });
}

#[test]
fn test_unions_round_trip_union_container() {
    let c = UnionContainer {
        null_to_end: None,
        dedup: IntOrString::String("d".to_string()),
        singleton_unwrap: 5,
        optional_plus_null: Some(TOrString::T(T { v: 2 })),
    };
    assert_eq!(round_trip_union_container_sync(c.clone()).unwrap(), c);
}
