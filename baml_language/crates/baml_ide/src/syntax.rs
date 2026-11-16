//! Position ↔ tree primitives shared by every cursor-based feature.
//!
//! These are the only helpers that touch raw syntax trees for *addressing*
//! (finding what the cursor is on, mapping a definition back to its name
//! token). Anything that derives *meaning* from syntax belongs in a compiler
//! crate; features here only navigate.

use std::sync::Arc;

use baml_base::SourceFile;
use baml_compiler2_hir::contributions::Definition;
use baml_syntax::{
    index::TreeIndex,
    nodes::FileTree,
    token::{Ident, TokenAtOffset, TokenIdx, TokenKind, TokenSet as _},
    tree::AnyNodeId,
};
use text_size::{TextRange, TextSize};

/// A token of a file's typed tree, with the tree it belongs to: what the
/// cursor is on. Navigation is by index into the file's token array.
#[derive(Clone)]
pub struct TokenRef {
    tree: Arc<FileTree>,
    idx: TokenIdx,
}

impl TokenRef {
    pub fn tree(&self) -> &FileTree {
        &self.tree
    }

    pub fn idx(&self) -> TokenIdx {
        self.idx
    }

    pub fn kind(&self) -> TokenKind {
        self.tree.tokens().kind(self.idx)
    }

    pub fn text(&self) -> &str {
        self.tree.token_text(self.idx)
    }

    pub fn range(&self) -> TextRange {
        self.tree.tokens().range(self.idx)
    }

    /// Whether the token is an identifier: a word or a contextual keyword.
    pub fn is_ident(&self) -> bool {
        Ident::KINDS.contains(self.kind())
    }

    /// The token right before this one, trivia included.
    pub fn prev(&self) -> Option<Self> {
        let idx = TokenIdx(self.idx.0.checked_sub(1)?);
        Some(Self {
            tree: Arc::clone(&self.tree),
            idx,
        })
    }

    /// The previous non-trivia token.
    pub fn prev_significant(&self) -> Option<Self> {
        let idx = self.tree.tokens().prev_significant(self.idx)?;
        Some(Self {
            tree: Arc::clone(&self.tree),
            idx,
        })
    }

    /// The node that owns this token, through the file's index.
    pub fn owner(&self, index: &TreeIndex) -> Option<AnyNodeId> {
        debug_assert!(index.is_for(&self.tree));
        index.token_owner(self.idx)
    }
}

/// Find the token in the file's tree that best covers `offset`.
///
/// - strictly inside one token: that token;
/// - exactly on a boundary: the right-hand token (the one the cursor is
///   entering), unless it is trivia, then the left;
/// - past the end, or an empty file: `None`.
///
/// For go-to-definition the caller filters on identifier tokens
/// ([`TokenRef::is_ident`]).
pub fn find_token_at_offset(
    db: &dyn salsa::Database,
    file: SourceFile,
    offset: TextSize,
) -> Option<TokenRef> {
    let tree = baml_parser::typed_tree(db, file);
    let idx = match tree.tokens().token_at_offset(offset) {
        TokenAtOffset::Single(idx) => idx,
        TokenAtOffset::Between { left, right } => {
            if tree.tokens().kind(right).is_trivia() {
                left
            } else {
                right
            }
        }
        TokenAtOffset::None => return None,
    };
    Some(TokenRef { tree, idx })
}

/// Map a top-level [`Definition`] to its source file and name span.
///
/// Looks up the `Contribution` for the definition in the target file's
/// `file_symbol_contributions`; the contribution carries the byte range of
/// the name token — exactly what go-to-definition needs.
///
/// Returns `None` if the definition is not found in the target file's
/// contributions (which should not happen for well-formed code).
pub(crate) fn definition_span<'db>(
    db: &'db dyn baml_compiler2_hir::Db,
    def: Definition<'db>,
) -> Option<(SourceFile, TextRange)> {
    let def_file = def.file(db);
    let contributions = baml_compiler2_hir::file_symbol_contributions(db, def_file);

    let name_span = contributions
        .types
        .iter()
        .find_map(|(_, contrib)| (contrib.definition == def).then_some(contrib.name_span))
        .or_else(|| {
            contributions
                .values
                .iter()
                .find_map(|(_, contrib)| (contrib.definition == def).then_some(contrib.name_span))
        })?;

    Some((def_file, name_span))
}

/// The dotted name chain ending at `token`: for the `fetch` in
/// `baml.http.fetch`, `["baml", "http", "fetch"]`. Walks name-dot pairs
/// leftward over the tokens (skipping trivia), so it works in every position
/// — expressions, type annotations, patterns. A bare token yields a
/// one-element chain.
pub(crate) fn dotted_chain_to(token: &TokenRef) -> Vec<baml_base::Name> {
    let mut chain = vec![baml_base::Name::new(token.text())];
    let mut cursor = token.clone();
    while let Some(dot) = cursor.prev_significant()
        && dot.kind() == TokenKind::Dot
        && let Some(word) = dot.prev_significant()
        && word.is_ident()
    {
        chain.push(baml_base::Name::new(word.text()));
        cursor = word;
    }
    chain.reverse();
    chain
}

/// The binding pattern introduced by a `let` or `for` statement, if any.
pub(crate) fn extract_pat_from_stmt(
    expr_body: &baml_compiler2_ast::ExprBody,
    stmt_id: baml_compiler2_ast::StmtId,
) -> Option<baml_compiler2_ast::PatId> {
    let stmt = expr_body
        .stmts
        .iter()
        .find_map(|(id, stmt)| (id == stmt_id).then_some(stmt))?;

    match stmt {
        baml_compiler2_ast::Stmt::Let { pattern, .. }
        | baml_compiler2_ast::Stmt::For {
            binding: pattern, ..
        } => Some(*pattern),
        _ => None,
    }
}
