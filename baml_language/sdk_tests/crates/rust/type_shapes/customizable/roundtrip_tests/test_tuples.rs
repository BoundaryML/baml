use baml_sdk::{
    Arguments, BamlOptions, Input, Target, Type, TypeBindings, Value, invoke_sync,
    tuples::{
        IntOrString,
        IntStringIntStringIntStringIntStringIntStringIntStringIntTupleOrString as WideOrString,
        IntStringTupleOrString, IntStringTupleOrStringIntTuple, TupleBox, TupleHolder,
        TuplePairBox, WideTupleHolder, call_tuple_callback, call_tuple_callback_sync,
        make_pair_sync, make_tuple_swapper_sync, pair_first_sync, pair_with_sync,
        round_trip_holder_sync, round_trip_nested_sync, round_trip_optional_slot_sync,
        round_trip_optional_tuple_sync, round_trip_pair, round_trip_pair_sync,
        round_trip_repeated_sync, round_trip_single_sync, round_trip_tuple_arms_sync,
        round_trip_tuple_box_sync, round_trip_tuple_list_sync, round_trip_tuple_map_sync,
        round_trip_tuple_or_string_sync, round_trip_wide_holder_sync,
        round_trip_wide_or_string_sync, round_trip_wide_sync, round_trip_wide20_sync,
        swap_pair_sync, throw_pair_sync, tuple_generic_identity_sync,
    },
};

fn s(value: &str) -> String {
    value.to_string()
}

fn call(name: &str, args: Vec<(&str, Input)>, types: Vec<(&str, Type)>) -> Value {
    try_call(name, args, types).unwrap_or_else(|error| panic!("{name} failed: {error}"))
}

