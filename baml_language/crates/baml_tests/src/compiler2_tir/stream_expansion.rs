//! Stream type expansion snapshots for the PPIR → HIR → TIR pipeline.
//!
//! The six matrices below preserve the original twenty-three scenarios while
//! making each semantic case visible through descriptive declarations.

use super::support::{make_db, render_tir};
use crate::engine::TestDbExt;

#[test]
fn stream_default_expansion_matrix() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
// primitives_get_null_union
class Primitives {
    name string
    count int
    flag bool
    score float
}

// enum_field_unchanged
enum Status {
    Active
    Inactive
}
class WithEnum {
    status Status
}

class DefaultInner {
    value string
}

// class_field_gets_stream_prefix
class ClassFieldOuter {
    inner DefaultInner
}

// literal_fields_unchanged
class WithLiterals {
    type "resume"
    version 1
    enabled true
}

// list_field_recurses
class WithLists {
    strings string[]
    ints int[]
    classes DefaultInner[]
}

// map_field_recurses_value
class WithMaps {
    simple map<string, int>
    complex map<string, DefaultInner>
}

// union_field_recurses_variants
class WithUnions {
    simple int | string
    with_class DefaultInner | string
    mixed int | DefaultInner
}

// optional_expands_to_union_with_null
class WithOptionals {
    name string?
    inner DefaultInner?
}

// type_alias_expansion
type SimpleAlias = string
type ClassAlias = DefaultInner
type UnionAlias = int | DefaultInner
type OptionalAlias = DefaultInner?

// recursive_class
class TreeNode {
    value string
    children TreeNode[]
}
"#,
    );
    insta::assert_snapshot!(render_tir(&db, file));
}

#[test]
fn stream_annotation_semantics_matrix() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
// stream_done_field_keeps_type_as_is
class WithDone {
    name string @stream.done
    age int
}

// stream_done_block_attr. The holder is intentional: block attributes are
// consumed when the annotated class is reached through a referenced type path.
class AtomicPoint {
    @@stream.done
    x float
    y float
}
class DoneReferences {
    point AtomicPoint
}

// stream_not_null_field, repaired to the supported must_exist spelling.
class WithMustExist {
    gpa float @stream.must_exist
    name string
}

// stream_not_null_block_attr_on_referenced_class, likewise repaired. Both
// singular and list references exercise lookup of the block attribute.
class MustExistEducation {
    @@stream.must_exist
    school string
    year int
}
class MustExistReferences {
    education MustExistEducation
    educations MustExistEducation[]
}

// stream_done_and_not_null, repaired to done + must_exist.
class Combined {
    id string @stream.done @stream.must_exist
    name string @stream.done
    age int @stream.must_exist
    score float
}
"#,
    );
    insta::assert_snapshot!(render_tir(&db, file));
}

#[test]
fn stream_cross_file_reference() {
    let mut db = make_db();
    db.file(
        "inner.baml",
        r#"
class Education {
    school string
    year int
}
"#,
    );
    let file = db.file(
        "test.baml",
        r#"
// cross_file_class_reference
class Resume {
    name string
    education Education[]
}
"#,
    );
    insta::assert_snapshot!(render_tir(&db, file));
}

#[test]
fn stream_pending_default_alias_matrix() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
// type_alias_to_list_in_union_gets_correct_pending_default
type Ints = int[]
class WithAliasUnion {
    data Ints | string
}

// chained_alias_to_list_in_union
type IntList = int[]
type MyList = IntList
class WithChainedAlias {
    data MyList | string
}

// type_alias_to_map_in_union_gets_empty_map_default
type Config = map<string, string>
class WithMapAlias {
    settings Config | int
}
"#,
    );
    insta::assert_snapshot!(render_tir(&db, file));
}

#[test]
fn stream_field_metadata_matrix() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
// field_alias_preserved_on_stream_class
class WithAlias {
    bar string @alias("baz")
    count int @alias("cnt")
}

// field_description_preserved_stream_done_stripped
class WithDesc {
    name string @description("The name") @stream.done
    age int @description("Age in years")
}
"#,
    );
    insta::assert_snapshot!(render_tir(&db, file));
}

#[test]
fn stream_generic_argument_matrix() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
class Box<T> {
    value T
}

// stream_companion_preserves_generic_args_in_class_field
class Container {
    inner Box<int>
}

// stream_companion_preserves_generic_args_for_llm_return_type
client Dummy = openai.ResponsesClient.new(model = "gpt-4")
function GetBoxedInt() -> Box<int> {
    client: Dummy
    prompt: `Give me a box`
}
"#,
    );
    insta::assert_snapshot!(render_tir(&db, file));
}
