//! Built-in external operations.
//!
//! This module contains implementations of external operations defined in
//! `baml_builtins` with the `#[external]` attribute.
//!
//! Each operation is an async function that takes `Arc<OpContext>` and
//! `ResolvedArgs`, returning `Result<ResolvedValue, OpError>`.

pub mod fs;
pub mod net;
pub mod sys;
