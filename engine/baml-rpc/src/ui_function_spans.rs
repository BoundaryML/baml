use crate::{
    ast::{BamlFunctionDefinition, BamlTypeDefinition},
    rpc::ApiEndpoint,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct ListFunctionSpansRequest {
    project_id: String,
    function_call_id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ListFunctionSpansResponse {
    function_spans: Vec<api::FunctionSpan>,
    function_definitions: Vec<BamlFunctionDefinition>,
    type_definitions: Vec<BamlTypeDefinition>,
}

struct ListFunctionSpans;

impl ApiEndpoint for ListFunctionSpans {
    type Request = ListFunctionSpansRequest;
    type Response = ListFunctionSpansResponse;

    const PATH: &'static str = "/v1/function-spans";
}

pub mod api {
    use serde::{Deserialize, Serialize};

    use crate::base::EpochMsTimestamp;

    #[derive(Debug, Deserialize, Serialize)]
    pub struct FunctionSpan {
        pub function_span_id: String,
        pub source: String,
        pub function_id: String,
        #[serde(rename = "start_epoch_ms")]
        pub start_time: Option<EpochMsTimestamp>,
        #[serde(rename = "end_epoch_ms")]
        pub end_time: Option<EpochMsTimestamp>,
        pub baml_options: serde_json::Value,
        pub inputs: Vec<FunctionInput>,
        pub output: serde_json::Value,
        pub status: String,
        pub error: serde_json::Value,
    }

    #[derive(Debug, Deserialize, Serialize)]
    pub struct FunctionInput {
        pub field: String,
        pub value: serde_json::Value,
    }
}
