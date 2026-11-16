//! Roundtrip coverage for `baml_sdk::lists` — list Ty variants.

use baml_sdk::lists::{
    IntOrString, ListContainer, round_trip_ints_sync, round_trip_list_container_sync,
    round_trip_optional_strings_sync, round_trip_union_list_sync,
};

#[test]
fn test_lists_round_trip_ints() {
    assert_eq!(round_trip_ints_sync(vec![1, 2, 3]).unwrap(), vec![1, 2, 3]);
}

#[test]
fn test_lists_round_trip_empty_list() {
    // Regression pinned from the python encoder: an empty list used to
    // encode as an unset `list_value` oneof, which the engine read as null
    // and returned as `None`. An empty list must stay distinct from null on
    // the wire.
    assert_eq!(round_trip_ints_sync(vec![]).unwrap(), Vec::<i64>::new());
}

#[test]
fn test_lists_round_trip_optional_strings() {
    assert_eq!(
        round_trip_optional_strings_sync(vec![Some("a".to_string()), None, Some("b".to_string())])
            .unwrap(),
        vec![Some("a".to_string()), None, Some("b".to_string())]
    );
}

#[test]
fn test_lists_round_trip_union_list() {
    let xs = vec![
        IntOrString::Int(1),
        IntOrString::String("two".to_string()),
        IntOrString::Int(3),
    ];
    assert_eq!(round_trip_union_list_sync(xs.clone()).unwrap(), xs);
}

#[test]
fn test_lists_round_trip_list_container() {
    let c = ListContainer {
        ints: vec![1, 2],
        optional_strings: vec![None, Some("z".to_string())],
        union_list: vec![IntOrString::Int(1), IntOrString::String("x".to_string())],
    };
    assert_eq!(round_trip_list_container_sync(c.clone()).unwrap(), c);
}
