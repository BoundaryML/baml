//! Core type inference snapshot tests.

use baml_base::Name;
use baml_compiler2_hir::{package::PackageId, scope::ScopeKind};
use baml_compiler2_hir_ty::package_interface::{
    ExportedType, package_interface, package_resolution_context,
};
use baml_compiler2_ppir::resolve::{ResolvedName, resolve_name_at_in_scope};
use baml_type::{FunctionParamMode, QualifiedTypeName, Ty, TyAttr};
use text_size::TextSize;

use super::support::{expr_type_in_function, make_db, render_tir};
use crate::engine::TestDbExt;

fn find_function_scope_id<'db>(
    db: &'db baml_db::ProjectDatabase,
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
    let file = db.file("test.baml", "function f() -> int { return 1; }");
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
    let file = db.file("test.baml", "function f() -> int { let x = 1; return x; }");
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
    let file = db.file(
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
    let file = db.file(
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
      name: string | null
    }
    ");
}

#[test]
fn type_mismatch() {
    let mut db = make_db();
    let file = db.file("test.baml", "function f() -> string { return 1; }");
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
    let file = db.file(
        "test.baml",
        "class Foo { name string }\nfunction f(x: Foo) -> string { return x.missing; }",
    );
    insta::assert_snapshot!(render_tir(&db, file), @"
    class user.Foo {
      name: string
    }
    function user.f(x: user.Foo) -> string throws never {
      { : never
        return x.missing : !error
      }
      !! 64..73: type `Foo` has no member `missing`
    }
    class user.Foo$stream {
      name: string | null
    }
    ");
}

#[test]
fn unresolved_field_chained_access() {
    // Test: in `data.inner.foo`, if `inner` doesn't exist on the class,
    // the squiggly should only cover "inner", not "data.inner".
    let mut db = make_db();
    let file = db.file(
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
        return data.inner.foo : !error
      }
      !! 73..87: type `Data` has no member `inner`
    }
    class user.Data$stream {
      name: string | null
    }
    ");
}

#[test]
fn unresolved_field_span_should_narrow_to_member() {
    // Regression test: the diagnostic span for an unresolved member should cover
    // only the member name ("feelin"), not the entire expression ("user.Sentiment.feelin").
    let mut db = make_db();
    let file = db.file(
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
        return s.feelin : !error
      }
      !! 83..91: type `Sentiment` has no member `feelin`
    }
    class user.Sentiment$stream {
      feeling: string | null
    }
    ");
}

#[test]
fn unresolved_dotted_root_span_should_narrow_to_root() {
    // B-539 regression: when the root of a dotted access (`o.value`) is an
    // unresolved name, the diagnostic should underline only `o`, not the whole
    // `o.value` expression.
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        "\
function f() -> string {
  return o.value;
}",
    );
    insta::assert_snapshot!(render_tir(&db, file), @"
    function user.f() -> string throws never {
      { : never
        return o.value : !error
      }
      !! 34..41: unresolved name: o.value
    }
    ");
}

#[test]
fn unknown_field_access_uses_narrowing_diagnostic() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        "\
function load(raw: unknown) -> string {
  return raw.email.to_lower_case();
}",
    );

    let output = render_tir(&db, file);
    assert!(
        output.contains("cannot access field `email` on `unknown`"),
        "expected unknown-specific field access diagnostic, got:\n{output}"
    );
    assert!(
        !output.contains("type `unknown` has no member `email`"),
        "unknown field access should not use the generic missing-member wording:\n{output}"
    );
}

