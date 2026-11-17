//! Program metadata and shared owned function metadata.

pub use btel_types::{
    DefinitionKey, FunctionMetadata, FunctionMetadataTable, RuntimeFunctionKind,
    RuntimeFunctionOrigin, SourceSpan,
};

use crate::ids::{ProgramId, SourceSnapshotId};

/// A point-in-time program metadata snapshot, including dynamic functions.
/// `program_id` identifies the linked program by its content
/// (`bex_vm_types::identity::linked_program_id`), the same for every run of
/// it; a program whose packages carry no metadata gets a random one per
/// engine instance instead.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProgramMetadata {
    pub program_id: ProgramId,
    pub source_snapshot_id: Option<SourceSnapshotId>,
    pub function_table: FunctionMetadataTable,
}
