//! The Prisma Schema AST.

#![deny(rust_2018_idioms, unsafe_code)]
#![allow(clippy::derive_partial_eq_without_eq)]
mod ast;
mod parser;

pub use self::parser::parse_prompt;
