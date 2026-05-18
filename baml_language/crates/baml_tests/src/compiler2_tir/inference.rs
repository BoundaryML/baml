//! Core type inference snapshot tests.

use baml_base::Name;
use baml_compiler2_hir::{package::PackageId, scope::ScopeKind};
use baml_compiler2_tir::{
    inference::infer_scope_types,
    package_interface::{ExportedType, package_interface, package_resolution_context},
    resolve::{ResolvedName, resolve_name_at_in_scope},
    ty::{FunctionParamMode, QualifiedTypeName, Ty},
};
use text_size::TextSize;

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
fn resolver_initializer_shadowing_uses_previous_binding() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        "function f() -> int { let x = 1; let x = x + 1; x }",
    );

    let index = baml_compiler2_ppir::file_semantic_index(&db, file);
    let function_scope = index
        .scopes
        .iter()
        .enumerate()
        .find_map(|(idx, scope)| {
            (matches!(scope.kind, ScopeKind::Function)
                && scope.name.as_ref().is_some_and(|name| name.as_str() == "f"))
            .then_some(baml_compiler2_hir::scope::FileScopeId::new(idx as u32))
        })
        .expect("function scope");
    let x_bindings = index.scope_bindings[function_scope.index() as usize]
        .bindings
        .iter()
        .filter(|binding| binding.name == Name::new("x"))
        .collect::<Vec<_>>();
    assert_eq!(x_bindings.len(), 2);

    let offset = TextSize::from(file.text(&db).find("x + 1").expect("initializer x") as u32);
    let resolved =
        resolve_name_at_in_scope(&db, file, offset, &Name::new("x"), Some(&Name::new("f")));

    assert_eq!(
        resolved,
        ResolvedName::Local {
            name: Name::new("x"),
            definition_site: Some(x_bindings[0].site),
        }
    );
}

