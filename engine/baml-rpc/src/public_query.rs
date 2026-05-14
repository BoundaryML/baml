use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::rpc::ApiEndpoint;

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct QueryRequest {
    pub sql: String,
    #[ts(optional)]
    pub mode: Option<QueryMode>,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum QueryMode {
    Interactive,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct QueryColumn {
    pub name: String,
    #[serde(rename = "type")]
    pub r#type: String,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct QueryStats {
    #[ts(type = "number")]
    pub elapsed_ms: u64,
    #[ts(type = "number")]
    pub rows_read: u64,
    #[ts(type = "number")]
    pub bytes_read: u64,
    #[ts(type = "number")]
    pub result_rows: u64,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct QueryResponse {
    pub columns: Vec<QueryColumn>,
    #[ts(type = "Array<Record<string, unknown>>")]
    pub rows: Vec<serde_json::Map<String, serde_json::Value>>,
    pub stats: QueryStats,
    pub query_id: String,
    pub warnings: Vec<String>,
}

pub struct PublicQuery;

impl ApiEndpoint for PublicQuery {
    type Request<'a> = QueryRequest;
    type Response<'a> = QueryResponse;

    const PATH: &'static str = "/v1/query";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_typescript_bindings() {
        let _ = QueryRequest::export();
        let _ = QueryMode::export();
        let _ = QueryColumn::export();
        let _ = QueryStats::export();
        let _ = QueryResponse::export();
    }
}
