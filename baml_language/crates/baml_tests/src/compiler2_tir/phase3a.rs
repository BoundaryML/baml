//! Phase 3A must-fix gap tests.
//!
//! Each test documents a gap from the Phase 3A checklist. Snapshots capture
//! the current (possibly incorrect) behavior so regressions are visible
//! as the gaps get fixed.

use super::support::{make_db, render_tir};

// ── 3A-1. Union normalization ────────────────────────────────────────────

#[test]
fn union_normalization_deduplicates() {
    let mut db = make_db();
    let file = db.add_file("test.baml", "function f(x: int | int) -> int { return x; }");
    insta::assert_snapshot!(render_tir(&db, file), @r"
    function user.f(x: int | int) -> int {
      { : never
        return x : int | int
      }
    }
    ");
}

#[test]
fn union_normalization_alias() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        "type A = int | string\nfunction f(x: A) -> string { return x; }",
    );
    insta::assert_snapshot!(render_tir(&db, file), @r"
    type user.A = int | string
    function user.f(x: user.A) -> string {
      { : never
        return x : user.A
      }
      !! 58..59: type mismatch: expected string, got user.A
    }
    ");
}

// ── 3A-2. UnknownType diagnostic ─────────────────────────────────────────

#[test]
fn unknown_type_in_param() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        "function f(x: Nonexistent) -> int { return 0; }",
    );
    insta::assert_snapshot!(render_tir(&db, file), @r"
    function user.f(x: unknown) -> int {
      { : never
        return 0 : 0
      }
      !! 11..25: unresolved type: Nonexistent
    }
    ");
}

#[test]
fn unknown_type_in_return() {
    let mut db = make_db();
    let file = db.add_file("test.baml", "function f() -> DoesNotExist { return 0; }");
    insta::assert_snapshot!(render_tir(&db, file), @r"
    function user.f() -> unknown {
      { : never
        return 0 : 0
      }
      !! 15..28: unresolved type: DoesNotExist
    }
    ");
}

// ── 3A-3. UnresolvedName diagnostic ──────────────────────────────────────

#[test]
fn unresolved_variable() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        "function f() -> int { return nonexistent_var; }",
    );
    insta::assert_snapshot!(render_tir(&db, file), @r"
    function user.f() -> int {
      { : never
        return nonexistent_var : unknown
      }
      !! 29..44: unresolved name: nonexistent_var
    }
    ");
}

#[test]
fn unresolved_variable_in_let() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        "function f() -> int { let x = unknown_thing; return x; }",
    );
    insta::assert_snapshot!(render_tir(&db, file), @r"
    function user.f() -> int {
      { : never
        let x = unknown_thing : unknown
        return x : unknown
      }
      !! 30..43: unresolved name: unknown_thing
    }
    ");
}

// ── 3A-4. ArgumentCountMismatch diagnostic ───────────────────────────────

#[test]
fn too_many_args() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        "function add(a: int, b: int) -> int { return a + b; }\nfunction f() -> int { return add(1, 2, 3); }",
    );
    insta::assert_snapshot!(render_tir(&db, file), @r"
    function user.add(a: int, b: int) -> int {
      { : never
        return a Add b : int
      }
    }
    function user.f() -> int {
      { : never
        return add(1, 2, 3) : int
      }
      !! 83..95: expected 2 argument(s), got 3
    }
    ");
}

#[test]
fn too_few_args() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        "function add(a: int, b: int) -> int { return a + b; }\nfunction f() -> int { return add(1); }",
    );
    insta::assert_snapshot!(render_tir(&db, file), @r"
    function user.add(a: int, b: int) -> int {
      { : never
        return a Add b : int
      }
    }
    function user.f() -> int {
      { : never
        return add(1) : int
      }
      !! 83..89: expected 2 argument(s), got 1
    }
    ");
}

// ── 3A-5. NotCallable diagnostic ─────────────────────────────────────────

#[test]
fn calling_non_function() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        "function f() -> int { let x = 42; return x(1); }",
    );
    insta::assert_snapshot!(render_tir(&db, file), @r"
    function user.f() -> int {
      { : never
        let x = 42 : 42 -> int
        return x(1) : unknown
      }
      !! 41..45: type `int` is not callable
    }
    ");
}

#[test]
fn calling_class_as_function() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        "class Foo { name string }\nfunction f() -> int { return Foo(1); }",
    );
    insta::assert_snapshot!(render_tir(&db, file), @r"
    class user.Foo {
      name: string
    }
    function user.f() -> int {
      { : never
        return Foo(1) : unknown
      }
      !! 55..61: type `user.Foo` is not callable
    }
    ");
}

