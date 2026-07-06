//! Per-file signature hashing for the bytecode cache's per-file reuse.
//!
//! A file's *signature* is its source text with every function-body region
//! blanked out (function/method/LLM bodies — the `FUNCTION_BODY` family of
//! syntax nodes). Editing a body leaves the signature hash unchanged;
//! editing anything else — declarations, parameters, types, class fields,
//! attributes, let initializers, tests, even comments outside bodies —
//! changes it.
//!
//! This is TypeScript's version/signature split done at source level: the
//! content hash answers "did the file change at all", the signature hash
//! answers "can that change be observed by other files' compiled code".
//! Body-only edits can't be (bodies are invisible across files), so files
//! whose referenced symbols don't intersect any signature change can reuse
//! their previously compiled bytecode verbatim.
//!
//! Deliberately conservative: anything not provably a body counts as
//! signature. False "signature changed" verdicts only cost a wider
//! recompile, never correctness.

use baml_db::{SourceFile, baml_compiler_parser::syntax_tree, baml_compiler_syntax::SyntaxKind};
use baml_project::ProjectDatabase;
use sha2::{Digest, Sha256};

/// Hash of `file`'s signature-relevant text (body regions excluded).
pub(crate) fn file_signature_hash(db: &ProjectDatabase, file: SourceFile) -> [u8; 32] {
    let tree = syntax_tree(db, file);
    let text = file.text(db);

    let mut body_ranges: Vec<(usize, usize)> = tree
        .descendants()
        .filter(|node| {
            matches!(
                node.kind(),
                SyntaxKind::FUNCTION_BODY
                    | SyntaxKind::LLM_FUNCTION_BODY
                    | SyntaxKind::EXPR_FUNCTION_BODY
            )
        })
        .map(|node| {
            let range = node.text_range();
            (range.start().into(), range.end().into())
        })
        .collect();
    body_ranges.sort_unstable();

    // Merge nested/overlapping ranges (lambda bodies inside bodies).
    let mut merged: Vec<(usize, usize)> = Vec::with_capacity(body_ranges.len());
    for (start, end) in body_ranges {
        match merged.last_mut() {
            Some((_, last_end)) if start <= *last_end => *last_end = (*last_end).max(end),
            _ => merged.push((start, end)),
        }
    }

    // Hash the gaps between bodies, length-framed so segment boundaries are
    // unambiguous. Body content AND length are both excluded — a body may
    // grow or shrink freely without touching the signature.
    let mut hasher = Sha256::new();
    let bytes = text.as_bytes();
    let mut cursor = 0usize;
    for (start, end) in merged {
        let gap = &bytes[cursor..start];
        hasher.update((gap.len() as u64).to_le_bytes());
        hasher.update(gap);
        cursor = end;
    }
    let tail = &bytes[cursor..];
    hasher.update((tail.len() as u64).to_le_bytes());
    hasher.update(tail);
    hasher.finalize().into()
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    fn hash_of(source: &str) -> [u8; 32] {
        let mut db = ProjectDatabase::new();
        db.set_project_root(Path::new("/test-project"));
        db.add_or_update_file(Path::new("/test-project/main.baml"), source);
        let file = db.get_source_files()[0];
        file_signature_hash(&db, file)
    }

    #[test]
    fn body_edits_do_not_change_the_signature() {
        let base = hash_of("function f(x: int) -> int {\n  x + 1\n}\n");
        let body_edit = hash_of("function f(x: int) -> int {\n  let y = x;\n  y + 2\n}\n");
        assert_eq!(base, body_edit, "body-only edit must keep the signature");
    }

    #[test]
    fn signature_edits_change_the_signature() {
        let base = hash_of("function f(x: int) -> int {\n  x + 1\n}\n");
        for (label, source) in [
            ("param type", "function f(x: string) -> int {\n  x + 1\n}\n"),
            ("name", "function g(x: int) -> int {\n  x + 1\n}\n"),
            (
                "return type",
                "function f(x: int) -> string {\n  x + 1\n}\n",
            ),
            (
                "added item",
                "function f(x: int) -> int {\n  x + 1\n}\nclass C {\n  a int\n}\n",
            ),
        ] {
            assert_ne!(
                base,
                hash_of(source),
                "{label} edit must change the signature"
            );
        }
    }

    #[test]
    fn class_field_edits_change_the_signature() {
        let base = hash_of("class C {\n  a int\n  b string\n}\n");
        let reordered = hash_of("class C {\n  b string\n  a int\n}\n");
        assert_ne!(base, reordered, "field layout is signature");
    }
}
