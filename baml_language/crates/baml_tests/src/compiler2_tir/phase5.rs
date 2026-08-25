//! Phase 5 tests: `.baml` builtin stub files and HIR integration.
//!
//! Verifies that the builtin stub files are correctly loaded into the compiler2
//! HIR pipeline. After `set_project_root`, `package_items(db, "baml")` should
//! contain `Array`, `Map`, `String`, `Media`, `Request`, `Response`, and the
//! function declarations in `env`, `math`, and `sys` namespaces.

use std::fmt::Write;

/// Lower a raw AST type expression through hir's scratch store and
/// hir_ty's one lowering road (the MIR pattern), to a plain type plus
/// the sink's lowering diagnostics. Resolution context comes from
/// `file`'s package/namespace.
fn lower_type_expr_hir_in(
    db: &baml_project::ProjectDatabase,
    file: baml_base::SourceFile,
    expr: &baml_compiler2_ast::TypeExpr,
) -> (
    baml_type::Ty,
    Vec<baml_compiler2_hir_ty::lower::LoweringDiag>,
) {
    let mut builder = baml_compiler2_hir::type_ref::TypeRefBuilder::new();
    let id = builder.lower(expr);
    let (store, _spans) = builder.finish();
    let ctx = baml_compiler2_hir_ty::lower::lower_ctx_for_file(db, file);
    let (ty, diagnostics) = ctx.lower_type_ref_with_diagnostics(&store, id);
    (ty.to_plain(), diagnostics)
}

fn lower_type_expr_hir(
    db: &baml_project::ProjectDatabase,
    expr: &baml_compiler2_ast::TypeExpr,
) -> baml_type::Ty {
    let file = baml_compiler2_hir::compiler2_all_files(db)
        .into_iter()
        .next()
        .expect("db has at least one file");
    lower_type_expr_hir_in(db, file, expr).0
}

use baml_base::Name;
use baml_compiler2_hir::{
    contributions::Definition,
    package::{PackageId, package_items},
};
use baml_project::ProjectDatabase;

// ── Test helpers ─────────────────────────────────────────────────────────────

fn make_db() -> ProjectDatabase {
    let mut db = ProjectDatabase::new();
    db.set_project_root(std::path::Path::new("."));
    db
}

/// Declared generic parameter names, for assertions that care about the names
/// rather than the bounds.
fn generic_param_names(params: &[baml_compiler2_ppir::item_data::GenericParamData]) -> Vec<Name> {
    params.iter().map(|param| param.name.clone()).collect()
}

/// Build a sorted, human-readable summary of what `package_items(db, "baml")`
/// contains, separated by namespace.
fn render_baml_package_items(db: &ProjectDatabase) -> String {
    let baml_pkg = PackageId::new(db, Name::new("baml"));
    let items = package_items(db, baml_pkg);

    let mut output = String::new();

    // Sort namespace paths for deterministic output
    let mut ns_paths: Vec<_> = items.namespaces.keys().cloned().collect();
    ns_paths.sort();

    for ns_path in &ns_paths {
        let ns_items = &items.namespaces[ns_path];
        let ns_str = if ns_path.is_empty() {
            "baml".to_string()
        } else {
            format!(
                "baml.{}",
                ns_path
                    .iter()
                    .map(|n| n.as_str())
                    .collect::<Vec<_>>()
                    .join(".")
            )
        };

        writeln!(output, "namespace {ns_str}:").ok();

        // Sort type names
        let mut type_names: Vec<_> = ns_items.types.keys().cloned().collect();
        type_names.sort();
        for name in &type_names {
            let def = &ns_items.types[name];
            match def {
                Definition::Class(class_loc) => {
                    let class_data = baml_compiler2_ppir::item_data::class_data(db, *class_loc);
                    let gp_str = if class_data.generic_params.is_empty() {
                        String::new()
                    } else {
                        format!(
                            "<{}>",
                            class_data
                                .generic_params
                                .iter()
                                .map(|param| param.name.as_str())
                                .collect::<Vec<_>>()
                                .join(", ")
                        )
                    };
                    let method_names: Vec<String> = class_data
                        .methods
                        .iter()
                        .map(|mid| {
                            baml_compiler2_ppir::item_data::function_data(db, *mid)
                                .name
                                .to_string()
                        })
                        .collect();
                    writeln!(
                        output,
                        "  class {name}{gp_str} {{ methods: [{}] }}",
                        method_names.join(", ")
                    )
                    .ok();
                }
                Definition::Enum(_) => {
                    writeln!(output, "  enum {name}").ok();
                }
                _ => {
                    writeln!(output, "  type {name}").ok();
                }
            }
        }

        // Sort value names
        let mut value_names: Vec<_> = ns_items.values.keys().cloned().collect();
        value_names.sort();
        for name in &value_names {
            let def = &ns_items.values[name];
            match def {
                Definition::Function(func_loc) => {
                    let func_data = baml_compiler2_ppir::item_data::function_data(db, *func_loc);
                    let gp_str = if func_data.generic_params.is_empty() {
                        String::new()
                    } else {
                        format!(
                            "<{}>",
                            func_data
                                .generic_params
                                .iter()
                                .map(|param| param.name.as_str())
                                .collect::<Vec<_>>()
                                .join(", ")
                        )
                    };
                    writeln!(output, "  function {name}{gp_str}").ok();
                }
                _ => {
                    writeln!(output, "  value {name}").ok();
                }
            }
        }
    }

    output
}

