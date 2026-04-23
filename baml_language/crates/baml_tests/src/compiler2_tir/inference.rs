//! Core type inference snapshot tests.

use baml_base::Name;
use baml_compiler2_hir::{package::PackageId, scope::ScopeKind};
use baml_compiler2_tir::{inference::infer_scope_types, package_interface::package_interface};

use super::support::{expr_type_in_function, make_db, render_tir};

fn find_function_scope_id<'db>(
    db: &'db baml_project::ProjectDatabase,
    file: baml_base::SourceFile,
    name: &str,
) -> baml_compiler2_hir::scope::ScopeId<'db> {
    let index = baml_compiler2_ppir::file_semantic_index(db, file);
    index
        .scope_ids
        .iter()
        .copied()
        .find(|scope_id| {
            let scope = &index.scopes[scope_id.file_scope_id(db).index() as usize];
            matches!(scope.kind, ScopeKind::Function)
                && scope
                    .name
                    .as_ref()
                    .is_some_and(|scope_name| scope_name.as_str() == name)
        })
        .unwrap_or_else(|| panic!("missing function scope {name}"))
}

#[test]
fn literal_int() {
    let mut db = make_db();
    let file = db.add_file("test.baml", "function f() -> int { return 1; }");
    insta::assert_snapshot!(render_tir(&db, file), @"
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
    insta::assert_snapshot!(render_tir(&db, file), @"
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
    insta::assert_snapshot!(render_tir(&db, file), @"
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
    insta::assert_snapshot!(render_tir(&db, file), @"
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
    insta::assert_snapshot!(render_tir(&db, file), @"
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
    insta::assert_snapshot!(render_tir(&db, file), @"
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
    insta::assert_snapshot!(render_tir(&db, file), @"
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
    insta::assert_snapshot!(render_tir(&db, file), @"
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
    insta::assert_snapshot!(render_tir(&db, file), @"
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
    insta::assert_snapshot!(render_tir(&db, file), @"
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
    insta::assert_snapshot!(render_tir(&db, file), @"
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
    insta::assert_snapshot!(render_tir(&db, file), @"
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
    insta::assert_snapshot!(render_tir(&db, file), @"
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

#[test]
fn unresolved_path_after_valid_type() {
    // Test: when a path like `baml.media.Image.missing` fails, `missing` should be
    // reported as unresolved (not `Image`, which is a valid type in the media namespace).
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        "function f() -> int { return baml.media.Image.missing; }",
    );
    // The error should mention `missing`, not `Image`
    let output = render_tir(&db, file);
    assert!(
        output.contains("unresolved name: missing"),
        "Expected error to mention 'missing' as unresolved, got:\n{output}"
    );
    assert!(
        !output.contains("unresolved name: Image"),
        "Error should NOT mention 'Image' as unresolved (it's a valid type), got:\n{output}"
    );
}

#[test]
fn io_input_requires_baml_prefix() {
    let mut db = make_db();
    let file = db.add_file("test.baml", "function f() -> string { io.input(\"x\") }");

    let output = render_tir(&db, file);
    assert!(
        output.contains("unresolved name: io"),
        "Expected bare io namespace to be rejected, got:\n{output}"
    );
}

#[test]
fn env_builtin_calls_require_baml_prefix() {
    let mut db = make_db();
    let file = db.add_file("test.baml", "function f() -> string? { env.get(\"X\") }");

    let output = render_tir(&db, file);
    assert!(
        output.contains("unresolved name: env"),
        "Expected bare env builtin call to be rejected, got:\n{output}"
    );
}

#[test]
fn function_type_throws_inference_opens_immediate_callback_param() {
    let mut db = make_db();
    let file = db.add_file(
        "callback.baml",
        "function direct(cb: (value: int) -> string) -> string { let handler = cb; return \"ok\"; }",
    );

    assert_eq!(
        expr_type_in_function(&db, file, "direct", "cb"),
        "(value: int) -> string throws __effect_param_0"
    );
}

#[test]
fn function_type_throws_package_interface_exports_effect_params() {
    let mut db = make_db();
    let file = db.add_file(
        "callback.baml",
        "function direct(cb: (value: int) -> string) -> string { return \"ok\"; }",
    );

    let scope_id = find_function_scope_id(&db, file, "direct");
    let _ = infer_scope_types(&db, scope_id);

    let iface = package_interface(&db, PackageId::new(&db, Name::new("user")));
    let exported = iface
        .lookup_function(&[], &Name::new("direct"))
        .expect("exported function");

    assert_eq!(exported.generic_params, vec![Name::new("__effect_param_0")]);
    assert_eq!(
        format!("{}", exported.params[0].1),
        "(value: int) -> string throws __effect_param_0"
    );
}

#[test]
fn returning_callback_forwarder_matches_omitted_function_type_return_annotation() {
    let mut db = make_db();
    let file = db.add_file(
        "callback_return.baml",
        r#"function wrap(cb: (x: int) -> int) -> int {
  return cb(1)
}

function demo() -> ((x: int) -> int) -> int {
  return wrap
}"#,
    );

    let output = render_tir(&db, file);
    assert!(
        !output.contains("type mismatch"),
        "expected function-valued return annotation to preserve callback forwarding surface, got:\n{output}"
    );
}
