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
    use std::time::Duration;

    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use time::OffsetDateTime;

    #[derive(Debug, Deserialize, Serialize)]
    pub struct FunctionSpan {
        pub function_span_id: String,
        pub source: String,
        pub function_id: String,
        #[serde(rename = "start_epoch_ms", with = "super::api")]
        pub start_time: Option<time::OffsetDateTime>,
        #[serde(rename = "end_epoch_ms", with = "super::api")]
        pub end_time: Option<time::OffsetDateTime>,
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

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<time::OffsetDateTime>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let formatted: Option<u64> = serde::Deserialize::deserialize(deserializer)?;
        match formatted {
            Some(epoch_ms) => {
                let v = OffsetDateTime::from_unix_timestamp_nanos(
                    Duration::from_millis(epoch_ms).as_nanos() as i128,
                )
                .map_err(serde::de::Error::custom)?;
                Ok(Some(v))
            }
            None => Ok(None),
        }
    }

    pub fn serialize<S>(v: &Option<OffsetDateTime>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match v {
            Some(v) => {
                let epoch_millis =
                    Duration::from_nanos(v.unix_timestamp_nanos() as u64).as_millis();
                serializer.serialize_u64(epoch_millis as u64)
            }
            None => serializer.serialize_none(),
        }
    }
}
