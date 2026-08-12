//! Roundtrip coverage for `baml_sdk::lists` — list Ty variants.

use baml_sdk::lists::{
    IntOrString, ListContainer, round_trip_ints, round_trip_list_container,
    round_trip_optional_strings, round_trip_union_list,
};

#[test]
fn test_lists_round_trip_ints() {
    assert_eq!(round_trip_ints(vec![1, 2, 3]).unwrap(), vec![1, 2, 3]);
}

#[test]
fn test_lists_round_trip_empty_list() {
    // Regression pinned from the python encoder: an empty list used to
    // encode as an unset `list_value` oneof, which the engine read as null
    // and returned as `None`. An empty list must stay distinct from null on
    // the wire.
    assert_eq!(round_trip_ints(vec![]).unwrap(), Vec::<i64>::new());
}

#[test]
fn test_lists_round_trip_optional_strings() {
    assert_eq!(
        round_trip_optional_strings(vec![Some("a".to_string()), None, Some("b".to_string())])
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
    assert_eq!(round_trip_union_list(xs.clone()).unwrap(), xs);
}

#[test]
fn test_lists_round_trip_list_container() {
    let c = ListContainer {
        ints: vec![1, 2],
        optional_strings: vec![None, Some("z".to_string())],
        union_list: vec![IntOrString::Int(1), IntOrString::String("x".to_string())],
    };
    assert_eq!(round_trip_list_container(c.clone()).unwrap(), c);
}
