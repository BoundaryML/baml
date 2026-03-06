//! Phase 5 tests: `.baml` builtin stub files and HIR integration.
//!
//! Verifies that the builtin stub files are correctly loaded into the compiler2
//! HIR pipeline. After `set_project_root`, `package_items(db, "baml")` should
//! contain `Array`, `Map`, `String`, `Media`, `Request`, `Response`, and the
//! function declarations in `env`, `math`, and `sys` namespaces.

use std::fmt::Write;

use baml_base::Name;
use baml_compiler2_hir::{
    contributions::Definition,
    file_item_tree,
    package::{PackageId, package_items},
};
use baml_project::ProjectDatabase;

// ── Test helpers ─────────────────────────────────────────────────────────────

fn make_db() -> ProjectDatabase {
    let mut db = ProjectDatabase::new();
    db.set_project_root(std::path::Path::new("."));
    db
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
                    let item_tree = file_item_tree(db, class_loc.file(db));
                    let class_data = &item_tree[class_loc.id(db)];
                    let gp_str = if class_data.generic_params.is_empty() {
                        String::new()
                    } else {
                        format!(
                            "<{}>",
                            class_data
                                .generic_params
                                .iter()
                                .map(|n| n.as_str())
                                .collect::<Vec<_>>()
                                .join(", ")
                        )
                    };
                    let method_names: Vec<String> = class_data
                        .methods
                        .iter()
                        .map(|mid| item_tree.functions[mid].name.to_string())
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
                    let item_tree = file_item_tree(db, func_loc.file(db));
                    let func_data = &item_tree[func_loc.id(db)];
                    let gp_str = if func_data.generic_params.is_empty() {
                        String::new()
                    } else {
                        format!(
                            "<{}>",
                            func_data
                                .generic_params
                                .iter()
                                .map(|n| n.as_str())
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

    // Package "env" has get and get_or_panic (they call baml.env.get / baml.sys.panic)
    let env_pkg = PackageId::new(&db, Name::new("env"));
    let env_items = package_items(&db, env_pkg);
    let empty_ns: Vec<Name> = vec![];
    let env_root = env_items
        .namespaces
        .get(&empty_ns)
        .expect("env package should have root namespace");
    assert!(
        env_root.values.contains_key(&Name::new("get")),
        "env.get should be in env package"
    );
    assert!(
        env_root.values.contains_key(&Name::new("get_or_panic")),
        "env.get_or_panic should be in env package"
    );
}

#[test]
fn baml_package_contains_math_and_sys() {
    let db = make_db();
    let baml_pkg = PackageId::new(&db, Name::new("baml"));
    let items = package_items(&db, baml_pkg);

    let math_ns_path = vec![Name::new("math")];
    assert!(
        items.namespaces.contains_key(&math_ns_path),
        "baml.math namespace should exist"
    );

    let sys_ns_path = vec![Name::new("sys")];
    assert!(
        items.namespaces.contains_key(&sys_ns_path),
        "baml.sys namespace should exist"
    );

    let math_ns = &items.namespaces[&math_ns_path];
    assert!(
        math_ns.values.contains_key(&Name::new("trunc")),
        "baml.math.trunc should exist"
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

    let item_tree = file_item_tree(&db, class_loc.file(&db));
    let class_data = &item_tree[class_loc.id(&db)];

    assert_eq!(
        class_data.generic_params,
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

    let item_tree = file_item_tree(&db, class_loc.file(&db));
    let class_data = &item_tree[class_loc.id(&db)];

    assert_eq!(
        class_data.generic_params,
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

    let item_tree = file_item_tree(&db, class_loc.file(&db));
    let class_data = &item_tree[class_loc.id(&db)];

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

    let item_tree = file_item_tree(&db, class_loc.file(&db));
    let class_data = &item_tree[class_loc.id(&db)];

    let method_names: Vec<String> = class_data
        .methods
        .iter()
        .map(|mid| item_tree.functions[mid].name.to_string())
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

    let item_tree = file_item_tree(&db, class_loc.file(&db));
    let class_data = &item_tree[class_loc.id(&db)];

    let method_names: Vec<String> = class_data
        .methods
        .iter()
        .map(|mid| item_tree.functions[mid].name.to_string())
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
    let mut found_math = false;
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
        // env.baml is at <builtin>/baml/env/env.baml → namespace ["env"]
        if path_str == "<builtin>/baml/env/env.baml" {
            let pkg_info = file_package(&db, *file);
            assert_eq!(pkg_info.package.as_str(), "baml");
            assert_eq!(
                pkg_info.namespace_path,
                vec![Name::new("env")],
                "env/env.baml should be in baml.env namespace"
            );
            found_env = true;
        }
        // http.baml is at <builtin>/baml/http/http.baml → namespace ["http"]
        if path_str == "<builtin>/baml/http/http.baml" {
            let pkg_info = file_package(&db, *file);
            assert_eq!(pkg_info.package.as_str(), "baml");
            assert_eq!(
                pkg_info.namespace_path,
                vec![Name::new("http")],
                "http/http.baml should be in baml.http namespace"
            );
            found_http = true;
        }
        // math.baml is at <builtin>/baml/math/math.baml → namespace ["math"]
        if path_str == "<builtin>/baml/math/math.baml" {
            let pkg_info = file_package(&db, *file);
            assert_eq!(pkg_info.package.as_str(), "baml");
            assert_eq!(
                pkg_info.namespace_path,
                vec![Name::new("math")],
                "math/math.baml should be in baml.math namespace"
            );
            found_math = true;
        }
        // sys.baml is at <builtin>/baml/sys/sys.baml → namespace ["sys"]
        if path_str == "<builtin>/baml/sys/sys.baml" {
            let pkg_info = file_package(&db, *file);
            assert_eq!(pkg_info.package.as_str(), "baml");
            assert_eq!(
                pkg_info.namespace_path,
                vec![Name::new("sys")],
                "sys/sys.baml should be in baml.sys namespace"
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
    assert!(found_math, "math.baml not found in compiler2 files");
    assert!(found_sys, "sys.baml not found in compiler2 files");
}

// ── 5.6: Ty::RustType is lowered from TypeExpr::Rust ─────────────────────────

#[test]
fn rust_type_field_lowers_to_rust_type() {
    use baml_compiler2_ast::TypeExpr;
    use baml_compiler2_hir::package::{PackageId, package_items};
    use baml_compiler2_tir::lower_type_expr::lower_type_expr;

    let db = make_db();
    let baml_pkg = PackageId::new(&db, Name::new("baml"));
    let items = package_items(&db, baml_pkg);

    // Lower $rust_type — should produce Ty::RustType
    let mut diags = Vec::new();
    let ty = lower_type_expr(&db, &TypeExpr::Rust, &items, &mut diags);

    assert_eq!(ty, baml_compiler2_tir::ty::Ty::RustType);
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
