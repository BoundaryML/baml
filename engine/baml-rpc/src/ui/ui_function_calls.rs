use std::{fmt, fmt::Display};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::ui_types;
use crate::{base::EpochMsTimestamp, rpc::ApiEndpoint, ProjectId};

// TODO: Add support for `in`, `exists`, `contains` operators
#[derive(Debug, Deserialize, Serialize, TS, Clone)]
#[ts(export)]
pub enum StringOperator {
    #[serde(rename = "eq")]
    Eq,
    #[serde(rename = "ne")]
    Ne,
    #[serde(rename = "regex")]
    Regex,
    #[serde(rename = "contains")]
    Contains,
}

#[derive(Debug, Deserialize, Serialize, TS, Clone)]
#[ts(export)]
pub enum NumericOperator {
    #[serde(rename = "eq")]
    Eq,
    #[serde(rename = "ne")]
    Ne,
    #[serde(rename = "gt")]
    Gt,
    #[serde(rename = "lt")]
    Lt,
    #[serde(rename = "gte")]
    Gte,
    #[serde(rename = "lte")]
    Lte,
}

#[derive(Debug, Deserialize, Serialize, TS, Clone)]
#[ts(export)]
pub enum BooleanOperator {
    #[serde(rename = "eq")]
    Eq,
    #[serde(rename = "ne")]
    Ne,
}

#[derive(Debug, Deserialize, Serialize, TS, Clone)]
#[ts(export)]
#[serde(untagged)]
pub enum FilterExpressionValue {
    String(String),
    #[ts(type = "number")]
    Number(u64),
    Boolean(bool),
}

#[derive(Debug, Deserialize, Serialize, TS, Clone)]
#[ts(export)]
pub struct FilterExpressionFormat {
    pub operator: String,
    pub value: FilterExpressionValue,
}

impl From<FilterExpressionFormat> for FilterExpression<String> {
    fn from(format: FilterExpressionFormat) -> Self {
        match (format.operator.as_str(), format.value) {
            ("eq", FilterExpressionValue::String(value)) => FilterExpression::Any {
                operator: StringOperator::Eq,
                value,
            },
            ("ne", FilterExpressionValue::String(value)) => FilterExpression::Any {
                operator: StringOperator::Ne,
                value,
            },
            ("regex", FilterExpressionValue::String(value)) => FilterExpression::Any {
                operator: StringOperator::Regex,
                value,
            },
            ("contains", FilterExpressionValue::String(value)) => FilterExpression::Any {
                operator: StringOperator::Contains,
                value,
            },
            ("eq", FilterExpressionValue::Number(value)) => FilterExpression::Numeric {
                operator: NumericOperator::Eq,
                value,
            },
            ("ne", FilterExpressionValue::Number(value)) => FilterExpression::Numeric {
                operator: NumericOperator::Ne,
                value,
            },
            ("gt", FilterExpressionValue::Number(value)) => FilterExpression::Numeric {
                operator: NumericOperator::Gt,
                value,
            },
            ("lt", FilterExpressionValue::Number(value)) => FilterExpression::Numeric {
                operator: NumericOperator::Lt,
                value,
            },
            ("gte", FilterExpressionValue::Number(value)) => FilterExpression::Numeric {
                operator: NumericOperator::Gte,
                value,
            },
            ("lte", FilterExpressionValue::Number(value)) => FilterExpression::Numeric {
                operator: NumericOperator::Lte,
                value,
            },
            ("eq", FilterExpressionValue::Boolean(value)) => FilterExpression::Boolean {
                operator: BooleanOperator::Eq,
                value,
            },
            ("ne", FilterExpressionValue::Boolean(value)) => FilterExpression::Boolean {
                operator: BooleanOperator::Ne,
                value,
            },
            _ => panic!("Invalid operator or value type combination"),
        }
    }
}

#[derive(Debug, Deserialize, Serialize, TS, Clone)]
#[ts(export)]
pub struct TagFilter {
    pub operator: StringOperator,
    pub key: String,
    pub value: String,
}