// ── 3A-6. MissingReturnExpression diagnostic ─────────────────────────────

#[test]
fn missing_return() {
    let mut db = make_db();
    let file = db.add_file("test.baml", "function f() -> int { let x = 1; }");
    insta::assert_snapshot!(render_tir(&db, file), @r"
    function user.f() -> int {
      { : int
        let x = 1 : 1 -> int
      }
      !! 19..34: missing return: expected `int`
    }
    ");
}

#[test]
fn block_ending_in_stmt() {
    let mut db = make_db();
    let file = db.add_file("test.baml", "function f() -> string { let x = \"hello\"; }");
    insta::assert_snapshot!(render_tir(&db, file), @r#"
    function user.f() -> string {
      { : string
        let x = "hello" : "hello" -> string
      }
      !! 22..43: missing return: expected `string`
    }
    "#);
}

// ── 3A-7. InvalidBinaryOp / InvalidUnaryOp diagnostics ──────────────────

#[test]
fn invalid_binary_op_string_minus_int() {
    let mut db = make_db();
    let file = db.add_file("test.baml", "function f() -> int { return \"hello\" - 5; }");
    insta::assert_snapshot!(render_tir(&db, file), @r#"
    function user.f() -> int {
      { : never
        return "hello" Sub 5 : unknown
      }
      !! 28..40: operator `Sub` cannot be applied to `"hello"` and `5`
    }
    "#);
}

#[test]
fn invalid_binary_op_bool_add() {
    let mut db = make_db();
    let file = db.add_file("test.baml", "function f() -> int { return true + false; }");
    insta::assert_snapshot!(render_tir(&db, file), @r"
    function user.f() -> int {
      { : never
        return true Add false : unknown
      }
      !! 29..41: operator `Add` cannot be applied to `true` and `false`
    }
    ");
}

#[test]
fn invalid_unary_op_neg_string() {
    let mut db = make_db();
    let file = db.add_file("test.baml", "function f() -> int { return -\"hello\"; }");
    insta::assert_snapshot!(render_tir(&db, file), @r#"
    function user.f() -> int {
      { : never
        return Neg "hello" : unknown
      }
      !! 28..37: operator `Neg` cannot be applied to `"hello"`
    }
    "#);
}

// ── 3A-8. NotIndexable diagnostic ────────────────────────────────────────

#[test]
fn indexing_bool() {
    let mut db = make_db();
    let file = db.add_file("test.baml", "function f(x: bool) -> int { return x[0]; }");
    insta::assert_snapshot!(render_tir(&db, file), @r"
    function user.f(x: bool) -> int {
      { : never
        return x[0] : unknown
      }
      !! 36..40: type `bool` is not indexable
    }
    ");
}

#[test]
fn indexing_int() {
    let mut db = make_db();
    let file = db.add_file("test.baml", "function f(x: int) -> int { return x[0]; }");
    insta::assert_snapshot!(render_tir(&db, file), @r"
    function user.f(x: int) -> int {
      { : never
        return x[0] : unknown
      }
      !! 35..39: type `int` is not indexable
    }
    ");
}

// ── 3A-9. FloatLiteral in TypeExpr ───────────────────────────────────────

#[test]
fn float_literal_in_annotation() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        "function f(x: 3.14 | 2.72) -> float { return x; }",
    );
    insta::assert_snapshot!(render_tir(&db, file), @r"
    function user.f(x: 3.14 | 2.72) -> float {
      { : never
        return x : 3.14 | 2.72
      }
    }
    ");
}

// ── 3A-10. if-without-else should produce Optional(T) ────────────────────

#[test]
fn if_without_else_optional() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        "function f(x: bool) -> int? { return if (x) { 5 }; }",
    );
    insta::assert_snapshot!(render_tir(&db, file), @r"
    function user.f(x: bool) -> int? {
      { : never
        return : void
          if (x : bool) : void
            { : 5
              5 : 5
            }
      }
      !! 36..49: `if` without `else` cannot be used as a value; add an `else` branch
    }
    ");
}

#[test]
fn if_without_else_let_binding() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        "function f(x: bool) -> int { let y = if (x) { 5 }; return y ?? 0; }",
    );
    insta::assert_snapshot!(render_tir(&db, file), @r"
    function user.f(x: bool) -> int {
      { : never
        let y = : void
          if (x : bool) : void
            { : 5
              5 : 5
            }
        return y : void
        0 : unknown
      }
      !! 36..49: `if` without `else` cannot be used as a value; add an `else` branch
      !! 58..59: `if` without `else` cannot be used as a value; add an `else` branch
      !! 50..59: unreachable code: 1 statement(s) after diverging statement
    }
    ");
}

// ── 3A-11. Match expression: pattern binding + scrutinee narrowing ───────

#[test]
fn match_enum_variants() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"enum Color { Red
Green
Blue }
function f(x: Color) -> string {
  return match (x) {
    Color.Red => "red"
    Color.Green => "green"
    Color.Blue => "blue"
  };
}"#,
    );
    insta::assert_snapshot!(render_tir(&db, file), @r#"
    enum user.Color
    function user.f(x: user.Color) -> string {
      { : never
        return : "red" | "green" | "blue"
          match (x : user.Color) : "red" | "green" | "blue"
            Color.Red =>
              "red" : "red"
            Color.Green =>
              "green" : "green"
            Color.Blue =>
              "blue" : "blue"
      }
    }
    "#);
}

#[test]
fn match_catch_all() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"function f(x: int) -> int {
  return match (x) {
    y => y + 1
  };
}"#,
    );
    insta::assert_snapshot!(render_tir(&db, file), @r"
    function user.f(x: int) -> int {
      { : never
        return : int
          match (x : int) : int
            y =>
              y Add 1 : int
      }
    }
    ");
}