// ── 5.1: package_items contains expected types and functions ─────────────────

#[test]
fn baml_package_contains_array_and_map() {
    let db = make_db();
    let baml_pkg = PackageId::new(&db, Name::new("baml"));
    let items = package_items(&db, baml_pkg);

    // Root namespace should have Array, Map, String
    let root_ns = items.namespaces.get(&vec![]);
    assert!(root_ns.is_some(), "baml root namespace should exist");
    let root_ns = root_ns.unwrap();

    assert!(
        root_ns.types.contains_key(&Name::new("Array")),
        "Array should be in baml root namespace"
    );
    assert!(
        root_ns.types.contains_key(&Name::new("Map")),
        "Map should be in baml root namespace"
    );
    assert!(
        root_ns.types.contains_key(&Name::new("String")),
        "String should be in baml root namespace"
    );

    // Media types are in the baml.media namespace
    let media_ns_path = vec![Name::new("media")];
    let media_ns = items.namespaces.get(&media_ns_path);
    assert!(media_ns.is_some(), "baml.media namespace should exist");
    let media_ns = media_ns.unwrap();
    for name in &["Image", "Audio", "Video", "Pdf"] {
        assert!(
            media_ns.types.contains_key(&Name::new(name)),
            "{name} should be in baml.media namespace"
        );
    }
}

#[test]
fn baml_package_contains_http_namespace() {
    let db = make_db();
    let baml_pkg = PackageId::new(&db, Name::new("baml"));
    let items = package_items(&db, baml_pkg);

    let http_ns_path = vec![Name::new("http")];
    let http_ns = items.namespaces.get(&http_ns_path);
    assert!(http_ns.is_some(), "baml.http namespace should exist");
    let http_ns = http_ns.unwrap();

    assert!(
        http_ns.types.contains_key(&Name::new("Request")),
        "Request should be in baml.http namespace"
    );
    assert!(
        http_ns.types.contains_key(&Name::new("Response")),
        "Response should be in baml.http namespace"
    );
}

#[test]
fn baml_package_contains_env_functions() {
    let db = make_db();
    let baml_pkg = PackageId::new(&db, Name::new("baml"));
    let items = package_items(&db, baml_pkg);

    // baml.env has the low-level get ($rust_io_function)
    let env_ns_path = vec![Name::new("env")];
    let env_ns = items.namespaces.get(&env_ns_path);
    assert!(env_ns.is_some(), "baml.env namespace should exist");
    let env_ns = env_ns.unwrap();
    assert!(
        env_ns.values.contains_key(&Name::new("get")),
        "baml.env.get (low-level) should be in baml.env namespace"
    );
    assert!(
        env_ns.values.contains_key(&Name::new("get_or_panic")),
        "baml.env.get_or_panic should be in baml.env namespace"
    );
}

