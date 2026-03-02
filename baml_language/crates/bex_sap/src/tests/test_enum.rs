use super::*;

test_deserializer!(
    test_enum,
    r#"TWO"#,
    enum_ty("Category", vec![variant("ONE"), variant("TWO")]),
    empty_db(),
    "TWO"
);

test_deserializer!(
    case_insensitive,
    r#"two"#,
    enum_ty("Category", vec![variant("ONE"), variant("TWO")]),
    empty_db(),
    "TWO"
);

test_deserializer!(
    with_quotes,
    r#""TWO""#,
    enum_ty("Category", vec![variant("ONE"), variant("TWO")]),
    empty_db(),
    "TWO"
);

test_deserializer!(
    from_enum_list_single,
    r#"["TWO"]"#,
    enum_ty("Category", vec![variant("ONE"), variant("TWO")]),
    empty_db(),
    "TWO"
);

test_deserializer!(
    from_enum_list_multi,
    r#"["TWO", "THREE"]"#,
    enum_ty("Category", vec![variant("ONE"), variant("TWO")]),
    empty_db(),
    "TWO"
);

test_deserializer!(
    from_string_with_extra_text_after_1,
    r#""ONE: The description of k1""#,
    enum_ty("Category", vec![variant("ONE"), variant("TWO")]),
    empty_db(),
    "ONE"
);

test_deserializer!(
    from_string_and_case_mismatch,
    "The answer is One",
    enum_ty("Category", vec![variant("ONE"), variant("TWO")]),
    empty_db(),
    "ONE"
);

test_deserializer!(
    from_string_and_case_mismatch_wrapped,
    "**one** is the answer",
    enum_ty("Category", vec![variant("ONE"), variant("TWO")]),
    empty_db(),
    "ONE"
);

test_deserializer!(
    from_string_and_case_mismatch_upper,
    "**ONE** is the answer",
    enum_ty("PascalCaseCategory", vec![variant("One"), variant("Two")]),
    empty_db(),
    "One"
);

test_deserializer!(
    from_string_with_extra_text_after_2,
    r#""ONE - The description of an enum value""#,
    enum_ty("Category", vec![variant("ONE"), variant("TWO")]),
    empty_db(),
    "ONE"
);

test_deserializer!(
    case_sensitive_non_ambiguous_match,
    r#"TWO" is one of the correct answers."#,
    enum_ty("Category", vec![variant("ONE"), variant("TWO")]),
    empty_db(),
    "TWO"
);

test_failing_deserializer!(
    case_insensitive_ambiguous_match,
    r#"Two" is one of the correct answers."#,
    enum_ty("Category", vec![variant("ONE"), variant("TWO")]),
    empty_db()
);

test_failing_deserializer!(
    from_string_with_extra_text_after_3,
    r#""ONE - is the answer, not TWO""#,
    enum_ty("Category", vec![variant("ONE"), variant("TWO")]),
    empty_db()
);

test_failing_deserializer!(
    from_string_with_extra_text_after_4,
    r#""ONE. is the answer, not TWO""#,
    enum_ty("Category", vec![variant("ONE"), variant("TWO")]),
    empty_db()
);

test_failing_deserializer!(
    from_string_with_extra_text_after_5,
    r#""ONE: is the answer, not TWO""#,
    enum_ty("Category", vec![variant("ONE"), variant("TWO")]),
    empty_db()
);

test_deserializer!(
    aliases_1,
    r#"k1"#,
    enum_ty(
        "Category",
        vec![
            variant_with_aliases("ONE", vec!["k1"]),
            variant_with_aliases("TWO", vec!["k-2-3.1_1"]),
            variant_with_aliases("THREE", vec!["NUMBER THREE"]),
        ]
    ),
    empty_db(),
    "ONE"
);

test_deserializer!(
    aliases_2,
    r#"k-2-3.1_1"#,
    enum_ty(
        "Category",
        vec![
            variant_with_aliases("ONE", vec!["k1"]),
            variant_with_aliases("TWO", vec!["k-2-3.1_1"]),
            variant_with_aliases("THREE", vec!["NUMBER THREE"]),
        ]
    ),
    empty_db(),
    "TWO"
);

