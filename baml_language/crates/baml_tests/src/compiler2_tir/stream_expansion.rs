//! Stream type expansion snapshot tests for PPIR → HIR → TIR pipeline.
//!
//! Tests that `@stream.*` annotations produce correct `stream_*` class/alias
//! definitions, matching the expansion rules in `01b-stream-expansion-rules.md`.

use super::support::{make_db, render_tir};

// ── Default expansion (no annotations) ──────────────────────────────────────

#[test]
fn primitives_get_null_union() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        "\
class Primitives {
    name string
    count int
    flag bool
    score float
}",
    );
    insta::assert_snapshot!(render_tir(&db, file));
}

#[test]
fn enum_field_unchanged() {
    let mut db = make_db();
    db.add_file(
        "enums.baml",
        "\
enum Status {
    Active
    Inactive
}",
    );
    let file = db.add_file(
        "test.baml",
        "\
class WithEnum {
    status Status
}",
    );
    insta::assert_snapshot!(render_tir(&db, file));
}

#[test]
fn class_field_gets_stream_prefix() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        "\
class Inner {
    value string
}

class Outer {
    inner Inner
}",
    );
    insta::assert_snapshot!(render_tir(&db, file));
}

#[test]
fn literal_fields_unchanged() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"
class WithLiterals {
    type "resume"
    version 1
    enabled true
}
"#,
    );
    insta::assert_snapshot!(render_tir(&db, file));
}

#[test]
fn list_field_recurses() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        "\
class Inner {
    value string
}

class WithLists {
    strings string[]
    ints int[]
    classes Inner[]
}",
    );
    insta::assert_snapshot!(render_tir(&db, file));
}

#[test]
fn map_field_recurses_value() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        "\
class Inner {
    value string
}

class WithMaps {
    simple map<string, int>
    complex map<string, Inner>
}",
    );
    insta::assert_snapshot!(render_tir(&db, file));
}

#[test]
fn union_field_recurses_variants() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        "\
class Inner {
    value string
}

class WithUnions {
    simple int | string
    with_class Inner | string
    mixed int | Inner
}",
    );
    insta::assert_snapshot!(render_tir(&db, file));
}

#[test]
fn optional_expands_to_union_with_null() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        "\
class Inner {
    value string
}

class WithOptionals {
    name string?
    inner Inner?
}",
    );
    insta::assert_snapshot!(render_tir(&db, file));
}

#[test]
fn type_alias_expansion() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        "\
class Inner {
    value string
}

type SimpleAlias = string
type ClassAlias = Inner
type UnionAlias = int | Inner
type OptionalAlias = Inner?",
    );
    insta::assert_snapshot!(render_tir(&db, file));
}

#[test]
fn recursive_class() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        "\
class TreeNode {
    value string
    children TreeNode[]
}",
    );
    insta::assert_snapshot!(render_tir(&db, file));
}

// ── @stream.done ────────────────────────────────────────────────────────────

#[test]
fn stream_done_field_keeps_type_as_is() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        "\
class WithDone {
    name string @stream.done
    age int
}",
    );
    insta::assert_snapshot!(render_tir(&db, file));
}

#[test]
fn stream_done_block_attr() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        "\
class AtomicPoint {
    @@stream.done
    x float
    y float
}",
    );
    insta::assert_snapshot!(render_tir(&db, file));
}

// ── @stream.not_null ────────────────────────────────────────────────────────

#[test]
fn stream_not_null_field() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        "\
class WithNotNull {
    gpa float @stream.not_null
    name string
}",
    );
    insta::assert_snapshot!(render_tir(&db, file));
}

#[test]
fn stream_not_null_block_attr_on_referenced_class() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        "\
class NotNullEducation {
    @@stream.not_null
    school string
    year int
}

class References {
    education NotNullEducation
    educations NotNullEducation[]
}",
    );
    insta::assert_snapshot!(render_tir(&db, file));
}

// ── Combined annotations ────────────────────────────────────────────────────

#[test]
fn stream_done_and_not_null() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        "\
class Combined {
    id string @stream.done @stream.not_null
    name string @stream.done
    age int @stream.not_null
    score float
}",
    );
    insta::assert_snapshot!(render_tir(&db, file));
}

// ── Cross-file ──────────────────────────────────────────────────────────────

#[test]
fn cross_file_class_reference() {
    let mut db = make_db();
    db.add_file(
        "inner.baml",
        "\
class Education {
    school string
    year int
}",
    );
    let file = db.add_file(
        "test.baml",
        "\
class Resume {
    name string
    education Education[]
}",
    );
    insta::assert_snapshot!(render_tir(&db, file));
}

// ── @@dynamic passthrough ───────────────────────────────────────────────────

#[test]
fn dynamic_attr_passes_through() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        "\
class DynamicClass {
    name string
    @@dynamic
}",
    );
    insta::assert_snapshot!(render_tir(&db, file));
}
