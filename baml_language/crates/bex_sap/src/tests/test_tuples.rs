//! Tuples `(T1, T2, ...)`: fixed-arity JSON arrays whose positions have their own types.

use super::*;

// ============================================================================
// Exact arity
// ============================================================================

test_deserializer!(
    test_tuple_exact,
    r#"[1, "a"]"#,
    baml_ty!(tuple(int, string)),
    baml_db! {},
    [1, "a"]
);

test_deserializer!(
    test_tuple_exact_in_prose,
    r#"Here you go:
```json
[1, "a", true]
```"#,
    baml_ty!(tuple(int, string, bool)),
    baml_db! {},
    [1, "a", true]
);

test_deserializer!(
    test_tuple_elements_coerce_independently,
    r#"["1", 2, "true"]"#,
    baml_ty!(tuple(int, string, bool)),
    baml_db! {},
    [1, "2", true]
);

test_deserializer!(
    test_tuple_with_class_and_enum_elements,
    r#"[{"name": "x"}, "RED"]"#,
    baml_ty!(tuple(Item, Color)),
    baml_db! {
        enum Color { RED, BLUE }
        class Item { name: string, }
    },
    [{"name": "x"}, "RED"]
);

test_failing_deserializer!(
    test_tuple_element_failure_fails_the_tuple,
    r#"[1, "not a number"]"#,
    baml_ty!(tuple(int, int)),
    baml_db! {}
);

// ============================================================================
// Wrong arity
// ============================================================================

test_deserializer!(
    test_tuple_extra_items_are_dropped,
    r#"[1, "a", true, null]"#,
    baml_ty!(tuple(int, string)),
    baml_db! {},
    [1, "a"]
);

test_failing_deserializer!(
    test_tuple_missing_required_item,
    r#"[1]"#,
    baml_ty!(tuple(int, string)),
    baml_db! {}
);

test_failing_deserializer!(
    test_tuple_from_empty_array,
    r#"[]"#,
    baml_ty!(tuple(int, string)),
    baml_db! {}
);

test_deserializer!(
    test_tuple_missing_optional_and_list_items_default,
    r#"[1]"#,
    baml_ty!(tuple(int, (string | null), [int])),
    baml_db! {},
    [1, null, []]
);

// ============================================================================
// Non-array input
// ============================================================================

test_deserializer!(
    test_single_value_to_one_tuple,
    r#"5"#,
    baml_ty!(tuple(int)),
    baml_db! {},
    [5]
);

test_failing_deserializer!(
    test_single_value_to_pair_fails,
    r#"5"#,
    baml_ty!(tuple(int, int)),
    baml_db! {}
);

test_failing_deserializer!(
    test_object_is_not_a_tuple,
    r#"{"0": 1, "1": "a"}"#,
    baml_ty!(tuple(int, string)),
    baml_db! {}
);

// ============================================================================
// Unions with lists
// ============================================================================

test_deserializer!(
    test_tuple_or_list_picks_tuple,
    r#"[1, "a"]"#,
    baml_ty!(((tuple(int, string)) | [int])),
    baml_db! {},
    [1, "a"]
);

test_deserializer!(
    test_list_or_tuple_picks_tuple,
    r#"[1, "a"]"#,
    baml_ty!(([int] | (tuple(int, string)))),
    baml_db! {},
    [1, "a"]
);

test_deserializer!(
    test_tuple_or_list_picks_list_for_other_arity,
    r#"[1, 2, 3]"#,
    baml_ty!(((tuple(int, string)) | [int])),
    baml_db! {},
    [1, 2, 3]
);

test_deserializer!(
    test_tuple_or_list_single_value_goes_to_list,
    r#"5"#,
    baml_ty!(((tuple(int, string)) | [int])),
    baml_db! {},
    [5]
);

test_deserializer!(
    test_tuple_arity_selects_union_member,
    r#"[1, 2, 3]"#,
    baml_ty!(((tuple(int, int)) | (tuple(int, int, int)))),
    baml_db! {},
    [1, 2, 3]
);

test_deserializer!(
    test_list_of_tuples,
    r#"[[1, "a"], [2, "b"]]"#,
    baml_ty!([(tuple(int, string))]),
    baml_db! {},
    [[1, "a"], [2, "b"]]
);

// ============================================================================
// Tuple-versus-list preference
// ============================================================================
//
// JSON equality cannot tell a tuple from a list, so these tests render the parsed value's shape:
// tuples as `(..)` and lists as `[..]`.