// ── 3A-12. Union member field access ─────────────────────────────────────

#[test]
fn union_field_access_shared() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"class Cat { name string
legs int }
class Dog { name string
legs int }
function f(x: Cat | Dog) -> string { return x.name; }"#,
    );
    insta::assert_snapshot!(render_tir(&db, file), @r"
    class user.Cat {
      name: string
      legs: int
    }
    class user.Dog {
      name: string
      legs: int
    }
    function user.f(x: user.Cat | user.Dog) -> string {
      { : never
        return x.name : string | string
      }
    }
    ");
}

#[test]
fn union_field_access_missing_on_some() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"class Cat { name string
whiskers int }
class Dog { name string
tail bool }
function f(x: Cat | Dog) -> int { return x.whiskers; }"#,
    );
    insta::assert_snapshot!(render_tir(&db, file), @r"
    class user.Cat {
      name: string
      whiskers: int
    }
    class user.Dog {
      name: string
      tail: bool
    }
    function user.f(x: user.Cat | user.Dog) -> int {
      { : never
        return x.whiskers : unknown
      }
      !! 115..126: unresolved member: user.Dog.whiskers
    }
    ");
}

#[test]
fn union_field_access_missing_on_one_of_three() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"class A { name string }
class B { name string }
class C { age int }
function f(x: A | B | C) -> string { return x.name; }"#,
    );
    // C has no `name` field → error on the whole union
    insta::assert_snapshot!(render_tir(&db, file), @r"
    class user.A {
      name: string
    }
    class user.B {
      name: string
    }
    class user.C {
      age: int
    }
    function user.f(x: user.A | user.B | user.C) -> string {
      { : never
        return x.name : unknown
      }
      !! 111..118: unresolved member: user.C.name
    }
    ");
}

#[test]
fn union_field_access_missing_on_two_of_three() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"class A { name string }
class B { age string }
class C { age int }
function f(x: A | B | C) -> string { return x.name; }"#,
    );
    // C has no `name` field → error on the whole union
    insta::assert_snapshot!(render_tir(&db, file), @r"
    class user.A {
      name: string
    }
    class user.B {
      age: string
    }
    class user.C {
      age: int
    }
    function user.f(x: user.A | user.B | user.C) -> string {
      { : never
        return x.name : unknown
      }
      !! 110..117: unresolved member: user.B.name
      !! 110..117: unresolved member: user.C.name
    }
    ");
}

#[test]
fn union_field_access_different_types() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"class A { value int }
class B { value string }
function f(x: A | B) -> string { return x.value; }"#,
    );
    // Both have `value` but different types → union of field types
    insta::assert_snapshot!(render_tir(&db, file), @r"
    class user.A {
      value: int
    }
    class user.B {
      value: string
    }
    function user.f(x: user.A | user.B) -> string {
      { : never
        return x.value : int | string
      }
      !! 86..94: type mismatch: expected string, got int | string
    }
    ");
}

#[test]
fn union_field_access_optional_member() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"class A { name string }
class B { name string }
function f(x: A | B | null) -> string { return x.name; }"#,
    );
    // null in union → can't access field (needs narrowing first)
    insta::assert_snapshot!(render_tir(&db, file), @r"
    class user.A {
      name: string
    }
    class user.B {
      name: string
    }
    function user.f(x: user.A | user.B | null) -> string {
      { : never
        return x.name : unknown
      }
      !! 94..101: unresolved member: null.name
    }
    ");
}
