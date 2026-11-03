//! The BAML parser: a file's text to its typed [`baml_syntax`] tree.
//!
//! [`parse`] lexes, parses and returns the tree with every syntax error found.
//! The tree is complete for any input — a required position the text does not
//! fill is a `Missing`, tokens that fit no node are a `Bogus` — and both only
//! ever appear alongside an error in [`Parse::errors`].
//!
//! Organization: `cursor` is the read head over the tokens, `sink` the arenas
//! and diagnostics being produced, `parser` the combinators that need both.
//! The grammar itself is free functions and `Parse` impls over `p` in `items`
//! and `types`; it cannot reach the cursor's position or the sink's output
//! except through their APIs.

use baml_syntax::{
    nodes::{Arenas, FileTree},
    tree::TokenRange,
};

pub mod error;
pub use error::ParseError;

mod cursor;
mod function_union;
mod items;
mod parser;
mod sink;
#[deprecated = "placeholders: the expression, statement and string layers are not parsed yet; each stub is deleted when its layer lands"]
mod stubs;
mod types;

#[cfg(test)]
mod tests;

/// A parsed file.
#[derive(Debug, Clone, PartialEq, Eq)]
#[must_use]
pub struct Parse {
    pub tree: FileTree,
    pub errors: Vec<ParseError>,
    /// The token ranges of constructs the parser does not read yet (their
    /// grammar layer has not landed): each is a `Bogus` in the tree that is
    /// not an error. Empty once every layer is ported; until then it tells a
    /// consumer exactly where the tree is incomplete.
    #[deprecated = "empty once every grammar layer is parsed, then removed"]
    pub placeholders: Vec<TokenRange>,
}

/// Parse `text` completely.
#[expect(
    deprecated,
    reason = "the placeholders are filled until the last grammar layer lands"
)]
pub fn parse(text: &str) -> Parse {
    let input = cursor::Input::new(text);
    let mut arenas = Arenas::default();
    let mut errors = Vec::new();
    let mut placeholders = Vec::new();
    let root = {
        let out = sink::Sink::new(&mut arenas, &mut errors, &mut placeholders);
        let mut p = parser::Parser::new(cursor::Cursor::new(&input), out);
        items::source_file(&mut p)
    };
    let tree = FileTree::new(text.into(), input.into_tokens(), arenas, root);
    Parse {
        tree,
        errors,
        placeholders,
    }
}