#[test]
fn binary_op_int_add() {
    let mut db = make_db();
    let file = db.file(
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
    let file = db.file(
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
    block user.f {
    }
    block user.f {
    }
    ");
}

#[test]
fn enum_variant_resolution() {
    let mut db = make_db();
    let file = db.file(
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
    let file = db.file("test.baml", "class Point { x int\ny float\nlabel string }");
    insta::assert_snapshot!(render_tir(&db, file), @"
    class user.Point {
      x: int
      y: float
      label: string
    }
    class user.Point$stream {
      x: int | null
      y: float | null
      label: string | null
    }
    ");
}

#[test]
fn resolve_type_alias_query() {
    let mut db = make_db();
    let file = db.file("test.baml", "type MyStr = string");
    insta::assert_snapshot!(render_tir(&db, file), @"
    type user.MyStr = string
    type user.MyStr$stream = string
    ");
}

#[test]
fn class_field_bigint() {
    // Asserts that `class Foo { x bigint }` lowers the field type to
    // `Ty::Bigint { .. }`, displayed as `bigint`.
    // Note: to_json returns `map<string, unknown>` for bigint until Phase 2
    // wires up the bigint.to_json() method.
    let mut db = make_db();
    let file = db.file("test.baml", "class Foo { x bigint }");
    insta::assert_snapshot!(render_tir(&db, file), @"
    class user.Foo {
      x: bigint
    }
    class user.Foo$stream {
      x: bigint | null
    }
    ");
}

#[test]
fn two_functions_independent() {
    let mut db = make_db();
    let file = db.file(
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
    let file = db.file(
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
    let file = db.file("test.baml", "function f() -> string { io.input(\"x\") }");

    let output = render_tir(&db, file);
    assert!(
        output.contains("unresolved name: io"),
        "Expected bare io namespace to be rejected, got:\n{output}"
    );
}

#[test]
fn env_builtin_calls_require_baml_prefix() {
    let mut db = make_db();
    let file = db.file("test.baml", "function f() -> string? { env.get(\"X\") }");

    let output = render_tir(&db, file);
    assert!(
        output.contains("unresolved name: env"),
        "Expected bare env builtin call to be rejected, got:\n{output}"
    );
}

#[test]
fn function_type_throws_inference_opens_immediate_callback_param() {
    let mut db = make_db();
    let file = db.file(
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
    let file = db.file(
        "callback.baml",
        "function direct(cb: (value: int) -> string) -> string { return \"ok\"; }",
    );

    let scope_id = find_function_scope_id(&db, file, "direct");
    let _ = baml_compiler2_hir_ty::ide::infer_for_scope(&db, scope_id);

    let iface = package_interface(&db, PackageId::new(&db, Name::new("user")));
    let exported = iface
        .lookup_function(&[], &Name::new("direct"))
        .expect("exported function");

    assert_eq!(
        exported.generic_params,
        vec![baml_type::ParamTy::new(0, Name::new("__effect_param_0"))]
    );
    assert_eq!(
        exported.params[0].ty.render_canonical(),
        "(value: int) -> string throws __effect_param_0"
    );
}

#[test]
fn package_interface_exports_optional_param_mode() {
    let mut db = make_db();
    db.file(
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
fn cross_file_out_of_body_implements_class_target_is_registered() {
    let mut db = make_db();
    db.file(
        "types.baml",
        r#"
class Dog {
    breed: string
}
"#,
    );
    let impl_file = db.file(
        "impl.baml",
        r#"
interface ToJson {
    function to_json(self) -> string throws never
}

implements ToJson for Dog {
    function to_json(self) -> string {
        return "dog"
    }
}
"#,
    );

    assert_eq!(
        baml_compiler2_ppir::item_data::file_free_impls(&db, impl_file).len(),
        1,
        "cross-file class target must remain a first-class out-of-body impl record"
    );

    let diagnostics = baml_db::collect_compiler2_diagnostics(&db);
    assert!(
        diagnostics.is_empty(),
        "cross-file class target should not produce diagnostics: {diagnostics:#?}"
    );

    // Membership goes through the canonical L1 seam (GlobalTypeContext's
    // `TypeContext::implements_interface`); no type aliases are involved here.
    use baml_type::normalize::TypeContext;
    let pkg_id = PackageId::new(&db, Name::new("user"));
    let _ = pkg_id;
    let ctx = baml_compiler2_hir_ty::facts::Facts::new(&db);
    let dog = Ty::Class(
        QualifiedTypeName::new(Name::new("user"), vec![], Name::new("Dog")),
        vec![],
        TyAttr::default(),
    );
    let to_json = baml_type::Interface::new(
        QualifiedTypeName::new(Name::new("user"), vec![], Name::new("ToJson")),
        vec![],
        vec![],
    );
    assert!(
        ctx.implements_interface(&dog, &to_json),
        "out-of-body implementation in another file should register Dog <: ToJson"
    );
}

/// The builtin `Equals`/`Compare` impls for primitives live in the `baml`
/// package, so a user-package query only finds them through the dependency
/// registries. `type_implements_with_deps` must bridge that gap (a bare
/// per-package lookup would miss them).
#[test]
fn builtin_equals_compare_visible_from_user_package() {
    use baml_type::normalize::TypeContext;

    let mut db = make_db();
    // A user file so the `user` package exists; `Bare` implements nothing.
    db.file("main.baml", "class Bare { x: int }");
    let user_pkg = PackageId::new(&db, Name::new("user"));

    let equals = baml_type::Interface::new(
        QualifiedTypeName::new(
            Name::new("baml"),
            vec![Name::new("ops")],
            Name::new("Equals"),
        ),
        vec![],
        vec![],
    );
    let compare = baml_type::Interface::new(
        QualifiedTypeName::new(
            Name::new("baml"),
            vec![Name::new("ops")],
            Name::new("Compare"),
        ),
        vec![],
        vec![],
    );
    let int_ty = Ty::int();
    let u8_ty = Ty::uint8array();
    let bare = Ty::Class(
        QualifiedTypeName::new(Name::new("user"), vec![], Name::new("Bare")),
        vec![],
        TyAttr::default(),
    );

    // The membership query walks the interface's package (`baml`) via the orphan
    // rule, so the builtin primitive impls are visible from the user package.
    let _ = user_pkg;
    let ctx = baml_compiler2_hir_ty::facts::Facts::new(&db);

    // int implements both Equals and Compare (impls in `baml`).
    assert!(ctx.implements_interface(&int_ty, &equals));
    assert!(ctx.implements_interface(&int_ty, &compare));
    // uint8array implements Equals but not Compare.
    assert!(ctx.implements_interface(&u8_ty, &equals));
    assert!(!ctx.implements_interface(&u8_ty, &compare));
    // A class with no `implements` satisfies neither.
    assert!(!ctx.implements_interface(&bare, &equals));
    assert!(!ctx.implements_interface(&bare, &compare));
}

#[test]
fn own_class_method_lookup_matches_exported_implicit_self_type() {
    let mut db = make_db();
    db.file(
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
    let file = db.file(
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
    let lambda_inference = baml_compiler2_hir_ty::ide::infer_for_scope(&db, lambda_scope_id)
        .expect("lambda scope has an owner");

    let main_loc = *baml_compiler2_ppir::item_data::file_functions(&db, file)
        .iter()
        .find(|&&loc| {
            baml_compiler2_ppir::item_data::function_data(&db, loc)
                .name
                .as_str()
                == "main"
        })
        .expect("main function");
    let main_body = baml_compiler2_ppir::function_body(&db, main_loc);
    let baml_compiler2_hir::body::FunctionBody::Expr(main_expr_body) = main_body.as_ref() else {
        panic!("main expression body");
    };
    // The lambda's body is an expression in `main`'s own arena.
    let root_expr = main_expr_body
        .exprs
        .iter()
        .find_map(|(_, expr)| {
            if let baml_compiler2_ast::Expr::Lambda(func_def) = expr {
                func_def.body
            } else {
                None
            }
        })
        .expect("lambda body");

    assert_eq!(
        lambda_inference
            .type_of_expr
            .get(&root_expr)
            .map(|ty| ty.to_plain().to_string()),
        Some("int".to_string())
    );
}

#[test]
fn lambda_parameter_shadowing_uses_parameter_declared_type() {
    let mut db = make_db();
    let file = db.file(
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

/// A function-valued return annotation must declare its throws (rule 5); an
/// effect-polymorphic forwarder is returned by eta-expanding at the concrete
/// throws surface. (Returning `wrap` directly does not instantiate its
/// synthetic effect param against the annotation — the forwarder value stays
/// generic — so the lambda pins the `never` instantiation.)
#[test]
fn returning_callback_forwarder_matches_explicit_function_type_return_annotation() {
    let mut db = make_db();
    let file = db.file(
        "callback_return.baml",
        r#"function wrap(cb: (x: int) -> int) -> int {
  return cb(1)
}

function demo() -> ((x: int) -> int throws never) -> int throws never {
  return (cb: (x: int) -> int throws never) -> int { wrap(cb) }
}"#,
    );

    let output = render_tir(&db, file);
    assert!(
        !output.contains("type mismatch"),
        "expected function-valued return annotation to accept the eta-expanded forwarder, got:\n{output}"
    );
}

/// Helper: does compiling `source` produce a type mismatch diagnostic?
fn has_type_mismatch(source: &str) -> bool {
    let mut db = make_db();
    db.file("test.baml", source);
    baml_db::collect_compiler2_diagnostics(&db)
        .iter()
        .any(|diag| diag.id == baml_compiler_diagnostics::DiagnosticId::TypeMismatch)
}

// ─── B-236: reassigning an unannotated local across container kinds ──────────
//
// `let x = {}` gives `x` an (evolving) map type. Reassigning the empty array
// `[]` used to be accepted silently: `x` stayed map-typed while it held an
// array at runtime, so indexing it aborted the VM with `expected map, got
// array`. Reassignment must instead be rejected at compile time.

#[test]
fn reassign_empty_map_local_to_array_is_rejected() {
    assert!(
        has_type_mismatch(
            r#"function main() -> int {
    let x = {};
    x = [];
    return 0;
}"#
        ),
        "reassigning [] into a map-typed local should report a type mismatch"
    );
}

#[test]
fn reassign_empty_array_local_to_map_is_rejected() {
    assert!(
        has_type_mismatch(
            r#"function main() -> int {
    let x = [];
    x = {};
    return 0;
}"#
        ),
        "reassigning {{}} into a list-typed local should report a type mismatch"
    );
}

#[test]
fn reassign_if_else_with_empty_array_else_branch_is_rejected() {
    // The exact ticket shape: the empty array lives in an if/else else-branch.
    assert!(
        has_type_mismatch(
            r#"function main() -> int {
    let x = {};
    x = if false { {"a": 1} } else { [] };
    return x["a"];
}"#
        ),
        "if/else result mixing a map then-branch with an empty-array else-branch \
         must not be assignable to a map-typed local"
    );
}

#[test]
fn reassign_unannotated_scalar_local_to_incompatible_type_is_rejected() {
    assert!(
        has_type_mismatch(
            r#"function main() -> int {
    let n = 1;
    n = "hello";
    return 0;
}"#
        ),
        "reassigning a string into an int-typed local should report a type mismatch"
    );
}

#[test]
fn empty_array_local_still_evolves_via_reassignment() {
    // Regression guard: an *empty* evolving list must still accept a populated
    // list of the same kind — only cross-kind reassignment is rejected.
    assert!(
        !has_type_mismatch(
            r#"function main() -> int {
    let a = [];
    a = [1, 2, 3];
    return 0;
}"#
        ),
        "reassigning a populated list into an empty-list local must stay allowed"
    );
}

// ─── Index-key type validation ───────────────────────────────────────────────
//
// The runtime does no key coercion: a list is subscripted by an `int` and a
// map by a `string`. A wrong-typed subscript used to slip past the checker and
// abort the VM (`expected int, got string` / `expected string, got int`).

#[test]
fn list_indexed_by_string_key_is_rejected() {
    assert!(
        has_type_mismatch(
            r#"function main() -> int {
    let x = [1, 2, 3];
    return x["a"];
}"#
        ),
        "indexing a list with a string key should report a type mismatch"
    );
}

#[test]
fn list_index_assign_with_string_key_is_rejected() {
    assert!(
        has_type_mismatch(
            r#"function main() -> int {
    let x = [];
    x["a"] = 1;
    return 0;
}"#
        ),
        "index-assigning a list with a string key should report a type mismatch"
    );
}

#[test]
fn map_indexed_by_int_key_is_rejected() {
    assert!(
        has_type_mismatch(
            r#"function main() -> int {
    let m = {"a": 1};
    return m[0];
}"#
        ),
        "indexing a map with an int key should report a type mismatch"
    );
}

#[test]
fn empty_map_index_assign_with_int_key_is_rejected() {
    assert!(
        has_type_mismatch(
            r#"function main() -> int {
    let m = {};
    m[0] = 1;
    return 0;
}"#
        ),
        "index-assigning an empty map with an int key should report a type mismatch"
    );
}

#[test]
fn well_typed_index_access_is_accepted() {
    // Regression guard: int-keyed list and string-keyed map (incl. evolving
    // empties) must stay valid.
    assert!(
        !has_type_mismatch(
            r#"function main() -> int {
    let x = [];
    x[0] = 1;
    let m = {};
    m["k"] = 2;
    return x[0] + m["k"];
}"#
        ),
        "int-keyed list and string-keyed map access must stay allowed"
    );
}

#[test]
fn list_indexed_by_nullable_int_is_rejected() {
    // A subscript must be non-null — the runtime has no null index (it aborts
    // with the confusing `type error: ... got any`).
    assert!(
        has_type_mismatch(
            r#"function main() -> int {
    let arr = [1, 2, 3];
    let i: int? = null;
    return arr[i];
}"#
        ),
        "indexing a list with a nullable int should report a type mismatch"
    );
}

#[test]
fn narrowed_nullable_index_is_accepted() {
    // Regression guard: a nullable index narrowed to non-null must stay valid.
    assert!(
        !has_type_mismatch(
            r#"function main() -> int {
    let arr = [1, 2, 3];
    let i: int? = 0;
    if i != null {
        return arr[i];
    }
    return 0;
}"#
        ),
        "a nullable index narrowed to non-null must stay allowed"
    );
}

#[test]
fn class_spread_requires_the_same_nominal_class_and_generic_arguments() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
class Left<T> { value T }
class Right<T> { value T }
class Wrapper<T, E> { body () -> T throws E }

function infer_from_spread(source: Left<int>) -> int {
  let copy = Left { ...source };
  copy.value
}

function expected_type_supplies_omitted_arguments() -> Wrapper<int, null> {
  Wrapper { body: () -> 1 }
}

function wrong_class() -> Left<int> {
  Left<int> { ...Right<int> { value: 1 } }
}

function wrong_type_argument() -> Left<int> {
  Left<int> { ...Left<string> { value: "bad" } }
}
"#,
    );
    let tir = render_tir(&db, file);
    assert!(!tir.contains("cannot infer type parameter `T`"), "{tir}");
    assert!(
        !tir.contains("expected Wrapper<int, null>, got Wrapper<int, never>"),
        "{tir}"
    );
    assert!(
        tir.contains("type mismatch: expected Left<int>, got Right<int>"),
        "{tir}"
    );
    assert!(
        tir.contains("type mismatch: expected Left<int>, got Left<string>"),
        "{tir}"
    );
}