fn try_call(
    name: &str,
    args: Vec<(&str, Input)>,
    types: Vec<(&str, Type)>,
) -> Result<Value, baml_bridge::Error<Value>> {
    invoke_sync(
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
    assert_eq!(round_trip_pair_sync((1, s("a"))).unwrap(), (1, s("a")));
}

#[tokio::test]
async fn test_tuples_round_trip_pair_async() {
    assert_eq!(round_trip_pair((2, s("b"))).await.unwrap(), (2, s("b")));
}

#[test]
fn test_tuples_round_trip_single_and_repeated() {
    assert_eq!(round_trip_single_sync((s("only"),)).unwrap(), (s("only"),));
    assert_eq!(round_trip_repeated_sync((1, 2, 3)).unwrap(), (1, 2, 3));
}

#[test]
fn test_tuples_round_trip_nested() {
    assert_eq!(
        round_trip_nested_sync(((1, s("a")), true)).unwrap(),
        ((1, s("a")), true)
    );
}

#[test]
fn test_tuples_round_trip_optional_slot_and_optional_tuple() {
    assert_eq!(round_trip_optional_slot_sync((1, None)).unwrap(), (1, None));
    assert_eq!(
        round_trip_optional_slot_sync((1, Some(s("a")))).unwrap(),
        (1, Some(s("a")))
    );
    assert_eq!(round_trip_optional_tuple_sync(None).unwrap(), None);
    assert_eq!(
        round_trip_optional_tuple_sync(Some((2, s("b")))).unwrap(),
        Some((2, s("b")))
    );
}

type Wide = (
    i64,
    String,
    i64,
    String,
    i64,
    String,
    i64,
    String,
    i64,
    String,
    i64,
    String,
    i64,
);

fn wide() -> Wide {
    (
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
    )
}

fn wide_parts(t: Wide) -> ([i64; 7], [String; 6]) {
    (
        [t.0, t.2, t.4, t.6, t.8, t.10, t.12],
        [t.1, t.3, t.5, t.7, t.9, t.11],
    )
}

#[test]
fn test_tuples_round_trip_wide() {
    assert_wide_class_and_union();
    assert_wide20();
    let back = round_trip_wide_sync(wide()).unwrap();
    assert_eq!(wide_parts(back), wide_parts(wide()));
}

fn assert_wide20() {
    let twenty = (
        0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19,
    );
    let back = round_trip_wide20_sync(twenty).unwrap();
    assert_eq!(
        [back.0, back.1, back.12, back.18, back.19],
        [0, 1, 12, 18, 19]
    );
}

fn assert_wide_class_and_union() {
    let holder = WideTupleHolder {
        wide: wide(),
        label: s("w"),
    };
    let back = round_trip_wide_holder_sync(&holder.clone()).unwrap();
    assert_eq!(back.label, "w");
    assert_eq!(wide_parts(back.wide), wide_parts(wide()));

    let WideOrString::IntStringIntStringIntStringIntStringIntStringIntStringIntTuple(back) =
        round_trip_wide_or_string_sync(wide()).unwrap()
    else {
        panic!("expected the wide tuple arm");
    };
    assert_eq!(wide_parts(back), wide_parts(wide()));
    let WideOrString::String(back) = round_trip_wide_or_string_sync(s("s")).unwrap() else {
        panic!("expected the string arm");
    };
    assert_eq!(back, "s");
}

fn assert_wide_shape_validation() {
    let pair = call(
        "make_pair",
        vec![("n", Input::new(1i64)), ("s", Input::new(s("a")))],
        vec![],
    );
    assert!(pair.decode::<Wide>().is_err());
    let back = call("round_trip_wide", vec![("t", Input::new(wide()))], vec![]);
    assert_eq!(
        wide_parts(back.decode::<Wide>().unwrap()),
        wide_parts(wide())
    );
    assert!(
        back.decode::<(
            i64,
            String,
            i64,
            String,
            i64,
            String,
            i64,
            String,
            i64,
            String,
            i64,
            String
        )>()
        .is_err()
    );
    assert!(
        try_call(
            "round_trip_wide",
            vec![(
                "t",
                Input::new((
                    s("0"),
                    s("1"),
                    2i64,
                    s("3"),
                    4i64,
                    s("5"),
                    6i64,
                    s("7"),
                    8i64,
                    s("9"),
                    10i64,
                    s("11"),
                    12i64,
                ))
            )],
            vec![]
        )
        .is_err()
    );
}

fn assert_generic_slots() {
    assert_eq!(pair_with_sync(s("x"), 1).unwrap(), (s("x"), 1));
    assert_eq!(
        pair_with_sync((true, s("y")), 2).unwrap(),
        ((true, s("y")), 2)
    );
}

#[test]
fn test_tuples_cross_by_copy() {
    let pair = call(
        "make_pair",
        vec![("n", Input::new(1i64)), ("s", Input::new(s("a")))],
        vec![],
    );
    let mut host = pair.decode::<(i64, String)>().unwrap();
    host.0 = 99;
    assert_eq!(
        call("round_trip_pair", vec![("t", Input::new(pair))], vec![])
            .decode::<(i64, String)>()
            .unwrap(),
        (1, s("a"))
    );
    assert_eq!(round_trip_pair_sync(host.clone()).unwrap(), (99, s("a")));
}

#[test]
fn test_tuples_wrong_arity_and_slots_rejected() {
    assert_wide_shape_validation();
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
        round_trip_tuple_list_sync(vec![(s("a"), 1), (s("b"), 2)]).unwrap(),
        vec![(s("a"), 1), (s("b"), 2)]
    );
    let map = baml_bridge::Map::from_iter([(s("k"), (1, true))]);
    assert_eq!(round_trip_tuple_map_sync(map.clone()).unwrap(), map);
}

