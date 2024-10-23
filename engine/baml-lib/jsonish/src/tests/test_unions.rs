use super::*;

//
const FOO_FILE: &str = r#"
class Foo {
  hi string[]
}

class Bar {
  foo string
}
"#;

test_deserializer!(
  test_union,
  FOO_FILE,
  r#"{"hi": ["a", "b"]}"#,
  FieldType::union(vec![FieldType::class("Foo"), FieldType::class("Bar")]),
  {"hi": ["a", "b"]}
);

const SPUR_FILE: &str = r###"
enum CatA {
  A
}

enum CatB {
  C
  D
}

class CatAPicker {
  cat CatA
}

class CatBPicker {
  cat CatB
  item int
}

enum CatC {
  E
  F 
  G 
  H 
  I
}

class CatCPicker {
  cat CatC
  item  int | string | null
  data int?
}
"###;

test_deserializer!(
  test_union2,
  SPUR_FILE,
  r#"```json
  {
    "cat": "E",
    "item": "28558C",
    "data": null
  }
  ```"#,
  FieldType::union(vec![FieldType::class("CatAPicker"), FieldType::class("CatBPicker"), FieldType::class("CatCPicker")]),
  {
    "cat": "E",
    "item": "28558C",
    "data": null
  }
);

const CUSTOMER_FILE2: &str = r###"
enum AssistantType {
  ETF @alias("ETFAssistantAPI")
  Stock @alias("StockAssistantAPI")
}

class AssistantAPI {
  action AssistantType
  instruction string @description("Detailed instructions for the assistants API to be able to process the request")
  user_message string @description("The message to keep the user informed")

  @@description(#"
    Used for 
  "#)
}

enum AskClarificationAction {
  ASK_CLARIFICATION @alias("AskClarificationAPI")
}

class AskClarificationAPI {
  action AskClarificationAction
  question string @description("The clarification question to ask the user")
}

enum RespondToUserAction {
  RESPOND_TO_USER @alias("RespondToUserAPI")
}

class RespondToUserAPI {
  action RespondToUserAction
  sections UI[]
}

class Message {
  role string
  message string
}



enum UIType {
  CompanyBadge @description("Company badge UI type")
  Markdown @description("Markdown text UI type")
  NumericalSlider @description("Numerical slider UI type")
  BarGraph @description("Bar graph UI type")
  ScatterPlot @description("Scatter plot UI type")
}

class MarkdownContent {
  text string
}

class CompanyBadgeContent {
  name string
  symbol string
  logo_url string
}

class NumericalSliderContent {
  title string
  min float
  max float
  value float
}

class TabContent {
  title string
  content string
}

class GraphDataPoint {
  name string
  expected float
  reported float
}

class ScatterDataPoint {
  x string
  y float
}

class ScatterPlotContent {
  expected ScatterDataPoint[]
  reported ScatterDataPoint[]
}

class UIContent {
  richText MarkdownContent?
  companyBadge CompanyBadgeContent?
  numericalSlider NumericalSliderContent?
  barGraph GraphDataPoint[] | null
  scatterPlot ScatterPlotContent?
  foo string?
}

class UI {
  section_title string
  type UIType[] @alias(types)
  content UIContent
}

"###;

test_deserializer!(
  test_union3,
  CUSTOMER_FILE2,
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
  FieldType::union(vec![FieldType::class("RespondToUserAPI"), FieldType::class("AskClarificationAPI"), FieldType::class("AssistantAPI").as_list()]),
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

const CONTACT_INFO: &str = r#"
class PhoneNumber {
  value string @check(valid_phone_number, {{this|regex_match("\(?\d{3}\)?[-.\s]?\d{3}[-.\s]?\d{4}")}})
  foo int? // A nullable marker indicating PhoneNumber was chosen.
}

class EmailAddress {
  value string @check(valid_email, {{this|regex_match("^[_]*([a-z0-9]+(\.|_*)?)+@([a-z][a-z0-9-]+(\.|-*\.))+[a-z]{2,6}$")}})
  bar int? // A nullable marker indicating EmailAddress was chosen.
}

class ContactInfo {
  primary PhoneNumber | EmailAddress
}
"#;

test_deserializer!(
  test_phone_number_regex,
  CONTACT_INFO,
  r#"{"primary": {"value": "908-797-8281"}}"#,
  FieldType::Class("ContactInfo".to_string()),
  {"primary": {"value": "908-797-8281", "foo": null}}
);

test_deserializer!(
  test_email_regex,
  CONTACT_INFO,
  r#"{"primary": {"value": "help@boundaryml.com"}}"#,
  FieldType::Class("ContactInfo".to_string()),
  {"primary": {"value": "help@boundaryml.com", "bar": null}}
);
