//! Program and function metadata derived from the compiled program.

use crate::ids::{FunctionId, ProgramId, SourceSnapshotId};

/// Stable key naming a definition site (`function:pkg.f`, `class:pkg.C`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DefinitionKey(pub String);

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

/// Metadata for one compiled function, identified within this program.
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
}

/// One metadata row per compiled function.
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
    pub function_table: FunctionMetadataTable,
}
