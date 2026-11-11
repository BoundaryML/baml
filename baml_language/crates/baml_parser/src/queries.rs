//! The parse as Salsa queries over a [`SourceFile`].
//!
//! One tracked parse per file holds the tree and the errors as two tracked
//! fields, so each is cut off on its own: an edit that changes the tree but
//! leaves the error list equal re-runs nothing that read only the errors. The
//! tree is never backdated — its text is part of it — which is the cutoff a
//! whole-tree comparison gives. There is no separate lexing query: tokens are
//! part of the tree, and lexing is a small fraction of the parse.

use std::sync::Arc;

use baml_base::SourceFile;
use baml_syntax::{index::TreeIndex, nodes::FileTree};

use crate::{ParseError, parse};

/// A file's parse: its tree and its errors, each tracked on its own.
#[salsa::tracked]
pub struct TypedParse<'db> {
    #[tracked]
    #[returns(clone)]
    pub tree: Arc<FileTree>,

    #[tracked]
    #[returns(clone)]
    pub errors: Arc<[ParseError]>,
}

/// Parse `file`.
#[salsa::tracked(returns(clone))]
pub fn parse_typed(db: &dyn salsa::Database, file: SourceFile) -> TypedParse<'_> {
    let parsed = parse(file.text(db));
    TypedParse::new(db, Arc::new(parsed.tree), parsed.errors.into())
}

/// The tree of `file`.
pub fn typed_tree(db: &dyn salsa::Database, file: SourceFile) -> Arc<FileTree> {
    parse_typed(db, file).tree(db)
}

/// The syntax errors of `file`; empty for a file that parses cleanly.
pub fn typed_parse_errors(db: &dyn salsa::Database, file: SourceFile) -> Arc<[ParseError]> {
    parse_typed(db, file).errors(db)
}

/// Parent and owner links over the tree of `file`, for navigation. The
/// compiler never needs them, so they are a query of their own, built only
/// where something asks.
#[salsa::tracked(returns(clone))]
pub fn tree_index(db: &dyn salsa::Database, file: SourceFile) -> Arc<TreeIndex> {
    Arc::new(TreeIndex::build(&typed_tree(db, file)))
}
