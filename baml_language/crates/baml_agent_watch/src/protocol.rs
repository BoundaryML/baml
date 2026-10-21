//! Wire protocol for the Agent Activity page.
//!
//! JSON text frames on `/api/agent-activity/ws`. On connect the server sends
//! one `hello` with the session so far (baselines, change history, runs,
//! notes, workflows); after that it streams `change`, `run`, `note`, `git`,
//! and `flows` messages as they happen. A change carries only a file's new
//! content: the page derives the previous content from the baseline and the
//! earlier changes, so a reconnect replays exactly what a live page saw.
//!
//! Keep in sync with `typescript2/pkg-agent-activity/src/engine/protocol.ts`.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

pub(crate) const PROTOCOL_VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CommitInfo {
    pub(crate) sha: String,
    pub(crate) subject: String,
    /// Unix milliseconds.
    pub(crate) time: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GitState {
    pub(crate) branch: Option<String>,
    pub(crate) head: String,
    pub(crate) head_subject: String,
    /// HEAD when the session started.
    pub(crate) start_head: String,
    /// Commits reachable from HEAD but not from `start_head`, oldest first.
    pub(crate) commits: Vec<CommitInfo>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SessionInfo {
    pub(crate) root: String,
    pub(crate) name: String,
    /// Unix milliseconds.
    pub(crate) started_at: u64,
    pub(crate) git: Option<GitState>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum ChangeKind {
    Created,
    Modified,
    Deleted,
}

/// An image the page loads from the server instead of receiving inline.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MediaRef {
    /// Root-relative URL on this server; the path ends in the content address.
    pub(crate) url: String,
    pub(crate) mime: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FileChange {
    /// Monotonic per session, starting at 1.
    pub(crate) seq: u64,
    /// Unix milliseconds when the write settled.
    pub(crate) time: u64,
    /// Relative to the session root, `/`-separated.
    pub(crate) path: String,
    pub(crate) kind: ChangeKind,
    /// New content; `None` for deletions and for files not shown as text.
    pub(crate) after: Option<String>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub(crate) binary: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub(crate) too_large: bool,
    pub(crate) size: Option<u64>,
    /// The new content when it is an image.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) media: Option<MediaRef>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RunKind {
    BamlRun,
    BamlTest,
    Command,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RunStatus {
    Running,
    Passed,
    Failed,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LlmCall {
    pub client: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_tokens: Option<u64>,
}

/// A function or declaration a run executed, located in the source.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunFrame {
    pub name: String,
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub llm: Option<LlmCall>,
}

/// Something the agent executed: `baml run`, `baml test`, or any command.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunInfo {
    pub id: String,
    pub kind: RunKind,
    pub command: String,
    pub status: RunStatus,
    pub started_at: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ended_at: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<RunFrame>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frames: Option<Vec<RunFrame>>,
    #[serde(default)]
    pub touches: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub log: Option<String>,
}

/// Something the agent said, from its transcript.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AgentNote {
    pub(crate) id: String,
    pub(crate) time: u64,
    pub(crate) text: String,
    pub(crate) agent: String,
}

/// An entry function's control-flow graph with its callees inlined, as the
/// playground's graph view renders it.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Workflow {
    pub(crate) name: String,
    /// Root-relative file declaring the entry function.
    pub(crate) path: String,
    /// The prepared `ControlFlowGraph`; source spans carry root-relative paths.
    pub(crate) graph: serde_json::Value,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FlowsMessage {
    /// Root-relative project directory.
    pub(crate) project: String,
    pub(crate) workflows: Vec<Workflow>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct HelloMessage {
    pub(crate) protocol: u32,
    pub(crate) session: SessionInfo,
    pub(crate) baselines: BTreeMap<String, Option<String>>,
    /// Paths in `baselines` that were images when the session started.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub(crate) baseline_media: BTreeMap<String, MediaRef>,
    pub(crate) changes: Vec<FileChange>,
    pub(crate) runs: Vec<RunInfo>,
    pub(crate) notes: Vec<AgentNote>,
    pub(crate) flows: Vec<FlowsMessage>,
}

#[derive(Serialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub(crate) enum ServerMessage<'a> {
    Hello(&'a HelloMessage),
    Change {
        change: &'a FileChange,
        /// Present on the first change to a path: its content when the session started.
        #[serde(skip_serializing_if = "Option::is_none")]
        baseline: Option<&'a Option<String>>,
        /// With `baseline`, when the path was an image when the session started.
        #[serde(rename = "baselineMedia", skip_serializing_if = "Option::is_none")]
        baseline_media: Option<&'a MediaRef>,
    },
    Git {
        git: &'a GitState,
    },
    Run {
        run: &'a RunInfo,
    },
    Note {
        note: &'a AgentNote,
    },
    Flows(&'a FlowsMessage),
}

impl ServerMessage<'_> {
    pub(crate) fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|err| {
            tracing::error!("agent watch: could not encode a message: {err}");
            String::from("{}")
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn change_serializes_with_the_page_field_names() {
        let change = FileChange {
            seq: 3,
            time: 10,
            path: "baml_src/main.baml".into(),
            kind: ChangeKind::Created,
            after: Some("class A {}".into()),
            binary: false,
            too_large: false,
            size: Some(10),
            media: None,
        };
        let baseline = None;
        let json = ServerMessage::Change {
            change: &change,
            baseline: Some(&baseline),
            baseline_media: None,
        }
        .to_json();
        assert_eq!(
            json,
            r#"{"type":"change","change":{"seq":3,"time":10,"path":"baml_src/main.baml","kind":"created","after":"class A {}","size":10},"baseline":null}"#
        );
    }

    #[test]
    fn run_kinds_use_kebab_case() {
        assert_eq!(
            serde_json::to_string(&RunKind::BamlRun).unwrap(),
            r#""baml-run""#
        );
        assert_eq!(
            serde_json::to_string(&RunStatus::Passed).unwrap(),
            r#""passed""#
        );
    }
}
