//! Position ↔ CST primitives shared by every cursor-based feature.
//!
//! These are the only helpers that touch raw syntax trees for *addressing*
//! (finding what the cursor is on, mapping a definition back to its name
//! token). Anything that derives *meaning* from syntax belongs in a compiler
//! crate; features here only navigate.

use baml_base::SourceFile;
use baml_compiler_syntax::{SyntaxKind, SyntaxToken, TokenAtOffset};
use baml_compiler2_hir::contributions::Definition;
use text_size::{TextRange, TextSize};

/// Find the leaf token in the file's CST that best covers `offset`.
///
/// Uses `rowan::SyntaxNode::token_at_offset`, which returns [`TokenAtOffset`]:
/// - `Single(tok)` — cursor sits inside one token.
/// - `Between(left, right)` — cursor is exactly at a boundary; we prefer the
///   right-hand token (the one the cursor is entering), falling back to left
///   when the right side is whitespace.
/// - `None` — file is empty.
///
/// For go-to-definition the caller filters on identifier tokens
/// (`token.kind() == SyntaxKind::WORD`).
pub fn find_token_at_offset(
    db: &dyn salsa::Database,
    file: SourceFile,
    offset: TextSize,
) -> Option<SyntaxToken> {
    let tree = baml_compiler_parser::syntax_tree(db, file);
    match tree.token_at_offset(offset) {
        TokenAtOffset::Single(tok) => Some(tok),
        TokenAtOffset::Between(left, right) => {
            if right.kind() != SyntaxKind::WHITESPACE && right.kind() != SyntaxKind::NEWLINE {
                Some(right)
            } else {
                Some(left)
            }
        }
        TokenAtOffset::None => None,
    }
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
