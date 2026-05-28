//! M4d.2 — HIR lowering for `TAGGED_TEMPLATE_EXPR` (BEP-049 §10).
//!
//! These tests drive a real tagged template (interpolation + `${for}` +
//! `${if}`) through the *full* compiler2 front end — parse → AST lowering →
//! HIR semantic indexing (`walk_tagged_segment`) → TIR — via the diagnostics
//! pipeline, which stops before MIR (MIR lowering of tagged templates lands
//! in M4e.1, so the executing `baml_test!` macro can't be used yet).
//!
//! The AST-level structural assertions live in `baml_compiler2_ast`; here we
//! guard that the lowered node survives the rest of the front end without
//! panicking or emitting spurious diagnostics. Tag-resolution diagnostics
//! (M4d.3) and tag-aware type inference (M4d.4) add assertions on top of this
//! harness once those milestones land.

use baml_compiler_diagnostics::Severity;
use baml_compiler2_hir::compiler2_all_files;
use baml_project::{ProjectDatabase, collect_compiler2_diagnostics};

/// Run the compiler2 front end over `source` and return every error-severity
/// diagnostic message. Panics here would surface a front-end crash on the
/// tagged-template path.
fn front_end_errors(source: &str) -> Vec<String> {
    let mut db = ProjectDatabase::new();
    let _root = db.set_project_root(std::path::Path::new("."));
    let _file = db.add_file("main.baml", source);
    // Force the salsa front end to run (parse → AST → HIR → TIR).
    let _all = compiler2_all_files(&db);
    collect_compiler2_diagnostics(&db)
        .into_iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| format!("[{:?}] {}", d.phase, d.message))
        .collect()
}

#[test]
fn tagged_template_with_interp_for_and_if_survives_front_end() {
    // `sql` is an ordinary function here — M4d.2 only lowers the tag
    // structurally; the `//baml:tagged_string` marker + signature checks are
    // M4d.3. The template mixes a literal interpolation, a `${for}` whose
    // body references the loop binding `x`, and an `${if}/${else}` chain.
    let source = r#"
function sql(x: int) -> string {
  "ok"
}

function Demo(items: int[]) -> string {
  sql`SELECT ${1} ${for (let x in items)}col_${x}, ${endfor}${if (true)}WHERE 1${else}ALL${endif}`
}
"#;
    let errors = front_end_errors(source);
    assert!(
        errors.is_empty(),
        "tagged template should produce no front-end errors yet (TIR treats it \
         as Unknown until M4d.4), but got:\n{}",
        errors.join("\n")
    );
}

#[test]
fn tagged_template_as_let_initializer_survives_front_end() {
    // Exercises the `LetStmt::initializer()` classifier path (a tagged
    // template bound with `let`), distinct from the block tail-expression
    // path covered above.
    let source = r#"
function sql(x: int) -> string {
  "ok"
}

function Demo() -> string {
  let q = sql`SELECT ${1}`
  q
}
"#;
    let errors = front_end_errors(source);
    assert!(
        errors.is_empty(),
        "tagged template let-initializer should produce no front-end errors \
         yet, but got:\n{}",
        errors.join("\n")
    );
}
