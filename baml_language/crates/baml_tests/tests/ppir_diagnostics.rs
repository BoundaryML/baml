//! Integration tests for PPIR stream-annotation diagnostics.
//!
//! These tests exercise the full `ppir_stream_diagnostics` pipeline through a
//! real `ProjectDatabase`.  They live here (in `baml_tests`) rather than in
//! `baml_compiler_ppir/src/tests.rs` because `baml_compiler_ppir` must not
//! take `baml_project` as a dev-dependency: doing so would introduce a second
//! copy of `baml_compiler_ppir` in the dependency graph, which breaks Salsa's
//! `Db` trait-bound resolution.

use tempfile::tempdir;

use baml_compiler_diagnostics::HirDiagnostic;
use baml_compiler_ppir::ppir_stream_diagnostics;
use baml_project::ProjectDatabase;

// ── helpers ──────────────────────────────────────────────────────────────────

/// Run `ppir_stream_diagnostics` on `source` inside a throwaway temp project.
fn stream_diagnostics(source: &str) -> Vec<HirDiagnostic> {
    let dir = tempdir().expect("failed to create temp project dir");

    let mut db = ProjectDatabase::new();
    db.set_project_root(dir.path());
    let file_path = dir.path().join("test.baml");
    let file = db.add_or_update_file(&file_path, source);
    ppir_stream_diagnostics(&db, file)
}

// ── @stream.* type-level attribute diagnostics ───────────────────────────────

#[test]
fn unknown_stream_type_attr_is_reported() {
    let diagnostics = stream_diagnostics(
        r#"class Foo {
  name string @stream.typo
}"#,
    );

    let found = diagnostics.iter().any(|d| {
        matches!(
            d,
            HirDiagnostic::UnknownAttribute { attr_name, .. }
                if attr_name == "stream.typo"
        )
    });

    assert!(
        found,
        "expected UnknownAttribute for 'stream.typo', got: {diagnostics:#?}"
    );
}

#[test]
fn conflicting_not_null_and_starts_as_is_reported() {
    let diagnostics = stream_diagnostics(
        r#"class Foo {
  name string @stream.not_null @stream.starts_as("x")
}"#,
    );

    let found = diagnostics.iter().any(|d| {
        matches!(
            d,
            HirDiagnostic::ConflictingStreamAttributes {
                first_attr,
                second_attr,
                ..
            } if *first_attr == "stream.not_null" && *second_attr == "stream.starts_as"
        )
    });

    assert!(
        found,
        "expected ConflictingStreamAttributes for not_null+starts_as, got: {diagnostics:#?}"
    );
}

#[test]
fn conflicting_done_and_type_is_reported() {
    let diagnostics = stream_diagnostics(
        r#"class Foo {
  name string @stream.done @stream.type(string)
}"#,
    );

    let found = diagnostics.iter().any(|d| {
        matches!(
            d,
            HirDiagnostic::ConflictingStreamAttributes {
                first_attr,
                second_attr,
                ..
            } if *first_attr == "stream.done" && *second_attr == "stream.type"
        )
    });

    assert!(
        found,
        "expected ConflictingStreamAttributes for done+type, got: {diagnostics:#?}"
    );
}

// ── @@stream.* block-level attribute diagnostics ─────────────────────────────

#[test]
fn block_attr_with_args_is_rejected() {
    let diagnostics = stream_diagnostics(
        r#"class Foo {
  @@stream.done("unexpected")
  name string
}"#,
    );

    let found = diagnostics.iter().any(|d| {
        matches!(
            d,
            HirDiagnostic::UnexpectedAttributeArg { attr_name, .. }
                if attr_name == "stream.done"
        )
    });

    assert!(
        found,
        "expected UnexpectedAttributeArg for @@stream.done(\"unexpected\"), got: {diagnostics:#?}"
    );
}

#[test]
fn unknown_stream_block_attr_is_reported() {
    let diagnostics = stream_diagnostics(
        r#"class Foo {
  @@stream.unknown_block_attr
  name string
}"#,
    );

    let found = diagnostics.iter().any(|d| {
        matches!(
            d,
            HirDiagnostic::UnknownAttribute { attr_name, .. }
                if attr_name == "stream.unknown_block_attr"
        )
    });

    assert!(
        found,
        "expected UnknownAttribute for '@@stream.unknown_block_attr', got: {diagnostics:#?}"
    );
}

// ── clean inputs produce no diagnostics ──────────────────────────────────────

#[test]
fn valid_stream_type_attr_produces_no_diagnostics() {
    let diagnostics = stream_diagnostics(
        r#"class Foo {
  name string @stream.type(string)
}"#,
    );

    assert!(
        diagnostics.is_empty(),
        "expected no diagnostics for valid @stream.type, got: {diagnostics:#?}"
    );
}

#[test]
fn valid_stream_done_block_attr_produces_no_diagnostics() {
    let diagnostics = stream_diagnostics(
        r#"class Foo {
  @@stream.done
  name string
}"#,
    );

    assert!(
        diagnostics.is_empty(),
        "expected no diagnostics for valid @@stream.done, got: {diagnostics:#?}"
    );
}

#[test]
fn valid_stream_not_null_type_attr_produces_no_diagnostics() {
    let diagnostics = stream_diagnostics(
        r#"class Foo {
  name string @stream.not_null
}"#,
    );

    assert!(
        diagnostics.is_empty(),
        "expected no diagnostics for valid @stream.not_null, got: {diagnostics:#?}"
    );
}

#[test]
fn non_stream_class_produces_no_diagnostics() {
    let diagnostics = stream_diagnostics(
        r#"class Resume {
  name string
  age int
}"#,
    );

    assert!(
        diagnostics.is_empty(),
        "expected no diagnostics for plain class, got: {diagnostics:#?}"
    );
}
