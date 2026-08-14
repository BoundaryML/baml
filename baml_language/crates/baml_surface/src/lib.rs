//! `baml_surface` — the semantic surface of a BAML project.
//!
//! High-level, object-flavored access to BAML code: lightweight `Copy` handles
//! over the compiler's interned item ids, whose methods are thin wrappers over
//! individual Salsa queries. Nothing here computes; anything that does lands in
//! a compiler crate first and is *wrapped* here.
//!
//! This layer is the API boundary between the compiler
//! (`baml_compiler2_{hir,ppir,hir_ty}`) and everything that describes code to a
//! human or a tool: `baml describe`, SDK codegen's symbol pool, the playground
//! schemas, and the editor crate's hover/completion internals. Consumers
//! depend on this crate, not on compiler internals — so the compiler's
//! internals (notably the in-flight type-system rework) can change under a
//! stable API.
//!
//! Two rules keep the boundary real:
//!
//! - **Handles are ids.** Every handle is `Copy`, `Eq`, `Hash` — usable as a
//!   map key, storable in other Salsa structs, and meaningless without a
//!   database. Properties are methods taking `&dyn Db`, memoized at the query
//!   layer below, never materialized here.
//! - **All type-system facts come through [`facts`].** That module is the
//!   single file importing from `baml_compiler2_hir_ty`, and its doc header is
//!   the contract the type provider must keep answering.

pub mod display;
pub mod export;
pub mod facts;
pub mod handles;
pub mod head;
pub mod ids;

#[cfg(test)]
mod export_tests;
#[cfg(test)]
mod handles_tests;
#[cfg(test)]
mod ids_tests;

// ── Db trait ──────────────────────────────────────────────────────────────────

/// Database trait for `baml_surface`.
///
/// Extends `baml_compiler2_ppir::Db` (and through it the HIR/workspace chain),
/// so a handle method can reach every query in [`facts`]' contract.
#[salsa::db]
pub trait Db: baml_compiler2_ppir::Db {}

// In this crate's own test build, the crate is compiled a second time (the
// dev-dependency on `baml_project` closes a cycle back to the published
// rlib), so `baml_project`'s production impl targets the *other* instance of
// this trait. This test-local impl covers the test-local instance; the two
// never coexist in one crate graph's eyes.
#[cfg(test)]
#[salsa::db]
impl Db for baml_project::ProjectDatabase {}

// ── Public API re-exports ─────────────────────────────────────────────────────

// Re-exported so `Function::origin`/`Global::origin` callers need no direct
// ast-crate dependency.
pub use baml_compiler2_ast::{FunctionOrigin, LetOrigin};
pub use display::TyDisplayFormat;
pub use export::{
    MemberExport, PackageExport, SymbolExport, export_member, export_package, export_symbol,
};
pub use handles::{
    AssocType, Class, Client, Enum, Field, FieldOwner, Function, FunctionOwner, Global, Impl,
    ImplMethod, Interface, Member, Namespace, Package, RequiredMethod, RetryPolicy, Symbol,
    SymbolKind, TemplateString, Test, Throws, TypeAlias, Variant,
};
pub use head::{TyHead, impl_attaches, ty_head};
pub use ids::{IdKind, InvalidSymbolId, Resolved, SymbolId, resolve};
