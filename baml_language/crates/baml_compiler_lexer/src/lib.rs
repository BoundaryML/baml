//! Lexical analysis for BAML.
//!
//! Provides lossless tokenization using Logos, preserving all source text including
//! whitespace and comments for perfect reconstruction.

use baml_base::SourceFile;

mod tokens;
pub use tokens::{Token, TokenKind, lex_lossless, reconstruct_source};

/// Tracked: tokenize a source file
/// This function performs lexical analysis on a BAML source file,
/// converting the raw text into a sequence of tokens.
#[salsa::tracked]
pub fn lex_file(db: &dyn salsa::Database, file: SourceFile) -> Vec<Token> {
    let text = file.text(db);
    lex_lossless(text, file.file_id(db))
}
