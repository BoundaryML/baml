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
            CompanyBadge,
            Markdown,
            NumericalSlider,
            BarGraph,
            ScatterPlot
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
