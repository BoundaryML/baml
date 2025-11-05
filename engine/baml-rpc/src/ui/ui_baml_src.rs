use baml_ids::ProjectId;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{rpc::ApiEndpoint, EpochMsTimestamp};

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct BamlSourceBundle {
    pub project_id: String,
    pub bundle_blake3_hash: String,
    pub content_map: std::collections::BTreeMap<String, String>,
    #[ts(type = "number", optional)]
    pub created_at: Option<EpochMsTimestamp>,
    #[ts(type = "number", optional)]
    pub updated_at: Option<EpochMsTimestamp>,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct BamlSourceCode {
    pub project_id: String,
    pub file_blake3_hash: String,
    pub content: String,
    pub mime_type: String,
    #[ts(type = "number", optional)]
    pub uploaded_at: Option<EpochMsTimestamp>,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct AstNodeDefinition {
    pub project_id: String,
    pub ast_node_id: String,
    #[ts(type = "unknown")]
    pub ast_node_definition: serde_json::Value,
    pub flattened_dependencies_ast_nodes: Vec<String>,
    pub baml_src_node_ids: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct BamlSourceQuery {
    pub project_id: String,
    pub function_calls: Vec<FunctionCallQuery>,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct FunctionCallQuery {
    pub ast_node_id: String,
    pub baml_source_fingerprint: String,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct BamlSourceQueryResponse {
    pub function_definitions: Vec<AstNodeDefinition>,
    pub type_definitions: Vec<AstNodeDefinition>,
    pub baml_source_bundles: Vec<BamlSourceBundle>,
}

// New API endpoint for getting BAML source bundle by function call ID
#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct GetBamlSrcBundleRequest {
    #[ts(type = "string")]
    pub project_id: ProjectId,
    pub function_call_id: String,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct GetBamlSrcBundleResponse {
    pub files: Vec<BamlSrcFile>,
    pub baml_src_hash: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct BamlSrcFile {
    pub file_name: String,
    pub content: String,
}

pub struct GetBamlSrcBundle;

impl ApiEndpoint for GetBamlSrcBundle {
    type Request<'a> = GetBamlSrcBundleRequest;
    type Response<'a> = GetBamlSrcBundleResponse;

    const PATH: &'static str = "/v1/ui/get-baml-src-bundle";
}
