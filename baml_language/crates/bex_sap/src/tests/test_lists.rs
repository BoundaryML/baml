use crate::{
    baml_db, baml_ty,
    deserializer::coercer::ParsingContext,
    sap_model::{ArrayTy, MediaTy, Ty, TyResolved, TyResolvedRef, TypeRefDb},
};

test_deserializer!(
    test_list,
    r#"["a", "b"]"#,
    baml_ty!([string]),
    baml_db! {},
    ["a", "b"]
);

test_deserializer!(
    test_list_with_quotes,
    r#"["\"a\"", "\"b\""]"#,
    baml_ty!([string]),
    baml_db! {},
    ["\"a\"", "\"b\""]
);

test_deserializer!(
    test_list_with_extra_text,
    r#"["a", "b"] is the output."#,
    baml_ty!([string]),
    baml_db! {},
    ["a", "b"]
);

test_deserializer!(
    test_list_with_invalid_extra_text,
    r#"[a, b] is the output."#,
    baml_ty!([string]),
    baml_db! {},
    ["a", "b"]
);

test_deserializer!(
    test_list_object_from_string,
    r#"[{"a": 1, "b": "hello"}, {"a": 2, "b": "world"}]"#,
    baml_ty!([Foo]),
    baml_db!{
        class Foo {
            a: int,
            b: string,
        }
    },
    [{"a": 1, "b": "hello"}, {"a": 2, "b": "world"}]
);

test_deserializer!(
    test_class_list,
    r#"
    [
    {
      "date": "01/01",
      "description": "Transaction 1",
      "transaction_amount": -100.00,
      "transaction_type": "Withdrawal"
    },
    {
      "date": "01/02",
      "description": "Transaction 2",
      "transaction_amount": -2,000.00,
      "transaction_type": "Withdrawal"
    },
    {
      "date": "01/03",
      "description": "Transaction 3",
      "transaction_amount": -300.00,
      "transaction_type": "Withdrawal"
    },
    {
      "date": "01/04",
      "description": "Transaction 4",
      "transaction_amount": -4,000.00,
      "transaction_type": "Withdrawal"
    },
    {
      "date": "01/05",
      "description": "Transaction 5",
      "transaction_amount": -5,000.00,
      "transaction_type": "Withdrawal"
    }
  ]
    "#,
    baml_ty!([ListClass]),
    baml_db!{
        class ListClass {
            date: string,
            description: string,
            transaction_amount: float,
            transaction_type: string,
        }
    },
    [
        {
          "date": "01/01",
          "description": "Transaction 1",
          "transaction_amount": -100.00,
          "transaction_type": "Withdrawal"
        },
        {
          "date": "01/02",
          "description": "Transaction 2",
          "transaction_amount": -2000.00,
          "transaction_type": "Withdrawal"
        },
        {
          "date": "01/03",
          "description": "Transaction 3",
          "transaction_amount": -300.00,
          "transaction_type": "Withdrawal"
        },
        {
          "date": "01/04",
          "description": "Transaction 4",
          "transaction_amount": -4000.00,
          "transaction_type": "Withdrawal"
        },
        {
          "date": "01/05",
          "description": "Transaction 5",
          "transaction_amount": -5000.00,
          "transaction_type": "Withdrawal"
        }
      ]
);

// The trailing number may still grow, so it has no partial parse yet.
test_partial_deserializer!(
    test_list_streaming,
    r#"[1234, 5678"#,
    baml_ty!([int]),
    baml_db! {},
    [1234]
);

test_partial_deserializer!(
    test_list_streaming_2,
    r#"[1234"#,
    baml_ty!([int]),
    baml_db! {},
    []
);

