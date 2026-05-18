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
    insta::assert_snapshot!(render_tir(&db, file), @"
    function user.f(x: int | int) -> int throws never {
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
    insta::assert_snapshot!(render_tir(&db, file), @"
    type user.A = int | string
    function user.f(x: user.A) -> string throws never {
      { : never
        return x : user.A
      }
      !! 58..59: type mismatch: expected string, got user.A
    }
    type user.A$stream = int | string
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
    insta::assert_snapshot!(render_tir(&db, file), @"
    function user.f(x: unknown) -> int throws never {
      { : never
        return 0 : 0
      }
      !! 13..25: unresolved type: Nonexistent
    }
    ");
}

#[test]
fn unknown_type_in_return() {
    let mut db = make_db();
    let file = db.add_file("test.baml", "function f() -> DoesNotExist { return 0; }");
    insta::assert_snapshot!(render_tir(&db, file), @"
    function user.f() -> unknown throws never {
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
    insta::assert_snapshot!(render_tir(&db, file), @"
    function user.f() -> int throws never {
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
    insta::assert_snapshot!(render_tir(&db, file), @"
    function user.f() -> int throws never {
      { : never
        let x = unknown_thing : unknown
        return x : unknown
      }
      !! 30..43: unresolved name: unknown_thing
    }
    ");
}

// ── Optional function parameters ─────────────────────────────────────────

#[test]
fn optional_params_accept_omission_and_named_override() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"
function search(query: string, max: int = 10) -> string { query }
function f() -> string {
    let a = search("cats")
    return search("dogs", max = 5)
}
"#,
    );
    let tir = render_tir(&db, file);
    insta::assert_snapshot!("optional_params_accept_omission_and_named_override", tir);
    assert!(
        tir.contains("function user.search(query: string, max: int = 10) -> string"),
        "{tir}"
    );
    assert!(tir.contains("let a = search(\"cats\") : string"), "{tir}");
    assert!(
        tir.contains("return search(\"dogs\", max = 5) : string"),
        "{tir}"
    );
    assert!(!tir.contains("!!"), "unexpected diagnostics:\n{tir}");
}

#[test]
fn optional_param_call_binding_diagnostics() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"
function search(query: string, max: int = 10) -> string { query }
function positional_default() -> string { search("cats", 5) }
function positional_after_named() -> string { search(query = "cats", 5) }
function duplicate_named() -> string { search(query = "cats", max = 1, max = 2) }
function unknown_named() -> string { search(q = "cats") }
"#,
    );
    let tir = render_tir(&db, file);
    insta::assert_snapshot!("optional_param_call_binding_diagnostics", tir);
    assert!(tir.contains("defaulted parameter `max` must be passed by name"));
    assert!(tir.contains("positional arguments cannot appear after named arguments"));
    assert!(tir.contains("duplicate named argument `max`"));
    assert!(tir.contains("unknown named argument `q`"));
    assert!(tir.contains("missing required argument `query`"));
}

#[test]
fn optional_param_default_declaration_diagnostics() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"
function type_mismatch(a: int = "bad") -> int { a }
function forward_ref(a: int = b, b: int = 1) -> int { a }
function forward_ref_in_match(seed: int, a: int = match (seed) { 1 => b, _ => 0 }, b: int = 1) -> int { a }
function required_after_default(a: int = 1, b: int) -> int { b }
"#,
    );
    let tir = render_tir(&db, file);
    insta::assert_snapshot!("optional_param_default_declaration_diagnostics", tir);
    assert!(tir.contains("type mismatch: expected int, got \"bad\""));
    assert!(tir.contains("default for parameter `a` cannot reference later parameter `b`"));
    assert!(tir.contains("function user.forward_ref_in_match"));
    assert!(tir.contains("required parameter `b` cannot appear after a defaulted parameter"));
}

#[test]
fn optional_param_default_forward_reference_is_scope_aware() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"
function shadow_later_param(a: int = { let b = 1; b }, b: int = 2) -> int { a }
"#,
    );
    let tir = render_tir(&db, file);
    assert!(
        !tir.contains("default for parameter `a` cannot reference later parameter `b`"),
        "{tir}"
    );
}

