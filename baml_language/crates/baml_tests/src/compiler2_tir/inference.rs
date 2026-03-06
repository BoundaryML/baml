//! Core type inference snapshot tests.

use super::support::{make_db, render_tir};

#[test]
fn literal_int() {
    let mut db = make_db();
    let file = db.add_file("test.baml", "function f() -> int { return 1; }");
    insta::assert_snapshot!(render_tir(&db, file), @r"
    function user.f() -> int {
      { : never
        return 1 : 1
      }
    }
    ");
}

#[test]
fn let_binding_widens() {
    let mut db = make_db();
    let file = db.add_file("test.baml", "function f() -> int { let x = 1; return x; }");
    insta::assert_snapshot!(render_tir(&db, file), @r"
    function user.f() -> int {
      { : never
        let x = 1 : 1 -> int
        return x : int
      }
    }
    ");
}

#[test]
fn class_field_access() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        "class Foo { name string }\nfunction f(x: Foo) -> string { return x.name; }",
    );
    insta::assert_snapshot!(render_tir(&db, file), @r"
    class user.Foo {
      name: string
    }
    function user.f(x: user.Foo) -> string {
      { : never
        return x.name : string
      }
    }
    ");
}

#[test]
fn type_mismatch() {
    let mut db = make_db();
    let file = db.add_file("test.baml", "function f() -> string { return 1; }");
    insta::assert_snapshot!(render_tir(&db, file), @r"
    function user.f() -> string {
      { : never
        return 1 : 1
      }
      !! 32..33: type mismatch: expected string, got 1
    }
    ");
}

#[test]
fn unresolved_field() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        "class Foo { name string }\nfunction f(x: Foo) -> string { return x.missing; }",
    );
    insta::assert_snapshot!(render_tir(&db, file), @r"
    class user.Foo {
      name: string
    }
    function user.f(x: user.Foo) -> string {
      { : never
        return x.missing : unknown
      }
      !! 63..73: unresolved member: user.Foo.missing
    }
    ");
}

#[test]
fn binary_op_int_add() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        "function f(a: int, b: int) -> int { return a + b; }",
    );
    insta::assert_snapshot!(render_tir(&db, file), @r"
    function user.f(a: int, b: int) -> int {
      { : never
        return a Add b : int
      }
    }
    ");
}

#[test]
fn if_else_joins_types() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        "function f(x: bool) -> int { return if (x) { 1 } else { 2 }; }",
    );
    insta::assert_snapshot!(render_tir(&db, file), @r"
    function user.f(x: bool) -> int {
      { : never
        return : 1 | 2
          if (x : bool) : 1 | 2
            { : 1
              1 : 1
            }
          else
            { : 2
              2 : 2
            }
      }
    }
    ");
}

#[test]
fn enum_variant_resolution() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        "enum Color { Red\nGreen\nBlue }\nfunction f() -> Color { return Color.Red; }",
    );
    insta::assert_snapshot!(render_tir(&db, file), @r"
    enum user.Color
    function user.f() -> user.Color {
      { : never
        return Color.Red : user.Color.Red
      }
    }
    ");
}

#[test]
fn resolve_class_fields_query() {
    let mut db = make_db();
    let file = db.add_file("test.baml", "class Point { x int\ny float\nlabel string }");
    insta::assert_snapshot!(render_tir(&db, file), @r"
    class user.Point {
      x: int
      y: float
      label: string
    }
    ");
}

#[test]
fn resolve_type_alias_query() {
    let mut db = make_db();
    let file = db.add_file("test.baml", "type MyStr = string");
    insta::assert_snapshot!(render_tir(&db, file), @"type user.MyStr = string");
}

#[test]
fn two_functions_independent() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        "function ok() -> int { return 1; }\nfunction bad() -> string { return 42; }",
    );
    insta::assert_snapshot!(render_tir(&db, file), @r"
    function user.ok() -> int {
      { : never
        return 1 : 1
      }
    }
    function user.bad() -> string {
      { : never
        return 42 : 42
      }
      !! 69..71: type mismatch: expected string, got 42
    }
    ");
}