fn shape<N: TypeIdent>(
    value: &crate::deserializer::types::BamlValueWithFlags<'_, '_, '_, N>,
) -> String {
    use crate::baml_value::BamlValue;
    let join = |items: &[crate::deserializer::types::BamlValueWithFlags<'_, '_, '_, N>]| {
        items.iter().map(shape).collect::<Vec<_>>().join(", ")
    };
    match &value.value {
        BamlValue::Array(arr) => format!("[{}]", join(&arr.value)),
        BamlValue::Tuple(tuple) => format!("({})", join(&tuple.value)),
        _ => serde_json::to_value(value).unwrap().to_string(),
    }
}

/// Parses `raw` (as a complete or a partial answer) and returns the shape of the result.
fn parse_shape(
    raw: &str,
    ty: &Ty<'_, &'static str>,
    db: &TypeRefDb<'_, &'static str>,
    is_done: bool,
) -> String {
    let parsed = crate::jsonish::parse(raw, crate::jsonish::ParseOptions::default(), is_done)
        .expect("jsonish::parse failed");
    let ctx = crate::deserializer::coercer::ParsingContext::new(db);
    let target = db.resolve(ty).unwrap();
    let value = TyResolvedRef::coerce(&ctx, target, &parsed)
        .expect("coerces")
        .expect("has a value");
    shape(&value)
}

macro_rules! test_shape {
    ($name:ident, $raw:expr, $ty:expr, $is_done:expr, $expected:expr) => {
        #[test]
        fn $name() {
            let db: TypeRefDb<'_, &str> = baml_db! {};
            let ty: Ty<'_, &str> = $ty;
            assert_eq!(parse_shape($raw, &ty, &db, $is_done), $expected);
        }
    };
}

// A perfect tuple match beats a perfect list match in either union order.
test_shape!(
    test_perfect_tuple_beats_perfect_list,
    "[1, 1]",
    baml_ty!(((tuple(int, int)) | [int])),
    true,
    "(1, 1)"
);

test_shape!(
    test_perfect_tuple_beats_earlier_perfect_list,
    "[1, 1]",
    baml_ty!(([int] | (tuple(int, int)))),
    true,
    "(1, 1)"
);

// The same holds when the answer has to be dug out of prose.
test_shape!(
    test_perfect_tuple_beats_perfect_list_in_prose,
    "Here you go:\n```json\n[1, 1]\n```",
    baml_ty!(([int] | (tuple(int, int)))),
    true,
    "(1, 1)"
);

// A second perfect list does not end the search before the tuple is tried.
test_shape!(
    test_perfect_tuple_beats_two_perfect_lists,
    "[1, 1]",
    baml_ty!(([int] | [(int | string)] | (tuple(int, int)))),
    true,
    "(1, 1)"
);

// Only a tuple that matches without repair wins: dropping an extra item is a repair.
test_shape!(
    test_list_beats_tuple_with_extra_items,
    "[1, 2, 3]",
    baml_ty!(((tuple(int, int)) | [int])),
    true,
    "[1, 2, 3]"
);

// Coercing an element is a repair too.
test_shape!(
    test_list_beats_tuple_with_coerced_elements,
    "[1.5, 2.5]",
    baml_ty!(((tuple(int, int)) | [float])),
    true,
    "[1.5, 2.5]"
);

// The first element only fits the list, which then becomes the hint for the second; the hinted
// list still yields to the perfect tuple.
test_shape!(
    test_perfect_tuple_beats_hinted_list,
    "[[1, 2, 3], [3, 4]]",
    baml_ty!([((tuple(int, int)) | [int])]),
    true,
    "[[1, 2, 3], (3, 4)]"
);

test_shape!(
    test_perfect_tuple_beats_hinted_list_list_first,
    "[[1, 2, 3], [3, 4]]",
    baml_ty!([([int] | (tuple(int, int)))]),
    true,
    "[[1, 2, 3], (3, 4)]"
);

test_shape!(
    test_list_of_tuple_or_list_picks_tuples,
    "[[1, 2], [3, 4]]",
    baml_ty!([([int] | (tuple(int, int)))]),
    true,
    "[(1, 2), (3, 4)]"
);

// Partial answers follow the same rules.
test_shape!(
    test_partial_perfect_tuple_beats_perfect_list,
    "[1, 1, ",
    baml_ty!(([int] | (tuple(int, int)))),
    false,
    "(1, 1)"
);

// `[1, ` has a perfect partial list but no partial pair (the second int has no default).
test_shape!(
    test_partial_list_while_tuple_has_no_partial,
    "[1, ",
    baml_ty!(([int] | (tuple(int, int)))),
    false,
    "[1]"
);

test_shape!(
    test_partial_perfect_tuple_beats_hinted_list,
    "[[1, 2, 3], [3, 4]",
    baml_ty!([([int] | (tuple(int, int)))]),
    false,
    "[[1, 2, 3], (3, 4)]"
);

