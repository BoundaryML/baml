//! BEP-049: structural diagnostics for backtick template block tags
//! (`${for}`/`${endfor}`, `${if}`/`${else}`/`${else if}`/`${endif}`).
//!
//! Before this, an unclosed / mismatched / stray block tag compiled SILENTLY
//! (the body absorbed to EOF, or the stray tag was dropped — sometimes losing
//! trailing content). These pin that every structural mistake now produces a
//! user-facing diagnostic pointing at the offending tag. Covers both the M5
//! `prompt` path and plain untagged backticks (the shared segment builder).

use baml_db::{ProjectDatabase, collect_compiler2_diagnostics};
use baml_tests::engine::TestDbExt;

/// Compile `source` and return the user-facing diagnostic messages.
fn messages(source: &str) -> Vec<String> {
    let mut db = ProjectDatabase::new();
    db.workspace(std::path::Path::new("."));
    db.file("test.baml", source);
    collect_compiler2_diagnostics(&db)
        .iter()
        .map(|d| d.message.clone())
        .collect()
}

fn prompt(body: &str) -> String {
    format!("function G() -> string {{\n  client: \"openai/gpt-4o-mini\"\n  prompt: `{body}`\n}}\n")
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
fn out_of_order_else_branches_diagnose() {
    // A second ${else} in the same chain.
    assert_has(
        &prompt("${if (true)}a${else}b${else}c${endif}"),
        "duplicate ${else}",
    );
    assert_has(
        &untagged("${if (true)}a${else}b${else}c${endif}"),
        "duplicate ${else}",
    );
    // ${else if} after the chain's ${else} is out of order.
    assert_has(
        &prompt("${if (true)}a${else}b${else if (false)}c${endif}"),
        "${else if} after ${else}",
    );
    assert_has(
        &untagged("${if (true)}a${else}b${else if (false)}c${endif}"),
        "${else if} after ${else}",
    );
}

#[test]
fn empty_interpolation_diagnoses() {
    assert_has(&prompt("x${}y"), "empty interpolation");
    assert_has(&untagged("x${}y"), "empty interpolation");
}

#[test]
fn for_header_accepts_const_binding() {
    // `${for (const x in xs)}` must parse like the host `for` (which accepts the
    // contextual `const`) — not fall into the C-style path and error.
    let msgs = messages(&prompt("${for (const x in [1, 2])}${x}${endfor}"));
    // No parse failure (the old bug routed `const` into the C-style path).
    assert!(
        msgs.iter().all(|m| !m.contains("unexpected")
            && !m.contains("Unexpected")
            && !m.contains("'let' or ';'")
            && !m.contains("unclosed")
            && !m.contains("stray")),
        "const for-header should parse cleanly, got: {msgs:#?}"
    );
    // Reaching binding handling is proven by the standard `const`→`let` advisory.
    assert!(
        msgs.iter().any(|m| m.contains("treated like `let`")),
        "expected the const→let advisory (proves the binding was recognized), got: {msgs:#?}"
    );
}

#[test]
fn unresolved_name_in_interp_reports_cleanly() {
    // BEP §4: a real unresolved name in `${…}` must surface a clean diagnostic,
    // not slip through as `Ty::Unknown` and ICE at runtime lowering. The
    // untagged-template desugaring never introduces a fresh name reference, so a
    // bare unresolved name here is always genuine user code and is retained.
    let msgs = messages(&untagged("${ nope }"));
    assert!(
        msgs.iter()
            .any(|m| m.contains("nope") || m.to_lowercase().contains("unresolved")),
        "expected an unresolved-name diagnostic, got: {msgs:#?}"
    );
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

fn assert_none(source: &str, forbidden: &str) {
    let msgs = messages(source);
    assert!(
        msgs.iter().all(|m| !m.contains(forbidden)),
        "expected NO diagnostic containing {forbidden:?}, got: {msgs:#?}\nsource:\n{source}"
    );
}

#[test]
fn unresolved_name_in_interp_reports() {
    // An unknown name in a `${…}` value segment must produce a name-resolution
    // diagnostic — not compile silently. Before the fix the untagged template
    // truncated ALL diagnostics from typing its desugared form (to drop
    // synthetic `.to_string()` noise), which also swallowed this genuine
    // `UnresolvedName`; the resulting `Ty::Unknown` then ICEd at MIR runtime
    // lowering. Now the genuine name error survives the truncation.
    assert_has(&untagged("${ nope }"), "nope");
    assert_has(&prompt("${ nope }"), "nope");
}

#[test]
fn unresolved_name_in_statement_only_interp_reports() {
    // The unknown is on the RHS of a `let` inside a side-effect-only
    // `${ let … }` segment whose binding is read by a later segment. The
    // spliced statement is typed in the shared concat scope, so the
    // `UnresolvedName` on `nope` is raised and retained.
    assert_has(&untagged("${ let a = nope }${a}"), "nope");
}

#[test]
fn cross_site_let_has_no_spurious_diagnostic() {
    // The valid BEP-049 §4 cross-site `let` must type cleanly: no unresolved
    // name for `w`, and no synthetic `.to_string()` / concat noise leaking out.
    assert_none(&untagged("${ let w = \"hi\" }${w}"), "nresolved");
    assert_none(&untagged("${ let w = \"hi\" }${w}"), "cannot be called");
    assert_none(&untagged("${ let w = \"hi\" }${w}"), "not a function");
}

#[test]
fn nullable_interp_still_reports_on_original_span_only() {
    // Suppression of synthetic noise must remain intact: a nullable interp
    // surfaces the strict-stringify `… may be null` error from the §11 segment
    // check (original `${…}` span), NOT the synthetic
    // `(… ) | null is not a function — it cannot be called` from the elaborated
    // `a.to_string()`. (Retaining `UnresolvedName` must not re-admit this.)
    let src = "function G(a: string?) -> string {\n  let s = `${a}`;\n  s\n}\n";
    let msgs = messages(src);
    assert!(
        msgs.iter().any(|m| m.to_lowercase().contains("null")),
        "expected a may-be-null strict-stringify diagnostic, got: {msgs:#?}"
    );
    assert!(
        msgs.iter().all(|m| !m.contains("it cannot be called")),
        "synthetic `.to_string()` NotCallable noise must stay suppressed, got: {msgs:#?}"
    );
}
