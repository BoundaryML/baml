//! Host-neutral TypeScript tooling over the canonical BAML compiler database.
//!
//! Hosts provide complete source snapshots. This crate never reparses BAML or
//! guesses symbol identity: diagnostics and navigation come from
//! `ProjectDatabase`, `baml_lsp2_actions`, and `baml_surface`.
//!
//! This commit introduces the public API surface only: every operation is
//! declared with its final signature and a `todo!()` body. Behavior lands in
//! the compiler-project/bridge-sessions implementation commit.

mod emit;
mod protocol;

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use baml_project::ProjectDatabase;
pub use emit::{EntrySpec, GeneratedFile, Segment, SegmentMap, SegmentRole, Target, VirtualModule};
pub use protocol::ToolingProtocol;
use serde::{Deserialize, Serialize};

pub const PROTOCOL_VERSION: &str = "baml.tooling.v1";
pub const TYPESCRIPT_IMPORTS_CAPABILITY: &str = "typescriptImports.v1";
pub const RENAME_CAPABILITY: &str = "rename.v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Capabilities {
    pub protocol: String,
    pub compiler_version: String,
    pub features: Vec<String>,
}

impl Default for Capabilities {
    fn default() -> Self {
        Self {
            protocol: PROTOCOL_VERSION.to_string(),
            compiler_version: baml_version::CANONICAL_VERSION.to_string(),
            features: vec![
                TYPESCRIPT_IMPORTS_CAPABILITY.to_string(),
                RENAME_CAPABILITY.to_string(),
            ],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceInput {
    pub path: PathBuf,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Location {
    pub path: PathBuf,
    pub start_utf8: u32,
    pub length_utf8: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolingDiagnostic {
    pub code: String,
    pub message: String,
    pub severity: String,
    pub location: Option<Location>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckResult {
    pub revision: u64,
    pub diagnostics: Vec<ToolingDiagnostic>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectLayout {
    pub config_path: PathBuf,
    pub roots: Vec<PathBuf>,
    pub source_files: Vec<PathBuf>,
    pub watch_files: Vec<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Hover {
    pub markdown: String,
    pub location: Location,
    pub symbol_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompletionItem {
    pub label: String,
    pub kind: String,
    pub detail: String,
    pub documentation: String,
    pub symbol_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextEdit {
    pub location: Location,
    pub new_text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceEdit {
    pub edits: Vec<TextEdit>,
}

#[derive(Debug, thiserror::Error)]
pub enum ToolingError {
    #[error("stale update for {path}: version {received} is not newer than {current}")]
    StaleUpdate {
        path: PathBuf,
        received: u64,
        current: u64,
    },
    #[error("not a BAML source or baml.toml path: {0}")]
    NonBamlPath(PathBuf),
    #[error("BAML file is not in this project: {0}")]
    UnknownFile(PathBuf),
    #[error("BAML symbol is not available at this position")]
    NoSymbol,
    #[error("unsupported rename: {0}")]
    UnsupportedRename(String),
    #[error("invalid BAML identifier: {0}")]
    InvalidIdentifier(String),
    #[error("rename would collide with existing symbol: {0}")]
    RenameCollision(String),
    #[error("BAML project has compiler errors")]
    CompilerErrors,
    #[error("failed to emit BAML bytecode: {0}")]
    Bytecode(String),
}

/// Long-lived compiler session. All offsets crossing this boundary are UTF-8
/// byte offsets, matching the compiler's native spans.
pub struct ProjectSession {
    root: PathBuf,
    config_path: PathBuf,
    config_text: String,
    target: Target,
    db: ProjectDatabase,
    versions: HashMap<PathBuf, u64>,
    revision: u64,
    last_good: HashMap<String, VirtualModule>,
}

impl ProjectSession {
    /// Opens a session over a complete host snapshot. Only BAML sources and
    /// the project's own `baml.toml` may enter the compiler database, so a
    /// snapshot carrying anything else is rejected outright.
    pub fn open(
        root: impl AsRef<Path>,
        files: Vec<SourceInput>,
        target: Target,
    ) -> Result<Self, ToolingError> {
        let _ = (root, files, target);
        todo!("implemented in the bridge-sessions commit")
    }

    pub fn update_file(
        &mut self,
        path: &Path,
        text: Option<&str>,
        version: u64,
    ) -> Result<u64, ToolingError> {
        let _ = (path, text, version);
        todo!("implemented in the bridge-sessions commit")
    }

    pub fn check(&self) -> CheckResult {
        todo!("implemented in the bridge-sessions commit")
    }

    pub fn emit_virtual_module(&self, entry: &EntrySpec) -> Result<VirtualModule, ToolingError> {
        let _ = entry;
        todo!("implemented in the bridge-sessions commit")
    }

    /// Editor emission preserves a last-known-good declaration across an
    /// invalid intermediate compiler revision.
    pub fn emit_editor_module(&mut self, entry: &EntrySpec) -> VirtualModule {
        let _ = entry;
        todo!("implemented in the bridge-sessions commit")
    }

    pub fn definition_at(&self, path: &Path, offset_utf8: u32) -> Vec<Location> {
        let _ = (path, offset_utf8);
        todo!("implemented in the bridge-sessions commit")
    }

    pub fn references_at(&self, path: &Path, offset_utf8: u32) -> Vec<Location> {
        let _ = (path, offset_utf8);
        todo!("implemented in the bridge-sessions commit")
    }

    pub fn definition_for_symbol(&self, symbol_id: &str) -> Vec<Location> {
        let _ = symbol_id;
        todo!("implemented in the bridge-sessions commit")
    }

    pub fn references_for_symbol(&self, symbol_id: &str) -> Vec<Location> {
        let _ = symbol_id;
        todo!("implemented in the bridge-sessions commit")
    }

    pub fn hover_for_symbol(&self, symbol_id: &str) -> Option<Hover> {
        let _ = symbol_id;
        todo!("implemented in the bridge-sessions commit")
    }

    pub fn completions(&self, entry: &EntrySpec) -> Vec<CompletionItem> {
        let _ = entry;
        todo!("implemented in the bridge-sessions commit")
    }

    pub fn prepare_rename(&self, symbol_id: &str) -> Result<Location, ToolingError> {
        let _ = symbol_id;
        todo!("implemented in the bridge-sessions commit")
    }

    pub fn rename(&self, symbol_id: &str, new_name: &str) -> Result<WorkspaceEdit, ToolingError> {
        let _ = (symbol_id, new_name);
        todo!("implemented in the bridge-sessions commit")
    }

    pub fn layout(&self) -> ProjectLayout {
        todo!("implemented in the bridge-sessions commit")
    }

    pub fn watch_files(&self) -> Vec<PathBuf> {
        todo!("implemented in the bridge-sessions commit")
    }

    pub fn fingerprint(&self) -> String {
        todo!("implemented in the bridge-sessions commit")
    }

    pub fn capabilities(&self) -> Capabilities {
        todo!("implemented in the bridge-sessions commit")
    }

    pub fn revision(&self) -> u64 {
        todo!("implemented in the bridge-sessions commit")
    }

    pub fn project_id(&self) -> String {
        todo!("implemented in the bridge-sessions commit")
    }
}

/// Sessions keyed by canonical project root for native hosts serving multiple
/// TypeScript projects in one process.
#[derive(Default)]
pub struct ToolingWorkspace {
    sessions: HashMap<PathBuf, ProjectSession>,
}

impl ToolingWorkspace {
    /// A rejected snapshot leaves any session already open at this root
    /// untouched: a bad open must not evict a working project.
    pub fn open(
        &mut self,
        root: impl AsRef<Path>,
        files: Vec<SourceInput>,
        target: Target,
    ) -> Result<&mut ProjectSession, ToolingError> {
        let _ = (root, files, target);
        todo!("implemented in the bridge-sessions commit")
    }

    pub fn get(&self, root: &Path) -> Option<&ProjectSession> {
        let _ = root;
        todo!("implemented in the bridge-sessions commit")
    }

    pub fn get_mut(&mut self, root: &Path) -> Option<&mut ProjectSession> {
        let _ = root;
        todo!("implemented in the bridge-sessions commit")
    }
}
