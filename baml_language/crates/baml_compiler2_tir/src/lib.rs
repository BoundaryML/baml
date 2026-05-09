//! `baml_compiler2_tir` — Per-scope type inference for the compiler2 pipeline.
//!
//! Provides:
//! - `Ty` — the resolved type representation
//! - `ScopeInference` — per-scope expression type map with optional diagnostics
//! - `infer_scope_types(db, ScopeId)` — per-scope Salsa tracked query
//! - `TypeInferenceBuilder` — walks `ExprBody` within a scope, infers types
//! - `resolve_name_at(db, file, offset, name)` — on-demand name resolution
//! - `resolve_class_fields`, `resolve_type_alias` — per-item structural queries
//! - `CycleDetector` — runtime cycle guard for recursive type handling
//!
//! ## Architecture
//!
//! The main query is `infer_scope_types(db, ScopeId) -> ScopeInference`, which
//! returns expression types for a single scope — NOT a monolithic per-function
//! result. This gives fine-grained incrementality: editing a lambda body only
//! recomputes that lambda's `ScopeInference`, not the enclosing function's.

pub mod analysis;
pub mod builder;
pub mod callable;
pub mod cycle_detector;
pub mod exhaustiveness;
pub mod generics;
pub mod infer_context;
pub mod inference;
pub mod lower_type_expr;
pub mod narrowing;
pub mod normalize;
pub mod package_interface;
pub mod pattern_lowering;
pub mod resolve;
pub mod throw_inference;
pub mod throws_analysis;
pub mod ty;
pub mod user_facing;

// ── Db trait ──────────────────────────────────────────────────────────────────

/// Database trait for `compiler2_tir` queries.
///
/// Extends `baml_compiler2_hir::Db`. Use `infer_scope_types` for type
/// inference queries, `resolve_name_at` for name resolution.
#[salsa::db]
pub trait Db: baml_compiler2_ppir::Db {}