#[test]
fn baml_package_has_sys_but_not_math() {
    let db = make_db();
    let baml_pkg = PackageId::new(&db, Name::new("baml"));
    let items = package_items(&db, baml_pkg);

    // B-712 removed the `baml.math` namespace entirely: its aggregates moved to
    // `float[]` methods (`sum`/`mean`/`median`) and `trunc` became the private
    // root helper `_trunc_to_int`.
    let math_ns_path = vec![Name::new("math")];
    assert!(
        !items.namespaces.contains_key(&math_ns_path),
        "baml.math namespace should no longer exist"
    );

    let sys_ns_path = vec![Name::new("sys")];
    assert!(
        items.namespaces.contains_key(&sys_ns_path),
        "baml.sys namespace should exist"
    );

    let sys_ns = &items.namespaces[&sys_ns_path];
    assert!(
        sys_ns.values.contains_key(&Name::new("shell")),
        "baml.sys.shell should exist"
    );
    assert!(
        sys_ns.values.contains_key(&Name::new("sleep")),
        "baml.sys.sleep should exist"
    );
    assert!(
        sys_ns.values.contains_key(&Name::new("panic")),
        "baml.sys.panic should exist"
    );
}

// ── 5.2: generic_params tests ────────────────────────────────────────────────

#[test]
fn array_has_generic_param_t() {
    let db = make_db();
    let baml_pkg = PackageId::new(&db, Name::new("baml"));
    let items = package_items(&db, baml_pkg);

    let root_ns = items.namespaces.get(&vec![]).unwrap();
    let array_def = root_ns.types.get(&Name::new("Array")).unwrap();
    let Definition::Class(class_loc) = array_def else {
        panic!("Array should be a class");
    };

    let class_data = baml_compiler2_ppir::item_data::class_data(&db, *class_loc);

    assert_eq!(
        generic_param_names(&class_data.generic_params),
        vec![Name::new("T")],
        "Array should have generic_params [T]"
    );
}

#[test]
fn map_has_generic_params_k_v() {
    let db = make_db();
    let baml_pkg = PackageId::new(&db, Name::new("baml"));
    let items = package_items(&db, baml_pkg);

    let root_ns = items.namespaces.get(&vec![]).unwrap();
    let map_def = root_ns.types.get(&Name::new("Map")).unwrap();
    let Definition::Class(class_loc) = map_def else {
        panic!("Map should be a class");
    };

    let class_data = baml_compiler2_ppir::item_data::class_data(&db, *class_loc);

    assert_eq!(
        generic_param_names(&class_data.generic_params),
        vec![Name::new("K"), Name::new("V")],
        "Map should have generic_params [K, V]"
    );
}

#[test]
fn string_class_has_no_generic_params() {
    let db = make_db();
    let baml_pkg = PackageId::new(&db, Name::new("baml"));
    let items = package_items(&db, baml_pkg);

    let root_ns = items.namespaces.get(&vec![]).unwrap();
    let string_def = root_ns.types.get(&Name::new("String")).unwrap();
    let Definition::Class(class_loc) = string_def else {
        panic!("String should be a class");
    };

    let class_data = baml_compiler2_ppir::item_data::class_data(&db, *class_loc);

    assert!(
        class_data.generic_params.is_empty(),
        "String should have no generic params"
    );
}

// ── 5.3: Array method lookup ──────────────────────────────────────────────────

#[test]
fn array_has_expected_methods() {
    let db = make_db();
    let baml_pkg = PackageId::new(&db, Name::new("baml"));
    let items = package_items(&db, baml_pkg);

    let root_ns = items.namespaces.get(&vec![]).unwrap();
    let array_def = root_ns.types.get(&Name::new("Array")).unwrap();
    let Definition::Class(class_loc) = array_def else {
        panic!("Array should be a class");
    };

    let class_data = baml_compiler2_ppir::item_data::class_data(&db, *class_loc);

    let method_names: Vec<String> = class_data
        .methods
        .iter()
        .map(|mid| {
            baml_compiler2_ppir::item_data::function_data(&db, *mid)
                .name
                .to_string()
        })
        .collect();

    let expected = [
        "length", "at", "push", "pop", "concat", "reverse", "slice", "join",
    ];
    for m in &expected {
        assert!(
            method_names.iter().any(|n| n == m),
            "Array should have method {m}, got: {method_names:?}"
        );
    }
}

