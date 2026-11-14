use crate::{baml_db, baml_ty};

// test_union: union of two classes, Foo and Bar
test_deserializer!(
    test_union,
    r#"{"hi": ["a", "b"]}"#,
    baml_ty!((Foo | Bar)),
    baml_db!{
        class Foo {
            hi: [string],
        }
        class Bar {
            foo: string,
        }
    },
    {"hi": ["a", "b"]}
);

// test_union_full: skipped (uses old internal APIs like BamlValueWithFlags::Class, field_type())

// test_union2: union of 3 classes with enum fields
test_deserializer!(
    test_union2,
    r#"```json
  {
    "cat": "E",
    "item": "28558C",
    "data": null
  }
  ```"#,
    baml_ty!((CatAPicker | CatBPicker | CatCPicker)),
    baml_db!{
        enum CatA {
            A
        }
        enum CatB {
            C,
            D
        }
        enum CatC {
            E,
            F,
            G,
            H,
            I
        }
        class CatAPicker {
            cat: CatA,
        }
        class CatBPicker {
            cat: CatB,
            item: int,
        }
        class CatCPicker {
            cat: CatC,
            item: (int | string | null),
            data: (int | null),
        }
    },
    {
        "cat": "E",
        "item": "28558C",
        "data": null
    }
);

// test_union3: complex schema with many classes and enums
test_deserializer!(
    test_union3,
    r####"```json
{
  "action": "RespondToUserAPI",
  "sections": [
    {
      "section_title": "NVIDIA Corporation (NVDA) Latest Earnings Summary",
      "types": ["CompanyBadge", "Markdown", "BarGraph"],
      "content": {
        "companyBadge": {
          "name": "NVIDIA Corporation",
          "symbol": "NVDA",
          "logo_url": "https://upload.wikimedia.org/wikipedia/en/thumb/2/21/Nvidia_logo.svg/1920px-Nvidia_logo.svg.png"
        },
        "richText": {
          "text": "### Key Metrics for the Latest Earnings Report (2024-08-28)\n\n- **Earnings Per Share (EPS):** $0.68\n- **Estimated EPS:** $0.64\n- **Revenue:** $30.04 billion\n- **Estimated Revenue:** $28.74 billion\n\n#### Notable Highlights\n- NVIDIA exceeded both EPS and revenue estimates for the quarter ending July 28, 2024.\n- The company continues to show strong growth in its data center and gaming segments."
        },
        "barGraph": [
          {
            "name": "Earnings Per Share (EPS)",
            "expected": 0.64,
            "reported": 0.68
          },
          {
            "name": "Revenue (in billions)",
            "expected": 28.74,
            "reported": 30.04
          }
        ]
      }
    }
  ]
}
```"####,
    baml_ty!((RespondToUserAPI | AskClarificationAPI | [AssistantAPI])),
    baml_db!{
        enum AssistantType {
            ETF @alias("ETFAssistantAPI"),
            Stock @alias("StockAssistantAPI")
        }
        enum AskClarificationAction {
            ASK_CLARIFICATION @alias("AskClarificationAPI")
        }
        enum RespondToUserAction {
            RESPOND_TO_USER @alias("RespondToUserAPI")
        }
        enum UIType {
            CompanyBadge @description("Company badge UI type"),
            Markdown @description("Markdown text UI type"),
            NumericalSlider @description("Numerical slider UI type"),
            BarGraph @description("Bar graph UI type"),
            ScatterPlot @description("Scatter plot UI type")
        }
        class AssistantAPI {
            action: AssistantType,
            instruction: string,
            user_message: string,
        }
        class AskClarificationAPI {
            action: AskClarificationAction,
            question: string,
        }
        class MarkdownContent {
            text: string,
        }
        class CompanyBadgeContent {
            name: string,
            symbol: string,
            logo_url: string,
        }
        class NumericalSliderContent {
            title: string,
            min: float,
            max: float,
            value: float,
        }
        class GraphDataPoint {
            name: string,
            expected: float,
            reported: float,
        }
        class ScatterDataPoint {
            x: string,
            y: float,
        }
        class ScatterPlotContent {
            expected: [ScatterDataPoint],
            reported: [ScatterDataPoint],
        }
        class UIContent {
            richText: (MarkdownContent | null),
            companyBadge: (CompanyBadgeContent | null),
            numericalSlider: (NumericalSliderContent | null),
            barGraph: ([GraphDataPoint] | null),
            scatterPlot: (ScatterPlotContent | null),
            foo: (string | null),
        }
        class UI {
            section_title: string,
            r#type: [UIType] @alias("types"),
            content: UIContent,
        }
        class RespondToUserAPI {
            action: RespondToUserAction,
            sections: [UI],
        }
    },
    {
        "action": "RESPOND_TO_USER",
        "sections": [
            {
                "section_title": "NVIDIA Corporation (NVDA) Latest Earnings Summary",
                "type": ["CompanyBadge", "Markdown", "BarGraph"],
                "content": {
                    "companyBadge": {
                        "name": "NVIDIA Corporation",
                        "symbol": "NVDA",
                        "logo_url": "https://upload.wikimedia.org/wikipedia/en/thumb/2/21/Nvidia_logo.svg/1920px-Nvidia_logo.svg.png"
                    },
                    "richText": {
                        "text": "### Key Metrics for the Latest Earnings Report (2024-08-28)\n\n- **Earnings Per Share (EPS):** $0.68\n- **Estimated EPS:** $0.64\n- **Revenue:** $30.04 billion\n- **Estimated Revenue:** $28.74 billion\n\n#### Notable Highlights\n- NVIDIA exceeded both EPS and revenue estimates for the quarter ending July 28, 2024.\n- The company continues to show strong growth in its data center and gaming segments."
                    },
                    "scatterPlot": null,
                    "numericalSlider": null,
                    "barGraph": [
                        {
                            "name": "Earnings Per Share (EPS)",
                            "expected": 0.64,
                            "reported": 0.68
                        },
                        {
                            "name": "Revenue (in billions)",
                            "expected": 28.74,
                            "reported": 30.04
                        }
                    ],
                    "foo": null
                }
            }
        ]
    }
);

test_deserializer!(
    test_ignore_float_in_string_if_string_in_union,
    "1 cup unsalted butter, room temperature",
    baml_ty!((float | string)),
    baml_db! {},
    "1 cup unsalted butter, room temperature"
);

test_deserializer!(
    test_ignore_int_if_string_in_union,
    "1 cup unsalted butter, room temperature",
    baml_ty!((int | string)),
    baml_db! {},
    "1 cup unsalted butter, room temperature"
);

// test_try_cast_union_early_return_preserves_incomplete_flag: skipped (uses internal APIs)

// ============================================================================
// Multi-variant nullable union
// ============================================================================

test_deserializer!(
    test_union_multi_picks_int,
    r#"42"#,
    baml_ty!((string | int | null)),
    baml_db! {},
    42
);

test_deserializer!(
    test_union_multi_picks_string,
    r#""hello""#,
    baml_ty!((string | int | null)),
    baml_db! {},
    "hello"
);

test_deserializer!(
    test_union_multi_accepts_null,
    r#"null"#,
    baml_ty!((string | int | null)),
    baml_db! {},
    null
);

// ============================================================================
// Specificity among perfect matches
// ============================================================================
//
// A member strictly more specific than every other perfect match wins regardless of union
// order; otherwise the leftmost perfect match does. These tests report the member type the
// value was parsed as, since the parsed JSON alone cannot tell `(1,)` from `(int,)`.

mod specificity {
    use crate::{
        baml_db, baml_ty,
        deserializer::{
            coercer::{ParsingContext, TypeCoercer},
            types::BamlValueWithFlags,
        },
        sap_model::{Ty, TyResolvedRef, TypeName, TypeRefDb, UnionTy},
    };

    /// The member type the value parsed as.
    fn chosen(value: &BamlValueWithFlags<'_, '_, '_, &'static str>) -> String {
        value.meta.ty.type_name().into_owned()
    }

    /// The member type each element of a parsed list parsed as.
    fn chosen_elements(value: &BamlValueWithFlags<'_, '_, '_, &'static str>) -> String {
        let crate::baml_value::BamlValue::Array(arr) = &value.value else {
            panic!("a list");
        };
        let items: Vec<_> = arr.value.iter().map(chosen).collect();
        format!("[{}]", items.join(", "))
    }

    /// Parses `raw` at `ty` and returns `report` of the result.
    fn parse_chosen(
        raw: &str,
        ty: &Ty<'_, &'static str>,
        db: &TypeRefDb<'_, &'static str>,
        report: fn(&BamlValueWithFlags<'_, '_, '_, &'static str>) -> String,
    ) -> String {
        let parsed = crate::jsonish::parse(raw, crate::jsonish::ParseOptions::default(), true)
            .expect("jsonish::parse failed");
        let ctx = ParsingContext::new(db);
        let target = db.resolve(ty).unwrap();
        let value = TyResolvedRef::coerce(&ctx, target, &parsed)
            .expect("coerces")
            .expect("has a value");
        report(&value)
    }

    /// Runs the strict (`try_cast`) and coercing union paths directly and checks they both
    /// choose `expected`: with no hint, and, when `expected` is a clear winner, with a hint at
    /// each member. (Without a clear winner a hinted perfect match comes first, as the leftmost
    /// does when there is no hint.)
    fn assert_both_paths(
        raw: &str,
        ty: &Ty<'_, &'static str>,
        db: &TypeRefDb<'_, &'static str>,
        expected: &str,
        clear_winner: bool,
    ) {
        let TyResolvedRef::Union(union) = db.resolve(ty).unwrap() else {
            panic!("a union");
        };
        let value = from_json(serde_json::from_str(raw).expect("valid JSON"));
        let member_hints = (0..union.variants.len()).map(Some).filter(|_| clear_winner);
        let hints = std::iter::once(None).chain(member_hints);
        for hint in hints {
            let mut ctx = ParsingContext::new(db);
            ctx.union_variant_hint = hint;
            let strict = UnionTy::try_cast(&ctx, union, &value).expect("casts");
            assert_eq!(chosen(&strict), expected, "try_cast with hint {hint:?}");
            let coerced = UnionTy::coerce(&ctx, union, &value)
                .expect("coerces")
                .expect("has a value");
            assert_eq!(chosen(&coerced), expected, "coerce with hint {hint:?}");
        }
    }

    /// A complete jsonish value for `json`, as a strict parse would see it.
    fn from_json(json: serde_json::Value) -> crate::jsonish::Value<'static> {
        use crate::jsonish::{CompletionState::Complete, Value};
        match json {
            serde_json::Value::Null => Value::Null,
            serde_json::Value::Bool(b) => Value::Boolean(b),
            serde_json::Value::Number(n) => Value::Number(n, Complete),
            serde_json::Value::String(s) => Value::String(s.into(), Complete),
            serde_json::Value::Array(items) => {
                Value::Array(items.into_iter().map(from_json).collect(), Complete)
            }
            serde_json::Value::Object(fields) => Value::Object(
                fields
                    .into_iter()
                    .map(|(k, v)| (k.into(), from_json(v)))
                    .collect(),
                Complete,
            ),
        }
    }

    /// Checks the member chosen for `$raw` by a full parse and by both union paths. Pass
    /// `leftmost` when no member is a clear winner.
    macro_rules! test_chosen {
        ($name:ident, $raw:expr, $ty:expr, $db:expr, $expected:expr $(, $leftmost:ident)?) => {
            #[test]
            fn $name() {
                let db: TypeRefDb<'_, &str> = $db;
                let ty: Ty<'_, &str> = $ty;
                let clear_winner = true $(&& stringify!($leftmost) != "leftmost")?;
                assert_eq!(parse_chosen($raw, &ty, &db, chosen), $expected, "parse");
                assert_both_paths($raw, &ty, &db, $expected, clear_winner);
            }
        };
    }

    // Tuples: the literal slot wins in either order.
    test_chosen!(
        test_literal_tuple_beats_int_tuple,
        "[1]",
        baml_ty!(((tuple(int)) | (tuple(1)))),
        baml_db! {},
        "(literal[1],)"
    );
    test_chosen!(
        test_literal_tuple_beats_int_tuple_literal_first,
        "[1]",
        baml_ty!(((tuple(1)) | (tuple(int)))),
        baml_db! {},
        "(literal[1],)"
    );

    // Slot by slot: more specific in one slot and equal in the rest still wins.
    test_chosen!(
        test_literal_slot_beats_in_wider_tuple,
        r#"[1, "a"]"#,
        baml_ty!(((tuple(int, string)) | (tuple(1, string)))),
        baml_db! {},
        "(literal[1], string)"
    );

    // Nullability is not ranked: `(int,)` and `(int?,)` have no winner, so the leftmost wins.
    test_chosen!(
        test_int_tuple_or_nullable_tuple_picks_leftmost,
        "[1]",
        baml_ty!(((tuple(int)) | (tuple((int | null))))),
        baml_db! {},
        "(int,)",
        leftmost
    );
    test_chosen!(
        test_nullable_tuple_or_int_tuple_picks_leftmost,
        "[1]",
        baml_ty!(((tuple((int | null))) | (tuple(int)))),
        baml_db! {},
        "(int | null,)",
        leftmost
    );

    // Slots pointing in opposite directions are incomparable: the leftmost wins.
    test_chosen!(
        test_incomparable_tuples_pick_leftmost,
        r#"[1, "a"]"#,
        baml_ty!(((tuple(int, (string | null))) | (tuple((int | null), string)))),
        baml_db! {},
        "(int, string | null)",
        leftmost
    );
    test_chosen!(
        test_incomparable_tuples_pick_leftmost_reversed,
        r#"[1, "a"]"#,
        baml_ty!(((tuple((int | null), string)) | (tuple(int, (string | null))))),
        baml_db! {},
        "(int | null, string)",
        leftmost
    );

    // A nullable literal slot is more specific than a nullable int slot.
    test_chosen!(
        test_nullable_literal_tuple_beats_nullable_int_tuple,
        "[1]",
        baml_ty!(((tuple((int | null))) | (tuple((1 | null))))),
        baml_db! {},
        "(literal[1] | null,)"
    );

    // Lists compare by element.
    test_chosen!(
        test_literal_list_beats_int_list,
        "[1, 1]",
        baml_ty!(([int] | [1])),
        baml_db! {},
        "literal[1][]"
    );
    test_chosen!(
        test_literal_list_beats_int_list_literal_first,
        "[1, 1]",
        baml_ty!(([1] | [int])),
        baml_db! {},
        "literal[1][]"
    );

    // Classes compare field by field.
    test_chosen!(
        test_literal_class_beats_int_class,
        r#"{"x": 1}"#,
        baml_ty!((CI | CL)),
        baml_db! {
            class CI { x: int, }
            class CL { x: 1, }
        },
        "CL"
    );
    test_chosen!(
        test_literal_class_beats_int_class_literal_first,
        r#"{"x": 1}"#,
        baml_ty!((CL | CI)),
        baml_db! {
            class CI { x: int, }
            class CL { x: 1, }
        },
        "CL"
    );

    // Classes with different fields are incomparable.
    test_chosen!(
        test_classes_with_different_fields_pick_leftmost,
        r#"{"x": 1}"#,
        baml_ty!((CI | CL)),
        baml_db! {
            class CI { x: int, }
            class CL { x: 1, y: (int | null), }
        },
        "CI",
        leftmost
    );

    // An enum variant is more specific than its enum.
    test_chosen!(
        test_enum_variant_beats_enum,
        r#""A""#,
        baml_ty!((E | E.A)),
        baml_db! { enum E { A, B } },
        "E.A"
    );

    // A clear winner must beat every other perfect match: `(1,)` beats `(int,)` but not
    // `(int?,)`, so the leftmost wins.
    test_chosen!(
        test_no_clear_winner_among_three_picks_leftmost,
        "[1]",
        baml_ty!(((tuple(int)) | (tuple((int | null))) | (tuple(1)))),
        baml_db! {},
        "(int,)",
        leftmost
    );

    // The first element only fits `(int,)` and becomes the hint for the second, which still
    // goes to the more specific `(1,)`.
    #[test]
    fn test_more_specific_member_beats_hinted_member() {
        let db: TypeRefDb<'_, &str> = baml_db! {};
        for ty in [
            baml_ty!([((tuple(int)) | (tuple(1)))]),
            baml_ty!([((tuple(1)) | (tuple(int)))]),
        ] {
            assert_eq!(
                parse_chosen("[[2], [1]]", &ty, &db, chosen_elements),
                "[(int,), (literal[1],)]"
            );
        }
        for ty in [baml_ty!([([int] | [1])]), baml_ty!([([1] | [int])])] {
            assert_eq!(
                parse_chosen("[[2], [1]]", &ty, &db, chosen_elements),
                "[int[], literal[1][]]"
            );
        }
    }

    // Without a clear winner, a hinted perfect match is kept: after `[null]` picks `(int?,)`,
    // `[1]` stays with it rather than going to the leftmost `(int,)`.
    #[test]
    fn test_hinted_member_wins_without_clear_winner() {
        let db: TypeRefDb<'_, &str> = baml_db! {};
        let ty = baml_ty!([((tuple(int)) | (tuple((int | null))))]);
        assert_eq!(
            parse_chosen("[[null], [1]]", &ty, &db, chosen_elements),
            "[(int | null,), (int | null,)]"
        );
    }

    // A perfect tuple still beats a perfect list, whatever the list's element type; among the
    // perfect tuples, specificity applies.
    test_chosen!(
        test_literal_tuple_beats_list_and_int_tuple,
        "[1, 1]",
        baml_ty!(([1] | (tuple(int, int)) | (tuple(1, 1)))),
        baml_db! {},
        "(literal[1], literal[1])"
    );
    test_chosen!(
        test_perfect_tuple_beats_more_specific_list,
        "[1, 1]",
        baml_ty!(([1] | (tuple(int, int)))),
        baml_db! {},
        "(int, int)"
    );

    // Recursive types compare without looping.
    test_chosen!(
        test_recursive_classes_compare,
        r#"{"x": 1, "next": {"x": 1, "next": null}}"#,
        baml_ty!((NI | NL)),
        baml_db! {
            class NI { x: int, next: (NI | null), }
            class NL { x: 1, next: (NL | null), }
        },
        "NL"
    );
}