test_deserializer!(
    aliases_3,
    r#"NUMBER THREE"#,
    enum_ty(
        "Category",
        vec![
            variant_with_aliases("ONE", vec!["k1"]),
            variant_with_aliases("TWO", vec!["k-2-3.1_1"]),
            variant_with_aliases("THREE", vec!["NUMBER THREE"]),
        ]
    ),
    empty_db(),
    "THREE"
);

test_deserializer!(
    no_punctuation,
    r#"number three"#,
    enum_ty(
        "Category",
        vec![
            variant_with_aliases("ONE", vec!["k1"]),
            variant_with_aliases("TWO", vec!["k-2-3.1_1"]),
            variant_with_aliases("THREE", vec!["NUMBER THREE"]),
        ]
    ),
    empty_db(),
    "THREE"
);

test_deserializer!(
    no_punctuation_2,
    r#"k-2-3 1_1"#,
    enum_ty(
        "Category",
        vec![
            variant_with_aliases("ONE", vec!["k1"]),
            variant_with_aliases("TWO", vec!["k-2-3.1_1"]),
            variant_with_aliases("THREE", vec!["NUMBER THREE"]),
        ]
    ),
    empty_db(),
    "TWO"
);

test_deserializer!(
    descriptions,
    r#"k1: The description of enum value une"#,
    enum_ty(
        "Category",
        vec![
            variant_with_aliases("ONE", vec!["k1"]),
            variant_with_aliases("TWO", vec!["k-2-3.1_1"]),
            variant_with_aliases("THREE", vec!["NUMBER THREE"]),
        ]
    ),
    empty_db(),
    "ONE"
);

test_deserializer!(
    descriptions_whitespace,
    r#"k-2-3.1_1 The description of enum value deux"#,
    enum_ty(
        "Category",
        vec![
            variant_with_aliases("ONE", vec!["k1"]),
            variant_with_aliases("TWO", vec!["k-2-3.1_1"]),
            variant_with_aliases("THREE", vec!["NUMBER THREE"]),
        ]
    ),
    empty_db(),
    "TWO"
);

test_deserializer!(
    descriptions_period,
    r#"k-2-3.1_1. The description of enum value deux"#,
    enum_ty(
        "Category",
        vec![
            variant_with_aliases("ONE", vec!["k1"]),
            variant_with_aliases("TWO", vec!["k-2-3.1_1"]),
            variant_with_aliases("THREE", vec!["NUMBER THREE"]),
        ]
    ),
    empty_db(),
    "TWO"
);

test_deserializer!(
    alias_with_text,
    r#"I would think k-2-3.1_1 is the best"#,
    enum_ty(
        "Category",
        vec![
            variant_with_aliases("ONE", vec!["k1"]),
            variant_with_aliases("TWO", vec!["k-2-3.1_1"]),
            variant_with_aliases("THREE", vec!["NUMBER THREE"]),
        ]
    ),
    empty_db(),
    "TWO"
);

test_deserializer!(
    multi_aliases,
    r#"k1 is the best! k-2-3.1_1 is bad. k1!"#,
    enum_ty(
        "Category",
        vec![
            variant_with_aliases("ONE", vec!["k1"]),
            variant_with_aliases("TWO", vec!["k-2-3.1_1"]),
            variant_with_aliases("THREE", vec!["NUMBER THREE"]),
        ]
    ),
    empty_db(),
    "ONE"
);

test_deserializer!(
    multi_aliases_1,
    r#"k1 is ok! k-2-3.1_1 is better. I would advise k-2-3.1_1!"#,
    enum_ty(
        "Category",
        vec![
            variant_with_aliases("ONE", vec!["k1"]),
            variant_with_aliases("TWO", vec!["k-2-3.1_1"]),
            variant_with_aliases("THREE", vec!["NUMBER THREE"]),
        ]
    ),
    empty_db(),
    "TWO"
);

// Too many ties
test_failing_deserializer!(
    multi_aliases_2,
    r#"k1 is the best! k-2-3.1_1 is bad. NUMBER_THREE!"#,
    enum_ty(
        "Category",
        vec![
            variant_with_aliases("ONE", vec!["k1"]),
            variant_with_aliases("TWO", vec!["k-2-3.1_1"]),
            variant_with_aliases("THREE", vec!["NUMBER THREE"]),
        ]
    ),
    empty_db()
);

