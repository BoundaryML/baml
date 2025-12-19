//! Stackification MIR to bytecode compiler (v2).
//!
//! This module compiles MIR (Mid-level IR) control flow graphs to VM bytecode
//! using a "stackification" approach that classifies locals as Virtual or Real:
//!
//! - **Virtual locals**: Single-use temporaries that are inlined at use site
//! - **Real locals**: Multi-use or cross-block variables that need stack slots
//!
//! This produces more efficient bytecode than the naive approach by avoiding
//! unnecessary store/load pairs for temporary values.

mod analysis;
mod emit;

use std::collections::HashMap;

use baml_vm::ObjectPool;
pub use emit::compile_mir_function;

/// Context for MIR codegen.
///
/// Contains all shared state needed during MIR compilation:
/// global mappings, class information, and the shared object pool.
pub(crate) struct MirCodegenContext<'ctx, 'obj> {
    /// Resolved global names to indices (function names -> global index).
    pub globals: &'ctx HashMap<String, usize>,
    /// Resolved class field indices (class name -> field name -> field index).
    pub classes: &'ctx HashMap<String, HashMap<String, usize>>,
    /// Pre-allocated Class object indices in the program's object pool.
    pub class_object_indices: &'ctx HashMap<String, usize>,
    /// Shared object pool for strings, etc.
    pub objects: &'obj mut ObjectPool,
}
