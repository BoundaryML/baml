//! Beautiful diagnostic rendering using Ariadne.
//!
//! This crate converts compiler errors into beautiful error messages.
//! It doesn't define error types - those live in each compiler phase.

pub mod compiler_error;

pub use compiler_error::{
    ColorMode, CompilerError, ParseError, TypeError, render_error, render_parse_error,
    render_report_to_string, render_type_error,
};