/// The declaration-site interface surface: required-method signatures resolve
/// with `Self` symbolic (a projection over the rigid `Self` bound by the
/// interface), method-level generic bounds resolve to interfaces, and field
/// types resolve in the same scope. Locks the surface queries the handle
/// layer reads.
#[test]
fn interface_declaration_surface_resolves_symbolically() {
    use baml_compiler2_hir_ty::interfaces::{
        resolve_interface_fields, resolve_interface_required_methods,
    };

    let mut db = make_db();
    let file = db.file(
        "iface.baml",
        r#"
interface Encoder {
  type Error

  limit int

  function encode(self, value: string) -> string throws Self.Error
  function pick<T extends Encoder>(self, options: T[]) -> T throws never
}
"#,
    );

    let iface_loc = *baml_compiler2_ppir::item_data::file_interfaces(&db, file)
        .iter()
        .find(|&&i| {
            baml_compiler2_ppir::item_data::interface_data(&db, i)
                .name
                .as_str()
                == "Encoder"
        })
        .unwrap();

    let fields = resolve_interface_fields(&db, iface_loc);
    assert!(fields.diagnostics.is_empty(), "{:?}", fields.diagnostics);
    assert_eq!(fields.fields.len(), 1);
    assert_eq!(fields.fields[0].0.as_str(), "limit");
    assert_eq!(fields.fields[0].1.render_canonical(), "int");

    let methods = resolve_interface_required_methods(&db, iface_loc);
    assert_eq!(methods.len(), 2);

    let encode = &methods[0];
    assert_eq!(encode.name.as_str(), "encode");
    assert!(encode.diagnostics.is_empty(), "{:?}", encode.diagnostics);
    assert!(encode.generic_params.is_empty());
    // `Self` stays symbolic: the receiver is the rigid `Self` variable and the
    // declared throws is a projection through the interface bound.
    assert_eq!(
        encode.function_ty.render_canonical(),
        "(self: Self, value: string) -> string throws (Self as user.Encoder).Error"
    );

    let pick = &methods[1];
    assert_eq!(pick.name.as_str(), "pick");
    assert!(pick.diagnostics.is_empty(), "{:?}", pick.diagnostics);
    assert_eq!(pick.generic_params.len(), 1);
    let (param, bounds) = &pick.generic_params[0];
    assert_eq!(param.name().as_str(), "T");
    assert_eq!(bounds.len(), 1);
    assert_eq!(bounds[0].name.render_user_facing(), "Encoder");
}