#[test]
fn class_field_access() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        "class Foo { name string }\nfunction f(x: Foo) -> string { return x.name; }",
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
    function user.f(x: user.Foo) -> string throws never {
      { : never
        return x.name : string
      }
    }
    class user.Foo$stream {
      name: null | string
    }
    "#);
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
    function user.f(x: user.Foo) -> string throws never {
      { : never
        return x.missing : unknown
      }
      !! 66..73: type `user.Foo` has no member `missing`
    }
    class user.Foo$stream {
      name: null | string
    }
    "#);
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
    insta::assert_snapshot!(render_tir(&db, file), @r#"
    class user.Data {
      name: string
    }
    function user.Data.to_json(self: user.Data) -> baml.json.json throws baml.json.JsonSerializationError | baml.json.JsonParseError {
      map { "name": self.name.to_json() } : map<string, baml.json.json>
    }
    function user.Data.from_json(j: baml.json.json) -> user.Data throws baml.json.JsonParseError | baml.json.JsonDecodeError {
      Data { name: baml.json.from_json<string>(baml.json.field(j, "name")) } : user.Data
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
    "#);
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
    insta::assert_snapshot!(render_tir(&db, file), @r#"
    class user.Sentiment {
      feeling: string
    }
    function user.Sentiment.to_json(self: user.Sentiment) -> baml.json.json throws baml.json.JsonSerializationError | baml.json.JsonParseError {
      map { "feeling": self.feeling.to_json() } : map<string, baml.json.json>
    }
    function user.Sentiment.from_json(j: baml.json.json) -> user.Sentiment throws baml.json.JsonParseError | baml.json.JsonDecodeError {
      Sentiment { feeling: baml.json.from_json<string>(baml.json.field(j, "feeling")) } : user.Sentiment
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
    "#);
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
    insta::assert_snapshot!(render_tir(&db, file), @r#"
    class user.Point {
      x: int
      y: float
      label: string
    }
    function user.Point.to_json(self: user.Point) -> baml.json.json throws baml.json.JsonSerializationError | baml.json.JsonParseError {
      map { "x": self.x.to_json(), "y": self.y.to_json(), "label": self.label.to_json() } : map<string, baml.json.json>
    }
    function user.Point.from_json(j: baml.json.json) -> user.Point throws baml.json.JsonParseError | baml.json.JsonDecodeError {
      Point { x: baml.json.from_json<int>(baml.json.field(j, "x")), y: baml.json.from_json<float>(baml.json.field(j, "y")), label: baml.json.from_json<string>(baml.json.field(j, "label")) } : user.Point
    }
    class user.Point$stream {
      x: null | int
      y: null | float
      label: null | string
    }
    "#);
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
        format!("{}", exported.params[0].ty),
        "(value: int) -> string throws __effect_param_0"
    );
}

#[test]
fn package_interface_exports_optional_param_mode() {
    let mut db = make_db();
    db.add_file(
        "search.baml",
        "function Search(query: string, limit: int = 10) -> int { limit }",
    );

    let iface = package_interface(&db, PackageId::new(&db, Name::new("user")));
    let exported = iface
        .lookup_function(&[], &Name::new("Search"))
        .expect("exported function");

    assert_eq!(
        exported.params[0].name.as_ref().map(|name| name.as_str()),
        Some("query")
    );
    assert_eq!(exported.params[0].mode, FunctionParamMode::Required);
    assert_eq!(
        exported.params[1].name.as_ref().map(|name| name.as_str()),
        Some("limit")
    );
    assert_eq!(exported.params[1].mode, FunctionParamMode::Optional);
}

#[test]
fn own_class_method_lookup_matches_exported_implicit_self_type() {
    let mut db = make_db();
    db.add_file(
        "service.baml",
        r#"
class SearchService {
    base string

    function Run(self, query: string, limit: int = 20) -> int {
        limit
    }
}
"#,
    );

    let pkg_id = PackageId::new(&db, Name::new("user"));
    let iface = package_interface(&db, pkg_id);
    let Some(ExportedType::Class { methods, .. }) =
        iface.lookup_type(&[], &Name::new("SearchService"))
    else {
        panic!("exported class");
    };
    let exported_method = methods
        .iter()
        .find(|method| method.name.as_str() == "Run")
        .expect("exported method");

    let res_ctx = package_resolution_context(&db, pkg_id);
    let own_method = res_ctx
        .lookup_class_method(
            &db,
            &QualifiedTypeName::new(Name::new("user"), vec![], Name::new("SearchService")),
            &Name::new("Run"),
        )
        .expect("own method");

    assert_eq!(&own_method.function.params, &exported_method.params);
    assert!(
        !matches!(own_method.function.params[0].ty, Ty::Unknown { .. }),
        "implicit self should be reified before lowering"
    );
    assert_eq!(own_method.function.params[1].ty.to_string(), "string");
    assert_eq!(own_method.function.params[2].ty.to_string(), "int");
    assert_eq!(
        own_method.function.params[2].mode,
        FunctionParamMode::Optional
    );
}

#[test]
fn lambda_scope_retypes_capture_from_function_parameter() {
    let mut db = make_db();
    let file = db.add_file(
        "capture_param.baml",
        "function main(x: int) -> int { let f = () -> int { x }; return f(); }",
    );

    let index = baml_compiler2_ppir::file_semantic_index(&db, file);
    let lambda_scope_id = index
        .scope_ids
        .iter()
        .copied()
        .find(|scope_id| {
            let scope = &index.scopes[scope_id.file_scope_id(&db).index() as usize];
            matches!(scope.kind, ScopeKind::Lambda)
        })
        .expect("lambda scope");
    let lambda_inference = infer_scope_types(&db, lambda_scope_id);

    let item_tree = baml_compiler2_ppir::file_item_tree(&db, file);
    let (main_id, _) = item_tree
        .functions
        .iter()
        .find(|(_, func)| func.name.as_str() == "main")
        .expect("main function");
    let main_loc = baml_compiler2_hir::loc::FunctionLoc::new(&db, file, main_id);
    let main_body = baml_compiler2_ppir::function_body(&db, main_loc);
    let baml_compiler2_hir::body::FunctionBody::Expr(main_expr_body) = main_body.as_ref() else {
        panic!("main expression body");
    };
    let lambda_body = main_expr_body
        .exprs
        .iter()
        .find_map(|(_, expr)| {
            if let baml_compiler2_ast::Expr::Lambda(func_def) = expr
                && let Some(baml_compiler2_ast::FunctionBodyDef::Expr(lambda_body, _)) =
                    &func_def.body
            {
                Some(lambda_body)
            } else {
                None
            }
        })
        .expect("lambda body");
    let root_expr = lambda_body.root_expr.expect("lambda root expr");

    assert_eq!(
        lambda_inference
            .expression_type(root_expr)
            .map(ToString::to_string),
        Some("int".to_string())
    );
}

#[test]
fn lambda_parameter_shadowing_uses_parameter_declared_type() {
    let mut db = make_db();
    let file = db.add_file(
        "lambda_param_shadow.baml",
        r#"
function main() -> int {
    let x: string = "";
    let f = (x: int) -> int {
        x = 1;
        x
    };
    f(0)
}
"#,
    );

    let output = render_tir(&db, file);
    assert!(
        !output.contains("type mismatch: expected string, got int"),
        "lambda parameter assignment should use the parameter annotation, got:\n{output}"
    );
    assert!(
        output.contains("(x: int) -> int"),
        "expected lambda parameter to keep its int type, got:\n{output}"
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
