//! `baml_compiler2_tir` — Per-scope type inference for the compiler2 pipeline.
//!
//! Provides:
//! - `Ty` — the resolved type representation
//! - `ScopeInference` — per-scope expression type map with optional diagnostics
//! - `infer_scope_types(db, ScopeId)` — per-scope Salsa tracked query
//! - `TypeInferenceBuilder` — walks ExprBody within a scope, infers types
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

// Rust 1.93 surfaces a large volume of style-only Clippy churn in the
// compiler2 TIR crate. Keep the canary integration branch buildable while the
// compiler2 pipeline is still landing; pay down these lints separately.
#![allow(
    clippy::assigning_clones,
    clippy::bool_to_int_with_if,
    clippy::clone_on_copy,
    clippy::cloned_ref_to_slice_refs,
    clippy::collapsible_match,
    clippy::doc_markdown,
    clippy::elidable_lifetime_names,
    clippy::enum_variant_names,
    clippy::float_cmp,
    clippy::items_after_statements,
    clippy::let_and_return,
    clippy::manual_let_else,
    clippy::needless_pass_by_value,
    clippy::ptr_as_ptr,
    clippy::question_mark,
    clippy::redundant_closure,
    clippy::redundant_closure_for_method_calls,
    clippy::redundant_clone,
    clippy::ref_as_ptr,
    clippy::return_self_not_must_use,
    clippy::self_only_used_in_recursion,
    clippy::trivially_copy_pass_by_ref,
    clippy::uninlined_format_args,
    clippy::unnecessary_cast,
    clippy::unused_self
)]

pub mod builder;
pub mod cycle_detector;
pub mod generics;
pub mod infer_context;
pub mod inference;
pub mod lower_type_expr;
pub mod narrowing;
pub mod normalize;
pub mod resolve;
pub mod throw_inference;
pub mod ty;

// ── Db trait ──────────────────────────────────────────────────────────────────

/// Database trait for compiler2_tir queries.
///
/// Extends `baml_compiler2_ppir::Db` (transitively `baml_compiler2_hir::Db`).
/// Use `infer_scope_types` for type inference queries, `resolve_name_at` for
/// name resolution.
#[salsa::db]
pub trait Db: baml_compiler2_ppir::Db {}