#[test]
fn list_of_enums() {
    let category_enum = enum_ty(
        "Category",
        vec![
            variant_with_aliases("ONE", vec!["k1"]),
            variant_with_aliases("TWO", vec!["k-2-3.1_1"]),
            variant_with_aliases("THREE", vec!["NUMBER THREE"]),
        ],
    );
    let mut db = TypeRefDb::new();
    assert!(db.try_add("Category", category_enum).is_ok());
    let target_ty = array_of(annotated(Ty::Unresolved("Category")));

    let raw = r#"["k1", "k-2-3.1_1"]"#;
    let parsed = crate::jsonish::parse(raw, Default::default(), true).unwrap();
    let ctx = crate::deserializer::coercer::ParsingContext::new(target_ty.as_ref(), &db);
    let annots = TypeAnnotations::default();
    let target = TyWithMeta::new(target_ty.as_ref(), &annots);
    let result = TyResolvedRef::coerce(&ctx, target, &parsed);
    assert!(result.is_ok());
    let value = result.unwrap().unwrap();
    let json_value = serde_json::to_value(&value).unwrap();
    let expected = serde_json::json!(["ONE", "TWO"]);
    assert_eq!(json_value, expected);
}

#[test]
fn list_of_enums_2() {
    let category_enum = enum_ty(
        "Category",
        vec![
            variant_with_aliases("ONE", vec!["k1"]),
            variant_with_aliases("TWO", vec!["k-2-3.1_1"]),
            variant_with_aliases("THREE", vec!["NUMBER THREE"]),
        ],
    );
    let mut db = TypeRefDb::new();
    assert!(db.try_add("Category", category_enum).is_ok());
    let target_ty = array_of(annotated(Ty::Unresolved("Category")));

    let raw = r#"I would think something like this!
```json
[k1, "k-2-3.1_1", "NUMBER THREE"]
```
"#;
    let parsed = crate::jsonish::parse(raw, Default::default(), true).unwrap();
    let ctx = crate::deserializer::coercer::ParsingContext::new(target_ty.as_ref(), &db);
    let annots = TypeAnnotations::default();
    let target = TyWithMeta::new(target_ty.as_ref(), &annots);
    let result = TyResolvedRef::coerce(&ctx, target, &parsed);
    assert!(result.is_ok());
    let value = result.unwrap().unwrap();
    let json_value = serde_json::to_value(&value).unwrap();
    let expected = serde_json::json!(["ONE", "TWO", "THREE"]);
    assert_eq!(json_value, expected);
}

test_deserializer!(
    test_numerical_enum,
    r#"
(such as 1040-X, 1040, etc.) or any payment vouchers.

Based on the criteria provided, this page does not qualify as a tax return form page. Therefore, the appropriate response is:

```json
null
```

This indicates that there is no relevant tax return form type present on the page.
    "#,
    optional(enum_ty(
        "TaxReturnFormType",
        vec![
            variant_with_aliases("F9325", vec!["9325"]),
            variant_with_aliases("F9465", vec!["9465"]),
            variant_with_aliases("F1040", vec!["1040"]),
            variant_with_aliases("F1040X", vec!["1040-X"]),
        ]
    )),
    empty_db(),
    null
);

test_failing_deserializer!(
    test_ambiguous_substring_enum,
    "The answer is not car or car-2!",
    enum_ty(
        "Car",
        vec![
            variant_with_aliases("A", vec!["car"]),
            variant_with_aliases("B", vec!["car-2"]),
        ]
    ),
    empty_db()
);

test_deserializer!(
    test_weird_characters,
    r#"
The text "Buy cheap watches now! Limited time offer!!!" is typically characterized by unsolicited
offers and urgency ($^{$_{Ω}$rel}$), which are common traits of spam messages. Therefore, it should be classified as:

- **SPAM**
    "#,
    enum_ty("MessageType", vec![variant("SPAM"), variant("NOT_SPAM")]),
    empty_db(),
    "SPAM"
);

// test_enum_from_string is skipped because it uses `res.meta_mut().streaming_behavior.done = true`
