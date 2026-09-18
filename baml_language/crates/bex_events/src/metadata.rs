//! Program metadata and shared owned function metadata.

pub use btel_types::{
    DefinitionKey, FunctionMetadata, FunctionMetadataTable, RuntimeFunctionKind,
    RuntimeFunctionOrigin, SourceSpan,
};

use crate::ids::{ProgramId, SourceSnapshotId};

/// A point-in-time program metadata snapshot, including dynamic functions. `program_id`
/// is currently random per engine instance (artifact-local joins only, not
/// durable cross-run identity).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProgramMetadata {
    pub program_id: ProgramId,
    pub source_snapshot_id: Option<SourceSnapshotId>,
    pub function_table: FunctionMetadataTable,
}