#[test]
fn optional_param_default_forward_reference_checks_lambda_bodies() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"
function lambda_capture_later_param(a: int = { let f = () -> int { b }; f() }, b: int = 1) -> int { a }
"#,
    );
    let tir = render_tir(&db, file);
    insta::assert_snapshot!(
        "optional_param_default_forward_reference_checks_lambda_bodies",
        tir.as_str()
    );
    assert!(
        tir.contains("default for parameter `a` cannot reference later parameter `b`"),
        "{tir}"
    );
}

#[test]
fn self_param_default_reports_single_semantic_error() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"
class Counter {
  value int

  function Current(self = null) -> int {
    self.value
  }
}
"#,
    );
    let tir = render_tir(&db, file);
    assert_eq!(tir.matches("`self` cannot have a default value").count(), 1);
    assert!(
        !tir.contains("type mismatch: expected user.Counter, got null"),
        "{tir}"
    );
}

// ── 3A-4. ArgumentCountMismatch diagnostic ───────────────────────────────

#[test]
fn too_many_args() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        "function add(a: int, b: int) -> int { return a + b; }\nfunction f() -> int { return add(1, 2, 3); }",
    );
    insta::assert_snapshot!(render_tir(&db, file), @"
    function user.add(a: int, b: int) -> int throws never {
      { : never
        return a + b : int
      }
    }
    function user.f() -> int throws never {
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
    insta::assert_snapshot!(render_tir(&db, file), @"
    function user.add(a: int, b: int) -> int throws never {
      { : never
        return a + b : int
      }
    }
    function user.f() -> int throws never {
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
    insta::assert_snapshot!(render_tir(&db, file), @"
    function user.f() -> int throws never {
      { : never
        let x = 42 : 42 -> int
        return x(1) : unknown
      }
      !! 41..45: `int` is not a function — it cannot be called
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
    insta::assert_snapshot!(render_tir(&db, file), @r#"
    class user.Foo {
      name: string
    }
    function user.Foo.to_json(self: user.Foo) -> baml.json.json throws baml.json.JsonSerializationError | baml.json.JsonParseError {
      map { "name": self.name.to_json() } : map<string, baml.json.json>
    }
    function user.Foo.from_json(j: baml.json.json) -> user.Foo throws baml.json.JsonParseError | baml.json.JsonDecodeError {
      Foo { name: baml.json.from_json<string>(baml.json.field(j, "name")) } : user.Foo
    }
    function user.f() -> int throws never {
      { : never
        return Foo(1) : unknown
      }
      !! 55..61: `user.Foo` is not a function — it cannot be called
    }
    class user.Foo$stream {
      name: null | string
    }
    "#);
}

// ── 3A-6. MissingReturnExpression diagnostic ─────────────────────────────

#[test]
fn missing_return() {
    let mut db = make_db();
    let file = db.add_file("test.baml", "function f() -> int { let x = 1; }");
    insta::assert_snapshot!(render_tir(&db, file), @"
    function user.f() -> int throws never {
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
    function user.f() -> string throws never {
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
    function user.f() -> int throws never {
      { : never
        return "hello" - 5 : unknown
      }
      !! 28..40: operator `Sub` cannot be applied to `"hello"` and `5`
    }
    "#);
}

#[test]
fn invalid_binary_op_bool_add() {
    let mut db = make_db();
    let file = db.add_file("test.baml", "function f() -> int { return true + false; }");
    insta::assert_snapshot!(render_tir(&db, file), @"
    function user.f() -> int throws never {
      { : never
        return true + false : unknown
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
    function user.f() -> int throws never {
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
    insta::assert_snapshot!(render_tir(&db, file), @"
    function user.f(x: bool) -> int throws never {
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
    insta::assert_snapshot!(render_tir(&db, file), @"
    function user.f(x: int) -> int throws never {
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
    insta::assert_snapshot!(render_tir(&db, file), @"
    function user.f(x: 3.14 | 2.72) -> float throws never {
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
    insta::assert_snapshot!(render_tir(&db, file), @"
    function user.f(x: bool) -> int? throws never {
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
    insta::assert_snapshot!(render_tir(&db, file), @"
    function user.f(x: bool) -> int throws never {
      { : never
        let y = : void
          if (x : bool) : void
            { : 5
              5 : 5
            }
        return y ?? 0 : void
      }
      !! 36..49: `if` without `else` cannot be used as a value; add an `else` branch
      !! 58..64: did you mean `y`? `y ?? 0` is unnecessary, because `y` cannot be null
      !! 58..64: `if` without `else` cannot be used as a value; add an `else` branch
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
    function user.f(x: user.Color) -> string throws never {
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
    let y => y + 1
  };
}"#,
    );
    insta::assert_snapshot!(render_tir(&db, file), @"
    function user.f(x: int) -> int throws never {
      { : never
        return : int
          match (x : int) : int
            y =>
              y + 1 : int
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
    insta::assert_snapshot!(render_tir(&db, file), @r#"
    class user.Cat {
      name: string
      legs: int
    }
    function user.Cat.to_json(self: user.Cat) -> baml.json.json throws baml.json.JsonSerializationError | baml.json.JsonParseError {
      map { "name": self.name.to_json(), "legs": self.legs.to_json() } : map<string, baml.json.json>
    }
    function user.Cat.from_json(j: baml.json.json) -> user.Cat throws baml.json.JsonParseError | baml.json.JsonDecodeError {
      Cat { name: baml.json.from_json<string>(baml.json.field(j, "name")), legs: baml.json.from_json<int>(baml.json.field(j, "legs")) } : user.Cat
    }
    class user.Dog {
      name: string
      legs: int
    }
    function user.Dog.to_json(self: user.Dog) -> baml.json.json throws baml.json.JsonSerializationError | baml.json.JsonParseError {
      map { "name": self.name.to_json(), "legs": self.legs.to_json() } : map<string, baml.json.json>
    }
    function user.Dog.from_json(j: baml.json.json) -> user.Dog throws baml.json.JsonParseError | baml.json.JsonDecodeError {
      Dog { name: baml.json.from_json<string>(baml.json.field(j, "name")), legs: baml.json.from_json<int>(baml.json.field(j, "legs")) } : user.Dog
    }
    function user.f(x: user.Cat | user.Dog) -> string throws never {
      { : never
        return x.name : string | string
      }
    }
    class user.Cat$stream {
      name: null | string
      legs: null | int
    }
    class user.Dog$stream {
      name: null | string
      legs: null | int
    }
    "#);
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
    insta::assert_snapshot!(render_tir(&db, file), @r#"
    class user.Cat {
      name: string
      whiskers: int
    }
    function user.Cat.to_json(self: user.Cat) -> baml.json.json throws baml.json.JsonSerializationError | baml.json.JsonParseError {
      map { "name": self.name.to_json(), "whiskers": self.whiskers.to_json() } : map<string, baml.json.json>
    }
    function user.Cat.from_json(j: baml.json.json) -> user.Cat throws baml.json.JsonParseError | baml.json.JsonDecodeError {
      Cat { name: baml.json.from_json<string>(baml.json.field(j, "name")), whiskers: baml.json.from_json<int>(baml.json.field(j, "whiskers")) } : user.Cat
    }
    class user.Dog {
      name: string
      tail: bool
    }
    function user.Dog.to_json(self: user.Dog) -> baml.json.json throws baml.json.JsonSerializationError | baml.json.JsonParseError {
      map { "name": self.name.to_json(), "tail": self.tail.to_json() } : map<string, baml.json.json>
    }
    function user.Dog.from_json(j: baml.json.json) -> user.Dog throws baml.json.JsonParseError | baml.json.JsonDecodeError {
      Dog { name: baml.json.from_json<string>(baml.json.field(j, "name")), tail: baml.json.from_json<bool>(baml.json.field(j, "tail")) } : user.Dog
    }
    function user.f(x: user.Cat | user.Dog) -> int throws never {
      { : never
        return x.whiskers : unknown
      }
      !! 118..126: type `user.Dog` has no member `whiskers`
    }
    class user.Cat$stream {
      name: null | string
      whiskers: null | int
    }
    class user.Dog$stream {
      name: null | string
      tail: null | bool
    }
    "#);
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
    insta::assert_snapshot!(render_tir(&db, file), @r#"
    class user.A {
      name: string
    }
    function user.A.to_json(self: user.A) -> baml.json.json throws baml.json.JsonSerializationError | baml.json.JsonParseError {
      map { "name": self.name.to_json() } : map<string, baml.json.json>
    }
    function user.A.from_json(j: baml.json.json) -> user.A throws baml.json.JsonParseError | baml.json.JsonDecodeError {
      A { name: baml.json.from_json<string>(baml.json.field(j, "name")) } : user.A
    }
    class user.B {
      name: string
    }
    function user.B.to_json(self: user.B) -> baml.json.json throws baml.json.JsonSerializationError | baml.json.JsonParseError {
      map { "name": self.name.to_json() } : map<string, baml.json.json>
    }
    function user.B.from_json(j: baml.json.json) -> user.B throws baml.json.JsonParseError | baml.json.JsonDecodeError {
      B { name: baml.json.from_json<string>(baml.json.field(j, "name")) } : user.B
    }
    class user.C {
      age: int
    }
    function user.C.to_json(self: user.C) -> baml.json.json throws baml.json.JsonSerializationError | baml.json.JsonParseError {
      map { "age": self.age.to_json() } : map<string, baml.json.json>
    }
    function user.C.from_json(j: baml.json.json) -> user.C throws baml.json.JsonParseError | baml.json.JsonDecodeError {
      C { age: baml.json.from_json<int>(baml.json.field(j, "age")) } : user.C
    }
    function user.f(x: user.A | user.B | user.C) -> string throws never {
      { : never
        return x.name : unknown
      }
      !! 114..118: type `user.C` has no member `name`
    }
    class user.A$stream {
      name: null | string
    }
    class user.B$stream {
      name: null | string
    }
    class user.C$stream {
      age: null | int
    }
    "#);
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
    insta::assert_snapshot!(render_tir(&db, file), @r#"
    class user.A {
      name: string
    }
    function user.A.to_json(self: user.A) -> baml.json.json throws baml.json.JsonSerializationError | baml.json.JsonParseError {
      map { "name": self.name.to_json() } : map<string, baml.json.json>
    }
    function user.A.from_json(j: baml.json.json) -> user.A throws baml.json.JsonParseError | baml.json.JsonDecodeError {
      A { name: baml.json.from_json<string>(baml.json.field(j, "name")) } : user.A
    }
    class user.B {
      age: string
    }
    function user.B.to_json(self: user.B) -> baml.json.json throws baml.json.JsonSerializationError | baml.json.JsonParseError {
      map { "age": self.age.to_json() } : map<string, baml.json.json>
    }
    function user.B.from_json(j: baml.json.json) -> user.B throws baml.json.JsonParseError | baml.json.JsonDecodeError {
      B { age: baml.json.from_json<string>(baml.json.field(j, "age")) } : user.B
    }
    class user.C {
      age: int
    }
    function user.C.to_json(self: user.C) -> baml.json.json throws baml.json.JsonSerializationError | baml.json.JsonParseError {
      map { "age": self.age.to_json() } : map<string, baml.json.json>
    }
    function user.C.from_json(j: baml.json.json) -> user.C throws baml.json.JsonParseError | baml.json.JsonDecodeError {
      C { age: baml.json.from_json<int>(baml.json.field(j, "age")) } : user.C
    }
    function user.f(x: user.A | user.B | user.C) -> string throws never {
      { : never
        return x.name : unknown
      }
      !! 113..117: type `user.B` has no member `name`
      !! 113..117: type `user.C` has no member `name`
    }
    class user.A$stream {
      name: null | string
    }
    class user.B$stream {
      age: null | string
    }
    class user.C$stream {
      age: null | int
    }
    "#);
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
    insta::assert_snapshot!(render_tir(&db, file), @r#"
    class user.A {
      value: int
    }
    function user.A.to_json(self: user.A) -> baml.json.json throws baml.json.JsonSerializationError | baml.json.JsonParseError {
      map { "value": self.value.to_json() } : map<string, baml.json.json>
    }
    function user.A.from_json(j: baml.json.json) -> user.A throws baml.json.JsonParseError | baml.json.JsonDecodeError {
      A { value: baml.json.from_json<int>(baml.json.field(j, "value")) } : user.A
    }
    class user.B {
      value: string
    }
    function user.B.to_json(self: user.B) -> baml.json.json throws baml.json.JsonSerializationError | baml.json.JsonParseError {
      map { "value": self.value.to_json() } : map<string, baml.json.json>
    }
    function user.B.from_json(j: baml.json.json) -> user.B throws baml.json.JsonParseError | baml.json.JsonDecodeError {
      B { value: baml.json.from_json<string>(baml.json.field(j, "value")) } : user.B
    }
    function user.f(x: user.A | user.B) -> string throws never {
      { : never
        return x.value : int | string
      }
      !! 86..94: type mismatch: expected string, got int | string
    }
    class user.A$stream {
      value: null | int
    }
    class user.B$stream {
      value: null | string
    }
    "#);
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
    insta::assert_snapshot!(render_tir(&db, file), @r#"
    class user.A {
      name: string
    }
    function user.A.to_json(self: user.A) -> baml.json.json throws baml.json.JsonSerializationError | baml.json.JsonParseError {
      map { "name": self.name.to_json() } : map<string, baml.json.json>
    }
    function user.A.from_json(j: baml.json.json) -> user.A throws baml.json.JsonParseError | baml.json.JsonDecodeError {
      A { name: baml.json.from_json<string>(baml.json.field(j, "name")) } : user.A
    }
    class user.B {
      name: string
    }
    function user.B.to_json(self: user.B) -> baml.json.json throws baml.json.JsonSerializationError | baml.json.JsonParseError {
      map { "name": self.name.to_json() } : map<string, baml.json.json>
    }
    function user.B.from_json(j: baml.json.json) -> user.B throws baml.json.JsonParseError | baml.json.JsonDecodeError {
      B { name: baml.json.from_json<string>(baml.json.field(j, "name")) } : user.B
    }
    function user.f(x: user.A | user.B | null) -> string throws never {
      { : never
        return x.name : (string | string)?
      }
      !! 94..101: did you mean `x?.name`? `x.name` does not handle the case when `x` is null
      !! 94..101: type mismatch: expected string, got (string | string)?
    }
    class user.A$stream {
      name: null | string
    }
    class user.B$stream {
      name: null | string
    }
    "#);
}

// ── Null coalescing operator (??) ──────────────────────────────────────────

#[test]
fn null_coalesce_unwraps_optional() {
    let mut db = make_db();
    let file = db.add_file("test.baml", "function f(x: int?) -> int { x ?? 0 }");
    insta::assert_snapshot!(render_tir(&db, file), @"
    function user.f(x: int?) -> int throws never {
      { : int
        x ?? 0 : int
      }
    }
    ");
}

#[test]
fn null_coalesce_with_variable_default() {
    let mut db = make_db();
    let file = db.add_file("test.baml", "function f(x: int?, y: int) -> int { x ?? y }");
    insta::assert_snapshot!(render_tir(&db, file), @"
    function user.f(x: int?, y: int) -> int throws never {
      { : int
        x ?? y : int
      }
    }
    ");
}

#[test]
fn null_coalesce_with_string() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"function f(name: string?) -> string { let x = "Anonymous"; name ?? x }"#,
    );
    insta::assert_snapshot!(render_tir(&db, file), @r#"
    function user.f(name: string?) -> string throws never {
      { : string
        let x = "Anonymous" : "Anonymous" -> string
        name ?? x : string
      }
    }
    "#);
}

// ── Optional chaining (?.) ─────────────────────────────────────────────────

#[test]
fn optional_field_access() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"
class User { name string }
function f(u: User?) -> string? { u?.name }
"#,
    );
    insta::assert_snapshot!(render_tir(&db, file));
}

#[test]
fn optional_chaining_with_null_coalesce() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"
class User { name string }
function f(u: User?, fallback: string) -> string { u?.name ?? fallback }
"#,
    );
    insta::assert_snapshot!(render_tir(&db, file));
}

#[test]
fn chained_optional_field_access() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"
class Address { street string }
class User { address Address? }
function f(u: User?) -> string? { u?.address?.street }
"#,
    );
    insta::assert_snapshot!(render_tir(&db, file));
}

#[test]
fn optional_method_call_basic() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"
class User {
    function getName(self) -> string { self.name }
    name string
}
function f(u: User?) -> string? { u?.getName() }
"#,
    );
    insta::assert_snapshot!(render_tir(&db, file));
}

#[test]
fn optional_call_chain_continues() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"
class User { name string }
function f(callback: (() -> User)?) -> string? {
    callback?.()?.name
}
"#,
    );
    insta::assert_snapshot!(render_tir(&db, file));
}

#[test]
fn optional_field_access_through_optional_alias() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"
class User { name string }
type MaybeUser = User?
function f(u: MaybeUser) -> string? { u?.name }
"#,
    );
    insta::assert_snapshot!(render_tir(&db, file));
}

#[test]
fn optional_index_through_optional_alias() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"
type MaybeInts = int[]?
function f(xs: MaybeInts) -> int? { xs?.[0] }
"#,
    );
    insta::assert_snapshot!(render_tir(&db, file));
}

// ── Void return type ───────────────────────────────────────────────────────

#[test]
fn void_function_basic() {
    let mut db = make_db();
    let file = db.add_file("test.baml", "function f() -> void { }");
    insta::assert_snapshot!(render_tir(&db, file), @r"
    function user.f() -> void throws never {
      { : void
      }
    }
    ");
}

#[test]
fn void_function_bare_return() {
    let mut db = make_db();
    let file = db.add_file("test.baml", "function f() -> void { return; }");
    insta::assert_snapshot!(render_tir(&db, file), @r"
    function user.f() -> void throws never {
      { : never
        return
      }
    }
    ");
}

#[test]
fn void_function_return_value_error() {
    let mut db = make_db();
    let file = db.add_file("test.baml", "function f() -> void { return 42; }");
    insta::assert_snapshot!(render_tir(&db, file), @r"
    function user.f() -> void throws never {
      { : never
        return 42 : 42
      }
      !! 30..32: type mismatch: expected void, got 42
    }
    ");
}

#[test]
fn void_function_result_used_error() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"
function g() -> void { }
function f() -> int { let x = g(); 1 }
"#,
    );
    insta::assert_snapshot!(render_tir(&db, file), @r"
    function user.g() -> void throws never {
      { : void
      }
    }
    function user.f() -> int throws never {
      { : 1
        let x = g() : void
        1 : 1
      }
      !! 56..59: cannot use return value of a void function
    }
    ");
}

#[test]
fn void_function_bare_call_ok() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"
function g() -> void { }
function f() -> int { g(); 1 }
"#,
    );
    insta::assert_snapshot!(render_tir(&db, file), @r"
    function user.g() -> void throws never {
      { : void
      }
    }
    function user.f() -> int throws never {
      { : 1
        g() : void
        1 : 1
      }
    }
    ");
}

#[test]
fn lambda_checks_against_aliased_and_optional_function_contexts() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"
type Body = () -> void throws never

function takes_direct(cb: Body) -> void {
    cb()
}

function takes_optional(cb: Body?) -> void {
    cb?.()
}

function main() -> void {
    takes_direct(() -> { assert.is_true(true); })
    takes_optional(() -> { assert.is_true(true); })
}
"#,
    );

    let tir = render_tir(&db, file);
    assert!(
        !tir.contains("type mismatch"),
        "expected lambda alias checking without mismatches, got:\n{tir}"
    );
    assert!(
        tir.contains("() -> { ... } : () -> void throws never"),
        "expected lambdas to inherit void-returning aliased function context, got:\n{tir}"
    );
}