#[derive(Debug, Deserialize, Serialize, TS, Clone)]
#[ts(export)]
#[serde(untagged)]
pub enum FilterExpression<T> {
    Numeric {
        operator: NumericOperator,
        #[ts(type = "number")]
        value: u64,
    },
    Boolean {
        operator: BooleanOperator,
        value: bool,
    },
    Any {
        operator: StringOperator,
        value: T,
    },
    Format(FilterExpressionFormat),
}

impl<T> FilterExpression<T> {
    pub fn new_string(operator: StringOperator, value: T) -> Self {
        Self::Any { operator, value }
    }

    pub fn new_numeric(operator: NumericOperator, value: u64) -> Self {
        Self::Numeric { operator, value }
    }

    pub fn new_boolean(operator: BooleanOperator, value: bool) -> Self {
        Self::Boolean { operator, value }
    }

    pub fn eq_string(value: T) -> Self {
        Self::new_string(StringOperator::Eq, value)
    }

    pub fn eq_numeric(value: u64) -> Self {
        Self::new_numeric(NumericOperator::Eq, value)
    }

    pub fn eq_boolean(value: bool) -> Self {
        Self::new_boolean(BooleanOperator::Eq, value)
    }
}

// Query parameters struct for URL deserialization
//
// This struct supports rich filtering through URL query parameters with the following format:
//
// Basic fields:
//   ?project_id=proj_123&function_call_id=call_456
//
// Filter fields using JSON format:
//   ?function_name={"op":"eq","v":"myFunction"}
//   ?status={"op":"ne","v":"error"}
//   ?startAt={"op":"gte","v":1748131389246}
//
// Supported operators: eq, ne, regex, gt, lt, gte, lte
//
// Examples:
//   - Get calls with function name pattern: ?function_name={"op":"regex","v":"^extract_.*"}
//   - Get calls excluding errors: ?status={"op":"ne","v":"error"}
//   - Get calls after a timestamp: ?startAt={"op":"gte","v":1748131389246}
//   - Complex query: ?project_id=proj_123&function_name={"op":"regex","v":"^test_.*"}&status={"op":"eq","v":"success"}

#[derive(Debug, Deserialize, Serialize, TS, Clone, Default)]
#[ts(export)]
pub enum SortDirection {
    #[serde(rename = "asc")]
    Ascending,
    #[default]
    #[serde(rename = "desc")]
    Descending,
}

#[derive(Debug, Deserialize, Serialize, TS, Clone)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum OrderField {
    FunctionName,
    StartTime,
    EndTime,
    Status,
    Streamed,
    CallType,
    Error,
}

impl Display for OrderField {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OrderField::FunctionName => write!(f, "function_name"),
            OrderField::StartTime => write!(f, "start_time"),
            OrderField::EndTime => write!(f, "end_time"),
            OrderField::Status => write!(f, "status"),
            OrderField::Streamed => write!(f, "streamed"),
            OrderField::CallType => write!(f, "call_type"),
            OrderField::Error => write!(f, "error"),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, TS)]
#[ts(export)]
pub struct OrderBy {
    pub field: OrderField,
    #[serde(default)]
    pub direction: SortDirection,
}

#[derive(Debug, Deserialize, Serialize, TS, Clone)]
#[ts(export)]
pub enum RelativeTime {
    #[serde(rename = "5m")]
    FiveMinutes,
    #[serde(rename = "1h")]
    OneHour,
    #[serde(rename = "6h")]
    SixHours,
    #[serde(rename = "12h")]
    TwelveHours,
    #[serde(rename = "24h")]
    TwentyFourHours,
    #[serde(rename = "3d")]
    ThreeDays,
    #[serde(rename = "7d")]
    SevenDays,
    #[serde(rename = "14d")]
    FourteenDays,
    #[serde(rename = "30d")]
    ThirtyDays,
}

