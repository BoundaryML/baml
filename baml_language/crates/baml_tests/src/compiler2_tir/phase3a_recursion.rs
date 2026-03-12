//! Phase 3B-18/19 cycle validation tests.
//!
//! Tests that invalid (unguarded) type alias cycles produce diagnostics,
//! while valid recursive types (guarded by containers) are accepted.

use super::support::{make_db, render_tir};

// ── 3B-18/19. Cycle validation ───────────────────────────────────────────

#[test]
fn type_alias_direct_self_reference() {
    // type A = A — direct self-reference with no base case.
    let mut db = make_db();
    let file = db.add_file("test.baml", "type A = A");
    insta::assert_snapshot!(render_tir(&db, file), @r"
    type user.A = user.A
      !! 0..10: recursive type alias cycle: A
    type user.stream_A = user.stream_A
      !! 0..0: recursive type alias cycle: stream_A
    ");
}

#[test]
fn type_alias_mutual_recursion() {
    // type A = B, type B = A — mutual cycle with no base case.
    let mut db = make_db();
    let file = db.add_file("test.baml", "type A = B\ntype B = A");
    insta::assert_snapshot!(render_tir(&db, file), @r"
    type user.A = user.B
      !! 0..10: recursive type alias cycle: A
    type user.B = user.A
      !! 10..21: recursive type alias cycle: B
    type user.stream_A = user.stream_B
      !! 0..0: recursive type alias cycle: stream_A
    type user.stream_B = user.stream_A
      !! 0..0: recursive type alias cycle: stream_B
    ");
}

#[test]
fn type_alias_indirect_cycle_three() {
    // type A = B, type B = C, type C = A — three-way cycle with no base case.
    let mut db = make_db();
    let file = db.add_file("test.baml", "type A = B\ntype B = C\ntype C = A");
    insta::assert_snapshot!(render_tir(&db, file), @r"
    type user.A = user.B
      !! 0..10: recursive type alias cycle: A
    type user.B = user.C
      !! 10..21: recursive type alias cycle: B
    type user.C = user.A
      !! 21..32: recursive type alias cycle: C
    type user.stream_A = user.stream_B
      !! 0..0: recursive type alias cycle: stream_A
    type user.stream_B = user.stream_C
      !! 0..0: recursive type alias cycle: stream_B
    type user.stream_C = user.stream_A
      !! 0..0: recursive type alias cycle: stream_C
    ");
}

#[test]
fn type_alias_valid_recursive_via_container() {
    // type JSON = string | int | JSON[] — valid recursive type (guarded by container).
    // No cycle diagnostic should be emitted.
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        "type JSON = string | int | bool | null | JSON[] | map<string, JSON>",
    );
    insta::assert_snapshot!(render_tir(&db, file), @r"
    type user.JSON = string | int | bool | null | user.JSON[] | map<string, user.JSON>
    type user.stream_JSON = string | int | bool | null | user.stream_JSON[] | map<string, user.stream_JSON>
    ");
}

#[test]
fn type_alias_cycle_used_in_function() {
    // Cycle in alias used as function param/return — cycle diagnostic on the alias.
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        "type Loop = Loop\nfunction f(x: Loop) -> Loop { return x; }",
    );
    insta::assert_snapshot!(render_tir(&db, file), @r"
    type user.Loop = user.Loop
      !! 0..16: recursive type alias cycle: Loop
    function user.f(x: user.Loop) -> user.Loop {
      { : never
        return x : user.Loop
      }
    }
    type user.stream_Loop = user.stream_Loop
      !! 0..0: recursive type alias cycle: stream_Loop
    ");
}

#[test]
fn class_field_self_reference() {
    // class Node { next Node } — required self-reference.
    // Unconstructable: you can't build a Node without already having a Node.
    let mut db = make_db();
    let file = db.add_file("test.baml", "class Node { next Node }");
    insta::assert_snapshot!(render_tir(&db, file), @r"
    class user.Node {
      next: user.Node
    }
      !! 0..24: class cycle: Node
    class user.stream_Node {
      next: null | user.stream_Node
    }
    ");
}

#[test]
fn class_field_mutual_reference() {
    // class Husband { wife Wife }, class Wife { husband Husband }
    // Both fields required — unconstructable mutual cycle.
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        "class Husband { wife Wife }\nclass Wife { husband Husband }",
    );
    insta::assert_snapshot!(render_tir(&db, file), @r"
    class user.Husband {
      wife: user.Wife
    }
      !! 0..27: class cycle: Husband -> Wife -> Husband
    class user.Wife {
      husband: user.Husband
    }
      !! 27..58: class cycle: Husband -> Wife -> Husband
    class user.stream_Husband {
      wife: null | user.stream_Wife
    }
    class user.stream_Wife {
      husband: null | user.stream_Husband
    }
    ");
}

// ── Edge cases: Optional / Union guardedness ──────────────────────────────
//
// Key question: does Optional or Union provide a structural base case?
// V1 says NO — only List and Map are structural.
// These tests document the expected behavior.

#[test]
fn type_alias_optional_self_reference() {
    // type A = A? — is Optional a structural guard?
    // Optional does NOT provide structural termination — this is invalid.
    let mut db = make_db();
    let file = db.add_file("test.baml", "type A = A?");
    insta::assert_snapshot!(render_tir(&db, file), @r"
    type user.A = user.A?
      !! 0..11: recursive type alias cycle: A
    type user.stream_A = user.stream_A | null
      !! 0..0: recursive type alias cycle: stream_A
    ");
}

#[test]
fn type_alias_union_with_base_case() {
    // type A = A | string — Union with a non-recursive base case.
    // Union does NOT provide structural termination — this is invalid.
    let mut db = make_db();
    let file = db.add_file("test.baml", "type A = A | string");
    insta::assert_snapshot!(render_tir(&db, file), @r"
    type user.A = user.A | string
      !! 0..19: recursive type alias cycle: A
    type user.stream_A = user.stream_A | string
      !! 0..0: recursive type alias cycle: stream_A
    ");
}

#[test]
fn type_alias_list_in_union() {
    // type A = A[] | string — List-guarded recursive in union.
    // List provides structural guard — this is valid.
    let mut db = make_db();
    let file = db.add_file("test.baml", "type A = A[] | string");
    insta::assert_snapshot!(render_tir(&db, file), @r"
    type user.A = user.A[] | string
    type user.stream_A = user.stream_A[] | string
    ");
}

#[test]
fn type_alias_optional_list_self_reference() {
    // type A = A[]? — List then Optional.
    // List provides structural guard — this is valid.
    let mut db = make_db();
    let file = db.add_file("test.baml", "type A = A[]?");
    insta::assert_snapshot!(render_tir(&db, file), @r"
    type user.A = user.A[]?
    type user.stream_A = user.stream_A[] | null
    ");
}

#[test]
fn type_alias_map_in_union() {
    // type A = map<string, A> | string — Map-guarded recursive in union.
    // Map provides structural guard — this is valid.
    let mut db = make_db();
    let file = db.add_file("test.baml", "type A = map<string, A> | string");
    insta::assert_snapshot!(render_tir(&db, file), @r"
    type user.A = map<string, user.A> | string
    type user.stream_A = map<string, user.stream_A> | string
    ");
}

#[test]
fn type_alias_mutual_cycle_through_optional() {
    // type A = B?, type B = A — mutual cycle through Optional.
    // Optional does NOT provide structural termination — both are invalid.
    let mut db = make_db();
    let file = db.add_file("test.baml", "type A = B?\ntype B = A");
    insta::assert_snapshot!(render_tir(&db, file), @r"
    type user.A = user.B?
      !! 0..11: recursive type alias cycle: A
    type user.B = user.A
      !! 11..22: recursive type alias cycle: B
    type user.stream_A = user.stream_B | null
      !! 0..0: recursive type alias cycle: stream_A
    type user.stream_B = user.stream_A
      !! 0..0: recursive type alias cycle: stream_B
    ");
}

#[test]
fn type_alias_mutual_cycle_through_list() {
    // type A = B[], type B = A — mutual cycle through List.
    // List provides structural guard — both are valid.
    let mut db = make_db();
    let file = db.add_file("test.baml", "type A = B[]\ntype B = A");
    insta::assert_snapshot!(render_tir(&db, file), @r"
    type user.A = user.B[]
    type user.B = user.A
    type user.stream_A = user.stream_B[]
    type user.stream_B = user.stream_A
    ");
}

// ── Class required-field cycle validation ─────────────────────────────────
//
// Classes with required (non-optional, non-list, non-map) fields that form
// a cycle are impossible to construct. These should produce diagnostics.
// Optional, list, and map fields break the cycle since they can be
// null/empty.

#[test]
fn class_required_field_mutual_cycle() {
    // class A { b B }, class B { a A } — both fields required.
    // Impossible to construct either.
    let mut db = make_db();
    let file = db.add_file("test.baml", "class A { b B }\nclass B { a A }");
    insta::assert_snapshot!(render_tir(&db, file), @r"
    class user.A {
      b: user.B
    }
      !! 0..15: class cycle: A -> B -> A
    class user.B {
      a: user.A
    }
      !! 15..31: class cycle: A -> B -> A
    class user.stream_A {
      b: null | user.stream_B
    }
    class user.stream_B {
      a: null | user.stream_A
    }
    ");
}

#[test]
fn class_required_field_self_cycle() {
    // class A { self_ref A } — required self-reference.
    // Impossible to construct.
    let mut db = make_db();
    let file = db.add_file("test.baml", "class A { self_ref A }");
    insta::assert_snapshot!(render_tir(&db, file), @r"
    class user.A {
      self_ref: user.A
    }
      !! 0..22: class cycle: A
    class user.stream_A {
      self_ref: null | user.stream_A
    }
    ");
}

#[test]
fn class_required_field_three_way_cycle() {
    // class A { b B }, class B { c C }, class C { a A } — three-way required cycle.
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        "class A { b B }\nclass B { c C }\nclass C { a A }",
    );
    insta::assert_snapshot!(render_tir(&db, file), @r"
    class user.A {
      b: user.B
    }
      !! 0..15: class cycle: A -> B -> C -> A
    class user.B {
      c: user.C
    }
      !! 15..31: class cycle: A -> B -> C -> A
    class user.C {
      a: user.A
    }
      !! 31..47: class cycle: A -> B -> C -> A
    class user.stream_A {
      b: null | user.stream_B
    }
    class user.stream_B {
      c: null | user.stream_C
    }
    class user.stream_C {
      a: null | user.stream_A
    }
    ");
}

#[test]
fn class_optional_field_breaks_cycle() {
    // class A { b B? }, class B { a A } — optional field breaks the cycle.
    // A can be constructed with b = null, then B can use that A.
    // Should NOT be an error.
    let mut db = make_db();
    let file = db.add_file("test.baml", "class A { b B? }\nclass B { a A }");
    insta::assert_snapshot!(render_tir(&db, file), @r"
    class user.A {
      b: user.B?
    }
    class user.B {
      a: user.A
    }
    class user.stream_A {
      b: null | user.stream_B | null
    }
    class user.stream_B {
      a: null | user.stream_A
    }
    ");
}

#[test]
fn class_list_field_breaks_cycle() {
    // class A { bs B[] }, class B { a A } — list field breaks the cycle.
    // A can be constructed with bs = [], then B can use that A.
    // Should NOT be an error.
    let mut db = make_db();
    let file = db.add_file("test.baml", "class A { bs B[] }\nclass B { a A }");
    insta::assert_snapshot!(render_tir(&db, file), @r"
    class user.A {
      bs: user.B[]
    }
    class user.B {
      a: user.A
    }
    class user.stream_A {
      bs: never[] | user.stream_B[]
    }
    class user.stream_B {
      a: null | user.stream_A
    }
    ");
}

#[test]
fn class_map_field_breaks_cycle() {
    // class A { bm map<string, B> }, class B { a A } — map field breaks the cycle.
    // A can be constructed with bm = {}, then B can use that A.
    // Should NOT be an error.
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        "class A { bm map<string, B> }\nclass B { a A }",
    );
    insta::assert_snapshot!(render_tir(&db, file), @r"
    class user.A {
      bm: map<string, user.B>
    }
    class user.B {
      a: user.A
    }
    class user.stream_A {
      bm: map<string, never> | map<string, user.stream_B>
    }
    class user.stream_B {
      a: null | user.stream_A
    }
    ");
}

#[test]
fn class_cycle_through_type_alias() {
    // class A { b AliasB }, type AliasB = B, class B { a A }
    // The alias is transparent — this is still a required cycle.
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        "class A { b AliasB }\ntype AliasB = B\nclass B { a A }",
    );
    insta::assert_snapshot!(render_tir(&db, file), @r"
    class user.A {
      b: user.AliasB
    }
      !! 0..20: class cycle: A -> B -> A
    type user.AliasB = user.B
    class user.B {
      a: user.A
    }
      !! 36..52: class cycle: A -> B -> A
    class user.stream_A {
      b: null | user.stream_AliasB
    }
    class user.stream_B {
      a: null | user.stream_A
    }
    type user.stream_AliasB = user.stream_B
    ");
}

#[test]
fn class_cycle_broken_by_alias_to_optional() {
    // class A { b AliasB }, type AliasB = B?, class B { a A }
    // The alias resolves to B? which is optional — breaks the cycle.
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        "class A { b AliasB }\ntype AliasB = B?\nclass B { a A }",
    );
    insta::assert_snapshot!(render_tir(&db, file), @r"
    class user.A {
      b: user.AliasB
    }
    type user.AliasB = user.B?
    class user.B {
      a: user.A
    }
    class user.stream_A {
      b: null | user.stream_AliasB
    }
    class user.stream_B {
      a: null | user.stream_A
    }
    type user.stream_AliasB = user.stream_B | null
    ");
}

#[test]
fn class_union_field_all_variants_same_class() {
    // class A { b B | B }, class B { a A }
    // Union where ALL variants resolve to the same class — still a hard dependency.
    let mut db = make_db();
    let file = db.add_file("test.baml", "class A { b B | B }\nclass B { a A }");
    insta::assert_snapshot!(render_tir(&db, file), @r"
    class user.A {
      b: user.B | user.B
    }
      !! 0..19: class cycle: A -> B -> A
    class user.B {
      a: user.A
    }
      !! 19..35: class cycle: A -> B -> A
    class user.stream_A {
      b: null | user.stream_B | user.stream_B
    }
    class user.stream_B {
      a: null | user.stream_A
    }
    ");
}

#[test]
fn class_union_field_different_variants_breaks_cycle() {
    // class A { b B | string }, class B { a A }
    // Union has a non-class variant (string) — can choose that to break cycle.
    // Should NOT be an error.
    let mut db = make_db();
    let file = db.add_file("test.baml", "class A { b B | string }\nclass B { a A }");
    insta::assert_snapshot!(render_tir(&db, file), @r"
    class user.A {
      b: user.B | string
    }
    class user.B {
      a: user.A
    }
    class user.stream_A {
      b: null | user.stream_B | string
    }
    class user.stream_B {
      a: null | user.stream_A
    }
    ");
}