#[test]
fn map_has_expected_methods() {
    let db = make_db();
    let baml_pkg = PackageId::new(&db, Name::new("baml"));
    let items = package_items(&db, baml_pkg);

    let root_ns = items.namespaces.get(&vec![]).unwrap();
    let map_def = root_ns.types.get(&Name::new("Map")).unwrap();
    let Definition::Class(class_loc) = map_def else {
        panic!("Map should be a class");
    };

    let class_data = baml_compiler2_ppir::item_data::class_data(&db, *class_loc);

    let method_names: Vec<String> = class_data
        .methods
        .iter()
        .map(|mid| {
            baml_compiler2_ppir::item_data::function_data(&db, *mid)
                .name
                .to_string()
        })
        .collect();

    let expected = ["length", "has", "keys", "values", "set", "get"];
    for m in &expected {
        assert!(
            method_names.iter().any(|n| n == m),
            "Map should have method {m}, got: {method_names:?}"
        );
    }
}

// ── 5.4: Snapshot test of baml package items ─────────────────────────────────

#[test]
fn snapshot_baml_package_items() {
    let db = make_db();
    let output = render_baml_package_items(&db);
    insta::assert_snapshot!(output);
}

// ── 5.5: file_package derivation for builtin paths ───────────────────────────

#[test]
fn file_package_derives_correct_namespaces() {
    use baml_compiler2_hir::file_package::file_package;

    let db = make_db();

    // The compiler2 extra files are NOT in project.files() (to avoid polluting
    // the v1 compiler). Use compiler2_all_files() to get the combined view.
    let files = baml_compiler2_hir::compiler2_all_files(&db);

    let mut found_containers = false;
    let mut found_env = false;
    let mut found_http = false;
    let mut found_sys = false;

    for file in &files {
        let path_str = file.path(&db).to_string_lossy().to_string();
        // containers.baml is at <builtin>/baml/containers.baml → namespace []
        if path_str == "<builtin>/baml/containers.baml" {
            let pkg_info = file_package(&db, *file);
            assert_eq!(pkg_info.package.as_str(), "baml");
            assert!(
                pkg_info.namespace_path.is_empty(),
                "containers.baml should be in root baml namespace, got {:?}",
                pkg_info.namespace_path
            );
            found_containers = true;
        }
        // env.baml is at <builtin>/baml/ns_env/env.baml → namespace ["env"]
        if path_str == "<builtin>/baml/ns_env/env.baml" {
            let pkg_info = file_package(&db, *file);
            assert_eq!(pkg_info.package.as_str(), "baml");
            assert_eq!(
                pkg_info.namespace_path,
                vec![Name::new("env")],
                "ns_env/env.baml should be in baml.env namespace"
            );
            found_env = true;
        }
        // http.baml is at <builtin>/baml/ns_http/http.baml → namespace ["http"]
        if path_str == "<builtin>/baml/ns_http/http.baml" {
            let pkg_info = file_package(&db, *file);
            assert_eq!(pkg_info.package.as_str(), "baml");
            assert_eq!(
                pkg_info.namespace_path,
                vec![Name::new("http")],
                "ns_http/http.baml should be in baml.http namespace"
            );
            found_http = true;
        }
        // sys.baml is at <builtin>/baml/ns_sys/sys.baml → namespace ["sys"]
        if path_str == "<builtin>/baml/ns_sys/sys.baml" {
            let pkg_info = file_package(&db, *file);
            assert_eq!(pkg_info.package.as_str(), "baml");
            assert_eq!(
                pkg_info.namespace_path,
                vec![Name::new("sys")],
                "ns_sys/sys.baml should be in baml.sys namespace"
            );
            found_sys = true;
        }
    }

    assert!(
        found_containers,
        "containers.baml not found in compiler2 files"
    );
    assert!(found_env, "env.baml not found in compiler2 files");
    assert!(found_http, "http.baml not found in compiler2 files");
    assert!(found_sys, "sys.baml not found in compiler2 files");
}

// ── 5.6: Ty::RustType is lowered from TypeExpr::Rust ─────────────────────────

