//! Jinja template rendering to `PromptAst`.
//!
//! The legacy raw-signature `execute_render_prompt` has been removed.
//! All rendering now goes through `execute_render_prompt_from_owned` in lib.rs
//! via the trait-based dispatch.