// Both the strict (`try_cast`) and the coercing union paths prefer the perfect tuple, with and
// without a list hint.
#[test]
fn test_union_paths_prefer_perfect_tuple() {
    use crate::deserializer::coercer::TypeCoercer;
    let db: TypeRefDb<'_, &str> = baml_db! {};
    let ty: Ty<'_, &str> = baml_ty!(([int] | (tuple(int, int))));
    let TyResolvedRef::Union(union) = db.resolve(&ty).unwrap() else {
        panic!("a union");
    };
    let value = complete_array(vec![num(1), num(1)]);
    for hint in [None, Some(0), Some(1)] {
        let mut ctx = crate::deserializer::coercer::ParsingContext::new(&db);
        ctx.union_variant_hint = hint;
        let strict = crate::sap_model::UnionTy::try_cast(&ctx, union, &value).expect("casts");
        assert_eq!(shape(&strict), "(1, 1)", "try_cast with hint {hint:?}");
        let coerced = crate::sap_model::UnionTy::coerce(&ctx, union, &value)
            .expect("coerces")
            .expect("has a value");
        assert_eq!(shape(&coerced), "(1, 1)", "coerce with hint {hint:?}");
    }
}

// ============================================================================
// Nesting and recursion
// ============================================================================

test_deserializer!(
    test_nested_tuple,
    r#"[1, ["x", true]]"#,
    baml_ty!(tuple(int, (tuple(string, bool)))),
    baml_db! {},
    [1, ["x", true]]
);

test_deserializer!(
    test_tuple_class_field,
    r#"{"pair": [1, "a"]}"#,
    baml_ty!(Holder),
    baml_db! {
        class Holder { pair: (tuple(int, string)), }
    },
    {"pair": [1, "a"]}
);

// type Cons = (int, Cons | null)
test_deserializer!(
    test_recursive_alias_through_tuple,
    r#"[1, [2, [3, null]]]"#,
    baml_ty!(Cons),
    baml_db! { type Cons = (tuple(int, (Cons | null))); },
    [1, [2, [3, null]]]
);

// type T = int | (T,): a string can never parse, and the implied 1-tuple must not recurse
// forever re-trying `T` on the same value.
#[test]
fn test_recursive_alias_through_one_tuple_terminates() {
    let db: TypeRefDb<'_, &str> = baml_db! { type T = (int | (tuple(T))); };
    let target_ty: Ty<'_, &str> = baml_ty!(T);
    let parsed = crate::jsonish::parse(
        r#""not a number""#,
        crate::jsonish::ParseOptions::default(),
        true,
    )
    .expect("jsonish::parse failed");
    let ctx = crate::deserializer::coercer::ParsingContext::new(&db);
    let target_ty = db.resolve(&target_ty).unwrap();
    let result = TyResolvedRef::coerce(&ctx, target_ty, &parsed);
    assert!(
        !matches!(result, Ok(Some(_))),
        "a string is neither an int nor a tuple of them"
    );
}

test_deserializer!(
    test_recursive_alias_through_one_tuple_wraps,
    r#"[[5]]"#,
    baml_ty!(T),
    baml_db! { type T = (int | (tuple(T))); },
    [[5]]
);

// ============================================================================
// Streaming
// ============================================================================

// The first item is still arriving (`1` may become `12`) and ints have no partial parse.
test_partial_none_deserializer!(
    test_partial_tuple_first_int_in_progress,
    r#"[1"#,
    baml_ty!(tuple(int, string)),
    baml_db! {}
);

// The string slot has a default (`""`), so the tuple shows as soon as the int completes.
test_partial_deserializer!(
    test_partial_tuple_pending_string_defaults,
    r#"[1,"#,
    baml_ty!(tuple(int, string)),
    baml_db! {},
    [1, ""]
);

test_partial_deserializer!(
    test_partial_tuple_string_in_progress,
    r#"[1, "a"#,
    baml_ty!(tuple(int, string)),
    baml_db! {},
    [1, "a"]
);

test_deserializer!(
    test_partial_tuple_complete,
    r#"[1, "a"]"#,
    baml_ty!(tuple(int, string)),
    baml_db! {},
    [1, "a"]
);

// The int slot has no default, so nothing shows until it arrives.
test_partial_none_deserializer!(
    test_partial_tuple_pending_int_yields_nothing,
    r#"["a","#,
    baml_ty!(tuple(string, int)),
    baml_db! {}
);

test_partial_deserializer!(
    test_partial_tuple_after_pending_int_arrives,
    r#"["a", 1, "#,
    baml_ty!(tuple(string, int, [string])),
    baml_db! {},
    ["a", 1, []]
);

test_partial_deserializer!(
    test_partial_tuple_nested_defaults,
    r#"["#,
    baml_ty!(tuple((string | null), (tuple(string, [int])))),
    baml_db! {},
    [null, ["", []]]
);

