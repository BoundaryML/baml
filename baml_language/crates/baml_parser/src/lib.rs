//! The BAML parser: a file's text to its typed [`baml_syntax`] tree.
//!
//! [`parse`] lexes, parses and returns the tree with every syntax error found.
//! The tree is complete for any input — a required position the text does not
//! fill is a `Missing`, tokens that fit no node are a `Bogus` — and both only
//! ever appear alongside an error in [`Parse::errors`].
//!
//! A compile reads a file through the Salsa queries in [`queries`]:
//! [`typed_tree`] and [`typed_parse_errors`] are cut off separately, so a
//! change that leaves the errors alone re-runs nothing that read only them;
//! [`tree_index`] adds the parent and owner links the IDE navigates with.
//!
//! Organization: `cursor` is the read head over the tokens, `sink` the arenas
//! and diagnostics being produced, `parser` the combinators that need both.
//! The grammar itself is free functions and `Parse` impls over `p` in
//! `items`, `types`, `expr`, `stmts`, `patterns` and `templates`, with the
//! pure lookaheads in `lookahead`; it cannot reach the cursor's position or
//! the sink's output except through their APIs.

use baml_syntax::{
    nodes::{Arenas, FileTree, NodeKind},
    token::TokenIdx,
    tree::{AnyNodeId, Missing, TokenRange, Visitor},
};

pub mod error;
pub use error::ParseError;
pub mod queries;
pub use queries::{TypedParse, parse_typed, tree_index, typed_parse_errors, typed_tree};

mod cursor;
mod expr;
mod function_union;
mod items;
mod lookahead;
mod parser;
mod patterns;
mod sink;
mod stmts;
mod templates;
mod types;

#[cfg(test)]
mod tests;

/// A parsed file.
#[derive(Debug, Clone, PartialEq, Eq)]
#[must_use]
pub struct Parse {
    pub tree: FileTree,
    pub errors: Vec<ParseError>,
}

/// Parse `text` completely.
pub fn parse(text: &str) -> Parse {
    let input = cursor::Input::new(text);
    let mut arenas = Arenas::default();
    let mut errors = Vec::new();
    let root = {
        let out = sink::Sink::new(&mut arenas, &mut errors);
        let mut p = parser::Parser::new(cursor::Cursor::new(&input), out);
        items::source_file(&mut p)
    };
    let tree = FileTree::new(text.into(), input.into_tokens(), arenas, root);
    debug_assert!(
        !errors.is_empty() || is_complete(&tree),
        "a parse with no errors holds a `Missing` or a `Bogus`"
    );
    Parse { tree, errors }
}

/// Whether the tree holds no `Missing` and no `Bogus`: what an error-free
/// parse promises.
fn is_complete(tree: &FileTree) -> bool {
    struct Completeness {
        complete: bool,
        pending: Vec<AnyNodeId>,
    }
    impl Visitor for Completeness {
        fn node(&mut self, _field: &'static str, id: AnyNodeId) {
            if Arenas::kind(id) == NodeKind::Bogus {
                self.complete = false;
            } else {
                self.pending.push(id);
            }
        }
        fn token(&mut self, _field: &'static str, _token: TokenIdx) {}
        fn range(&mut self, _field: &'static str, _range: TokenRange) {}
        fn missing(&mut self, _field: &'static str, _missing: Missing) {
            self.complete = false;
        }
    }
    let mut walk = Completeness {
        complete: true,
        pending: vec![tree.root_id()],
    };
    while let Some(id) = walk.pending.pop() {
        tree.arenas().walk_node(id, &mut walk);
        if !walk.complete {
            return false;
        }
    }
    true
}