/// An optional callback parameter is a callback slot too: its omitted
/// `throws` opens to a synthetic effect param rather than an E0151.
#[test]
fn function_type_throws_inference_opens_optional_callback_param() {
    let mut db = make_db();
    let file = db.file(
        "callback.baml",
        "function opt(cb: ((value: int) -> string)?) -> string { let handler = cb; return \"ok\"; }",
    );

    assert_eq!(
        expr_type_in_function(&db, file, "opt", "cb"),
        "((int) -> string throws __effect_param_0) | null"
    );
}

/// The effect param survives narrowing: invoking the callback through a
/// null check calls it at the same synthetic effect.
#[test]
fn optional_callback_effect_survives_narrowing_and_invocation() {
    let mut db = make_db();
    let file = db.file(
        "callback.baml",
        r#"function apply_optional(callback: ((value: int) -> int)?, value: int) -> int {
  if (callback != null) {
    return callback(value)
  }
  value
}"#,
    );

    insta::assert_snapshot!(render_tir(&db, file), @r"
    function user.apply_optional(callback: ((value: int) -> int throws __effect_param_0) | null, value: int) -> int throws never {
      { : int
        if (callback != null : bool) : void
          { : never
            return callback<__effect_param_0>(value) : int
          }
        value : int
      }
    }
    block user.apply_optional {
    }
    ");
}

