//! Roundtrip coverage for `baml_sdk::tuples`: a BAML tuple is a native Rust
//! tuple, and one wider than the bridge's native tuple impls (13 slots
//! here) is a generated named tuple struct, in every position the generated
//! SDK reaches.
//!
//! The generated signatures make a wrong arity or slot a compile error, so
//! the rejection cases go through the dynamic surface (`invoke`, `Value`):
//! an argument the engine refuses, and a received tuple decoded at a type it
//! does not have. A tuple received with no declared tuple type (`unknown`)
//! keeps its carried type: it decodes at that type, or as a list (of the
//! join of its slot types, or of dynamic values), and goes back as the same
//! tuple.
use baml_sdk::tuples::{
    IntOrString, IntStringTupleOrString, IntStringTupleOrStringIntTuple,
    Tuple13_IntStringIntStringIntStringIntStringIntStringIntStringInt as Wide, TupleBox,
    TupleHolder, TuplePairBox, call_tuple_callback, call_tuple_callback_async, make_pair,
    make_tuple_swapper, pair_first, round_trip_holder, round_trip_nested, round_trip_optional_slot,
    round_trip_optional_tuple, round_trip_pair, round_trip_pair_async, round_trip_repeated,
    round_trip_single, round_trip_tuple_arms, round_trip_tuple_box, round_trip_tuple_list,
    round_trip_tuple_map, round_trip_tuple_or_string, round_trip_wide, swap_pair, throw_pair,
    tuple_generic_identity,
};
use baml_sdk::{Arguments, BamlOptions, Input, Target, Type, TypeBindings, Value, invoke};

fn s(value: &str) -> String {
    value.to_string()
}

/// Call `user.tuples.<name>` through the dynamic surface.
fn call(name: &str, args: Vec<(&str, Input)>, types: Vec<(&str, Type)>) -> Value {
    try_call(name, args, types).unwrap_or_else(|error| panic!("{name} failed: {error}"))
}

fn try_call(
    name: &str,
    args: Vec<(&str, Input)>,
    types: Vec<(&str, Type)>,
) -> Result<Value, baml_bridge::Error<Value>> {
    invoke(
        Target::named(format!("user.tuples.{name}")),
        args.into_iter()
            .map(|(key, value)| (key.to_string(), value))
            .collect::<Arguments>(),
        types
            .into_iter()
            .map(|(key, value)| (key.to_string(), value))
            .collect::<TypeBindings>(),
        BamlOptions::default(),
    )
}

#[test]
fn test_tuples_round_trip_pair() {
    assert_eq!(round_trip_pair((1, s("a"))).unwrap(), (1, s("a")));
}

#[tokio::test]
async fn test_tuples_round_trip_pair_async() {
    assert_eq!(
        round_trip_pair_async((2, s("b"))).await.unwrap(),
        (2, s("b"))
    );
}

#[test]
fn test_tuples_round_trip_single_and_repeated() {
    assert_eq!(round_trip_single((s("only"),)).unwrap(), (s("only"),));
    assert_eq!(round_trip_repeated((1, 2, 3)).unwrap(), (1, 2, 3));
}

#[test]
fn test_tuples_round_trip_nested() {
    assert_eq!(
        round_trip_nested(((1, s("a")), true)).unwrap(),
        ((1, s("a")), true)
    );
}

#[test]
fn test_tuples_round_trip_optional_slot_and_optional_tuple() {
    assert_eq!(round_trip_optional_slot((1, None)).unwrap(), (1, None));
    assert_eq!(
        round_trip_optional_slot((1, Some(s("a")))).unwrap(),
        (1, Some(s("a")))
    );
    assert_eq!(round_trip_optional_tuple(None).unwrap(), None);
    assert_eq!(
        round_trip_optional_tuple(Some((2, s("b")))).unwrap(),
        Some((2, s("b")))
    );
}

#[test]
fn test_tuples_round_trip_wide() {
    let wide = Wide(
        0,
        s("1"),
        2,
        s("3"),
        4,
        s("5"),
        6,
        s("7"),
        8,
        s("9"),
        10,
        s("11"),
        12,
    );
    let back = round_trip_wide(wide.clone()).unwrap();
    assert_eq!(back, wide);
    // The named struct's slots read as a native tuple's do.
    assert_eq!((back.0, back.12), (0, 12));
}

