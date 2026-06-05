//! BEP-049: structural diagnostics for backtick template block tags
//! (`${for}`/`${endfor}`, `${if}`/`${else}`/`${else if}`/`${endif}`).
//!
//! Before this, an unclosed / mismatched / stray block tag compiled SILENTLY
//! (the body absorbed to EOF, or the stray tag was dropped — sometimes losing
//! trailing content). These pin that every structural mistake now produces a
//! user-facing diagnostic pointing at the offending tag. Covers both the M5
//! `prompt` path and plain untagged backticks (the shared segment builder).

use baml_project::{ProjectDatabase, collect_compiler2_diagnostics};

/// Compile `source` and return the user-facing diagnostic messages.
fn messages(source: &str) -> Vec<String> {
    let mut db = ProjectDatabase::new();
    db.set_project_root(std::path::Path::new("."));
    db.add_file("test.baml", source);
    collect_compiler2_diagnostics(&db)
        .iter()
        .map(|d| d.message.clone())
        .collect()
}

fn prompt(body: &str) -> String {
    format!(
        "client<llm> C {{\n  provider openai\n  options {{ model \"m\" api_key \"k\" }}\n}}\n\nfunction G() -> string {{\n  client C\n  prompt `{body}`\n}}\n"
    )
}

fn untagged(body: &str) -> String {
    format!("function G() -> string {{\n  let s = `{body}`;\n  s\n}}\n")
}

fn assert_has(source: &str, needle: &str) {
    let msgs = messages(source);
    assert!(
        msgs.iter().any(|m| m.contains(needle)),
        "expected a diagnostic containing {needle:?}, got: {msgs:#?}\nsource:\n{source}"
    );
}

#[test]
fn unclosed_for_diagnoses() {
    assert_has(&prompt("${for (let x in [1])}${x}"), "unclosed ${for}");
    assert_has(&untagged("${for (let x in [1])}${x}"), "unclosed ${for}");
}

#[test]
fn unclosed_if_diagnoses() {
    assert_has(&prompt("${if (true)}yes"), "unclosed ${if}");
    assert_has(&untagged("${if (true)}yes"), "unclosed ${if}");
}

#[test]
fn mismatched_for_close_diagnoses() {
    // ${for} closed by ${endif}
    assert_has(&prompt("${for (let x in [1])}${x}${endif}"), "${endfor}");
    assert_has(&untagged("${for (let x in [1])}${x}${endif}"), "${endfor}");
}

#[test]
fn mismatched_if_close_diagnoses() {
    // ${if} closed by ${endfor}
    assert_has(&prompt("${if (true)}a${endfor}"), "${endif}");
}

#[test]
fn stray_endfor_diagnoses() {
    assert_has(&prompt("a${endfor}b"), "stray ${endfor}");
    assert_has(&untagged("a${endfor}b"), "stray ${endfor}");
}

#[test]
fn stray_endif_diagnoses() {
    assert_has(&prompt("a${endif}b"), "stray ${endif}");
}

#[test]
fn stray_else_diagnoses() {
    assert_has(&prompt("a${else}b"), "stray ${else}");
    assert_has(&prompt("a${else if (true)}b"), "stray ${else if}");
}

#[test]
fn empty_interpolation_diagnoses() {
    assert_has(&prompt("x${}y"), "empty interpolation");
    assert_has(&untagged("x${}y"), "empty interpolation");
}

#[test]
fn well_formed_blocks_have_no_structural_diagnostic() {
    // A correctly-closed for/if must NOT trip any structural diagnostic.
    let src = prompt(
        "${for (let x in [1, 2])}${x}${endfor}${if (true)}a${else if (false)}b${else}c${endif}",
    );
    let msgs = messages(&src);
    assert!(
        msgs.iter().all(|m| !m.contains("unclosed")
            && !m.contains("stray")
            && !m.contains("empty interpolation")),
        "well-formed template should have no structural diagnostics, got: {msgs:#?}"
    );
}