#[derive(Debug, Deserialize, Serialize, TS, Clone)]
#[ts(export)]
pub enum FunctionCallStatus {
    #[serde(rename = "success")]
    Success,
    #[serde(rename = "error")]
    Error,
    #[serde(rename = "running")]
    Running,
    #[serde(rename = "incomplete")]
    Incomplete,
}

impl fmt::Display for FunctionCallStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FunctionCallStatus::Success => write!(f, "success"),
            FunctionCallStatus::Error => write!(f, "error"),
            FunctionCallStatus::Running => write!(f, "running"),
            FunctionCallStatus::Incomplete => write!(f, "incomplete"),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct ListFunctionCallsRequest {
    #[ts(optional)]
    pub order_by: Option<OrderBy>,
    #[ts(type = "string")]
    pub project_id: ProjectId,
    /// Maximum number of function calls to return. Defaults to 100 if not specified.
    #[ts(optional)]
    pub limit: Option<u32>,
    /// Keyset cursor for pagination: fetch the next page after this id.
    /// Stripe-style naming. Mutually exclusive with `endingBefore`.
    #[ts(optional)]
    pub starting_after: Option<String>,
    /// Keyset cursor for pagination: fetch the previous page ending before this id.
    /// Stripe-style naming. Mutually exclusive with `startingAfter`.
    #[ts(optional)]
    pub ending_before: Option<String>,
    #[ts(optional)]
    pub function_call_id: Option<FilterExpression<String>>,
    #[ts(optional)]
    pub function_id: Option<FilterExpression<String>>,
    #[ts(optional)]
    pub function_name: Option<FilterExpression<String>>,
    #[ts(type = "FilterExpression<number>", optional)]
    pub start_time: Option<FilterExpression<EpochMsTimestamp>>,
    #[ts(type = "FilterExpression<number>", optional)]
    pub end_time: Option<FilterExpression<EpochMsTimestamp>>,
    #[ts(optional)]
    pub status: Option<FilterExpression<FunctionCallStatus>>,
    #[ts(optional)]
    pub tags: Option<Vec<TagFilter>>,
    #[ts(optional)]
    pub error_filters: Option<Vec<TagFilter>>,
    #[ts(optional)]
    pub streamed: Option<FilterExpression<bool>>,
    #[ts(optional)]
    pub relative_time: Option<RelativeTime>,
    /// Search filter for LLM request and response content
    #[ts(optional)]
    pub search: Option<String>,
    /// Whether to include detailed information (details, llm_request, llm_response) in the response.
    /// Set to false for table views to improve performance. Defaults to true.
    #[ts(optional)]
    pub include_details: Option<bool>,
}

