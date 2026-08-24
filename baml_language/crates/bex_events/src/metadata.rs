//! Program/function metadata: the per-artifact join table that turns the
//! compact `function_id` on every event back into a real definition.
//!
//! The join path is `program_id + function_id -> FunctionMetadata ->
//! definition_key / source / revision / semantic lanes`. Events stay
//! compact (ids only); everything semantic lives here and is registered once
//! per engine with the direct consumer. Enrichment fields
//! (`source_snapshot_id`, `revision_id`, `semantic_lanes`, lambda
//! identity) are modeled but `None` until authoritative compiler/cloud
//! sources populate them — consumers must tolerate that.

use crate::ids::{FunctionId, ProgramId, SourceSnapshotId};

/// Identity of a definition revision (BEP-053). Not yet populated by the
/// runtime.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RevisionId(pub String);

/// Stable key naming a definition site (`function:pkg.f`, `class:pkg.C`).
/// The cross-revision join handle: two revisions of the same definition
/// share a `DefinitionKey` but differ in [`RevisionId`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DefinitionKey(pub String);

/// A 256-bit semantic hash (BEP-053 lane value). The runtime never computes
/// these on hot paths; they arrive via compiler/cloud enrichment.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Hash256(pub [u8; 32]);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceSpan {
    pub file_id: u32,
    pub start: u32,
    pub end: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RuntimeFunctionKind {
    Bytecode,
    SysOp(String),
    Native,
    NativeUnresolved,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RuntimeFunctionOrigin {
    UserDefined,
    Companion,
    Internal,
    Builtin,
    AutoDerive,
}

impl From<bex_vm_types::FunctionKind> for RuntimeFunctionKind {
    fn from(value: bex_vm_types::FunctionKind) -> Self {
        match value {
            bex_vm_types::FunctionKind::Bytecode => Self::Bytecode,
            bex_vm_types::FunctionKind::SysOp(op) => Self::SysOp(format!("{op:?}")),
            bex_vm_types::FunctionKind::NativeUnresolved => Self::NativeUnresolved,
            bex_vm_types::FunctionKind::Native(_) => Self::Native,
        }
    }
}

impl From<bex_vm_types::FunctionOrigin> for RuntimeFunctionOrigin {
    fn from(value: bex_vm_types::FunctionOrigin) -> Self {
        match value {
            bex_vm_types::FunctionOrigin::UserDefined => Self::UserDefined,
            bex_vm_types::FunctionOrigin::Companion => Self::Companion,
            bex_vm_types::FunctionOrigin::Internal => Self::Internal,
            bex_vm_types::FunctionOrigin::Builtin => Self::Builtin,
            bex_vm_types::FunctionOrigin::AutoDerive => Self::AutoDerive,
        }
    }
}

/// BEP-053 semantic-hash lanes for a definition. Distinct revisions may
/// share identical lanes (a formatting-only change) — consumers must not
/// collapse revisions by lane equality.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SemanticLanes {
    pub direct_interface: Hash256,
    pub effective_interface: Hash256,
    pub direct_implementation: Option<Hash256>,
    pub effective_implementation: Option<Hash256>,
}

/// One row of the program function table. `function_id` is the join
/// key from events; it is NOT stable across recompiles (it is a per-run
/// sequential id), so rows are only meaningful with their own artifact's
/// program metadata.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FunctionMetadata {
    pub function_id: FunctionId,
    pub fqn: String,
    pub display_name: String,
    pub source_file: Option<String>,
    pub source_span: Option<SourceSpan>,
    pub kind: RuntimeFunctionKind,
    pub origin: RuntimeFunctionOrigin,
    pub owner_type: Option<DefinitionKey>,
    pub parent_function: Option<DefinitionKey>,
    pub lambda_path: Option<String>,
    pub definition_key: Option<DefinitionKey>,
    pub package_name: Option<String>,
    pub namespace: Vec<String>,
    pub source_snapshot_id: Option<SourceSnapshotId>,
    pub revision_id: Option<RevisionId>,
    pub semantic_lanes: Option<SemanticLanes>,
}

/// The function table registered with the direct consumer. Contains one
/// row per compile-time function plus reserved synthetic rows (the
/// spawn-closure root and the unknown-function sentinel) with ids just past
/// the pool.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FunctionMetadataTable {
    pub functions: Vec<FunctionMetadata>,
}

impl FunctionMetadataTable {
    #[must_use]
    pub fn get(&self, function_id: FunctionId) -> Option<&FunctionMetadata> {
        self.functions.iter().find(|f| f.function_id == function_id)
    }
}

/// Program-level metadata an engine derives at construction. `program_id`
/// is currently random per engine instance (artifact-local joins only, not
/// durable cross-run identity).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProgramMetadata {
    pub program_id: ProgramId,
    pub source_snapshot_id: Option<SourceSnapshotId>,
    pub revision_id: Option<RevisionId>,
    pub function_table: FunctionMetadataTable,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn metadata(function_id: u32, revision: &str) -> FunctionMetadata {
        FunctionMetadata {
            function_id: FunctionId(function_id),
            fqn: "user.main".to_string(),
            display_name: "main".to_string(),
            source_file: Some("main.baml".to_string()),
            source_span: None,
            kind: RuntimeFunctionKind::Bytecode,
            origin: RuntimeFunctionOrigin::UserDefined,
            owner_type: None,
            parent_function: None,
            lambda_path: None,
            definition_key: Some(DefinitionKey("function:user.main".to_string())),
            package_name: Some("user".to_string()),
            namespace: Vec::new(),
            source_snapshot_id: None,
            revision_id: Some(RevisionId(revision.to_string())),
            semantic_lanes: Some(SemanticLanes {
                direct_interface: Hash256([1; 32]),
                effective_interface: Hash256([2; 32]),
                direct_implementation: Some(Hash256([3; 32])),
                effective_implementation: Some(Hash256([4; 32])),
            }),
        }
    }

    #[test]
    fn function_metadata_supports_bep_053_join_without_collapsing_revisions() {
        let table = FunctionMetadataTable {
            functions: vec![metadata(1, "rev-1"), metadata(2, "rev-2")],
        };

        let first = table.get(FunctionId(1)).unwrap();
        let second = table.get(FunctionId(2)).unwrap();

        assert_eq!(
            first.definition_key,
            Some(DefinitionKey("function:user.main".to_string()))
        );
        assert_eq!(first.semantic_lanes, second.semantic_lanes);
        assert_ne!(first.revision_id, second.revision_id);
    }
}
