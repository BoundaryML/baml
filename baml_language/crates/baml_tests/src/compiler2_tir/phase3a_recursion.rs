//! Phase 3B-18/19 cycle validation tests.
//!
//! Tests that invalid (unguarded) type alias cycles produce diagnostics,
//! while valid recursive types (guarded by containers) are accepted.

use super::support::{make_db, render_tir};
use crate::engine::TestDbExt;

fn render_labeled_cases(cases: &[(&str, &str)]) -> String {
    cases
        .iter()
        .map(|(label, source)| {
            let mut db = make_db();
            let file = db.file("test.baml", source);
            format!("=== {label} ===\n{}", render_tir(&db, file).trim_end())
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

#[test]
fn type_alias_unguarded_scc_matrix() {
    insta::assert_snapshot!(render_labeled_cases(&[
        ("type_alias_direct_self_reference", "type A = A",),
        ("type_alias_mutual_recursion", "type A = B\ntype B = A",),
        (
            "type_alias_indirect_cycle_three",
            "type A = B\ntype B = C\ntype C = A",
        ),
    ]));
}

#[test]
fn invalid_alias_used_downstream() {
    insta::assert_snapshot!(render_labeled_cases(&[(
        "type_alias_cycle_used_in_function",
        "type Loop = Loop\nfunction f(x: Loop) -> Loop { return x; }",
    )]));
}

#[test]
fn type_alias_nonstructural_guard_matrix() {
    insta::assert_snapshot!(render_labeled_cases(&[
        ("type_alias_optional_self_reference", "type A = A?"),
        ("type_alias_union_with_base_case", "type A = A | string",),
        (
            "type_alias_mutual_cycle_through_optional",
            "type A = B?\ntype B = A",
        ),
    ]));
}

#[test]
fn type_alias_structural_guard_matrix() {
    insta::assert_snapshot!(render_labeled_cases(&[
        (
            "type_alias_valid_recursive_via_container",
            "type JSON = string | int | bool | null | JSON[] | map<string, JSON>",
        ),
        ("type_alias_list_in_union", "type A = A[] | string"),
        ("type_alias_optional_list_self_reference", "type A = A[]?",),
        (
            "type_alias_map_in_union",
            "type A = map<string, A> | string",
        ),
        (
            "type_alias_mutual_cycle_through_list",
            "type A = B[]\ntype B = A",
        ),
    ]));
}

#[test]
fn class_required_cycle_matrix() {
    insta::assert_snapshot!(render_labeled_cases(&[
        (
            "class_field_self_reference + class_required_field_self_cycle (duplicate self-cycle)",
            "class Node { next Node }",
        ),
        (
            "class_field_mutual_reference + class_required_field_mutual_cycle (duplicate two-node cycle)",
            "class Husband { wife Wife }\nclass Wife { husband Husband }",
        ),
        (
            "class_required_field_three_way_cycle",
            "class A { b B }\nclass B { c C }\nclass C { a A }",
        ),
    ]));
}

#[test]
fn class_structural_guard_matrix() {
    insta::assert_snapshot!(render_labeled_cases(&[
        (
            "class_optional_field_breaks_cycle",
            "class A { b B? }\nclass B { a A }",
        ),
        (
            "class_list_field_breaks_cycle",
            "class A { bs B[] }\nclass B { a A }",
        ),
        (
            "class_map_field_breaks_cycle",
            "class A { bm map<string, B> }\nclass B { a A }",
        ),
    ]));
}

#[test]
fn class_cycle_alias_transparency_matrix() {
    insta::assert_snapshot!(render_labeled_cases(&[
        (
            "class_cycle_through_type_alias",
            "class A { b AliasB }\ntype AliasB = B\nclass B { a A }",
        ),
        (
            "class_cycle_broken_by_alias_to_optional",
            "class A { b AliasB }\ntype AliasB = B?\nclass B { a A }",
        ),
    ]));
}

#[test]
fn class_union_cycle_matrix() {
    insta::assert_snapshot!(render_labeled_cases(&[
        (
            "class_union_field_all_variants_same_class",
            "class A { b B | B }\nclass B { a A }",
        ),
        (
            "class_union_field_different_variants_breaks_cycle",
            "class A { b B | string }\nclass B { a A }",
        ),
    ]));
}