#[test]
fn test_tuples_round_trip_union_arms() {
    assert_eq!(
        round_trip_tuple_or_string_sync((1, s("a"))).unwrap(),
        IntStringTupleOrString::IntStringTuple((1, s("a")))
    );
    assert_eq!(
        round_trip_tuple_or_string_sync(s("s")).unwrap(),
        IntStringTupleOrString::String(s("s"))
    );
    assert_eq!(
        round_trip_tuple_arms_sync((1, s("a"))).unwrap(),
        IntStringTupleOrStringIntTuple::IntStringTuple((1, s("a")))
    );
    assert_eq!(
        round_trip_tuple_arms_sync((s("a"), 1)).unwrap(),
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
    assert_eq!(round_trip_holder_sync(&holder).unwrap(), holder);
}

#[test]
fn test_tuples_round_trip_generic_argument() {
    assert_generic_slots();
    let boxed = TupleBox::<(i64, String)> { value: (1, s("a")) };
    assert_eq!(round_trip_tuple_box_sync(&boxed).unwrap(), boxed);
}

#[test]
fn test_tuples_make_swap_and_index() {
    assert_eq!(make_pair_sync(1, s("a")).unwrap(), (1, s("a")));
    assert_eq!(swap_pair_sync((1, s("a"))).unwrap(), (s("a"), 1));
    assert_eq!(pair_first_sync((7, s("a"))).unwrap(), 7);
    assert_eq!(
        round_trip_pair_sync(make_pair_sync(2, s("b")).unwrap()).unwrap(),
        (2, s("b"))
    );
}

#[test]
fn test_tuples_host_callback_args_and_results() {
    let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let record = std::sync::Arc::clone(&seen);
    let swapped = call_tuple_callback_sync(
        move |t: (i64, String)| {
            record.lock().unwrap().push(t.clone());
            (t.1, t.0)
        },
        (1, s("a")),
    )
    .unwrap();
    assert_eq!(swapped, (s("a"), 1));
    assert_eq!(*seen.lock().unwrap(), vec![(1, s("a"))]);

    let swapped = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(call_tuple_callback(
            |t: (i64, String)| async move { (t.1, t.0) },
            (2, s("b")),
        ))
        .unwrap();
    assert_eq!(swapped, (s("b"), 2));
}

#[test]
fn test_tuples_thrown_value() {
    let error = throw_pair_sync((1, s("a"))).unwrap_err();
    let baml_bridge::Error::Thrown { value, .. } = error else {
        panic!("expected the thrown tuple, got {error}");
    };
    assert_eq!(*value, (1, s("a")));
}

#[test]
fn test_tuples_dynamic_value_keeps_carried_type() {
    let held = call(
        "tuple_as_unknown",
        vec![("t", Input::new((IntOrString::Int(1), true)))],
        vec![],
    );
    assert_eq!(
        held.decode::<(IntOrString, bool)>().unwrap(),
        (IntOrString::Int(1), true)
    );
    let carried = call(
        "unknown_has_carried_tuple_type",
        vec![("v", Input::new(held.clone()))],
        vec![],
    );
    assert!(carried.decode::<bool>().unwrap());
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
    assert_eq!(pair.decode::<(i64, String)>().unwrap(), (1, s("a")));
    assert!(pair.decode::<Vec<IntOrString>>().is_err());
    assert!(pair.decode::<Vec<Value>>().is_err());
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
    assert!(!has_type(
        Input::new((1i64, s("a"))),
        vec![("T", Type::of::<(String, i64)>())]
    ));
    assert!(!has_type(
        Input::new(vec![IntOrString::Int(1), IntOrString::String(s("a"))]),
        pair_type()
    ));
    assert_eq!(
        tuple_generic_identity_sync::<(i64, String)>((1, s("a"))).unwrap(),
        (1, s("a"))
    );
}

#[test]
fn test_tuples_method_round_trip() {
    let pair_box = TuplePairBox { a: 1, b: s("a") };
    assert_eq!(pair_box.as_pair_sync().unwrap(), (1, s("a")));
    assert_eq!(
        pair_box.with_pair_sync((2, s("b"))).unwrap(),
        TuplePairBox { a: 2, b: s("b") }
    );
    assert_eq!(
        TuplePairBox::from_pair_sync((3, s("c"))).unwrap(),
        TuplePairBox { a: 3, b: s("c") }
    );

    tokio::runtime::Runtime::new().unwrap().block_on(async {
        assert_eq!(pair_box.as_pair().await.unwrap(), (1, s("a")));
        assert_eq!(
            pair_box.with_pair((4, s("d"))).await.unwrap(),
            TuplePairBox { a: 4, b: s("d") }
        );
        assert_eq!(
            TuplePairBox::from_pair((5, s("e"))).await.unwrap(),
            TuplePairBox { a: 5, b: s("e") }
        );
    });
}

#[test]
fn test_tuples_function_handle_round_trip() {
    let swap = make_tuple_swapper_sync().unwrap();
    assert_eq!(swap.call_sync(((1, s("a")),)).unwrap(), (s("a"), 1));
    assert_eq!(swap.call_sync(((2, s("b")),)).unwrap(), (s("b"), 2));
}