/// Per-call-site instantiation, the whole point of the effect param: a
/// nonthrowing callback resolves it to `never`, `null` leaves it
/// unconstrained (also `never`), and a throwing callback propagates its
/// precise error type to the caller.
#[test]
fn optional_callback_effect_instantiates_per_call_site() {
    let mut db = make_db();
    let file = db.file(
        "callback.baml",
        r#"class CallbackError {}

function apply_optional(callback: ((value: int) -> int)?, value: int) -> int {
  if (callback != null) {
    return callback(value)
  }
  value
}

function safe() -> int throws never {
  apply_optional((value: int) -> int { value + 1 }, 1)
}

function absent() -> int throws never {
  apply_optional(null, 1)
}

function risky() -> int throws never {
  apply_optional((value: int) -> int { throw CallbackError {} }, -1)
}"#,
    );

    let output = render_tir(&db, file);
    let violations: Vec<&str> = output
        .lines()
        .filter(|line| line.contains("declared throws"))
        // Drop the `!! <start>..<end>:` span prefix — this asserts on which
        // call sites violate and with what error type, not on byte offsets.
        .filter_map(|line| line.split_once(": ").map(|(_, message)| message))
        .collect();
    // Only `risky` violates, and it names the callback's PRECISE error type
    // — not `unknown`, which the `throws unknown` workaround would force.
    assert_eq!(
        violations,
        vec!["declared throws is `never`, but this function may also throw `CallbackError`"],
        "unexpected throws violations in:\n{output}"
    );
}

