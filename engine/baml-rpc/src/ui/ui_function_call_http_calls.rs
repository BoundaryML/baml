use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::ui_types::UiHttpCall;
use crate::{rpc::ApiEndpoint, FunctionCallId, ProjectId};

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct GetFunctionCallHttpCallsRequest {
    #[ts(type = "string")]
    pub project_id: ProjectId,
    #[ts(type = "string")]
    pub function_call_id: FunctionCallId,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct GetFunctionCallHttpCallsResponse {
    pub http_calls: Vec<UiHttpCall>,
}

pub struct GetFunctionCallHttpCalls;

impl ApiEndpoint for GetFunctionCallHttpCalls {
    type Request<'a> = GetFunctionCallHttpCallsRequest;
    type Response<'a> = GetFunctionCallHttpCallsResponse;

    const PATH: &'static str = "/v1/function-calls/http-calls";
}