// A class field holding a tuple with no partial parse holds the whole class back only if the
// tuple itself has no default.
test_partial_none_deserializer!(
    test_partial_class_with_pending_tuple_field,
    r#"{"pair": [1"#,
    baml_ty!(Holder),
    baml_db! {
        class Holder { pair: (tuple(int, string)), }
    }
);

test_partial_deserializer!(
    test_partial_class_with_tuple_field_in_progress,
    r#"{"pair": [1, "a"#,
    baml_ty!(Holder),
    baml_db! {
        class Holder { pair: (tuple(int, string)), }
    },
    {"pair": [1, "a"]}
);

// ============================================================================
// Scoring and candidate selection
// ============================================================================

fn coerce_one<'t>(
    db: &'t TypeRefDb<'t, &'static str>,
    ty: &'t Ty<'t, &'static str>,
    value: &'t crate::jsonish::Value<'t>,
) -> crate::deserializer::types::BamlValueWithFlags<'t, 't, 't, &'static str> {
    let ctx = crate::deserializer::coercer::ParsingContext::new(db);
    let target = db.resolve(ty).unwrap();
    TyResolvedRef::coerce(&ctx, target, value)
        .expect("coerces")
        .expect("has a value")
}

fn complete_array(items: Vec<crate::jsonish::Value<'static>>) -> crate::jsonish::Value<'static> {
    crate::jsonish::Value::Array(items, crate::jsonish::CompletionState::Complete)
}

fn num(n: i64) -> crate::jsonish::Value<'static> {
    crate::jsonish::Value::Number(n.into(), crate::jsonish::CompletionState::Complete)
}

fn float(f: f64) -> crate::jsonish::Value<'static> {
    crate::jsonish::Value::Number(
        serde_json::Number::from_f64(f).unwrap(),
        crate::jsonish::CompletionState::Complete,
    )
}

#[test]
fn test_tuple_score_sums_element_scores() {
    let db: TypeRefDb<'_, &str> = baml_db! {};
    let ty: Ty<'_, &str> = baml_ty!(tuple(int, int));
    let clean = complete_array(vec![num(1), num(2)]);
    let coerced = complete_array(vec![float(1.5), float(2.5)]);
    assert_eq!(coerce_one(&db, &ty, &clean).score(), 0);
    // Both elements went through FloatToInt (1 each); the tuple adds nothing of its own.
    assert_eq!(coerce_one(&db, &ty, &coerced).score(), 2);
}

#[test]
fn test_tuple_extra_items_score_like_extra_keys() {
    let db: TypeRefDb<'_, &str> = baml_db! {};
    let ty: Ty<'_, &str> = baml_ty!(tuple(int, int));
    let value = complete_array(vec![num(1), num(2), num(3), num(4)]);
    let parsed = coerce_one(&db, &ty, &value);
    let extras = parsed
        .conditions()
        .flags()
        .iter()
        .filter(|f| {
            matches!(
                f,
                crate::deserializer::deserialize_flags::Flag::TupleExtraItem(..)
            )
        })
        .count();
    assert_eq!(extras, 2);
    assert_eq!(parsed.score(), 2);
}

#[test]
fn test_tuple_missing_items_score_like_omitted_fields() {
    let db: TypeRefDb<'_, &str> = baml_db! {};
    let ty: Ty<'_, &str> = baml_ty!(tuple(int, (int | null), [int]));
    let value = complete_array(vec![num(1)]);
    // OptionalDefaultFromNoValue (1) + DefaultFromNoValue (100).
    assert_eq!(coerce_one(&db, &ty, &value).score(), 101);
}

// ============================================================================
// Defaults
// ============================================================================

#[test]
fn test_tuple_defaults() {
    let db: TypeRefDb<'_, &str> = baml_db! {
        class Row {
            defaults: (tuple(string, [int], (int | null))),
            never: (tuple(string, int)),
        }
    };
    let Some(TyResolvedRef::Class(row)) = db.resolve_name(&"Row") else {
        panic!("Row is a class");
    };
    assert_eq!(
        db.partial_default(&row.fields[0].ty).unwrap(),
        DefaultValue::Tuple(vec![
            DefaultValue::String(String::new()),
            DefaultValue::EmptyArray,
            DefaultValue::Null,
        ])
    );
    assert_eq!(
        db.partial_default(&row.fields[1].ty).unwrap(),
        DefaultValue::Never
    );
    // A complete object may not omit a tuple field.
    assert_eq!(
        db.missing_default(&row.fields[0].ty).unwrap(),
        DefaultValue::Never
    );
}

#[test]
fn test_tuple_type_name() {
    let pair: Ty<'_, &str> = baml_ty!(tuple(int, string));
    let single: Ty<'_, &str> = baml_ty!(tuple(int));
    assert_eq!(pair.type_name(), "(int, string)");
    assert_eq!(single.type_name(), "(int,)");
}
