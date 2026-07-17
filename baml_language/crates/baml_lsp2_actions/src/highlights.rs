//! `document_highlights_at` — same-file occurrences of the symbol at the
//! cursor, for `textDocument/documentHighlight`.
//!
//! Reuses the `usages` machinery restricted to the request file: the LSP
//! scopes highlights to one document, and editors re-request them on every
//! cursor move, so the package-wide scan `usages_at` does would be wasted
//! work. Unlike `usages_at` — which excludes the definition site by contract —
//! the declaration occurrence is part of the highlight group, so it is added
//! back when it lives in the request file.
//!
//! All occurrences are returned without a read/write distinction: BAML has no
//! reassignment, so the LSP `DocumentHighlightKind` split carries no signal.

use baml_base::{Name, SourceFile};
use baml_compiler_syntax::SyntaxKind;
use baml_compiler2_hir::semantic_index::BindingKind;
use text_size::{TextRange, TextSize};

use crate::{Db, definition, usages, utils};

/// Ranges in `file` to highlight for the symbol at `offset`, sorted in
/// document order.
///
/// Regular function (not cached); the expensive work is internally
/// Salsa-cached (`file_semantic_index`, `syntax_tree`, `function_body`, …).
/// Returns an empty `Vec` when the cursor is not on an identifier.
pub fn document_highlights_at(db: &dyn Db, file: SourceFile, offset: TextSize) -> Vec<TextRange> {
    let Some(token) = utils::find_token_at_offset(db, file, offset) else {
        return Vec::new();
    };
    if token.kind() != SyntaxKind::WORD {
        return Vec::new();
    }

    let mut ranges: Vec<TextRange> = usages::same_file_usages_at(db, file, offset)
        .into_iter()
        .map(|loc| loc.range)
        .collect();

    if let Some(decl) = declaration_name_range(db, file, offset, token.text()) {
        ranges.push(decl);
    }

    ranges.sort_by_key(|range| range.start());
    ranges.dedup();
    ranges
}

/// The name-token range of the declaration of the symbol at `offset`, when it
/// lives in `file`.
///
/// Locals resolve through the semantic index's recorded `name_range` —
/// `definition_at` returns the whole `let` statement for a local, which is
/// too wide to highlight. Both that range (which covers `let name`) and a
/// parameter's signature span (which covers `name: type`) are narrowed to
/// their name token. Items and fields fall through to `definition_at`, whose
/// ranges are already name tokens.
fn declaration_name_range(
    db: &dyn Db,
    file: SourceFile,
    offset: TextSize,
    name_text: &str,
) -> Option<TextRange> {
    let name = Name::new(name_text);

    if let Some(binding) = usages::local_binding_id_at(db, file, offset, &name) {
        let declaration_span = match binding.kind {
            BindingKind::Local(idx) => {
                let index = baml_compiler2_hir::file_semantic_index(db, file);
                let bindings = &index.scope_bindings[binding.scope.index() as usize];
                bindings.bindings[idx as usize].name_range
            }
            BindingKind::Parameter(param_idx) => {
                let func_loc = utils::enclosing_function_loc(db, file, offset)?;
                let sig_map =
                    baml_compiler2_hir::signature::function_signature_source_map(db, func_loc);
                sig_map.param_spans.get(param_idx).copied()?
            }
        };
        return name_token_in_range(db, file, declaration_span, name_text);
    }

    let def = definition::definition_at(db, file, offset)?;
    (def.file == file).then_some(def.range)
}

/// The range of the first `WORD` token inside `range` whose text is
/// `name_text`.
fn name_token_in_range(
    db: &dyn Db,
    file: SourceFile,
    range: TextRange,
    name_text: &str,
) -> Option<TextRange> {
    let tree = baml_compiler_parser::syntax_tree(db, file);
    let node = match tree.covering_element(range) {
        rowan::NodeOrToken::Token(token) => {
            return (token.kind() == SyntaxKind::WORD && token.text() == name_text)
                .then(|| token.text_range());
        }
        rowan::NodeOrToken::Node(node) => node,
    };
    node.descendants_with_tokens().find_map(|element| {
        let rowan::NodeOrToken::Token(token) = element else {
            return None;
        };
        (range.contains_range(token.text_range())
            && token.kind() == SyntaxKind::WORD
            && token.text() == name_text)
            .then(|| token.text_range())
    })
}
