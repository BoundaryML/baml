use super::*;

// test_union: union of two classes, Foo and Bar
test_deserializer!(
    test_union,
    r#"{"hi": ["a", "b"]}"#,
    union_of(vec![
        annotated(Ty::Unresolved("Foo")),
        annotated(Ty::Unresolved("Bar")),
    ]),
    crate::baml_db!{
        class Foo { hi: [string] }
        class Bar { foo: string }
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
    union_of(vec![
        annotated(Ty::Unresolved("CatAPicker")),
        annotated(Ty::Unresolved("CatBPicker")),
        annotated(Ty::Unresolved("CatCPicker")),
    ]),
    {
        let cat_a = enum_ty("CatA", vec![variant("A")]);
        let cat_b = enum_ty("CatB", vec![variant("C"), variant("D")]);
        let cat_c = enum_ty("CatC", vec![
            variant("E"), variant("F"), variant("G"), variant("H"), variant("I"),
        ]);

        let cat_a_picker = class_ty("CatAPicker", vec![
            field("cat", Ty::Unresolved("CatA")),
        ]);
        let cat_b_picker = class_ty("CatBPicker", vec![
            field("cat", Ty::Unresolved("CatB")),
            field("item", int_ty()),
        ]);
        let cat_c_picker = class_ty("CatCPicker", vec![
            field("cat", Ty::Unresolved("CatC")),
            field("item", union_of(vec![
                annotated(int_ty()),
                annotated(string_ty()),
                annotated(null_ty()),
            ])),
            optional_field("data", int_ty()),
        ]);

        let mut db = TypeRefDb::new();
        db.try_add("CatA", cat_a).ok().unwrap();
        db.try_add("CatB", cat_b).ok().unwrap();
        db.try_add("CatC", cat_c).ok().unwrap();
        db.try_add("CatAPicker", cat_a_picker).ok().unwrap();
        db.try_add("CatBPicker", cat_b_picker).ok().unwrap();
        db.try_add("CatCPicker", cat_c_picker).ok().unwrap();
        db
    },
    {
        "cat": "E",
        "item": "28558C",
        "data": null
    }
);

// test_union3: complex schema with many classes and enums
#[test]
fn test_union3() {
    // Build all enums
    let assistant_type = enum_ty(
        "AssistantType",
        vec![
            variant_with_aliases("ETF", vec!["ETFAssistantAPI"]),
            variant_with_aliases("Stock", vec!["StockAssistantAPI"]),
        ],
    );
    let ask_clarification_action = enum_ty(
        "AskClarificationAction",
        vec![variant_with_aliases(
            "ASK_CLARIFICATION",
            vec!["AskClarificationAPI"],
        )],
    );
    let respond_to_user_action = enum_ty(
        "RespondToUserAction",
        vec![variant_with_aliases(
            "RESPOND_TO_USER",
            vec!["RespondToUserAPI"],
        )],
    );
    let ui_type = enum_ty(
        "UIType",
        vec![
            variant("CompanyBadge"),
            variant("Markdown"),
            variant("NumericalSlider"),
            variant("BarGraph"),
            variant("ScatterPlot"),
        ],
    );

    // Build all classes
    let assistant_api = class_ty(
        "AssistantAPI",
        vec![
            field("action", Ty::Unresolved("AssistantType")),
            field("instruction", string_ty()),
            field("user_message", string_ty()),
        ],
    );
    let ask_clarification_api = class_ty(
        "AskClarificationAPI",
        vec![
            field("action", Ty::Unresolved("AskClarificationAction")),
            field("question", string_ty()),
        ],
    );
    let markdown_content = class_ty("MarkdownContent", vec![field("text", string_ty())]);
    let company_badge_content = class_ty(
        "CompanyBadgeContent",
        vec![
            field("name", string_ty()),
            field("symbol", string_ty()),
            field("logo_url", string_ty()),
        ],
    );
    let numerical_slider_content = class_ty(
        "NumericalSliderContent",
        vec![
            field("title", string_ty()),
            field("min", float_ty()),
            field("max", float_ty()),
            field("value", float_ty()),
        ],
    );
    let graph_data_point = class_ty(
        "GraphDataPoint",
        vec![
            field("name", string_ty()),
            field("expected", float_ty()),
            field("reported", float_ty()),
        ],
    );
    let scatter_data_point = class_ty(
        "ScatterDataPoint",
        vec![field("x", string_ty()), field("y", float_ty())],
    );
    let scatter_plot_content = class_ty(
        "ScatterPlotContent",
        vec![
            field(
                "expected",
                array_of(annotated(Ty::Unresolved("ScatterDataPoint"))),
            ),
            field(
                "reported",
                array_of(annotated(Ty::Unresolved("ScatterDataPoint"))),
            ),
        ],
    );
    let ui_content = class_ty(
        "UIContent",
        vec![
            // richText: MarkdownContent?
            AnnotatedField {
                name: Cow::Borrowed("richText"),
                ty: annotated(union_of(vec![
                    annotated(Ty::Unresolved("MarkdownContent")),
                    annotated(null_ty()),
                ])),
                class_in_progress_field_missing: AttrLiteral::Null,
                class_completed_field_missing: AttrLiteral::Null,
                aliases: vec![],
            },
            // companyBadge: CompanyBadgeContent?
            AnnotatedField {
                name: Cow::Borrowed("companyBadge"),
                ty: annotated(union_of(vec![
                    annotated(Ty::Unresolved("CompanyBadgeContent")),
                    annotated(null_ty()),
                ])),
                class_in_progress_field_missing: AttrLiteral::Null,
                class_completed_field_missing: AttrLiteral::Null,
                aliases: vec![],
            },
            // numericalSlider: NumericalSliderContent?
            AnnotatedField {
                name: Cow::Borrowed("numericalSlider"),
                ty: annotated(union_of(vec![
                    annotated(Ty::Unresolved("NumericalSliderContent")),
                    annotated(null_ty()),
                ])),
                class_in_progress_field_missing: AttrLiteral::Null,
                class_completed_field_missing: AttrLiteral::Null,
                aliases: vec![],
            },
            // barGraph: GraphDataPoint[] | null
            AnnotatedField {
                name: Cow::Borrowed("barGraph"),
                ty: annotated(union_of(vec![
                    annotated(array_of(annotated(Ty::Unresolved("GraphDataPoint")))),
                    annotated(null_ty()),
                ])),
                class_in_progress_field_missing: AttrLiteral::Null,
                class_completed_field_missing: AttrLiteral::Null,
                aliases: vec![],
            },
            // scatterPlot: ScatterPlotContent?
            AnnotatedField {
                name: Cow::Borrowed("scatterPlot"),
                ty: annotated(union_of(vec![
                    annotated(Ty::Unresolved("ScatterPlotContent")),
                    annotated(null_ty()),
                ])),
                class_in_progress_field_missing: AttrLiteral::Null,
                class_completed_field_missing: AttrLiteral::Null,
                aliases: vec![],
            },
            // foo: string?
            optional_field("foo", string_ty()),
        ],
    );
    let ui = class_ty(
        "UI",
        vec![
            field("section_title", string_ty()),
            field_with_aliases(
                "type",
                array_of(annotated(Ty::Unresolved("UIType"))),
                vec!["types"],
            ),
            field("content", Ty::Unresolved("UIContent")),
        ],
    );
    let respond_to_user_api = class_ty(
        "RespondToUserAPI",
        vec![
            field("action", Ty::Unresolved("RespondToUserAction")),
            field("sections", array_of(annotated(Ty::Unresolved("UI")))),
        ],
    );

    let mut db = TypeRefDb::new();
    db.try_add("AssistantType", assistant_type).ok().unwrap();
    db.try_add("AskClarificationAction", ask_clarification_action)
        .ok()
        .unwrap();
    db.try_add("RespondToUserAction", respond_to_user_action)
        .ok()
        .unwrap();
    db.try_add("UIType", ui_type).ok().unwrap();
    db.try_add("AssistantAPI", assistant_api).ok().unwrap();
    db.try_add("AskClarificationAPI", ask_clarification_api)
        .ok()
        .unwrap();
    db.try_add("MarkdownContent", markdown_content)
        .ok()
        .unwrap();
    db.try_add("CompanyBadgeContent", company_badge_content)
        .ok()
        .unwrap();
    db.try_add("NumericalSliderContent", numerical_slider_content)
        .ok()
        .unwrap();
    db.try_add("GraphDataPoint", graph_data_point).ok().unwrap();
    db.try_add("ScatterDataPoint", scatter_data_point)
        .ok()
        .unwrap();
    db.try_add("ScatterPlotContent", scatter_plot_content)
        .ok()
        .unwrap();
    db.try_add("UIContent", ui_content).ok().unwrap();
    db.try_add("UI", ui).ok().unwrap();
    db.try_add("RespondToUserAPI", respond_to_user_api)
        .ok()
        .unwrap();

    let target_ty = union_of(vec![
        annotated(Ty::Unresolved("RespondToUserAPI")),
        annotated(Ty::Unresolved("AskClarificationAPI")),
        annotated(array_of(annotated(Ty::Unresolved("AssistantAPI")))),
    ]);

    let raw = r####"```json
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
```"####;

    let parsed =
        crate::jsonish::parse(raw, Default::default(), true).expect("jsonish::parse failed");
    let ctx = crate::deserializer::coercer::ParsingContext::new(target_ty.as_ref(), &db);
    let annotations = TypeAnnotations::default();
    let target = TyWithMeta::new(target_ty.as_ref(), &annotations);
    let result = TyResolvedRef::coerce(&ctx, target, &parsed);
    assert!(result.is_ok(), "Failed to parse: {:?}", result);
    let value = result.unwrap();
    assert!(value.is_some(), "Coercion returned None");
    let value = value.unwrap();
    let json_value = serde_json::to_value(&value).unwrap();
    let expected = serde_json::json!({
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
    });
    assert_eq!(json_value, expected);
}

// test_phone_number_regex: skipped (@check/Assertion::evaluate is todo!())
// test_email_regex: skipped (@check/Assertion::evaluate is todo!())

test_deserializer!(
    test_ignore_float_in_string_if_string_in_union,
    "1 cup unsalted butter, room temperature",
    union_of(vec![annotated(float_ty()), annotated(string_ty()),]),
    empty_db(),
    "1 cup unsalted butter, room temperature"
);

test_deserializer!(
    test_ignore_int_if_string_in_union,
    "1 cup unsalted butter, room temperature",
    union_of(vec![annotated(int_ty()), annotated(string_ty()),]),
    empty_db(),
    "1 cup unsalted butter, room temperature"
);

// test_try_cast_union_early_return_preserves_incomplete_flag: skipped (uses internal APIs)
