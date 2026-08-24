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

// ── Bug 1: pending_default alias resolution ─────────────────────────────────

#[test]
fn type_alias_to_list_in_union_gets_correct_pending_default() {
    // type Ints = int[] — in a union, pending_default should resolve the alias
    // and return EmptyArray (not Null). This means the stream expansion should
    // NOT prepend an extra null to the union.
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        "\
type Ints = int[]

class WithAliasUnion {
    data Ints | string
}",
    );
    insta::assert_snapshot!(render_tir(&db, file));
}

#[test]
fn chained_alias_to_list_in_union() {
    // A -> B -> int[] — chained aliases should also resolve correctly
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        "\
type IntList = int[]
type MyList = IntList

class WithChainedAlias {
    data MyList | string
}",
    );
    insta::assert_snapshot!(render_tir(&db, file));
}

#[test]
fn type_alias_to_map_in_union_gets_empty_map_default() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        "\
type Config = map<string, string>

class WithMapAlias {
    settings Config | int
}",
    );
    insta::assert_snapshot!(render_tir(&db, file));
}

// ── Bug 2: field attributes lost on $stream ─────────────────────────────────

#[test]
fn field_alias_preserved_on_stream_class() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"
class WithAlias {
    bar string @alias("baz")
    count int @alias("cnt")
}
"#,
    );
    insta::assert_snapshot!(render_tir(&db, file));
}

#[test]
fn field_description_preserved_stream_done_stripped() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"
class WithDesc {
    name string @description("The name") @stream.done
    age int @description("Age in years")
}
"#,
    );
    insta::assert_snapshot!(render_tir(&db, file));
}

// ── Bug 3: package-scoped keys ──────────────────────────────────────────────
// Cross-package collision tests require multi-package test infrastructure.
// Covered by manual verification: ensure @@stream.* attrs from builtins
// don't leak into user types with the same name.
// TODO: Add project-based test in projects/stream_crosspackage/ once
// multi-package test support is available.

// ── Generic args threaded through stream-expanded references ────────────────

#[test]
fn stream_companion_preserves_generic_args_in_class_field() {
    // `Container.inner: Box<int>` should round-trip into `Container$stream`
    // with `inner: null | Box$stream<int>`. Without threading `generic_args`
    // through PPIR's stream rewrite, `Box$stream` would lose its arg and
    // mismatch the synthesized `Box$stream<T>` arity.
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"
class Box<T> {
    value T
}

class Container {
    inner Box<int>
}
"#,
    );
    insta::assert_snapshot!(render_tir(&db, file));
}

#[test]
fn stream_companion_preserves_generic_args_for_llm_return_type() {
    // Reviewer's exact scenario: an LLM function returning `Box<int>` pulls
    // the stream-expanded `Box$stream<int>` type companion into play. With
    // generic-arg threading, the reference matches `Box$stream<T>`'s arity.
    // (The legacy LLM `$stream`/`$parse_stream` function companions are gone;
    // the class-level `$stream` type expansion is what this pins now.)
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"
class Box<T> {
    value T
}

client Dummy = openai.ResponsesClient.new(model = "gpt-4");

function GetBoxedInt() -> Box<int> {
    client: Dummy
    prompt: `Give me a box`
}
"#,
    );
    insta::assert_snapshot!(render_tir(&db, file));
}
