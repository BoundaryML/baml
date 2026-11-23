//! The `POST /v1/builds` request and answer as BCS's `shared/api` names
//! them. `bcs-sdk` publishes no client for the endpoint, so the types
//! live here until it does.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RegisterBuild {
    pub build: Build,
    pub package: Package,
    pub program: Option<Program>,
    pub source: Option<Source>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Build {
    pub build_id: String,
    /// Unix nanoseconds in decimal: a JSON number cannot carry them
    /// exactly.
    pub timestamp_ns: String,
    pub origin: String,
    pub git: Option<Git>,
    pub source_hash: String,
    pub compilation_unit_hash: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Git {
    pub remote: Option<String>,
    pub hash: String,
    pub dirty: bool,
    pub branch: Option<String>,
    pub package_path: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Package {
    pub compilation_unit_hash: String,
    pub package_interface: String,
    pub dependencies: Vec<String>,
    pub name: String,
    pub version: Option<String>,
    pub compiler_version: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Program {
    pub program_id: String,
    pub static_packages: Vec<String>,
}

/// The source as it is uploaded: a UTF-8 JSON object of normalized path
/// to file text.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Source {
    pub sha256: String,
    pub byte_length: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum SourceUpload {
    NotRequested,
    AlreadyPresent,
    UploadRequired {
        presigned_put_url: String,
        expires_at_unix_ms: u64,
    },
}