test_partial_deserializer!(
    test_list_streaming_inside_json_block,
    r#"```json
["a","#,
    baml_ty!([string]),
    baml_db! {},
    ["a"]
);

// ============================================================================
// Arrays with nullable elements
// ============================================================================

test_deserializer!(
    test_array_nullable_element,
    r#"[1, 2, 3]"#,
    baml_ty!([(int | null)]),
    baml_db! {},
    [1, 2, 3]
);

// An incomplete number has no partial parse and `null` is not one either: the item waits.
test_partial_deserializer!(
    test_array_nullable_element_partial_item_dropped,
    r#"[1, 2"#,
    baml_ty!([(int | null)]),
    baml_db! {},
    [1]
);

test_deserializer!(
    test_array_nullable_element_accepts_null,
    r#"[1, null, 3]"#,
    baml_ty!([(int | null)]),
    baml_db! {},
    [1, null, 3]
);

// ============================================================================
// Items that do not fit the element type
// ============================================================================
//
// A list the model wrote holds only values of its element type. An item that
// is complete and does not fit fails the list: leaving it out would return a
// shorter list, or an empty one, that the caller cannot tell from a good
// reply. Four cases keep the item out without an error, and each has a test
// below: an item that is still arriving, a list the parser gathered itself
// from separate values in the text, `null` where a list is expected, and a
// list of media.

/// The rendered error of a complete parse of `raw` into `target`.
fn final_parse_error(raw: &str, target: &Ty<'_, &str>, db: &TypeRefDb<'_, &str>) -> String {
    let parsed = crate::jsonish::parse(raw, crate::jsonish::ParseOptions::default(), true)
        .expect("jsonish::parse failed");
    let ctx = ParsingContext::new(db);
    let target = db.resolve(target).unwrap();
    match TyResolvedRef::coerce(&ctx, target, &parsed) {
        Err(error) => error.to_string(),
        Ok(value) => panic!(
            "Parsing should have failed, got: {:?}",
            value.map(|value| ::serde_json::to_value(&value).unwrap())
        ),
    }
}

/// `image[]`: media is read from the reply's media parts, never from its text.
fn image_list() -> Ty<'static, &'static str> {
    Ty::Resolved(TyResolved::Array(ArrayTy {
        ty: Box::new(Ty::Resolved(TyResolved::Media(MediaTy::Image))),
    }))
}

test_failing_deserializer!(
    test_list_item_that_does_not_fit_fails,
    r#"[1, "two", 3]"#,
    baml_ty!([int]),
    baml_db! {}
);

test_failing_deserializer!(
    test_list_null_item_does_not_fit,
    r#"[1, null, 3]"#,
    baml_ty!([int]),
    baml_db! {}
);

test_failing_deserializer!(
    test_nested_list_item_that_does_not_fit_fails,
    r#"[[1, 2], [3, "x"]]"#,
    baml_ty!([[int]]),
    baml_db! {}
);

#[test]
fn test_list_item_error_names_the_item_and_the_reason() {
    let error = final_parse_error(r#"[1, "two", 3]"#, &baml_ty!([int]), &baml_db! {});
    assert!(
        error.contains("Failed to parse list item 1 as int"),
        "the error names the item: {error}"
    );
    assert!(
        error.contains(r#"Expected int, got String("two", Complete)"#),
        "the error carries the item's own failure: {error}"
    );
}

fn agent_reply_db() -> TypeRefDb<'static, &'static str> {
    baml_db! {
        class Search {
            kind: "search",
            query: string,
        }
        class Calculate {
            kind: "calculate",
            expression: string,
        }
        class Action {
            reason: string,
            tool: (Search | Calculate),
        }
        class Reply {
            actions: [Action],
        }
    }
}

// The reply that showed the defect: the one action names a tool outside the
// union, and the list came back empty.
test_failing_deserializer!(
    test_list_of_classes_with_only_an_unfit_item_fails,
    r#"{"actions": [{"reason": "look it up", "tool": {"kind": "browse", "url": "https://example.com"}}]}"#,
    baml_ty!(Reply),
    agent_reply_db()
);

test_failing_deserializer!(
    test_list_of_classes_with_one_unfit_item_fails,
    r#"{"actions": [
        {"reason": "look it up", "tool": {"kind": "search", "query": "weather"}},
        {"reason": "open it", "tool": {"kind": "browse", "url": "https://example.com"}}
    ]}"#,
    baml_ty!(Reply),
    agent_reply_db()
);

#[test]
fn test_list_of_classes_error_names_the_field_and_the_item() {
    let error = final_parse_error(
        r#"{"actions": [
            {"reason": "look it up", "tool": {"kind": "search", "query": "weather"}},
            {"reason": "open it", "tool": {"kind": "browse", "url": "https://example.com"}}
        ]}"#,
        &baml_ty!(Reply),
        &agent_reply_db(),
    );
    assert!(
        error.contains("actions: Failed to parse list item 1 as Action"),
        "the error names the field and the item: {error}"
    );
    assert!(
        error.contains(r#"Expected literal["search"], got String("browse", Complete)"#),
        "the error carries the reason the item does not fit: {error}"
    );
}

test_deserializer!(
    test_list_of_classes_that_all_fit,
    r#"{"actions": [
        {"reason": "look it up", "tool": {"kind": "search", "query": "weather"}},
        {"reason": "add", "tool": {"kind": "calculate", "expression": "1 + 1"}}
    ]}"#,
    baml_ty!(Reply),
    agent_reply_db(),
    {"actions": [
        {"reason": "look it up", "tool": {"kind": "search", "query": "weather"}},
        {"reason": "add", "tool": {"kind": "calculate", "expression": "1 + 1"}}
    ]}
);

// ── a single value where a list is expected ─────────────────────────────────

