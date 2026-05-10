//! Source-pattern → `DPat` lowering.
//!
//! Bridges the AST `Pattern` HIR to the deconstructed-pattern algorithm in
//! [`crate::exhaustiveness`]. This module owns the public per-pattern result
//! types:
//!
//! - [`PatternResult`] — the per-pattern output: a `DPat` for the matrix,
//!   the narrowed `matched_ty` for arm bodies, the upward-flowing
//!   `required_ty`, and any bindings introduced (typed at their final
//!   widened type for chains).
//!
//! - [`PatternBinding`] — a single name binding produced during the walk.
//!
//! The pattern-lowering walk itself (`analyze_and_lower` and friends) and
//! the [`crate::exhaustiveness::PatCtx`] impl for
//! [`crate::builder::TypeInferenceBuilder`] both live in `builder.rs`,
//! matching Rust's "trait impl with the type" convention.

use baml_base::Name;
use baml_compiler2_ast::PatId;

// Ergonomic re-exports so callers don't need two `use` lines.
pub use crate::exhaustiveness::{ArmId, WitnessPat, check_irrefutable, compute_match_usefulness};
use crate::{exhaustiveness::DPat, ty::Ty};

/// Result of lowering one source pattern.
///
/// Carries everything the surrounding inference layer needs:
/// - `dpat` for the exhaustiveness matrix,
/// - `required_ty` for upward type-flow into the scrutinee context,
/// - `matched_ty` for downward type-flow into the arm body,
/// - `bindings` to register in scope at the binding's final widened type.
#[derive(Debug, Clone)]
pub struct PatternResult {
    /// Deconstructed form for the matrix algorithm.
    pub dpat: DPat,
    /// Type the pattern *requires* of its scrutinee (upward flow). `None`
    /// for wildcards/bindings, which place no requirement. For chains, the
    /// widest type ascription. For class/array patterns, the synthesized
    /// shape type.
    pub required_ty: Option<Ty>,
    /// Type the scrutinee is *narrowed to* by this pattern (downward flow
    /// into the arm body). For `match x: A | B { f: A => …}`, the body
    /// sees `f: A`, not `A | B`.
    pub matched_ty: Ty,
    /// Bindings introduced by this pattern, each typed at the final
    /// widened type (chain semantics: every Bind in a chain shares the
    /// chain's rightmost `current_ty`).
    pub bindings: Vec<PatternBinding>,
}

/// A single name binding introduced by a pattern.
#[derive(Debug, Clone)]
pub struct PatternBinding {
    pub name: Name,
    pub pat_id: PatId,
    pub ty: Ty,
}
