//! Body type inference. Slice S5 implements the real `InferenceContext`;
//! this is the S0 stub so the spec harness exercises this engine (not TIR)
//! from day one. Every fixture is expected to fail until slices land.

use baml_compiler2_ast::{ExprId, PatId};
use baml_compiler2_hir::loc::FunctionLoc;
use baml_type::interned::Ty;
use rustc_hash::FxHashMap;

/// Inference side tables for one body owner, keyed by arena ids, mirroring
/// rust-analyzer's `InferenceResult`. Types are the hash-consed
/// `baml_type::interned` representation (this crate's native vocabulary);
/// they are materialized to plain `baml_type::Ty` only at consumer
/// boundaries, after resolve-all guarantees no inference variables remain.
/// Grows one map per slice; consumers must treat a missing entry as "not
/// inferred", never as an error.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct InferenceResult {
    pub type_of_expr: FxHashMap<ExprId, Ty>,
    pub type_of_binding: FxHashMap<PatId, Ty>,
}

/// Infers types for one function body.
///
/// S0 stub: returns empty tables. S5 replaces this with an
/// `InferenceContext` walk; the S1 body-owner ID will widen the key from
/// `FunctionLoc` to all body owners and make this a salsa query.
pub fn infer_function<'db>(
    _db: &'db dyn baml_compiler2_ppir::Db,
    _function: FunctionLoc<'db>,
) -> InferenceResult {
    InferenceResult::default()
}