// One value that fits the element type is a list of that value.
test_deserializer!(
    test_single_value_that_fits_is_a_list_of_one,
    r#"3"#,
    baml_ty!([int]),
    baml_db! {},
    [3]
);

test_deserializer!(
    test_single_class_that_fits_is_a_list_of_one,
    r#"{"actions": {"reason": "look it up", "tool": {"kind": "search", "query": "weather"}}}"#,
    baml_ty!(Reply),
    agent_reply_db(),
    {"actions": [{"reason": "look it up", "tool": {"kind": "search", "query": "weather"}}]}
);

// One value that does not fit is not an empty list.
test_failing_deserializer!(
    test_single_value_that_does_not_fit_fails,
    r#"two"#,
    baml_ty!([int]),
    baml_db! {}
);

test_failing_deserializer!(
    test_single_class_that_does_not_fit_fails,
    r#"{"actions": {"reason": "open it", "tool": {"kind": "browse", "url": "https://example.com"}}}"#,
    baml_ty!(Reply),
    agent_reply_db()
);

#[test]
fn test_single_value_error_names_the_list_and_the_reason() {
    let error = final_parse_error("two", &baml_ty!([int]), &baml_db! {});
    assert!(
        error.contains(r#"Expected int[], got String("two", Complete)"#),
        "the error names the list it expected: {error}"
    );
    assert!(
        error.contains(r#"<implied>: Expected int, got String("two", Complete)"#),
        "the error carries the reason the value is not an item: {error}"
    );
}

// `null` is the absence of a list, as a missing field is: both read as empty.
test_deserializer!(
    test_null_for_a_list_is_empty,
    r#"null"#,
    baml_ty!([int]),
    baml_db! {},
    []
);

test_deserializer!(
    test_null_and_missing_list_fields_are_empty,
    r#"{"first": null}"#,
    baml_ty!(Lists),
    baml_db! {
        class Lists {
            first: [string],
            second: [string],
        }
    },
    {"first": [], "second": []}
);

// ── items that are left out without an error ────────────────────────────────

// The last object is still arriving: its `a` cannot be an int, but the object
// is not closed, so the partial list leaves it out.
test_partial_deserializer!(
    test_partial_list_leaves_out_an_unfit_item_that_is_still_arriving,
    r#"[{"a": 1}, {"a": "x", "b": 2"#,
    baml_ty!([Foo]),
    baml_db! {
        class Foo {
            a: int,
        }
    },
    [{"a": 1}]
);

// Once that object closes the item is complete, and the list fails.
test_failing_deserializer!(
    test_complete_list_fails_on_the_item_a_partial_left_out,
    r#"[{"a": 1}, {"a": "x", "b": 2}]"#,
    baml_ty!([Foo]),
    baml_db! {
        class Foo {
            a: int,
        }
    }
);

// A complete item that does not fit will not come to fit: the partial parse
// fails as the complete one will.
test_partial_failing_deserializer!(
    test_partial_list_fails_on_a_complete_unfit_item,
    r#"[1, "two", 3"#,
    baml_ty!([int]),
    baml_db! {}
);

// The model wrote no list: the parser gathered the JSON objects it found in
// the text and offers them as one. Gathering keeps what fits.
test_deserializer!(
    test_objects_gathered_from_text_keep_what_fits,
    r#"Here is one: {"a": 1} and another: {"a": 2} and something else: {"b": 3}"#,
    baml_ty!([Foo]),
    baml_db! {
        class Foo {
            a: int,
        }
    },
    [{"a": 1}, {"a": 2}]
);

test_deserializer!(
    test_malformed_objects_gathered_from_text_keep_what_fits,
    r#"Here is one: {a: 1} and another: {a: 2} and something else: {b: 3}"#,
    baml_ty!([Foo]),
    baml_db! {
        class Foo {
            a: int,
        }
    },
    [{"a": 1}, {"a": 2}]
);

test_deserializer!(
    test_code_blocks_gathered_from_text_keep_what_fits,
    r#"First:
```json
{"a": 1}
```
Second:
```json
{"a": 2}
```
And a different one:
```json
{"b": 3}
```
"#,
    baml_ty!([Foo]),
    baml_db! {
        class Foo {
            a: int,
        }
    },
    [{"a": 1}, {"a": 2}]
);

// Media never comes from text, so text holds no item of a media list: the
// list is empty whatever the text says, and the caller fills it from the
// reply's media parts.
test_deserializer!(
    test_media_list_from_prose_is_empty,
    r#"I cannot draw that."#,
    image_list(),
    baml_db! {},
    []
);

test_deserializer!(
    test_media_list_from_a_list_is_empty,
    r#"["a lamp", "a second lamp"]"#,
    image_list(),
    baml_db! {},
    []
);
