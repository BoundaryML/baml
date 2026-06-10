use crate::ids::{EngineId, FunctionId, ProcessEuid, ProgramId, SourceSnapshotId};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RevisionId(pub String);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DefinitionKey(pub String);

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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SemanticLanes {
    pub direct_interface: Hash256,
    pub effective_interface: Hash256,
    pub direct_implementation: Option<Hash256>,
    pub effective_implementation: Option<Hash256>,
}

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

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FunctionMetadataTable {
    pub functions: Vec<FunctionMetadata>,
}

impl FunctionMetadataTable {
    #[must_use]
    pub fn get(&self, function_id: FunctionId) -> Option<&FunctionMetadata> {
        self.functions.iter().find(|f| f.function_id == function_id)
    }

    #[must_use]
    pub fn function_id_for_fqn(&self, fqn: &str) -> Option<FunctionId> {
        self.functions
            .iter()
            .find(|f| f.fqn == fqn)
            .map(|f| f.function_id)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProgramMetadata {
    pub program_id: ProgramId,
    pub source_snapshot_id: Option<SourceSnapshotId>,
    pub revision_id: Option<RevisionId>,
    pub function_table: FunctionMetadataTable,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EventFileHeaderV1 {
    pub process_euid: ProcessEuid,
    pub engine_id: EngineId,
    pub program_id: ProgramId,
    pub source_snapshot_id: Option<SourceSnapshotId>,
    pub revision_id: Option<RevisionId>,
    pub started_at_epoch_ns: u128,
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
