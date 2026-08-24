//! Spec-conformance type tests for the hir_ty plan (slice S0).
//!
//! `TYPE_SYSTEM.md` is prescriptive; these tests encode its mandated behavior
//! as rust-analyzer-style `//^^^ ty` annotation fixtures (see [`harness`]),
//! run against the `baml_compiler2_hir_ty` engine. The corpus starts red:
//! every fixture sits in `fixtures/pending/` (asserted to fail) until the
//! engine slice that infers it lands and promotes it to `fixtures/`. TIR is
//! not a backend here; the plan's S15 differential sweep compares against it
//! separately.
//!
//! Module map:
//! - [`harness`]: annotation extraction, the `check_types` check, and the
//!   `check_infer`-style [`harness::render_infer`] dump
//! - [`fixtures`]: directory-driven runner; a test is a `.baml` file under
//!   `fixtures/` (must pass) or `fixtures/pending/` (must fail until its
//!   slice lands), each with an insta snapshot of the infer dump

pub(crate) mod harness;

mod coherence;
mod fixtures;
mod pattern_corpus;
pub(crate) mod sweep;
mod tables;
