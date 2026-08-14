//! Throws-contract documentation coverage.
//!
//! Pins how the generator renders the inferred throws contract
//! (`callable_throws`) into generated documentation, using **unqualified**
//! type names. python renders a Google-style `Raises:` block, split between
//! the runtime `__doc__` (free functions) and the `.pyi` stub (methods);
//! ADAPTATION(rust): the Rust rendering is the idiomatic `# Errors` rustdoc
//! section (the one `clippy::missing_errors_doc` asks for) on the `///`
//! docs — the single surface for functions and methods alike — so every
//! case asserts on the generated source text (the test cwd is the
//! generated crate root).

use baml_sdk::raises_test::{DocLoader, InferredThrow, LoadDoc, PureLen, Reparse};

/// The generated `raises_test` module source. PROVISIONAL: the module's
/// on-disk layout is not pinned (the generator may emit
/// `src/raises_test/mod.rs`, `src/raises_test.rs`, or an inline module in
/// `src/lib.rs`), so probe each candidate.
fn _raises_test_source() -> String {
    for path in ["src/raises_test/mod.rs", "src/raises_test.rs", "src/lib.rs"] {
        if let Ok(src) = std::fs::read_to_string(path)
            && src.contains("fn LoadDoc(")
        {
            return src;
        }
    }
    panic!("generated raises_test source not found");
}

/// The doc-comment text attached to the item introduced at `marker` — the
/// Rust analogue of python's `inspect.getdoc`: the contiguous `///` block
/// directly above the line containing `marker` (attribute lines in between
/// are skipped), with the comment syntax stripped.
fn _getdoc(src: &str, marker: &str) -> Option<String> {
    let lines: Vec<&str> = src.lines().collect();
    let at = lines.iter().position(|line| line.contains(marker))?;
    let mut doc: Vec<&str> = Vec::new();
    for line in lines[..at].iter().rev() {
        let trimmed = line.trim_start();
        if let Some(text) = trimmed.strip_prefix("///") {
            doc.push(text.strip_prefix(' ').unwrap_or(text));
        } else if trimmed.starts_with("#[") {
            // Attributes may sit between the doc block and the item line.
            continue;
        } else {
            break;
        }
    }
    if doc.is_empty() {
        return None;
    }
    doc.reverse();
    Some(doc.join("\n"))
}

#[test]
fn test_raises_imports() {
    // python asserts the generated symbols import; in Rust the `use` above
    // resolving at compile time is the assertion. Touch each symbol so the
    // imports are exercised.
    let _: Option<DocLoader> = None;
    let _ = (InferredThrow, LoadDoc, PureLen, Reparse);
}

#[test]
fn test_raises_union_throws_lists_all_names() {
    let src = _raises_test_source();
    let doc = _getdoc(&src, "fn LoadDoc(").expect("no doc comment on LoadDoc");
    assert!(
        doc.trim_end()
            .ends_with("# Errors\n\nThrows `ParseError`, `TimeoutError`."),
        "{doc:?}"
    );
}

#[test]
fn test_raises_async_sibling_also_has_raises() {
    let src = _raises_test_source();
    let doc = _getdoc(&src, "fn LoadDoc_async(").expect("no doc comment on LoadDoc_async");
    assert!(
        doc.trim_end()
            .ends_with("# Errors\n\nThrows `ParseError`, `TimeoutError`."),
        "{doc:?}"
    );
}

#[test]
fn test_raises_single_throws() {
    let src = _raises_test_source();
    let doc = _getdoc(&src, "fn Reparse(").expect("no doc comment on Reparse");
    assert!(
        doc.trim_end().ends_with("# Errors\n\nThrows `ParseError`."),
        "{doc:?}"
    );
}

#[test]
fn test_raises_summary_precedes_raises_block() {
    let src = _raises_test_source();
    let doc = _getdoc(&src, "fn LoadDoc(").expect("no doc comment on LoadDoc");
    assert!(doc.starts_with("Load a document from a path."), "{doc:?}");
    assert!(doc.contains("\n\n# Errors\n"), "{doc:?}");
}

#[test]
fn test_raises_inferred_contract_without_clause_still_raises() {
    // No written `throws` clause, but the body throws ParseError — the
    // inferred contract (callable_throws) still surfaces an Errors section.
    let src = _raises_test_source();
    let doc = _getdoc(&src, "fn InferredThrow(").expect("no doc comment on InferredThrow");
    assert!(
        doc.trim_end().ends_with("# Errors\n\nThrows `ParseError`."),
        "{doc:?}"
    );
}

#[test]
fn test_raises_non_throwing_function_has_no_raises_block() {
    let src = _raises_test_source();
    let doc = _getdoc(&src, "fn PureLen(").unwrap_or_default();
    assert!(!doc.contains("# Errors"), "{doc:?}");
}

#[test]
fn test_raises_method_raises_block_in_pyi() {
    // python carries method `Raises:` blocks in the .pyi (the pyright/IDE
    // surface) only, with the runtime `.py` __doc__ trailer reserved for
    // free functions.
    // DIVERGENCE(rust): there is no stub/runtime split — the `///` docs on
    // the generated methods are the single surface for both, so the method
    // cases assert on the generated source like everything else here.
    let src = _raises_test_source();

    let load_block = _def_block(&src, "fn load(");
    assert!(
        load_block.contains("# Errors") && load_block.contains("`ParseError`"),
        "{load_block}"
    );

    let create_block = _def_block(&src, "fn create(");
    assert!(
        create_block.contains("# Errors") && create_block.contains("`TimeoutError`"),
        "{create_block}"
    );
}

/// The generated source's member block for `marker`: the attached `///` doc
/// comment (which is where Rust carries `# Errors`) plus the marker itself.
/// (python slices the `.pyi` source from `marker` down to the next
/// def/decorator line because a docstring sits *inside* a python `def`; Rust
/// doc comments *precede* the item, so the block is collected upward.)
fn _def_block(src: &str, marker: &str) -> String {
    assert!(src.contains(marker), "marker {marker:?} not found");
    let doc = _getdoc(src, marker).unwrap_or_default();
    format!("{doc}\n{marker}")
}
