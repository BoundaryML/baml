//! Expression position: the names and keywords that can start a value.
//!
//! The names come from [`baml_compiler2_ppir::resolve::names_in_scope_at`],
//! the enumeration counterpart of the resolver — so what is offered is what
//! would resolve, shadowing included, and nothing here re-walks a scope
//! chain.

use baml_base::SourceFile;
use baml_compiler2_ppir::resolve::{ScopeNameKind, names_in_scope_at};
use text_size::TextSize;

use super::completions::Completions;
use crate::symbols;

/// Keywords that can begin an expression or a statement inside a body.
///
/// The list is the language's, not a guess: each of these opens a form the
/// grammar accepts here. Declaration keywords (`class`, `function`, …) are
/// an item-position matter and are not offered inside a body.
const EXPRESSION_KEYWORDS: &[&str] = &[
    "let", "const", "if", "else", "match", "for", "while", "return", "throw", "catch", "spawn",
    "await", "defer", "break", "continue", "true", "false", "null",
];

pub(crate) fn complete(
    db: &dyn baml_compiler2_ppir::Db,
    file: SourceFile,
    offset: TextSize,
    out: &mut Completions,
) {
    for entry in names_in_scope_at(db, file, offset) {
        // The resolver would resolve a `$`-companion if a reader could write
        // one; none can, so the enumeration of what to WRITE drops them —
        // the same rule search enumerates by.
        if let ScopeNameKind::Item(def) = &entry.kind
            && symbols::is_synthesized(db, &entry.name, *def)
        {
            continue;
        }
        out.add_scope_name(db, file, &entry);
    }

    for keyword in EXPRESSION_KEYWORDS {
        out.add_keyword(keyword);
    }
}
