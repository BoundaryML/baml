use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::{Value as JsonValue, json};

use super::{BqlCursor, ExecuteOptions, ScriptResult, SnapshotToken, bql_schema};
use crate::QueryError;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct McpQueryRequest {
    pub query: String,
    #[serde(default)]
    pub params: BTreeMap<String, String>,
    #[serde(default)]
    pub cursor: Option<String>,
    #[serde(default)]
    pub snapshot: Option<String>,
    #[serde(default = "default_rows")]
    pub max_rows: usize,
    #[serde(default = "default_bytes")]
    pub max_bytes: usize,
}

impl McpQueryRequest {
    pub fn options(&self) -> Result<ExecuteOptions, QueryError> {
        Ok(ExecuteOptions {
            max_rows: self.max_rows,
            max_bytes: self.max_bytes,
            cursor: self.cursor.as_deref().map(BqlCursor::parse).transpose()?,
            snapshot: self
                .snapshot
                .as_deref()
                .map(SnapshotToken::parse)
                .transpose()?,
            params: self.params.clone(),
        })
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct McpSchemaRequest {}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct McpHydrateRequest {
    pub cid: String,
    #[serde(default = "default_hydrate_bytes")]
    pub max_bytes: usize,
    #[serde(default = "default_depth")]
    pub depth: u16,
    #[serde(default)]
    pub snapshot: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct McpToolDescriptor {
    pub name: &'static str,
    pub description: &'static str,
    pub input_schema: JsonValue,
}

#[must_use]
pub fn tool_descriptors() -> Vec<McpToolDescriptor> {
    vec![
        McpToolDescriptor {
            name: "baml_query",
            description: "Run a bounded BQL query and return rows plus a mandatory completeness footer.",
            input_schema: json!({
                "type": "object",
                "required": ["query"],
                "properties": {
                    "query": {"type": "string"},
                    "params": {"type": "object", "additionalProperties": {"type": "string"}},
                    "cursor": {"type": "string"},
                    "snapshot": {"type": "string"},
                    "max_rows": {"type": "integer", "minimum": 1, "maximum": super::HARD_MAX_ROWS},
                    "max_bytes": {"type": "integer", "minimum": 1024, "maximum": crate::HARD_MAX_BYTES}
                }
            }),
        },
        McpToolDescriptor {
            name: "baml_query_schema",
            description: "Return the typed BQL stage and field catalog.",
            input_schema: json!({"type": "object", "additionalProperties": false}),
        },
        McpToolDescriptor {
            name: "baml_hydrate",
            description: "Hydrate a captured value CID under explicit byte and depth bounds.",
            input_schema: json!({
                "type": "object",
                "required": ["cid"],
                "properties": {
                    "cid": {"type": "string"},
                    "max_bytes": {"type": "integer", "minimum": 1},
                    "depth": {"type": "integer", "minimum": 0},
                    "snapshot": {"type": "string"}
                }
            }),
        },
    ]
}

#[cfg(feature = "native")]
pub struct McpToolAdapter {
    engine: super::NativeBqlEngine,
}

#[cfg(feature = "native")]
impl McpToolAdapter {
    #[must_use]
    pub fn new(search_roots: Vec<std::path::PathBuf>) -> Self {
        Self {
            engine: super::NativeBqlEngine::new(search_roots),
        }
    }

    pub fn query(&self, request: McpQueryRequest) -> Result<ScriptResult, QueryError> {
        let options = request.options()?;
        self.engine.query(&request.query, options)
    }

    #[must_use]
    pub fn schema(&self, _request: McpSchemaRequest) -> super::BqlSchema {
        bql_schema()
    }

    pub fn hydrate(&self, request: McpHydrateRequest) -> Result<JsonValue, QueryError> {
        Err(QueryError::Bql(crate::BqlDiagnostic {
            code: "E_UNAVAILABLE",
            message: format!(
                "hydration for CID `{}` needs a value-CAS source; rerun with value capture enabled",
                request.cid
            ),
            start: 0,
            end: request.cid.len().max(1),
            line: 1,
            column: 1,
            source_line: request.cid,
            correction: None,
            valid: Vec::new(),
        }))
    }
}

const fn default_rows() -> usize {
    super::DEFAULT_LIMIT
}

const fn default_bytes() -> usize {
    crate::DEFAULT_MAX_BYTES
}

const fn default_hydrate_bytes() -> usize {
    64 * 1024
}

const fn default_depth() -> u16 {
    1
}