#[test]
fn test_tuples_wrong_arity_and_slots_rejected() {
    // An argument the generated signature would not compile is refused by
    // the engine: too few slots, too many, and slots out of order.
    assert!(try_call("round_trip_pair", vec![("t", Input::new((1i64,)))], vec![]).is_err());
    assert!(
        try_call(
            "round_trip_pair",
            vec![("t", Input::new((1i64, s("a"), 2i64)))],
            vec![]
        )
        .is_err()
    );
    assert!(
        try_call(
            "round_trip_pair",
            vec![("t", Input::new((s("a"), 1i64)))],
            vec![]
        )
        .is_err()
    );
    // A received tuple decodes only at its own arity and slot types.
    let pair = call(
        "make_pair",
        vec![("n", Input::new(1i64)), ("s", Input::new(s("a")))],
        vec![],
    );
    assert_eq!(pair.decode::<(i64, String)>().unwrap(), (1, s("a")));
    assert!(pair.decode::<(i64,)>().is_err());
    assert!(pair.decode::<(i64, String, i64)>().is_err());
    assert!(pair.decode::<(String, i64)>().is_err());
    assert!(pair.decode::<(i64, Option<String>)>().is_ok());
}

#[test]
fn test_tuples_round_trip_containers() {
    assert_eq!(
        round_trip_tuple_list(vec![(s("a"), 1), (s("b"), 2)]).unwrap(),
        vec![(s("a"), 1), (s("b"), 2)]
    );
    let map = baml_bridge::Map::from_iter([(s("k"), (1, true))]);
    assert_eq!(round_trip_tuple_map(map.clone()).unwrap(), map);
}

#[test]
fn test_tuples_round_trip_union_arms() {
    assert_eq!(
        round_trip_tuple_or_string((1, s("a"))).unwrap(),
        IntStringTupleOrString::IntStringTuple((1, s("a")))
    );
    assert_eq!(
        round_trip_tuple_or_string(s("s")).unwrap(),
        IntStringTupleOrString::String(s("s"))
    );
    // Same-arity arms: the engine's selected arm picks the variant.
    assert_eq!(
        round_trip_tuple_arms((1, s("a"))).unwrap(),
        IntStringTupleOrStringIntTuple::IntStringTuple((1, s("a")))
    );
    assert_eq!(
        round_trip_tuple_arms((s("a"), 1)).unwrap(),
        IntStringTupleOrStringIntTuple::StringIntTuple((s("a"), 1))
    );
}

#[test]
fn test_tuples_round_trip_class_fields() {
    let holder = TupleHolder {
        pair: (1, s("a")),
        nested: ((2, s("b")), false),
        maybe: Some((3, None)),
        pairs: vec![(s("c"), 4)],
    };
    assert_eq!(round_trip_holder(&holder).unwrap(), holder);
}

#[test]
fn test_tuples_round_trip_generic_argument() {
    let boxed = TupleBox::<(i64, String)> { value: (1, s("a")) };
    assert_eq!(round_trip_tuple_box(&boxed).unwrap(), boxed);
}

#[test]
fn test_tuples_make_swap_and_index() {
    assert_eq!(make_pair(1, s("a")).unwrap(), (1, s("a")));
    assert_eq!(swap_pair((1, s("a"))).unwrap(), (s("a"), 1));
    assert_eq!(pair_first((7, s("a"))).unwrap(), 7);
    // A tuple BAML made goes back as the tuple it is.
    assert_eq!(
        round_trip_pair(make_pair(2, s("b")).unwrap()).unwrap(),
        (2, s("b"))
    );
}

#[test]
fn test_tuples_host_callback_args_and_results() {
    let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let record = std::sync::Arc::clone(&seen);
    let swapped = call_tuple_callback(
        move |t: (i64, String)| {
            record.lock().unwrap().push(t.clone());
            (t.1, t.0)
        },
        (1, s("a")),
    )
    .unwrap();
    assert_eq!(swapped, (s("a"), 1));
    assert_eq!(*seen.lock().unwrap(), vec![(1, s("a"))]);

    // The async call, with an async callback.
    let swapped = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(call_tuple_callback_async(
            |t: (i64, String)| async move { (t.1, t.0) },
            (2, s("b")),
        ))
        .unwrap();
    assert_eq!(swapped, (s("b"), 2));
}

#[test]
fn test_tuples_thrown_value() {
    let error = throw_pair((1, s("a"))).unwrap_err();
    let baml_bridge::Error::Thrown { value, .. } = error else {
        panic!("expected the thrown tuple, got {error}");
    };
    assert_eq!(*value, (1, s("a")));
}