#[test]
fn rust_type_field_lowers_to_rust_type() {
    let db = make_db();

    // Lower $rust_type — should produce Ty::RustType
    let ty = lower_type_expr_hir(
        &db,
        &baml_compiler2_ast::TypeExprKind::Rust { attrs: vec![] }.at(Default::default()),
    );
    let diags: Vec<()> = Vec::new();

    assert_eq!(
        ty,
        baml_type::Ty::RustType {
            attr: Default::default()
        }
    );
    assert!(diags.is_empty(), "No diagnostics expected for $rust_type");
}

// ── 5.7: Existing tests are not broken by builtin registration ───────────────

#[test]
fn user_package_unaffected_by_builtins() {
    // Verifies that adding baml_builtins2 files doesn't pollute the user package.
    use super::support::{make_db as make_tir_db, render_tir};

    let mut db = make_tir_db();
    let file = db.add_file("test.baml", "class Foo { name string }");

    // Render should only show user.Foo, not any baml builtins
    let output = render_tir(&db, file);
    assert!(output.contains("user.Foo"), "user.Foo should appear");
    assert!(
        !output.contains("baml.Array"),
        "baml.Array should not appear in user file TIR"
    );
}

// ── Cross-namespace type resolution via root.* ──────────────────────────

/// Multi-namespace project: root defines Config, ns_llm defines Response.
/// Root file uses root.llm.Response, ns_llm file uses root.Config.
/// All types should resolve without diagnostics.
#[test]
fn cross_namespace_type_resolution_via_root() {
    let mut db = make_db();

    // Root namespace: defines Config
    let _root_file = db.add_file("main.baml", "class Config { key string }");

    // llm namespace: defines Response
    let ns_file = db.add_file("ns_llm/models.baml", "class Response { text string }");

    // From root namespace: resolve root.llm.Response
    let segments = vec![Name::new("root"), Name::new("llm"), Name::new("Response")];
    let (ty, diags) = lower_type_expr_hir_in(
        &db,
        _root_file,
        &baml_compiler2_ast::TypeExprKind::Path {
            segments,
            generic_args: vec![],
            associated_type_bindings: vec![],
            attrs: vec![],
        }
        .at(Default::default()),
    );
    assert!(
        diags.is_empty(),
        "root.llm.Response should resolve without errors, got: {:?}",
        diags
    );
    assert!(
        !matches!(ty, baml_type::Ty::Unknown { .. }),
        "root.llm.Response should not resolve to Unknown"
    );

    // From llm namespace: resolve root.Config
    let segments = vec![Name::new("root"), Name::new("Config")];
    let (ty, diags) = lower_type_expr_hir_in(
        &db,
        ns_file,
        &baml_compiler2_ast::TypeExprKind::Path {
            segments,
            generic_args: vec![],
            associated_type_bindings: vec![],
            attrs: vec![],
        }
        .at(Default::default()),
    );
    assert!(
        diags.is_empty(),
        "root.Config should resolve from llm namespace without errors, got: {:?}",
        diags
    );
    assert!(
        !matches!(ty, baml_type::Ty::Unknown { .. }),
        "root.Config should not resolve to Unknown from llm namespace"
    );
}

/// Same-namespace resolution: types in the same ns_* folder resolve without root. prefix.
#[test]
fn same_namespace_resolution_no_prefix() {
    let mut db = make_db();

    // Both files in ns_llm namespace
    let _f1 = db.add_file("ns_llm/types.baml", "class LLMConfig { model string }");
    let _f2 = db.add_file("ns_llm/client.baml", "class LLMClient { name string }");

    // From within llm namespace: resolve LLMConfig (no root. prefix)
    let segments = vec![Name::new("LLMConfig")];
    let (ty, diags) = lower_type_expr_hir_in(
        &db,
        _f1,
        &baml_compiler2_ast::TypeExprKind::Path {
            segments,
            generic_args: vec![],
            associated_type_bindings: vec![],
            attrs: vec![],
        }
        .at(Default::default()),
    );
    let _ = diags;
    assert!(
        diags.is_empty(),
        "LLMConfig should resolve within same namespace, got: {:?}",
        diags
    );
    assert!(
        !matches!(ty, baml_type::Ty::Unknown { .. }),
        "LLMConfig should not resolve to Unknown within same namespace"
    );
}

