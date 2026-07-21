//! BAML `///` doc-comment lowering coverage.
//!
//! Asserts on the doc comments of generated classes and enums so the
//! rules in `ns_docs/types.baml` are pinned end-to-end (CST → AST → HIR →
//! IR → codegen).
//!
//! ADAPTATION(rust): python asserts on the runtime `__doc__` of the
//! imported classes — a class docstring carrying the summary plus an
//! `Attributes:`/`Members:` rollup section. Rust has no runtime docstrings,
//! so these tests read the generated source of the `docs` namespace (the
//! test binary runs with the generated crate root as cwd) and assert on its
//! `///` doc-comment text. Rust docs also live on the documented item
//! itself — a field/variant doc sits on the field/variant, not in a parent
//! rollup — so each section assertion becomes a per-item doc assertion with
//! identical intent: summary present, documented fields/variants carry
//! their doc, undocumented ones are bare, and nothing leaks anywhere else.
//!
//! PROVISIONAL(rust-codegen): assumes the emitter writes the namespace
//! module at `src/docs/mod.rs`.

/// Read the generated source for the `docs` namespace.
fn docs_source() -> String {
    std::fs::read_to_string("src/docs/mod.rs").expect("read the generated docs module")
}

/// The `///` doc block attached to the first line containing `needle`: the
/// contiguous run of `///` lines directly above it (skipping any `#[...]`
/// attribute lines between the two), prefixes stripped, joined with
/// newlines — the Rust-source analogue of python's `inspect.getdoc`.
/// Empty when the item carries no doc.
fn item_doc(src: &str, needle: &str) -> String {
    let lines: Vec<&str> = src.lines().collect();
    let idx = lines
        .iter()
        .position(|line| line.contains(needle))
        .unwrap_or_else(|| panic!("no line containing {needle:?} in the docs module"));
    let mut doc: Vec<&str> = lines[..idx]
        .iter()
        .rev()
        .map(|line| line.trim_start())
        .skip_while(|line| line.starts_with("#["))
        .take_while(|line| line.starts_with("///"))
        .map(|line| {
            let body = &line[3..];
            body.strip_prefix(' ').unwrap_or(body)
        })
        .collect();
    doc.reverse();
    doc.join("\n")
}

#[test]
fn test_main_imports_symbols_reachable() {
    use baml_sdk as _;
    use baml_sdk::docs as _;
    use baml_sdk::docs::{Doc as _, Note as _, Priority as _, Sentiment as _};
}

#[test]
fn test_main_class_doc_summary_and_attributes_section() {
    // `item_doc` trims the surrounding whitespace and strips the `/// `
    // prefixes (like python's `inspect.getdoc` dedents) so the assertions
    // are stable against the emitted indent.
    let src = docs_source();

    assert_eq!(
        item_doc(&src, "pub struct Doc"),
        "A document with a title and an optional body."
    );
    assert_eq!(
        item_doc(&src, "pub title:"),
        "Title shown in lists and search results."
    );
    assert_eq!(item_doc(&src, "pub body:"), "Free-form body text.");
}

#[test]
fn test_main_undocumented_field_listed_as_bare_name_under_attributes() {
    // Note: `id` is documented, `text` is not. The "any-doc" rule says
    // the Attributes: section appears (because `id` carries a `///`)
    // and lists every field, with `text` rendered as a bare name.
    // ADAPTATION(rust): with per-item docs the rule surfaces as — `id`
    // carries its `///`, `text` carries none, and both fields exist.
    let src = docs_source();

    let summary = item_doc(&src, "pub struct Note");
    assert!(
        summary.starts_with("A multi-line summary.\nContinuation line"),
        "got:\n{summary:?}"
    );
    assert!(
        item_doc(&src, "pub id:").starts_with("Stable identifier"),
        "got:\n{src}"
    );
    // Bare-name entry: the field is present and carries no doc of its own.
    assert!(src.contains("pub text:"), "got:\n{src}");
    assert_eq!(item_doc(&src, "pub text:"), "");
}

#[test]
fn test_main_enum_doc_summary_and_members_section() {
    let src = docs_source();

    assert_eq!(
        item_doc(&src, "pub enum Sentiment"),
        "Sentiment labels surfaced by the model."
    );
    assert_eq!(item_doc(&src, "HAPPY"), "Smiling face.");
    assert_eq!(item_doc(&src, "SAD"), "Frowning face.");
    assert_eq!(item_doc(&src, "NEUTRAL"), "");
}

#[test]
fn test_main_enum_summary_only_omits_members_section() {
    // Priority has a class-level /// but no variant carries one — the
    // Members: section should be suppressed entirely. Variants are
    // still importable / iterable normally.
    use baml_sdk::docs::Priority;

    let src = docs_source();
    let doc = item_doc(&src, "pub enum Priority");
    assert_eq!(
        doc,
        "Pin the \"summary only, no member rollup\" case: this enum has a\n\
         class-level `///` but every variant is bare."
    );
    assert!(
        !doc.contains("Members:"),
        "Members: section leaked in:\n{doc}"
    );
    assert_eq!(item_doc(&src, "HIGH"), "");
    assert_eq!(item_doc(&src, "MEDIUM"), "");
    assert_eq!(item_doc(&src, "LOW"), "");
    // ADAPTATION(rust): python asserts the variant-value set via runtime
    // iteration; the wildcard-free match pins the same exact variant set at
    // compile time (a variant added or removed would not compile).
    match Priority::HIGH {
        Priority::HIGH | Priority::MEDIUM | Priority::LOW => {}
    }
}

#[test]
fn test_main_no_inline_field_or_variant_doc_artifacts() {
    // Field/variant `///` lines must not produce inline `# …` comments
    // or `"""…"""` attribute docstrings — they live exclusively inside
    // the parent's Attributes:/Members: section.
    // ADAPTATION(rust): the artifact classes are Rust-shaped here — a
    // field/variant doc must appear exactly once, as a `///` doc comment,
    // never duplicated into a plain `//` comment or a second location.
    let src = docs_source();
    for line in src.lines() {
        let trimmed = line.trim_start();
        if let Some(comment) = trimmed.strip_prefix("//")
            && !comment.starts_with('/')
        {
            assert!(
                !comment.contains("Title shown in lists"),
                "field doc leaked into a plain comment: {line}"
            );
            assert!(
                !comment.contains("Smiling face"),
                "variant doc leaked into a plain comment: {line}"
            );
        }
    }
    assert_eq!(src.matches("Title shown in lists").count(), 1, "{src}");
    assert_eq!(src.matches("Smiling face").count(), 1, "{src}");
}