/// A contract violation inside the callee names the optional callback
/// parameter it flows from, exactly as the immediate form does.
#[test]
fn optional_callback_throws_violation_names_the_callback_param() {
    let mut db = make_db();
    let file = db.file(
        "callback.baml",
        r#"function opt(callback: ((value: int) -> int)?, value: int) -> int throws never {
  if (callback != null) {
    return callback(value)
  }
  value
}"#,
    );

    let output = render_tir(&db, file);
    assert!(
        output.contains("this body may throw through callback `callback`"),
        "expected the callback-named violation wording, got:\n{output}"
    );
}

/// The synthetic effect param rides the package interface, so a consumer
/// package instantiates it per call site too.
#[test]
fn function_type_throws_package_interface_exports_optional_effect_params() {
    let mut db = make_db();
    let file = db.file(
        "callback.baml",
        "function opt(cb: ((value: int) -> string)?) -> string { return \"ok\"; }",
    );

    let scope_id = find_function_scope_id(&db, file, "opt");
    let _ = baml_compiler2_hir_ty::ide::infer_for_scope(&db, scope_id);

    let iface = package_interface(&db, PackageId::new(&db, Name::new("user")));
    let exported = iface
        .lookup_function(&[], &Name::new("opt"))
        .expect("exported function");

    assert_eq!(
        exported.generic_params,
        vec![baml_type::ParamTy::new(0, Name::new("__effect_param_0"))]
    );
    assert_eq!(
        exported.params[0].ty.render_canonical(),
        "((value: int) -> string throws __effect_param_0) | null"
    );
}

/// The opening stops at the callback root. A function type nested any
/// deeper — a list element here — is a stored/structural position with no
/// single call site to instantiate against, and keeps its E0151.
#[test]
fn nested_function_types_below_the_callback_root_stay_unopened() {
    let mut db = make_db();
    let file = db.file(
        "callback.baml",
        "function listed(cbs: ((value: int) -> int)[]) -> int { return 0; }",
    );

    let output = render_tir(&db, file);
    assert!(
        output.contains("function type must declare an explicit `throws` clause"),
        "expected E0151 for a list of callbacks, got:\n{output}"
    );
    assert!(
        !output.contains("__effect_param_"),
        "a list element is not a callback root and must not open, got:\n{output}"
    );
}
