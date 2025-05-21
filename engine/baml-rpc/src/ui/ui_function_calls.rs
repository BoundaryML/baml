use crate::base::EpochMsTimestamp;
use crate::rpc::ApiEndpoint;
use crate::ProjectId;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::ui_types;

#[derive(Debug, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Filter {
    pub env_id: Option<String>,
    pub person_id: Option<String>,
    pub api_key: Option<String>,
    pub client: Option<String>,
    pub function_id: Option<String>,
    pub function_name: Option<String>,
    pub session_id: Option<String>,
    pub call_type: Option<String>,
    #[ts(type = "number | null")]
    pub start_at: Option<EpochMsTimestamp>,
    #[ts(type = "number | null")]
    pub end_at: Option<EpochMsTimestamp>,
    pub relative_time: Option<String>,
    pub call_id: Option<String>,
    pub streamed: Option<bool>,
    pub status: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ListFunctionCallsRequest {
    #[ts(type = "string")]
    pub project_id: ProjectId,
    pub function_call_id: Option<String>,
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
