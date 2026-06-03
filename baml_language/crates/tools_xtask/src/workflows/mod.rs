//! Workflow generators and the shared helper modules they build on.
//!
//! Each `*.rs` workflow module exposes `pub fn workflow() -> Workflow`. The
//! generation driver in [`crate::tasks::workflows`] maps each module to its
//! output filename and serializes it.

pub mod runners;
pub mod steps;
pub mod vars;

pub mod cargo_tests;
pub mod ci;
pub mod size_gate;
pub mod wasm_pack_tests;
pub mod webview_tests;