/// Nested ns_* folders: ns_llm/ns_openai/ creates namespace ["llm", "openai"].
/// Resolve root.llm.openai.ResponsesClient from root namespace.
#[test]
fn nested_namespace_resolution() {
    let mut db = make_db();

    let _root_file = db.add_file("main.baml", "class Config { key string }");
    let _nested_file = db.add_file(
        "ns_llm/ns_openai/client.baml",
        "class ResponsesClient { model string }",
    );

    let pkg_id = PackageId::new(&db, Name::new("user"));
    let pkg_items = package_items(&db, pkg_id);

    // Verify the nested namespace exists
    assert!(
        pkg_items
            .namespaces
            .contains_key(&vec![Name::new("llm"), Name::new("openai")]),
        "Nested namespace ['llm', 'openai'] should exist"
    );

    // Resolve root.llm.openai.ResponsesClient from root namespace
    let segments = vec![
        Name::new("root"),
        Name::new("llm"),
        Name::new("openai"),
        Name::new("ResponsesClient"),
    ];
    let (ty, diags) = lower_type_expr_hir_in(
        &db,
        _root_file,
        &baml_compiler2_ast::TypeExprKind::Path {
            segments,
            generic_args: vec![],
            associated_type_bindings: vec![],
            attrs: vec![],
        }
        .at(Default::default()),
    );
    assert!(
        diags.is_empty(),
        "root.llm.openai.ResponsesClient should resolve, got: {:?}",
        diags
    );
    assert!(
        !matches!(ty, baml_type::Ty::Unknown { .. }),
        "root.llm.openai.ResponsesClient should not resolve to Unknown"
    );
}

#[test]
fn bare_name_cross_namespace_rejected() {
    // Config is in root, but ns_context is ["llm"] — bare "Config" should not resolve

    let mut db = make_db();
    let _root_file = db.add_file("main.baml", "class Config { key string }");
    let _ns_file = db.add_file("ns_llm/models.baml", "class Response { text string }");

    let segments = vec![Name::new("Config")];
    let (ty, diags) = lower_type_expr_hir_in(
        &db,
        _ns_file,
        &baml_compiler2_ast::TypeExprKind::Path {
            segments,
            generic_args: vec![],
            associated_type_bindings: vec![],
            attrs: vec![],
        }
        .at(Default::default()),
    );
    assert!(
        matches!(ty, baml_type::Ty::Error { .. }),
        "bare Config from ns_llm should not resolve (an unresolved name is the diagnosed \
         `!error` sentinel, never `unknown`)"
    );
    assert!(
        diags.len() == 1,
        "should emit exactly one diagnostic, got: {:?}",
        diags
    );
    let msg = baml_compiler2_hir_ty::lower::lowering_diag_error(&diags[0].kind).to_string();
    assert!(
        msg.contains("root.Config"),
        "diagnostic should suggest `root.Config`, got: {msg}"
    );
}

#[test]
fn multi_segment_bare_path_rejected() {
    // "ns2.MyClass" from ns1 without root. prefix should fail

    let mut db = make_db();
    let _f1 = db.add_file("ns_ns1/a.baml", "class Foo { x int }");
    let _f2 = db.add_file("ns_ns2/b.baml", "class MyClass { y string }");

    let segments = vec![Name::new("ns2"), Name::new("MyClass")];
    let (ty, diags) = lower_type_expr_hir_in(
        &db,
        _f1,
        &baml_compiler2_ast::TypeExprKind::Path {
            segments,
            generic_args: vec![],
            associated_type_bindings: vec![],
            attrs: vec![],
        }
        .at(Default::default()),
    );
    assert!(
        matches!(ty, baml_type::Ty::Error { .. }),
        "ns2.MyClass from ns1 should not resolve without root. prefix (an unresolved name \
         is the diagnosed `!error` sentinel, never `unknown`)"
    );
    assert!(!diags.is_empty(), "should emit UnresolvedType diagnostic");
    let msg = baml_compiler2_hir_ty::lower::lowering_diag_error(&diags[0].kind).to_string();
    assert!(
        msg.contains("unresolved type: ns2.MyClass"),
        "diagnostic should mention ns2.MyClass, got: {msg}"
    );
}
