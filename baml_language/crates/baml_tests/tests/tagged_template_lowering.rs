//! BEP-049 §10 tagged templates — front-end (parse → AST → HIR → TIR) tests.
//!
//! These drive tagged templates through the compiler2 front end via the
//! diagnostics pipeline (`collect_compiler2_diagnostics`), which stops before
//! MIR (MIR lowering of tagged templates lands in M4e.1, so the executing
//! `baml_test!` macro can't be used yet).
//!
//! - M4d.2 (AST lowering): structural assertions live in `baml_compiler2_ast`;
//!   here we guard that the lowered node survives HIR + TIR without panicking.
//! - M4d.3 (tag resolution + signature validation): the diagnostic tests below
//!   assert the tag-aware errors — unmarked tag, non-function tag, and a
//!   malformed `body: (...) -> TaggedString` first parameter.

use baml_compiler_diagnostics::Severity;
use baml_compiler2_hir::compiler2_all_files;
use baml_db::{ProjectDatabase, collect_compiler2_diagnostics};
use baml_tests::engine::TestDbExt;

/// Run the compiler2 front end over `source` and return every error-severity
/// diagnostic message. A panic here would surface a front-end crash on the
/// tagged-template path.
fn front_end_errors(source: &str) -> Vec<String> {
    let mut db = ProjectDatabase::new();
    let _root = db.workspace(std::path::Path::new("."));
    let _file = db.file("main.baml", source);
    // Force the salsa front end to run (parse → AST → HIR → TIR).
    let _all = compiler2_all_files(&db);
    collect_compiler2_diagnostics(&db)
        .into_iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| format!("[{:?}] {}", d.phase, d.message_with_primary_label()))
        .collect()
}

/// A well-formed `//baml:tagged_string` tag function used by the happy-path
/// tests. Its first parameter is `body: (...) -> TaggedString` per §10.
const VALID_TAG: &str = r#"
//baml:tagged_string
function sql(body: (x: int) -> baml.TaggedString) -> string {
  "ok"
}
"#;

#[test]
fn tagged_template_with_interp_for_and_if_survives_front_end() {
    // A valid tag with a template that mixes a literal interpolation, a
    // `${for}` whose body references the loop binding `x`, and an
    // `${if}/${else}` chain. M4d.3 validates the tag; segment/interp typing
    // is M4d.4, so no interpolation-level errors are expected yet.
    let source = format!(
        "{VALID_TAG}\n\
function Demo(items: int[]) -> string {{\n\
  sql`SELECT ${{1}} ${{for (let x in items)}}col_${{x}}, ${{endfor}}${{if (true)}}WHERE 1${{else}}ALL${{endif}}`\n\
}}\n"
    );
    let errors = front_end_errors(&source);
    assert!(
        errors.is_empty(),
        "a valid tagged template should produce no front-end errors, but got:\n{}",
        errors.join("\n")
    );
}

#[test]
fn tagged_template_as_let_initializer_survives_front_end() {
    // Exercises the `LetStmt::initializer()` classifier path (a tagged
    // template bound with `let`), distinct from the block tail-expression
    // path covered above.
    let source = format!(
        "{VALID_TAG}\n\
function Demo() -> string {{\n\
  let q = sql`SELECT ${{1}}`\n\
  q\n\
}}\n"
    );
    let errors = front_end_errors(&source);
    assert!(
        errors.is_empty(),
        "a valid tagged-template let-initializer should produce no front-end \
         errors, but got:\n{}",
        errors.join("\n")
    );
}

// ── M4d.3: tag resolution + signature validation ────────────────────────────

#[test]
fn tagged_tag_unmarked_function_errors() {
    // `sql` is a real function but lacks the `//baml:tagged_string` marker.
    let source = r#"
function sql(body: (x: int) -> baml.TaggedString) -> string { "ok" }
function Demo() -> string { sql`hi ${1}` }
"#;
    let errors = front_end_errors(source);
    assert!(
        errors
            .iter()
            .any(|e| e.contains("not a tagged-string function")),
        "expected unmarked-tag error, got:\n{}",
        errors.join("\n")
    );
}

#[test]
fn tagged_tag_marked_valid_no_errors() {
    let source = format!("{VALID_TAG}\nfunction Demo() -> string {{ sql`hi ${{1}}` }}\n");
    let errors = front_end_errors(&source);
    assert!(
        errors.is_empty(),
        "a valid marked tag must not error, got:\n{}",
        errors.join("\n")
    );
}

#[test]
fn tagged_tag_unresolved_name_reports_once() {
    // An undefined tag name resolves to nothing: the existing UnresolvedName
    // error should fire exactly once — M4d.3 must NOT add a duplicate tag error.
    let source = r#"
function Demo() -> string { nope`hi ${1}` }
"#;
    let errors = front_end_errors(source);
    let mentioning_nope = errors
        .iter()
        .filter(|e| e.to_lowercase().contains("nope"))
        .count();
    assert_eq!(
        mentioning_nope,
        1,
        "an unresolved tag must report once (via UnresolvedName), got:\n{}",
        errors.join("\n")
    );
}

