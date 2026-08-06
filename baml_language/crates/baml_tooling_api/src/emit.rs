//! Virtual-module and runtime-module emission types (API stub).

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::{ProjectSession, ToolingError};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Target {
    Node,
    Web,
}

impl Target {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Node => "node",
            Self::Web => "web",
        }
    }

    fn runtime_package(self) -> &'static str {
        match self {
            Self::Node => "@boundaryml/baml-bridge",
            Self::Web => "@boundaryml/baml-bridge-web",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "path", rename_all = "lowercase")]
pub enum EntrySpec {
    File(PathBuf),
    Client,
}

impl EntrySpec {
    pub(crate) fn cache_key(&self) -> String {
        match self {
            Self::File(path) => path.to_string_lossy().into_owned(),
            Self::Client => "baml:client".to_string(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SegmentRole {
    Declaration,
    Reference,
    Type,
    Documentation,
    Synthetic,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Segment {
    pub gen_start_utf16: u32,
    pub gen_length_utf16: u32,
    pub source_file: u32,
    pub source_start_utf8: u32,
    pub source_length_utf8: u32,
    pub symbol_id: String,
    pub signature_id: Option<String>,
    pub role: SegmentRole,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SegmentMap {
    pub version: u32,
    pub generated_file: String,
    pub sources: Vec<PathBuf>,
    pub source_hashes: Vec<String>,
    pub segments: Vec<Segment>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VirtualModule {
    pub id: String,
    pub runtime_id: String,
    pub code: String,
    pub declaration: String,
    pub map: SegmentMap,
    pub watch_files: Vec<PathBuf>,
    pub fingerprint: String,
    pub revision: u64,
    pub stale: bool,
}

impl VirtualModule {
    pub(crate) fn poison(id: String, revision: u64, fingerprint: String) -> Self {
        Self {
            runtime_id: String::new(),
            code: "const unavailable = new Proxy({}, { get() { throw new Error('BAML project has compiler errors'); } });\nexport const b = unavailable;\n".to_string(),
            declaration: "declare const unavailable: { readonly [name: string]: never };\nexport { unavailable as b };\n".to_string(),
            map: SegmentMap {
                version: 1,
                generated_file: id.clone(),
                sources: Vec::new(),
                source_hashes: Vec::new(),
                segments: Vec::new(),
            },
            id,
            watch_files: Vec::new(),
            fingerprint,
            revision,
            stale: true,
        }
    }

    pub fn into_generated(self) -> GeneratedFile {
        GeneratedFile {
            path: PathBuf::from(self.id),
            contents: self.declaration,
            map: self.map,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GeneratedFile {
    pub path: PathBuf,
    pub contents: String,
    pub map: SegmentMap,
}

pub(crate) fn emit_virtual_module(session: &ProjectSession, entry: &EntrySpec) -> VirtualModule {
    let _ = (session, entry);
    todo!("implemented in the bridge-sessions commit")
}

pub(crate) fn emit_runtime_module(session: &ProjectSession) -> Result<String, ToolingError> {
    let _ = session;
    todo!("implemented in the bridge-sessions commit")
}
