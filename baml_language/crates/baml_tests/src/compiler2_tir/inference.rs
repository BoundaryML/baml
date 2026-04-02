//! Core type inference snapshot tests.

use super::support::{make_db, render_tir};

#[test]
fn literal_int() {
    let mut db = make_db();
    let file = db.add_file("test.baml", "function f() -> int { return 1; }");
    insta::assert_snapshot!(render_tir(&db, file), @r"
    function user.f() -> int throws never {
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
    function user.f() -> int throws never {
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
    function user.f(x: user.Foo) -> string throws never {
      { : never
        return x.name : string
      }
    }
    class user.Foo$stream {
      name: null | string
    }
    ");
}

#[test]
fn type_mismatch() {
    let mut db = make_db();
    let file = db.add_file("test.baml", "function f() -> string { return 1; }");
    insta::assert_snapshot!(render_tir(&db, file), @r"
    function user.f() -> string throws never {
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
    function user.f(x: user.Foo) -> string throws never {
      { : never
        return x.missing : unknown
      }
      !! 66..73: type `user.Foo` has no member `missing`
    }
    class user.Foo$stream {
      name: null | string
    }
    ");
}

#[test]
fn unresolved_field_chained_access() {
    // Test: in `data.inner.foo`, if `inner` doesn't exist on the class,
    // the squiggly should only cover "inner", not "data.inner".
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        "\
class Data {
  name string
}
function f(data: Data) -> string {
  return data.inner.foo;
}",
    );
    insta::assert_snapshot!(render_tir(&db, file), @r"
    class user.Data {
      name: string
    }
    function user.f(data: user.Data) -> string throws never {
      { : never
        return data.inner.foo : unknown
      }
      !! 78..83: type `user.Data` has no member `inner`
    }
    class user.Data$stream {
      name: null | string
    }
    ");
}

#[test]
fn unresolved_field_span_should_narrow_to_member() {
    // Regression test: the diagnostic span for an unresolved member should cover
    // only the member name ("feelin"), not the entire expression ("user.Sentiment.feelin").
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        "\
class Sentiment {
  feeling string
}
function f(s: Sentiment) -> string {
  return s.feelin;
}",
    );
    insta::assert_snapshot!(render_tir(&db, file), @r"
    class user.Sentiment {
      feeling: string
    }
    function user.f(s: user.Sentiment) -> string throws never {
      { : never
        return s.feelin : unknown
      }
      !! 85..91: type `user.Sentiment` has no member `feelin`
    }
    class user.Sentiment$stream {
      feeling: null | string
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
    function user.f(a: int, b: int) -> int throws never {
      { : never
        return a + b : int
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
    function user.f(x: bool) -> int throws never {
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
    function user.f() -> user.Color throws never {
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
    class user.Point$stream {
      x: null | int
      y: null | float
      label: null | string
    }
    ");
}

#[test]
fn resolve_type_alias_query() {
    let mut db = make_db();
    let file = db.add_file("test.baml", "type MyStr = string");
    insta::assert_snapshot!(render_tir(&db, file), @r"
    type user.MyStr = string
    type user.MyStr$stream = string
    ");
}

#[test]
fn two_functions_independent() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        "function ok() -> int { return 1; }\nfunction bad() -> string { return 42; }",
    );
    insta::assert_snapshot!(render_tir(&db, file), @r"
    function user.ok() -> int throws never {
      { : never
        return 1 : 1
      }
    }
    function user.bad() -> string throws never {
      { : never
        return 42 : 42
      }
      !! 69..71: type mismatch: expected string, got 42
    }
    ");
}