#[test]
fn tagged_tag_not_a_function_errors() {
    // The tag resolves to a local of non-function type.
    let source = r#"
function Demo() -> string {
  let sql = 3
  sql`hi ${1}`
}
"#;
    let errors = front_end_errors(source);
    assert!(
        errors.iter().any(|e| e.contains("is not a function")),
        "expected not-a-function error, got:\n{}",
        errors.join("\n")
    );
}

#[test]
fn tagged_tag_bad_body_param_wrong_name_errors() {
    let source = r#"
//baml:tagged_string
function sql(notbody: (x: int) -> baml.TaggedString) -> string { "ok" }
function Demo() -> string { sql`hi ${1}` }
"#;
    let errors = front_end_errors(source);
    assert!(
        errors.iter().any(|e| e.contains("must be `body:")),
        "expected bad-body-param error (wrong name), got:\n{}",
        errors.join("\n")
    );
}

#[test]
fn tagged_tag_bad_body_param_wrong_return_errors() {
    let source = r#"
//baml:tagged_string
function sql(body: (x: int) -> string) -> string { "ok" }
function Demo() -> string { sql`hi ${1}` }
"#;
    let errors = front_end_errors(source);
    assert!(
        errors.iter().any(|e| e.contains("must be `body:")),
        "expected bad-body-param error (return not TaggedString), got:\n{}",
        errors.join("\n")
    );
}

#[test]
fn tagged_tag_missing_body_param_errors() {
    let source = r#"
//baml:tagged_string
function sql() -> string { "ok" }
function Demo() -> string { sql`hi ${1}` }
"#;
    let errors = front_end_errors(source);
    assert!(
        errors.iter().any(|e| e.contains("must be `body:")),
        "expected bad-body-param error (missing param), got:\n{}",
        errors.join("\n")
    );
}

// ── M4d.4: segment typing in body-lambda param scope ────────────────────────

/// A valid tag whose body lambda has a distinctly-named param (`role`) so the
/// for-binding tests can use `x` without colliding with the body param.
const ROLE_TAG: &str = r#"
//baml:tagged_string
function chat(body: (role: string) -> baml.TaggedString) -> string {
  "ok"
}
"#;

#[test]
fn tagged_interp_resolves_body_lambda_param() {
    // `${role}` resolves to the tag's body-lambda param `role: string`.
    let source = format!("{ROLE_TAG}\nfunction Demo() -> string {{ chat`hello ${{role}}` }}\n");
    let errors = front_end_errors(&source);
    assert!(
        errors.is_empty(),
        "the body-lambda param `role` should resolve inside the interpolation, got:\n{}",
        errors.join("\n")
    );
}

#[test]
fn tagged_interp_unknown_name_reports_unresolved() {
    let source =
        format!("{ROLE_TAG}\nfunction Demo() -> string {{ chat`hello ${{nonexistent}}` }}\n");
    let errors = front_end_errors(&source);
    assert!(
        errors
            .iter()
            .any(|e| e.contains("unresolved name: `nonexistent`")),
        "an unknown interpolation name must report UnresolvedName, got:\n{}",
        errors.join("\n")
    );
}

#[test]
fn tagged_for_binding_in_scope_inside_for_body() {
    // `x` is bound inside the `${for}` body (items: int[] → x: int). No errors.
    let source = format!(
        "{ROLE_TAG}\nfunction Demo(items: int[]) -> string {{ \
         chat`${{for (let x in items)}}col_${{x}}, ${{endfor}}` }}\n"
    );
    let errors = front_end_errors(&source);
    assert!(
        errors.is_empty(),
        "for-binding `x` should resolve inside the for body, got:\n{}",
        errors.join("\n")
    );
}

#[test]
fn tagged_for_binding_out_of_scope_after_endfor() {
    // `x` must NOT leak past `${endfor}` — the trailing `${x}` is unresolved.
    let source = format!(
        "{ROLE_TAG}\nfunction Demo(items: int[]) -> string {{ \
         chat`${{for (let x in items)}}a${{endfor}}${{x}}` }}\n"
    );
    let errors = front_end_errors(&source);
    assert!(
        errors.iter().any(|e| e.contains("unresolved name: `x`")),
        "for-binding `x` must be out of scope after endfor, got:\n{}",
        errors.join("\n")
    );
}

#[test]
fn tagged_for_non_iterable_collection_reports_not_iterable() {
    // Iterating a non-list value reports NotIterable.
    let source = format!(
        "{ROLE_TAG}\nfunction Demo(n: int) -> string {{ \
         chat`${{for (let x in n)}}a${{endfor}}` }}\n"
    );
    let errors = front_end_errors(&source);
    assert!(
        errors
            .iter()
            .any(|e| e.contains("cannot iterate over type")),
        "iterating an int must report NotIterable, got:\n{}",
        errors.join("\n")
    );
}

#[test]
fn tagged_body_param_does_not_leak_after_template() {
    // The body-lambda param `role` is in scope only inside the template's
    // interpolations — a reference to `role` after the template is unresolved.
    let source = format!(
        "{ROLE_TAG}\nfunction Demo() -> string {{\n  let r = chat`hi ${{role}}`\n  role\n}}\n"
    );
    let errors = front_end_errors(&source);
    assert!(
        errors.iter().any(|e| e.contains("unresolved name: `role`")),
        "the body-lambda param `role` must not leak past the template, got:\n{}",
        errors.join("\n")
    );
}
