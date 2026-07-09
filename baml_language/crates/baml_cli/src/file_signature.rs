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

/// Hash of `file`'s type-*layout* surface: exactly its type/impl declarations
/// (class, enum, interface, type alias, and out-of-body `implements … for …`),
/// with every function/method body blanked out.
///
/// This is [`file_signature_hash`] narrowed to the declarations whose *shape* a
/// type's layout is baked from — class field order/types, enum variant
/// order/discriminants, interface method-signature order (vtable slots), type
/// alias targets, generic-parameter lists, and the `cleanup`-method signature —
/// and nothing else. A free function's signature, a client, a test, a template
/// string, and every function body are all excluded. Blanking bodies matches
/// the signature hash's body-invariance: a field offset, a discriminant, a type
/// tag, and a vtable slot are all fixed by *declarations*, never by a body
/// (`has_cleanup` keys off the method's signature, not its body), so a
/// method-body edit inside an `implements`/interface block must not move this.
///
/// A file that declares none of these hashes to a fixed constant (an empty
/// span set), so a typeless file never registers a layout change.
///
/// The dirty-set pass fires the layout sentinel only when a modified file's
/// value here changes — so a function-only signature edit in a class-defining
/// file no longer forces a whole-project recompile, while any real layout move
/// (field/variant/alias reorder, a new generic parameter) still does. When in
/// doubt a construct is *included* (over-firing costs reuse, never
/// correctness); it must never legitimately depend on a free function's
/// signature — a type's layout is derived from its own declaration alone.
pub(crate) fn file_layout_hash(db: &ProjectDatabase, file: SourceFile) -> [u8; 32] {
    let tree = syntax_tree(db, file);
    let text = file.text(db);
    let bytes = text.as_bytes();

    // Top-level type/impl declarations. These never nest inside one another, so
    // the collected ranges are disjoint; sorting by start orders them.
    let mut decl_ranges: Vec<(usize, usize)> = tree
        .descendants()
        .filter(|node| {
            matches!(
                node.kind(),
                SyntaxKind::CLASS_DEF
                    | SyntaxKind::ENUM_DEF
                    | SyntaxKind::INTERFACE_DEF
                    | SyntaxKind::TYPE_ALIAS_DEF
                    | SyntaxKind::IMPLEMENTS_FOR
            )
        })
        .map(|node| {
            let range = node.text_range();
            (range.start().into(), range.end().into())
        })
        .collect();
    decl_ranges.sort_unstable();

    // Function/method body ranges (same family as the signature hash), blanked
    // wherever they fall inside a declaration span (an `implements`-block or
    // interface default-method body): a body never moves a layout.
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

    // Hash each declaration span's text with any contained body sub-spans cut
    // out, length-framed so segment and declaration boundaries are unambiguous.
    // No declarations → an empty hasher → a fixed constant for every typeless
    // file.
    let mut hasher = Sha256::new();
    for (ds, de) in decl_ranges {
        hasher.update(b"D");
        hasher.update(((de - ds) as u64).to_le_bytes());
        let mut cursor = ds;
        for &(bs, be) in &body_ranges {
            // Only bodies fully inside this declaration; they can't straddle a
            // top-level declaration boundary.
            if bs < ds || be > de {
                continue;
            }
            let gap = &bytes[cursor..bs];
            hasher.update((gap.len() as u64).to_le_bytes());
            hasher.update(gap);
            cursor = be;
        }
        let tail = &bytes[cursor..de];
        hasher.update((tail.len() as u64).to_le_bytes());
        hasher.update(tail);
    }
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

    fn layout_of(source: &str) -> [u8; 32] {
        let mut db = ProjectDatabase::new();
        db.set_project_root(Path::new("/test-project"));
        db.add_or_update_file(Path::new("/test-project/main.baml"), source);
        let file = db.get_source_files()[0];
        file_layout_hash(&db, file)
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

    // ── layout hash ──────────────────────────────────────────────────────────

    #[test]
    fn function_signature_edits_do_not_change_the_layout() {
        // The whole point: a function's parameter list, return type, and throws
        // are NOT layout, even in a file that also defines a class.
        let base = layout_of(
            "class Point {\n  x int\n  y int\n}\n\
             function f(a: int) -> int {\n  a\n}\n",
        );
        for (label, source) in [
            (
                "param added",
                "class Point {\n  x int\n  y int\n}\n\
                 function f(a: int, b: int) -> int {\n  a\n}\n",
            ),
            (
                "return type",
                "class Point {\n  x int\n  y int\n}\n\
                 function f(a: int) -> string {\n  \"x\"\n}\n",
            ),
            (
                "function body",
                "class Point {\n  x int\n  y int\n}\n\
                 function f(a: int) -> int {\n  a + 1\n}\n",
            ),
            (
                "function renamed",
                "class Point {\n  x int\n  y int\n}\n\
                 function g(a: int) -> int {\n  a\n}\n",
            ),
        ] {
            assert_eq!(
                base,
                layout_of(source),
                "{label} must not change the layout hash"
            );
        }
    }

    #[test]
    fn typeless_files_share_one_constant_layout_hash() {
        // A file with no type/impl declarations hashes to a fixed constant, so a
        // function-only edit (or any edit) never registers a layout change.
        let a = layout_of("function f(a: int) -> int {\n  a\n}\n");
        let b = layout_of("function g() -> string {\n  \"y\"\n}\nlet k = 3;\n");
        assert_eq!(a, b, "every typeless file shares the empty-layout constant");
        let a_edited = layout_of("function f(a: int, b: int) -> string {\n  \"z\"\n}\n");
        assert_eq!(a, a_edited, "a typeless file's layout never moves");
    }

    #[test]
    fn layout_moves_change_the_layout_hash() {
        let class_base = layout_of("class C {\n  a int\n  b string\n}\n");
        assert_ne!(
            class_base,
            layout_of("class C {\n  b string\n  a int\n}\n"),
            "field reorder is a layout move"
        );
        assert_ne!(
            class_base,
            layout_of("class C {\n  a int\n  b int\n}\n"),
            "field type change is a layout move"
        );

        let enum_base = layout_of("enum E {\n  A\n  B\n}\n");
        assert_ne!(
            enum_base,
            layout_of("enum E {\n  B\n  A\n}\n"),
            "variant reorder is a layout move"
        );

        let alias_base = layout_of("class D {\n  v int\n}\ntype T = int\n");
        assert_ne!(
            alias_base,
            layout_of("class D {\n  v int\n}\ntype T = D\n"),
            "alias target change is a layout move"
        );
    }

    #[test]
    fn class_type_parameter_edits_change_the_layout_hash() {
        // Verification bar #4: a class's generic-parameter list is part of its
        // layout surface (it feeds `generic_param_count`).
        let base = layout_of("class Box<T> {\n  value T\n}\n");
        assert_ne!(
            base,
            layout_of("class Box<T, U> {\n  value T\n}\n"),
            "adding a type parameter must change the layout hash"
        );
    }

    #[test]
    fn implements_block_body_edit_does_not_change_the_layout() {
        // A method body inside a class `implements` block is blanked: the vtable
        // slot order (layout) is fixed by the method signature, not its body.
        let base = layout_of(
            "interface Speaker {\n  function speak(self) -> string\n}\n\
             class Dog {\n  name string\n  implements Speaker {\n    \
             function speak(self) -> string {\n      \"woof\"\n    }\n  }\n}\n",
        );
        let body_edited = layout_of(
            "interface Speaker {\n  function speak(self) -> string\n}\n\
             class Dog {\n  name string\n  implements Speaker {\n    \
             function speak(self) -> string {\n      \"WOOF\"\n    }\n  }\n}\n",
        );
        assert_eq!(
            base, body_edited,
            "an implements-block method-body edit must not move the layout"
        );
    }
}
