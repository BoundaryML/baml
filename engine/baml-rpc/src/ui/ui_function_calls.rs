use crate::base::EpochMsTimestamp;
use crate::rpc::ApiEndpoint;
use crate::ProjectId;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::ui_types;

// TODO: Add support for `in`, `exists`, `contains` operators
#[derive(Debug, Deserialize, Serialize, TS)]
#[ts(export)]
pub enum Operator {
    #[serde(rename = "eq")]
    Eq,
    #[serde(rename = "ne")]
    Ne,
    #[serde(rename = "regex")]
    Regex,
    #[serde(rename = "gt")]
    Gt,
    #[serde(rename = "lt")]
    Lt,
    #[serde(rename = "gte")]
    Gte,
    #[serde(rename = "lte")]
    Lte,
}

#[derive(Debug, Deserialize, Serialize, TS)]
#[ts(export)]
pub struct FilterValue<T> {
    pub operator: Operator,
    pub value: T,
}

#[derive(Debug, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Filter {
    #[ts(optional)]
    pub env_id: Option<FilterValue<String>>,
    #[ts(optional)]
    pub person_id: Option<FilterValue<String>>,
    #[ts(optional)]
    pub api_key: Option<FilterValue<String>>,
    #[ts(optional)]
    pub client: Option<FilterValue<String>>,
    #[ts(optional)]
    pub function_id: Option<FilterValue<String>>,
    #[ts(optional)]
    pub function_name: Option<FilterValue<String>>,
    #[ts(optional)]
    pub session_id: Option<FilterValue<String>>,
    #[ts(optional)]
    pub call_type: Option<FilterValue<String>>,
    #[ts(type = "FilterValue<number>", optional)]
    pub start_at: Option<FilterValue<EpochMsTimestamp>>,
    #[ts(type = "FilterValue<number>", optional)]
    pub end_at: Option<FilterValue<EpochMsTimestamp>>,
    #[ts(optional)]
    pub relative_time: Option<FilterValue<String>>,
    #[ts(optional)]
    pub call_id: Option<FilterValue<String>>,
    #[ts(optional)]
    pub streamed: Option<FilterValue<bool>>,
    #[ts(optional)]
    pub status: Option<FilterValue<String>>,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ListFunctionCallsRequest {
    #[ts(type = "string")]
    pub project_id: ProjectId,
    #[ts(optional)]
    pub function_call_id: Option<String>,
    #[ts(optional)]
    pub filter: Option<Filter>,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ListFunctionCallsResponse {
    pub function_calls: Vec<ui_types::FunctionCall>,
    // pub function_definitions: Vec<ui_types::FunctionDefinition>,
    // pub type_definitions: Vec<ui_types::TypeDefinition>,
    #[ts(type = "Record<string, any>")]
    pub function_definitions: Vec<serde_json::Value>,
    #[ts(type = "Record<string, any>")]
    pub type_definitions: Vec<serde_json::Value>,
}

pub struct ListFunctionCalls;

impl ApiEndpoint for ListFunctionCalls {
    type Request<'a> = ListFunctionCallsRequest;
    type Response<'a> = ListFunctionCallsResponse;

    const PATH: &'static str = "/v1/function-calls";
}
