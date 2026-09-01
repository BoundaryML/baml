//! Playground notifications: what the server pushes to the webview, and how
//! project state becomes one.
//!
//! These types are a wire contract. The webview decodes them as
//! `PlaygroundNotification` (`typescript2/pkg-playground/src/worker-protocol.ts`),
//! nested inside the `playgroundNotification` frame of
//! [`crate::playground_ws::WsOutMessage`]; field names are the serde-camelCased
//! spelling that file declares. Shapes here are the pre-rework ones verbatim —
//! changing one is a protocol change, not a refactor.
//!
//! The *sources* are `baml_ide` (function/test listings, param schemas) and
//! the LSP diagnostics candidate, so this module is purely the projection
//! from analysis facts onto the wire.

use baml_ide::{ParamSchema, TypeSchema};
use serde::Serialize;

// ── Project surface ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FunctionInfo {
    pub name: String,
    pub kind: FunctionKind,
    pub origin: FunctionOrigin,
    pub signature: String,
    pub source_position: FunctionSourcePosition,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capabilities: Option<LlmCapabilities>,
    /// Parameter schemas for the playground args form; named types inside are
    /// refs into [`ProjectUpdate::types`]. `None` (omitted on the wire) means
    /// no schema is available and the UI degrades to raw JSON; `Some(vec![])`
    /// means the function takes no arguments. Do not collapse the empty vec —
    /// the UI relies on the distinction.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Vec<ParamSchema>>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FunctionSourcePosition {
    pub file: String,
    pub line: u32,
    pub column: u32,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum FunctionKind {
    Llm,
    Expr,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum FunctionOrigin {
    UserDefined,
    Companion,
    Internal,
    AutoDerive,
}

impl From<baml_ide::FunctionOrigin> for FunctionOrigin {
    fn from(origin: baml_ide::FunctionOrigin) -> Self {
        match origin {
            baml_ide::FunctionOrigin::UserDefined => Self::UserDefined,
            baml_ide::FunctionOrigin::Companion => Self::Companion,
            baml_ide::FunctionOrigin::Internal => Self::Internal,
            baml_ide::FunctionOrigin::AutoDerive => Self::AutoDerive,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LlmCapabilities {
    pub render_prompt: bool,
    pub build_request: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_name: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectDiagnostic {
    pub severity: &'static str,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectUpdate {
    /// Whether the installed engine matches the current sources. `false`
    /// means runs are refused until a rebuild lands.
    pub is_bex_current: bool,
    /// Generation of the installed engine that backs this project update.
    pub generation: u64,
    pub functions: Vec<FunctionInfo>,
    /// Shared type table for `FunctionInfo.params` refs: every named type
    /// referenced from any function's schema, defined exactly once and keyed
    /// by canonical dotted FQN. `None` (omitted on the wire) means the binary
    /// predates the args form; `Some` may be an empty map. Same `None` ≠
    /// `Some(empty)` discipline as `params`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub types: Option<std::collections::BTreeMap<String, TypeSchema>>,
    pub diagnostics: Vec<ProjectDiagnostic>,
}

// ── Notifications ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum PlaygroundNotification {
    #[serde(rename_all = "camelCase")]
    ListProjects { projects: Vec<String> },
    #[serde(rename_all = "camelCase")]
    UpdateProject {
        project: String,
        update: ProjectUpdate,
    },
    #[serde(rename_all = "camelCase")]
    OpenPlayground {
        project: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        function_name: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        test_name: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        testset_name: Option<String>,
    },
    #[serde(rename_all = "camelCase")]
    ControlFlowGraphResult {
        function_name: String,
        graph: Option<serde_json::Value>,
        #[serde(skip_serializing_if = "Option::is_none")]
        request_id: Option<u32>,
    },
    #[serde(rename_all = "camelCase")]
    CursorContext { context: serde_json::Value },
    #[serde(rename_all = "camelCase")]
    TestCollectionResult {
        project: String,
        generation: u64,
        call_id: u64,
        data: Vec<u8>,
        /// If a testset expansion failed, the name + error message.
        #[serde(skip_serializing_if = "Option::is_none")]
        expand_error: Option<TestExpandError>,
    },
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TestExpandError {
    pub testset_name: String,
    pub message: String,
}

// ── Projection from analysis facts ───────────────────────────────────────────

/// Build the project's playground surface from a database snapshot.
///
/// `is_bex_current`/`generation` come from the engine runtime, and
/// `diagnostics` from the LSP diagnostics candidate — both read on the owner
/// thread in the same continuation that calls this, so one update never mixes
/// two revisions' facts.
pub fn build_project_update(
    db: &baml_db::ProjectDatabase,
    is_bex_current: bool,
    generation: u64,
    diagnostics: Vec<ProjectDiagnostic>,
) -> ProjectUpdate {
    let listing = baml_ide::list_functions_with_metadata(db);

    let functions = listing
        .functions
        .into_iter()
        .map(|f| FunctionInfo {
            name: f.name,
            signature: f.signature,
            source_position: FunctionSourcePosition {
                file: f.source_position.file,
                line: f.source_position.line,
                column: f.source_position.column,
            },
            kind: if f.is_llm {
                FunctionKind::Llm
            } else {
                FunctionKind::Expr
            },
            origin: f.origin.into(),
            capabilities: if f.is_llm {
                Some(LlmCapabilities {
                    render_prompt: true,
                    build_request: true,
                    client_name: f.client_name,
                })
            } else {
                None
            },
            params: f.params,
        })
        .collect();

    ProjectUpdate {
        is_bex_current,
        generation,
        functions,
        types: Some(listing.types),
        diagnostics,
    }
}

/// Flatten an admitted diagnostics publication into the playground's
/// one-line-per-diagnostic form (`file.baml:12: message`).
pub fn flatten_diagnostics(
    documents: &[baml_lsp::diagnostics::PublishableDocument],
) -> Vec<ProjectDiagnostic> {
    let mut out = Vec::new();
    for doc in documents {
        let filename = doc
            .path
            .file_name()
            .map(|f| f.to_string_lossy().into_owned())
            .unwrap_or_default();
        for d in &doc.diagnostics {
            let severity = match d.severity {
                Some(lsp_types::DiagnosticSeverity::ERROR) => "error",
                Some(lsp_types::DiagnosticSeverity::WARNING) => "warning",
                _ => "info",
            };
            let line = d.range.start.line + 1;
            out.push(ProjectDiagnostic {
                severity,
                message: format!("{filename}:{line}: {}", d.message),
            });
        }
    }
    out
}
