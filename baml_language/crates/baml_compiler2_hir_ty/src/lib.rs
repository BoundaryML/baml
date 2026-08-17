//! Type inference for the BAML compiler, modeled on rust-analyzer's `hir-ty`.
//!
//! This crate will own body type inference: a real inference-variable table
//! with unification and snapshot/rollback, bidirectional checking driven by
//! expectations, canonicalizing union joins at control-flow merge points,
//! probe/confirm member resolution, an interface obligation queue, and
//! throws-effect inference. `baml_language/TYPE_SYSTEM.md` is the correctness
//! authority; the implementation plan and slice ordering live in this crate's
//! `README.md`.
//!
//! Layering (mirrors rust-analyzer's `hir-def -> hir-ty -> hir`):
//!
//! - This crate may depend on `baml_compiler2_ast`, `baml_compiler2_hir`,
//!   `baml_compiler2_ppir` and `baml_type`.
//! - It must NEVER depend on `baml_compiler2_mir` or
//!   `baml_compiler2_emit`. Until the cutover slice, nothing outside the test
//!   harness in `baml_tests` consumes it, and every existing compiler snapshot
//!   stays byte-identical.
//!
//! The spec-conformance test harness (rust-analyzer-style `//^ ty` annotation
//! checks plus `check_infer`-style dump snapshots) lives in
//! `baml_tests::type_spec` and runs against THIS crate's engine. The corpus
//! starts red: fixtures live in `fixtures/pending/`, asserted to fail, and
//! each slice promotes the fixtures it turns green. TIR is not a harness
//! backend; it only appears in the S15 differential sweep.

pub mod callable;
pub mod coherence;
pub mod defaults;
pub mod diagnostics;
pub mod exhaustiveness;
pub mod facts;
pub mod ide;
pub mod impls;
pub mod infer;
pub mod init_io;
pub mod interfaces;
pub mod lower;
pub mod method_resolution;
pub mod ops;
pub mod package_interface;
pub mod throw_facts;
