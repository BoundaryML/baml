use crate::{
    ast::{BamlFunctionDefinition, BamlTypeDefinition},
    rpc::ApiEndpoint,
};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ListFunctionSpansRequest {
    project_id: String,
    function_call_id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ListFunctionSpansResponse {
    function_spans: Vec<api::FunctionSpan>,
    #[ts(type = "any")]
    function_definitions: Vec<BamlFunctionDefinition>,
    #[ts(type = "any")]
    type_definitions: Vec<BamlTypeDefinition>,
}

pub struct ListFunctionSpans;

impl ApiEndpoint for ListFunctionSpans {
    type Request = ListFunctionSpansRequest;
    type Response = ListFunctionSpansResponse;

    const PATH: &'static str = "/v1/function-spans";
}

pub mod api {
    use serde::{Deserialize, Serialize};
    use ts_rs::TS;

    use crate::base::EpochMsTimestamp;

    #[derive(Debug, Serialize, Deserialize, TS)]
    #[ts(export)]
    pub struct FunctionSpan {
        pub function_span_id: String,
        pub source: String,
        pub function_id: String,
        #[serde(rename = "start_epoch_ms")]
        #[ts(type = "number | null")]
        pub start_time: Option<EpochMsTimestamp>,
        #[serde(rename = "end_epoch_ms")]
        #[ts(type = "number | null")]
        pub end_time: Option<EpochMsTimestamp>,
        #[ts(type = "any")]
        pub baml_options: serde_json::Value,
        pub inputs: Vec<FunctionInput>,
        #[ts(type = "any")]
        pub output: serde_json::Value,
        pub status: String,
        #[ts(type = "any")]
        pub error: serde_json::Value,
    }

    #[derive(Debug, Deserialize, Serialize, TS)]
    #[ts(export)]
    pub struct FunctionInput {
        pub field: String,
        #[ts(type = "any")]
        pub value: serde_json::Value,
    }
}