#[test]
fn test_tuples_dynamic_value_keeps_carried_type() {
    // `(1, true)` is an `(int | string, bool)` by its carried type.
    let held = call(
        "tuple_as_unknown",
        vec![("t", Input::new((IntOrString::Int(1), true)))],
        vec![],
    );
    assert_eq!(
        held.decode::<(IntOrString, bool)>().unwrap(),
        (IntOrString::Int(1), true)
    );
    // Sent back as it arrived, it restates that type, not `(int, bool)`.
    let carried = call(
        "unknown_has_carried_tuple_type",
        vec![("v", Input::new(held.clone()))],
        vec![],
    );
    assert!(carried.decode::<bool>().unwrap());
    // A host tuple states the type of its own Rust slots.
    let stated = call(
        "unknown_has_carried_tuple_type",
        vec![("v", Input::new((1i64, true)))],
        vec![],
    );
    assert!(!stated.decode::<bool>().unwrap());
}

#[test]
fn test_tuples_dynamic_value_decodes_as_a_tuple() {
    let pair = call(
        "make_pair",
        vec![("n", Input::new(1i64)), ("s", Input::new(s("a")))],
        vec![],
    );
    // Its own type first: the native tuple.
    assert_eq!(pair.decode::<(i64, String)>().unwrap(), (1, s("a")));
    // Then a list of the join of its slot types, then of dynamic values:
    // every element, in order.
    assert_eq!(
        pair.decode::<Vec<IntOrString>>().unwrap(),
        vec![IntOrString::Int(1), IntOrString::String(s("a"))]
    );
    let items = pair.decode::<Vec<Value>>().unwrap();
    assert_eq!(items.len(), 2);
    assert_eq!(items[0].decode::<i64>().unwrap(), 1);
    assert_eq!(items[1].decode::<String>().unwrap(), "a");
    // However it was decoded, the value goes back as the tuple it is.
    assert_eq!(
        call("round_trip_pair", vec![("t", Input::new(pair))], vec![])
            .decode::<(i64, String)>()
            .unwrap(),
        (1, s("a"))
    );
}

#[test]
fn test_tuples_reflected_type_token() {
    let pair_type = || vec![("T", Type::of::<(i64, String)>())];
    let has_type = |value: Input, types: Vec<(&str, Type)>| {
        call("value_has_type", vec![("v", value)], types)
            .decode::<bool>()
            .unwrap()
    };
    assert!(has_type(Input::new((1i64, s("a"))), pair_type()));
    // Slot order is part of the type.
    assert!(!has_type(
        Input::new((1i64, s("a"))),
        vec![("T", Type::of::<(String, i64)>())]
    ));
    // A list of the same elements is not the tuple.
    assert!(!has_type(
        Input::new(vec![IntOrString::Int(1), IntOrString::String(s("a"))]),
        pair_type()
    ));
    // A tuple type binds a type parameter of a generated generic function.
    assert_eq!(
        tuple_generic_identity::<(i64, String)>((1, s("a"))).unwrap(),
        (1, s("a"))
    );
}

#[test]
fn test_tuples_method_round_trip() {
    let pair_box = TuplePairBox { a: 1, b: s("a") };
    // An instance method returning a tuple, and one taking a tuple.
    assert_eq!(pair_box.as_pair().unwrap(), (1, s("a")));
    assert_eq!(
        pair_box.with_pair((2, s("b"))).unwrap(),
        TuplePairBox { a: 2, b: s("b") }
    );
    // A static method taking a tuple.
    assert_eq!(
        TuplePairBox::from_pair((3, s("c"))).unwrap(),
        TuplePairBox { a: 3, b: s("c") }
    );

    // The async bindings.
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        assert_eq!(pair_box.as_pair_async().await.unwrap(), (1, s("a")));
        assert_eq!(
            pair_box.with_pair_async((4, s("d"))).await.unwrap(),
            TuplePairBox { a: 4, b: s("d") }
        );
        assert_eq!(
            TuplePairBox::from_pair_async((5, s("e"))).await.unwrap(),
            TuplePairBox { a: 5, b: s("e") }
        );
    });
}

#[test]
fn test_tuples_function_handle_round_trip() {
    // A BAML closure returned to the host: its handle takes and returns
    // tuples.
    let swap = make_tuple_swapper().unwrap();
    assert_eq!(swap.call(((1, s("a")),)).unwrap(), (s("a"), 1));
    assert_eq!(swap.call(((2, s("b")),)).unwrap(), (s("b"), 2));
}