impl Default for ListFunctionCallsRequest {
    fn default() -> Self {
        Self {
            project_id: ProjectId::new(),
            order_by: Some(OrderBy {
                field: OrderField::StartTime,
                direction: SortDirection::Descending,
            }),
            limit: Some(100),
            starting_after: None,
            ending_before: None,
            function_call_id: None,
            function_id: None,
            function_name: None,
            start_time: None,
            end_time: None,
            status: None,
            tags: None,
            error_filters: None,
            streamed: None,
            relative_time: None,
            search: None,
            include_details: None,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ListFunctionCallsResponse {
    pub function_calls: Vec<ui_types::UiFunctionCall>,
    pub function_definitions: Vec<ui_types::UiFunctionDefinition>,
    pub type_definitions: Vec<ui_types::UiTypeDefinition>,
    /// Whether or not there are more elements available after this set.
    /// If false, this set comprises the end of the list. (Stripe-style)
    pub has_more: bool,
    /// Cursor of the next page (older items when ordering desc). Present only if there are more results.
    #[ts(optional)]
    pub next_cursor: Option<String>,
    /// Cursor of the previous page (newer items when ordering desc). Optional convenience.
    #[ts(optional)]
    pub prev_cursor: Option<String>,
}

pub struct ListFunctionCalls;

impl ApiEndpoint for ListFunctionCalls {
    type Request<'a> = ListFunctionCallsRequest;
    type Response<'a> = ListFunctionCallsResponse;

    const PATH: &'static str = "/v1/function-calls";
}

// use super::*;
// use serde_json::json;

#[test]
fn test_deserialize_list_function_calls_request_with_start_time() {
    let json_str = r#"{
        "projectId": "proj_01jvb3fnp1f09ta2a6g016t4kz",
        "startTime": {
            "operator": "gte",
            "value": 1748895831481
        }
    }"#;

    let request: ListFunctionCallsRequest = serde_json::from_str(json_str).unwrap();

    assert_eq!(
        request.project_id.to_string(),
        "proj_01jvb3fnp1f09ta2a6g016t4kz"
    );

    match request.start_time {
        Some(FilterExpression::Numeric { operator, value }) => {
            assert!(matches!(operator, NumericOperator::Gte));
            assert_eq!(value, 1748895831481);
        }
        _ => panic!("Expected Numeric filter expression for startTime"),
    }
}

#[test]
fn test_deserialize_list_function_calls_request_with_end_time() {
    let json_str = r#"{
        "projectId": "proj_01jvb3fnp1f09ta2a6g016t4kz",
        "endTime": {
            "operator": "lte",
            "value": 4294967295
        }
    }"#;

    let request: ListFunctionCallsRequest = serde_json::from_str(json_str).unwrap();

    assert_eq!(
        request.project_id.to_string(),
        "proj_01jvb3fnp1f09ta2a6g016t4kz"
    );

    match request.end_time {
        Some(FilterExpression::Numeric { operator, value }) => {
            assert!(matches!(operator, NumericOperator::Lte));
            assert_eq!(value, 4294967295);
        }
        _ => panic!("Expected Numeric filter expression for endTime"),
    }
}

// #[test]
// fn test_deserialize_tag_filter_expression() {
//     let json_str = r#"{
//         "operator": "eq",
//         "key": "test.tag",
//         "value": "testValue"
//     }"#;

//     let filter: FilterExpression<String> = serde_json::from_str(json_str).unwrap();

//     match filter {
//         FilterExpression::Tag {
//             operator,
//             key,
//             value,
//         } => {
//             assert!(matches!(operator, StringOperator::Eq));
//             assert_eq!(key, "test.tag");
//             assert_eq!(value, "testValue");
//         }
//         _ => panic!("Expected Tag filter expression"),
//     }
// }

// #[test]
// fn test_deserialize_list_function_calls_request_with_tags() {
//     let json_str = r#"{
//         "projectId": "proj_01jvb3fnp1f09ta2a6g016t4kz",
//         "tags": [
//             {
//                 "operator": "eq",
//                 "key": "test.tag",
//                 "value": "testValue"
//             },
//             {
//                 "operator": "eq",
//                 "key": "baml.language",
//                 "value": "typescript"
//             }
//         ]
//     }"#;

//     let request: ListFunctionCallsRequest = serde_json::from_str(json_str).unwrap();

//     assert_eq!(
//         request.project_id.to_string(),
//         "proj_01jvb3fnp1f09ta2a6g016t4kz"
//     );

//     let tags = request.tags.unwrap();
//     assert_eq!(tags.len(), 2);

//     match &tags[0] {
//         FilterExpression::Tag {
//             operator,
//             key,
//             value,
//         } => {
//             assert!(matches!(operator, StringOperator::Eq));
//             assert_eq!(key, "test.tag");
//             assert_eq!(value, "testValue");
//         }
//         _ => panic!("Expected Tag filter expression for first tag"),
//     }

//     match &tags[1] {
//         FilterExpression::Tag {
//             operator,
//             key,
//             value,
//         } => {
//             assert!(matches!(operator, StringOperator::Eq));
//             assert_eq!(key, "baml.language");
//             assert_eq!(value, "typescript");
//         }
//         _ => panic!("Expected Tag filter expression for second tag"),
//     }
// }
