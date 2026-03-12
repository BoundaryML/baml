use std::collections::HashMap;

use baml_base::{Name, QualifiedName};
use baml_compiler2_ast::ExprId;
use baml_type::{Ty, TyAttr};

use crate::builder::MirBuilder;
use crate::ir::*;

// --- Helper enums carried forward from old code ---

enum FieldSource {
    Named(ExprId),
    Spread(Local, usize),
}

enum SwitchKind {
    Integer,
    EnumDiscriminant(Name),
    TypeTag,
}

struct LoopContext {
    break_target: BlockId,
    continue_target: BlockId,
    watched_locals_depth: usize,
}

struct CatchContext {
    unwind_target: BlockId,
    error_local: Local,
}

struct PendingHeader {
    name: String,
}

struct VizContext {
    function_name: String,
    next_node_id: u32,
    parent_keys: Vec<String>,
    ordinal_counters: Vec<u32>,
}

impl VizContext {
    fn new(function_name: String) -> Self {
        Self {
            function_name,
            next_node_id: 0,
            parent_keys: Vec::new(),
            ordinal_counters: Vec::new(),
        }
    }
}

// --- New compiler2 entry point (stub) ---

use baml_compiler2_hir::loc::FunctionLoc;

pub fn lower_function<'db>(
    db: &'db dyn crate::Db,
    func_loc: FunctionLoc<'db>,
) -> Option<MirFunction> {
    todo!("lower_function: not yet implemented")
}
