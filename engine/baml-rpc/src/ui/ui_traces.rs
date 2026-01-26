use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::{
    ui_function_calls::{
        FilterExpression, FunctionCallStatus, OrderBy, OrderField, RelativeTime, SortDirection,
        TagFilter,
    },
    ui_types,
};
use crate::{base::EpochMsTimestamp, rpc::ApiEndpoint, ProjectId};

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct UsageEstimateAggregate {
    #[ts(type = "number", optional)]
    pub input_tokens: Option<u64>,
    #[ts(type = "number", optional)]
    pub output_tokens: Option<u64>,
    #[ts(type = "number", optional)]
    pub input_cost: Option<f64>,
    #[ts(type = "number", optional)]
    pub output_cost: Option<f64>,
    #[ts(type = "number", optional)]
    pub total_cost: Option<f64>,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct NodeDetails {
    // Aggregate info for performance - calculated server-side
    #[ts(type = "number")]
    pub children_functions: u32,
    #[ts(type = "number")]
    pub total_descendants: u32, // Total count including nested children
    #[ts(type = "number")]
    pub max_depth: u32, // Maximum nesting depth
    #[ts(optional)]
    pub usage_estimate_aggregate: Option<UsageEstimateAggregate>,

    // Lazy loading metadata
    pub has_children: bool,
    pub children_loaded: bool, // Whether children are included in this response
    #[ts(type = "number", optional)]
    pub children_limit: Option<u32>, // How many children were requested/returned
    #[ts(type = "number", optional)]
    pub children_offset: Option<u32>, // Pagination offset for children

    // Match highlighting - IDs of descendants that matched filters
    #[ts(optional)]
    pub matched_descendant_ids: Option<Vec<String>>,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct TraceCall {
    // Base function call data - reuse UiFunctionCall to avoid duplication
    pub function_call: ui_types::UiFunctionCall,

    // Node-specific metadata for hierarchy and lazy loading
    pub node_details: NodeDetails,

    // Recursive children - each child is also a TraceCall
    pub children: Vec<TraceCall>,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct CallStackEntry {
    pub function_call_id: String,
    pub function_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct ListTracesRequest {
    #[ts(optional)]
    pub order_by: Option<OrderBy>,
    #[ts(type = "string")]
    pub project_id: ProjectId,
    /// Maximum number of traces to return. Defaults to 100 if not specified.
    #[ts(optional)]
    pub limit: Option<u32>,

    /// Keyset cursor for pagination: fetch the next page after this id (Stripe-style).
    #[ts(optional)]
    pub starting_after: Option<String>,
    /// Keyset cursor for pagination: fetch the previous page ending before this id (Stripe-style).
    #[ts(optional)]
    pub ending_before: Option<String>,

    // Lazy loading controls
    /// Whether to include direct children in the response. Defaults to false.
    #[ts(optional)]
    pub include_children: Option<bool>,
    /// Maximum depth of children to load. Defaults to 1 if include_children is true.
    #[ts(optional)]
    pub max_depth: Option<u32>,
    /// Limit for children per trace. Defaults to 50.
    #[ts(optional)]
    pub children_limit: Option<u32>,
    /// Offset for children pagination within each trace.
    #[ts(optional)]
    pub children_offset: Option<u32>,
    /// Whether to calculate usage estimates. Defaults to true.
    #[ts(optional)]
    pub include_usage_estimates: Option<bool>,

    // Existing filters
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
    pub tag_filters: Option<Vec<TagFilter>>,
    #[ts(optional)]
    pub error_filters: Option<Vec<TagFilter>>,
    #[ts(optional)]
    pub streamed: Option<FilterExpression<bool>>,
    #[ts(optional)]
    pub relative_time: Option<RelativeTime>,
    /// Search term to filter across function_call_id, function_name, tags, error, input (args), and output
    #[ts(optional)]
    pub search: Option<String>,
    /// Filter to only show LLM function calls (function_type = 'baml_llm')
    #[ts(optional)]
    pub llm_only: Option<FilterExpression<bool>>,
    /// Whether to include matched_descendant_ids in the response for match highlighting. Defaults to false.
    #[ts(optional)]
    pub include_match_highlights: Option<bool>,
}

impl Default for ListTracesRequest {
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
            include_children: Some(false),
            max_depth: Some(1),
            children_limit: Some(50),
            children_offset: Some(0),
            include_usage_estimates: Some(true),
            function_call_id: None,
            function_id: None,
            function_name: None,
            start_time: None,
            end_time: None,
            status: None,
            tag_filters: None,
            error_filters: None,
            streamed: None,
            relative_time: None,
            search: None,
            llm_only: None,
            include_match_highlights: Some(false),
        }
    }
}

// Separate API for loading children of a specific trace/function call
#[derive(Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct GetTraceChildrenRequest {
    #[ts(type = "string")]
    pub project_id: ProjectId,
    pub function_call_id: String,
    /// Maximum depth to load. Defaults to 1.
    #[ts(optional)]
    pub max_depth: Option<u32>,
    /// Limit for children. Defaults to 100.
    #[ts(optional)]
    pub limit: Option<u32>,

    /// Whether to calculate usage estimates. Defaults to true.
    #[ts(optional)]
    pub include_usage_estimates: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct GetTraceChildrenResponse {
    // The function_call_id of the parent function call
    pub function_call_id: String,
    // The function_call details of the parent function call
    pub function_call: ui_types::UiFunctionCall,
    // The children of the parent function call
    pub children: Vec<TraceCall>,
    // Breadcrumb trail
    #[ts(type = "number")]
    pub total_children: u32, // For pagination
    pub has_more: bool, // Whether there are more children to load
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ListTracesResponse {
    pub traces: Vec<TraceCall>,
    pub function_definitions: Vec<ui_types::UiFunctionDefinition>,
    pub type_definitions: Vec<ui_types::UiTypeDefinition>,
    #[ts(type = "number")]
    pub total_traces: u32, // For pagination
    /// Whether or not there are more elements available after this set.
    /// If false, this set comprises the end of the list. (Stripe-style)
    pub has_more: bool,
    /// Cursor for next page (older items when ordering desc)
    #[ts(optional)]
    pub next_cursor: Option<String>,
    /// Cursor for previous page (newer items when ordering desc)
    #[ts(optional)]
    pub prev_cursor: Option<String>,
}

pub struct ListTraces;

impl ApiEndpoint for ListTraces {
    type Request<'a> = ListTracesRequest;
    type Response<'a> = ListTracesResponse;

    const PATH: &'static str = "/v1/traces";
}

pub struct GetTraceChildren;

impl ApiEndpoint for GetTraceChildren {
    type Request<'a> = GetTraceChildrenRequest;
    type Response<'a> = GetTraceChildrenResponse;

    const PATH: &'static str = "/v1/traces/children";
}

#[cfg(test)]
mod tests {
    use serde_json;

    use super::*;

    #[test]
    fn test_list_traces_lazy_loading() {
        let json_str = r#"{
            "projectId": "proj_01jvb3fnp1f09ta2a6g016t4kz",
            "includeChildren": true,
            "maxDepth": 2,
            "childrenLimit": 25
        }"#;

        let request: ListTracesRequest = serde_json::from_str(json_str).unwrap();
        assert_eq!(request.include_children, Some(true));
        assert_eq!(request.max_depth, Some(2));
        assert_eq!(request.children_limit, Some(25));
    }

    #[test]
    fn test_get_trace_children_request() {
        let json_str = r#"{
            "projectId": "proj_01jvb3fnp1f09ta2a6g016t4kz",
            "functionCallId": "call_123",
            "maxDepth": 3,
            "limit": 50,
            "offset": 0
        }"#;

        let request: GetTraceChildrenRequest = serde_json::from_str(json_str).unwrap();
        assert_eq!(request.function_call_id, "call_123");
        assert_eq!(request.max_depth, Some(3));
        assert_eq!(request.limit, Some(50));
    }
}

// API for listing function summaries with aggregate statistics
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct ListTraceFunctionSummariesRequest {
    #[ts(type = "string")]
    pub project_id: ProjectId,
    /// Maximum number of functions to return. Defaults to 100 if not specified.
    #[ts(optional)]
    pub limit: Option<u32>,
    /// Cursor for pagination: fetch functions starting after this function_name
    #[ts(optional)]
    pub starting_after: Option<String>,

    // Time filters
    #[ts(optional)]
    pub relative_time: Option<RelativeTime>,
    #[ts(type = "FilterExpression<number>", optional)]
    pub start_time: Option<FilterExpression<EpochMsTimestamp>>,
    #[ts(type = "FilterExpression<number>", optional)]
    pub end_time: Option<FilterExpression<EpochMsTimestamp>>,

    // Function filters
    #[ts(optional)]
    pub function_name: Option<FilterExpression<String>>,
    #[ts(optional)]
    pub function_id: Option<FilterExpression<String>>,
    #[ts(optional)]
    pub llm_only: Option<FilterExpression<bool>>,

    // Status and other filters
    #[ts(optional)]
    pub status: Option<FilterExpression<FunctionCallStatus>>,
    #[ts(optional)]
    pub tag_filters: Option<Vec<TagFilter>>,
    #[ts(optional)]
    pub error_filters: Option<Vec<TagFilter>>,
    #[ts(optional)]
    pub search: Option<String>,
}

impl Default for ListTraceFunctionSummariesRequest {
    fn default() -> Self {
        Self {
            project_id: ProjectId::new(),
            limit: Some(400),
            starting_after: None,
            relative_time: None,
            start_time: None,
            end_time: None,
            function_name: None,
            function_id: None,
            llm_only: None,
            status: None,
            tag_filters: None,
            error_filters: None,
            search: None,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct FunctionSummary {
    #[ts(optional)]
    pub function_id: Option<ui_types::UiFunctionIdString>,
    pub function_name: String,
    pub function_type: String, // 'baml_llm' or 'native'
    pub language: String,
    #[ts(type = "Record<string, unknown>")]
    pub tags: serde_json::Map<String, serde_json::Value>,

    // Aggregate statistics
    #[ts(type = "number")]
    pub total_traces: u64,
    #[ts(type = "number")]
    pub success_count: u64,
    #[ts(type = "number")]
    pub error_count: u64,
    #[ts(type = "number")]
    pub running_count: u64,

    // Time range for this function's traces
    #[ts(type = "number")]
    pub first_trace_time: EpochMsTimestamp,
    #[ts(type = "number")]
    pub last_trace_time: EpochMsTimestamp,

    // Optional cost aggregates (if available)
    #[ts(type = "number", optional)]
    pub total_cost: Option<f64>,
    #[ts(type = "number", optional)]
    pub avg_duration_ms: Option<f64>,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ListTraceFunctionSummariesResponse {
    pub summaries: Vec<FunctionSummary>,
    pub function_definitions: Vec<ui_types::UiFunctionDefinition>,
    pub type_definitions: Vec<ui_types::UiTypeDefinition>,
    pub has_more: bool,
    #[ts(optional)]
    pub next_cursor: Option<String>, // function_name for pagination
}

pub struct ListTraceFunctionSummaries;

impl ApiEndpoint for ListTraceFunctionSummaries {
    type Request<'a> = ListTraceFunctionSummariesRequest;
    type Response<'a> = ListTraceFunctionSummariesResponse;

    const PATH: &'static str = "/v1/traces/function-summaries";
}
