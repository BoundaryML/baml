//! Tooling emission for direct `.baml` imports (API stub).
//!
//! The emitter reuses the canonical `translate_ty` and `SymbolPool` code
//! paths and resolves compiler exports by stable symbol identity, never by
//! bare display name. Behavior lands in the SDK-emission commit together
//! with its tests.

use std::collections::BTreeSet;

use baml_codegen_types::SymbolPool;
use baml_surface::PackageExport;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ToolingDeclarationRole {
    Declaration,
    Type,
    Documentation,
    Synthetic,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ToolingMappedSpan {
    pub start_utf16: u32,
    pub length_utf16: u32,
    pub symbol_id: String,
    pub role: ToolingDeclarationRole,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ToolingEmitOutput {
    pub declaration: String,
    pub code: String,
    pub spans: Vec<ToolingMappedSpan>,
    pub type_exports: BTreeSet<String>,
    pub value_exports: BTreeSet<String>,
}

/// Emit the declaration and JavaScript projections for a direct `.baml`
/// import. Type expressions are translated from compiler-owned `Ty` values.
pub fn emit_tooling_module(
    pool: &SymbolPool,
    exports: &PackageExport,
    runtime_id: &str,
    runtime_package: &str,
    banner: &str,
) -> ToolingEmitOutput {
    let _ = (pool, exports, runtime_id, runtime_package, banner);
    todo!("implemented in the SDK-emission commit")
}
