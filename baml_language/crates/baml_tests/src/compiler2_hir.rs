//! Phase 2 tests for `baml_compiler2_hir`.
//!
//! Covers:
//! - Multi-file `package_items` aggregation
//! - Targeted unit tests: cross-file symbol merging
//! - Early-cutoff: comment-only changes don't re-run `namespace_items`

#[cfg(test)]
mod tests {
    use baml_base::Name;
    use baml_compiler2_hir::{
        file_semantic_index,
        loc::FunctionLoc,
        namespace::NamespaceId,
        package::{PackageId, package_items},
        signature::{
            elaborated_function_signature, function_parameter_defaults, function_signature,
        },
    };
    use baml_project::ProjectDatabase;
    use salsa::Setter;

    // ── Helper ────────────────────────────────────────────────────────────────

    /// Create a minimal test database with a project root at ".".
    fn make_db() -> ProjectDatabase {
        let mut db = ProjectDatabase::new();
        db.set_project_root(std::path::Path::new("."));
        db
    }

    fn find_function_loc<'db>(
        db: &'db ProjectDatabase,
        file: baml_base::SourceFile,
        name: &str,
    ) -> FunctionLoc<'db> {
        let item_tree = baml_compiler2_hir::file_item_tree(db, file);
        let local_id = item_tree
            .functions
            .iter()
            .find(|(_, func)| func.name.as_str() == name)
            .map(|(local_id, _)| *local_id)
            .unwrap_or_else(|| panic!("missing function {name}"));
        FunctionLoc::new(db, file, local_id)
    }

    fn find_method_loc<'db>(
        db: &'db ProjectDatabase,
        file: baml_base::SourceFile,
        class_name: &str,
        method_name: &str,
    ) -> FunctionLoc<'db> {
        let item_tree = baml_compiler2_hir::file_item_tree(db, file);
        let class = item_tree
            .classes
            .values()
            .find(|class| class.name.as_str() == class_name)
            .unwrap_or_else(|| panic!("missing class {class_name}"));
        let method_id = class
            .methods
            .iter()
            .find(|method_id| item_tree[**method_id].name.as_str() == method_name)
            .copied()
            .unwrap_or_else(|| panic!("missing method {class_name}.{method_name}"));
        FunctionLoc::new(db, file, method_id)
    }

    // ── 1. Targeted unit test: multi-file package_items ──────────────────────

    /// `user/a.baml` defines `class Foo`, `user/b.baml` defines `function bar` —
    /// `package_items(db, user_pkg)` contains both.
    #[test]
    fn package_items_merges_multiple_files() {
        let mut db = make_db();

        let _file_a = db.add_file("a.baml", "class Foo { name string }");
        let _file_b = db.add_file(
            "b.baml",
            "function bar(x: string) -> string { client GPT4\nprompt #\"hi\"# }",
        );

        let user_pkg_id = PackageId::new(&db, Name::new("user"));
        let items = package_items(&db, user_pkg_id);

        // Root namespace (empty path)
        let ns = items.namespaces.get(&vec![]).expect("user root namespace");

        // Foo should be in the type namespace
        assert!(
            ns.types.contains_key(&Name::new("Foo")),
            "Expected 'Foo' in type namespace. Got: {:?}",
            ns.types.keys().collect::<Vec<_>>()
        );

        // bar should be in the value namespace
        assert!(
            ns.values.contains_key(&Name::new("bar")),
            "Expected 'bar' in value namespace. Got: {:?}",
            ns.values.keys().collect::<Vec<_>>()
        );
    }

    /// Enums and type aliases appear in the type namespace.
    #[test]
    fn package_items_includes_enum_and_type_alias() {
        let mut db = make_db();
        let _f = db.add_file(
            "types.baml",
            "enum Color { Red\nGreen\nBlue }\ntype Str = string",
        );

        let pkg_id = PackageId::new(&db, Name::new("user"));
        let items = package_items(&db, pkg_id);
        let ns = items.namespaces.get(&vec![]).unwrap();

        assert!(
            ns.types.contains_key(&Name::new("Color")),
            "Expected Color enum"
        );
        assert!(
            ns.types.contains_key(&Name::new("Str")),
            "Expected Str type alias"
        );
    }

    /// Class methods are NOT contributed as top-level value symbols.
    #[test]
    fn class_methods_not_in_value_namespace() {
        let mut db = make_db();
        let _f = db.add_file(
            "methods.baml",
            "class MyClass {\n  name string\n  function helper(x: string) -> string { client C\nprompt #\"hi\"# }\n}",
        );

        let pkg_id = PackageId::new(&db, Name::new("user"));
        let items = package_items(&db, pkg_id);
        let ns = items.namespaces.get(&vec![]).unwrap();

        // The class itself should be in types
        assert!(
            ns.types.contains_key(&Name::new("MyClass")),
            "Expected MyClass"
        );
        // But the method should NOT be in the top-level value namespace
        assert!(
            !ns.values.contains_key(&Name::new("helper")),
            "helper() should NOT be a top-level value (it's a class method)"
        );
    }

    /// lookup_type and lookup_value helpers work correctly.
    #[test]
    fn package_items_lookup_helpers() {
        let mut db = make_db();
        let _f = db.add_file("lookup.baml", "class Point {}\nenum Dir { N\nS }");

        let pkg_id = PackageId::new(&db, Name::new("user"));
        let items = package_items(&db, pkg_id);

        assert!(
            items.lookup_type(&[], &Name::new("Point")).is_some(),
            "Point should resolve"
        );
        assert!(
            items.lookup_type(&[], &Name::new("Dir")).is_some(),
            "Dir should resolve"
        );
        assert!(
            items.lookup_type(&[], &Name::new("Missing")).is_none(),
            "Missing should not resolve"
        );
    }

    /// The new (namespace, item) API correctly handles namespace-qualified lookups.
    #[test]
    fn lookup_type_namespace_item_api() {
        let mut db = make_db();
        let _f1 = db.add_file("main.baml", "class Config { key string }");
        let _f2 = db.add_file("ns_llm/models.baml", "class Response { text string }");

        let pkg_id = PackageId::new(&db, Name::new("user"));
        let pkg_items = package_items(&db, pkg_id);

        // Response is only in ["llm"] namespace
        let response_name = Name::new("Response");
        assert!(
            pkg_items
                .lookup_type(&[Name::new("llm")], &response_name)
                .is_some(),
            "lookup_type(['llm'], 'Response') should find it"
        );
        assert!(
            pkg_items.lookup_type(&[], &response_name).is_none(),
            "lookup_type([], 'Response') should not find it in root"
        );

        // Config is only in root namespace
        let config_name = Name::new("Config");
        assert!(
            pkg_items.lookup_type(&[], &config_name).is_some(),
            "lookup_type([], 'Config') should find it in root"
        );
        assert!(
            pkg_items
                .lookup_type(&[Name::new("llm")], &config_name)
                .is_none(),
            "lookup_type(['llm'], 'Config') should not find it in llm namespace"
        );
    }

    // ── 2. namespace_items query ──────────────────────────────────────────────

    /// namespace_items for a specific NamespaceId returns the right symbols.
    #[test]
    fn namespace_items_for_user_root() {
        let mut db = make_db();
        let _f = db.add_file("ns.baml", "class Widget {}");

        let ns_id = NamespaceId::new(&db, Name::new("user"), vec![]);
        let ns = baml_compiler2_hir::namespace::namespace_items(&db, ns_id);

        assert!(ns.types.contains_key(&Name::new("Widget")));
    }

    // ── 3. file_item_tree Index access ────────────────────────────────────────

    /// The enriched ItemTree stores function params and return types.
    #[test]
    fn item_tree_stores_function_data() {
        let mut db = make_db();
        let file = db.add_file(
            "fn.baml",
            "function greet(name: string) -> string { client C\nprompt #\"hi\"# }",
        );

        let item_tree = baml_compiler2_hir::file_item_tree(&db, file);

        // Find the function in the item tree
        let func = item_tree
            .functions
            .values()
            .find(|f| f.name == Name::new("greet"));
        let func = func.expect("function 'greet' should be in item tree");

        assert_eq!(func.params.len(), 1, "greet should have 1 param");
        assert_eq!(
            func.params[0].name,
            Name::new("name"),
            "param name should be 'name'"
        );
        assert!(
            func.return_type.is_some(),
            "greet should have a return type"
        );
    }

    // ── 4. scope_bindings via FileSemanticIndex ───────────────────────────────

    /// Per-scope bindings are accessible from the FileSemanticIndex.
    /// The pre-interned ScopeId can be used to call scope_bindings_query.
    #[test]
    fn scope_bindings_returns_params_from_index() {
        let mut db = make_db();
        let file = db.add_file(
            "bindings.baml",
            "function add(a: int, b: int) -> int { client C\nprompt #\"hi\"# }",
        );

        let index = file_semantic_index(&db, file);

        // Find the function scope index
        let func_scope_idx = index
            .scopes
            .iter()
            .enumerate()
            .find(|(_, s)| matches!(s.kind, baml_compiler2_hir::scope::ScopeKind::Function));

        if let Some((i, _)) = func_scope_idx {
            // scope_bindings is directly accessible from the index (parallel vec)
            let bindings = &index.scope_bindings[i];
            assert_eq!(
                bindings.params.len(),
                2,
                "function 'add' should have 2 params"
            );
            // params are in order: a=0, b=1
            assert!(
                bindings
                    .params
                    .iter()
                    .any(|(n, idx)| n == &Name::new("a") && *idx == 0)
            );
            assert!(
                bindings
                    .params
                    .iter()
                    .any(|(n, idx)| n == &Name::new("b") && *idx == 1)
            );

            // scope_bindings_query also works using the pre-interned ScopeId
            let scope_id = index.scope_ids[i];
            let bindings2 = baml_compiler2_hir::scope_bindings_query(&db, scope_id);
            assert_eq!(bindings2.params.len(), 2);
        } else {
            panic!("No Function scope found in index");
        }
    }

    // ── 5. Duplicate name detection ─────────────────────────────────────────

    /// Two files defining `class Foo` in the same namespace produces a conflict.
    /// The first file alphabetically wins for resolution.
    #[test]
    fn duplicate_type_name_across_files_produces_conflict() {
        let mut db = make_db();
        let _file_a = db.add_file("a.baml", "class Foo { x int }");
        let _file_b = db.add_file("b.baml", "class Foo { y string }");

        let ns_id = NamespaceId::new(&db, Name::new("user"), vec![]);
        let ns = baml_compiler2_hir::namespace::namespace_items(&db, ns_id);

        // First wins (a.baml < b.baml alphabetically)
        assert!(ns.types.contains_key(&Name::new("Foo")));

        // Exactly one conflict for "Foo"
        assert_eq!(
            ns.conflicts().len(),
            1,
            "Expected 1 conflict, got: {:?}",
            ns.conflicts()
        );
        assert_eq!(ns.conflicts()[0].name, Name::new("Foo"));
        assert_eq!(ns.conflicts()[0].entries.len(), 2);
    }

    /// Three files all defining the same function name.
    #[test]
    fn duplicate_value_name_three_files() {
        let mut db = make_db();
        let _file_a = db.add_file(
            "a.baml",
            "function greet(x: string) -> string { client C\nprompt #\"hi\"# }",
        );
        let _file_b = db.add_file(
            "b.baml",
            "function greet(y: int) -> int { client C\nprompt #\"hey\"# }",
        );
        let _file_c = db.add_file(
            "c.baml",
            "function greet(z: bool) -> bool { client C\nprompt #\"yo\"# }",
        );

        let ns_id = NamespaceId::new(&db, Name::new("user"), vec![]);
        let ns = baml_compiler2_hir::namespace::namespace_items(&db, ns_id);

        // First wins
        assert!(ns.values.contains_key(&Name::new("greet")));

        // Five conflicts: greet, greet$render_prompt, greet$build_request,
        // greet$build_request_stream, greet$parse
        // Each LLM function expands to AST-level companions, all duplicated across 3 files.
        // ($stream and $parse_stream are PPIR-level and don't appear here.)
        assert_eq!(ns.conflicts().len(), 5);
        for conflict in ns.conflicts() {
            assert_eq!(conflict.entries.len(), 3);
        }
    }

    /// Different item kinds competing for the same type name (class vs enum).
    #[test]
    fn different_kinds_same_name_produces_conflict() {
        let mut db = make_db();
        let _file_a = db.add_file("a.baml", "class Thing { x int }");
        let _file_b = db.add_file("b.baml", "enum Thing { A\nB }");

        let ns_id = NamespaceId::new(&db, Name::new("user"), vec![]);
        let ns = baml_compiler2_hir::namespace::namespace_items(&db, ns_id);

        assert_eq!(ns.conflicts().len(), 1);
        let conflict = &ns.conflicts()[0];
        assert_eq!(conflict.name, Name::new("Thing"));

        // First alphabetically is a.baml (class), second is b.baml (enum)
        assert_eq!(conflict.entries[0].definition.kind_name(), "class");
        assert_eq!(conflict.entries[1].definition.kind_name(), "enum");

        // The resolved type should be the class (first wins)
        assert!(matches!(
            ns.types.get(&Name::new("Thing")),
            Some(baml_compiler2_hir::contributions::Definition::Class(_))
        ));
    }

    /// No conflict when names are unique across files.
    #[test]
    fn no_conflict_for_unique_names() {
        let mut db = make_db();
        let _file_a = db.add_file("a.baml", "class Foo { x int }");
        let _file_b = db.add_file("b.baml", "class Bar { y string }");

        let ns_id = NamespaceId::new(&db, Name::new("user"), vec![]);
        let ns = baml_compiler2_hir::namespace::namespace_items(&db, ns_id);

        assert!(ns.conflicts().is_empty());
    }

    /// Conflicts propagate to package_items.
    #[test]
    fn package_items_propagates_conflicts() {
        let mut db = make_db();
        let _file_a = db.add_file("a.baml", "class Dup {}");
        let _file_b = db.add_file("b.baml", "class Dup {}");

        let pkg_id = PackageId::new(&db, Name::new("user"));
        let items = package_items(&db, pkg_id);

        assert_eq!(items.conflicts().len(), 1);
        assert_eq!(items.conflicts()[0].name, Name::new("Dup"));

        // Resolution still works (first wins)
        let resolved = items.lookup_type(&[], &Name::new("Dup"));
        assert!(resolved.is_some());
    }

    /// Alphabetical file ordering is deterministic: a.baml always wins over z.baml.
    #[test]
    fn alphabetical_ordering_is_deterministic() {
        let mut db = make_db();
        // Add z.baml first, then a.baml — a.baml should still win
        let file_z = db.add_file("z.baml", "class Widget { z_field string }");
        let file_a = db.add_file("a.baml", "class Widget { a_field int }");

        let ns_id = NamespaceId::new(&db, Name::new("user"), vec![]);
        let ns = baml_compiler2_hir::namespace::namespace_items(&db, ns_id);

        assert_eq!(ns.conflicts().len(), 1);
        // The winner should be from a.baml
        let winner = ns.types.get(&Name::new("Widget")).unwrap();
        assert!(winner.file(&db) == file_a, "a.baml should win over z.baml");

        // Verify the conflict definitions are ordered: a.baml first, z.baml second
        assert!(ns.conflicts()[0].entries[0].definition.file(&db) == file_a);
        assert!(ns.conflicts()[0].entries[1].definition.file(&db) == file_z);
    }

    /// Same-file duplicates: enum Foo + class Foo in one file.
    #[test]
    fn same_file_duplicate_type_produces_conflict() {
        let mut db = make_db();
        let _file = db.add_file("mixed.baml", "enum Foo { A\nB }\nclass Foo { x int }");

        let ns_id = NamespaceId::new(&db, Name::new("user"), vec![]);
        let ns = baml_compiler2_hir::namespace::namespace_items(&db, ns_id);

        assert_eq!(ns.conflicts().len(), 1);
        assert_eq!(ns.conflicts()[0].name, Name::new("Foo"));
        assert_eq!(ns.conflicts()[0].entries.len(), 2);
        // enum appears first in source order
        assert_eq!(ns.conflicts()[0].entries[0].definition.kind_name(), "enum");
        assert_eq!(ns.conflicts()[0].entries[1].definition.kind_name(), "class");
    }

    /// Duplicate methods within a class produce a DuplicateDefinition diagnostic.
    #[test]
    fn duplicate_method_in_class_produces_diagnostic() {
        use baml_compiler2_hir::{contributions::DefinitionKind, diagnostic::Hir2Diagnostic};

        let mut db = make_db();
        let file = db.add_file(
            "dup_method.baml",
            "class Foo {\n  name string\n  function Bar(self) -> string { client C\nprompt #\"hi\"# }\n  function Bar(self) -> string { client C\nprompt #\"bye\"# }\n}",
        );

        let index = file_semantic_index(&db, file);
        let diags = index.diagnostics();

        let dups: Vec<_> = diags
            .iter()
            .filter(|d| matches!(d, Hir2Diagnostic::DuplicateDefinition { name, .. } if name == &Name::new("Bar")))
            .collect();
        assert_eq!(dups.len(), 1, "Expected 1 duplicate diagnostic for 'Bar'");

        let Hir2Diagnostic::DuplicateDefinition { name, scope, sites } = dups[0] else {
            panic!("expected DuplicateDefinition diagnostic");
        };
        assert_eq!(name, &Name::new("Bar"));
        assert_eq!(scope.as_ref().unwrap(), &Name::new("Foo"));
        assert_eq!(sites.len(), 2);
        assert!(sites.iter().all(|s| s.kind == DefinitionKind::Method));
    }

    /// Duplicate fields within a class produce a DuplicateDefinition diagnostic.
    #[test]
    fn duplicate_field_in_class_produces_diagnostic() {
        use baml_compiler2_hir::{contributions::DefinitionKind, diagnostic::Hir2Diagnostic};

        let mut db = make_db();
        let file = db.add_file(
            "dup_field.baml",
            "class Foo {\n  name string\n  name int\n}",
        );

        let index = file_semantic_index(&db, file);
        let diags = index.diagnostics();

        let dups: Vec<_> = diags
            .iter()
            .filter(|d| matches!(d, Hir2Diagnostic::DuplicateDefinition { name, .. } if name == &Name::new("name")))
            .collect();
        assert_eq!(dups.len(), 1);

        let Hir2Diagnostic::DuplicateDefinition { scope, sites, .. } = dups[0] else {
            panic!("expected DuplicateDefinition diagnostic");
        };
        assert_eq!(scope.as_ref().unwrap(), &Name::new("Foo"));
        assert_eq!(sites.len(), 2);
        assert!(sites.iter().all(|s| s.kind == DefinitionKind::Field));
    }

    /// Duplicate variants within an enum produce a DuplicateDefinition diagnostic.
    #[test]
    fn duplicate_variant_in_enum_produces_diagnostic() {
        use baml_compiler2_hir::{contributions::DefinitionKind, diagnostic::Hir2Diagnostic};

        let mut db = make_db();
        let file = db.add_file("dup_variant.baml", "enum Color { Red\nGreen\nRed }");

        let index = file_semantic_index(&db, file);
        let diags = index.diagnostics();

        let dups: Vec<_> = diags
            .iter()
            .filter(|d| matches!(d, Hir2Diagnostic::DuplicateDefinition { name, .. } if name == &Name::new("Red")))
            .collect();
        assert_eq!(dups.len(), 1);

        let Hir2Diagnostic::DuplicateDefinition { scope, sites, .. } = dups[0] else {
            panic!("expected DuplicateDefinition diagnostic");
        };
        assert_eq!(scope.as_ref().unwrap(), &Name::new("Color"));
        assert_eq!(sites.len(), 2);
        assert!(sites.iter().all(|s| s.kind == DefinitionKind::Variant));
    }

    /// Duplicate parameter names within a function signature — across any
    /// combination of positional and defaulted parameters — produce a
    /// `DuplicateDefinition` diagnostic.
    #[test]
    fn duplicate_function_param_produces_diagnostic() {
        use baml_compiler2_hir::{contributions::DefinitionKind, diagnostic::Hir2Diagnostic};

        let cases: &[(&str, &str, &str)] = &[
            (
                "positional/positional",
                "dup_param_pos_pos.baml",
                "function Foo(x: int, x: string) -> string { x }",
            ),
            (
                "positional/defaulted",
                "dup_param_pos_opt.baml",
                "function Foo(x: int, x: string = \"hi\") -> string { x }",
            ),
            (
                "defaulted/defaulted",
                "dup_param_opt_opt.baml",
                "function Foo(x: int = 0, x: string = \"hi\") -> string { x }",
            ),
        ];

        for (label, file_name, source) in cases {
            let mut db = make_db();
            let file = db.add_file(file_name, source);
            let index = file_semantic_index(&db, file);
            let diags = index.diagnostics();
            let dups: Vec<_> = diags
                .iter()
                .filter(|d| matches!(d, Hir2Diagnostic::DuplicateDefinition { name, .. } if name == &Name::new("x")))
                .collect();
            assert_eq!(
                dups.len(),
                1,
                "{label}: expected 1 duplicate diagnostic for 'x'"
            );
            let Hir2Diagnostic::DuplicateDefinition { scope, sites, .. } = dups[0] else {
                panic!("{label}: expected DuplicateDefinition diagnostic");
            };
            assert_eq!(scope.as_ref().unwrap(), &Name::new("Foo"), "{label}");
            assert_eq!(sites.len(), 2, "{label}");
            assert!(
                sites.iter().all(|s| s.kind == DefinitionKind::Parameter),
                "{label}: all sites should be Parameter kind"
            );
        }
    }

    /// Distinct parameter names in a function signature do not produce a
    /// duplicate diagnostic — regression guard against over-firing.
    #[test]
    fn distinct_function_params_have_no_duplicate_diagnostic() {
        use baml_compiler2_hir::diagnostic::Hir2Diagnostic;

        let mut db = make_db();
        let file = db.add_file(
            "distinct_params.baml",
            "function Foo(x: int, y: string = \"hi\") -> string { y }",
        );
        let index = file_semantic_index(&db, file);
        let diags = index.diagnostics();
        assert!(
            !diags
                .iter()
                .any(|d| matches!(d, Hir2Diagnostic::DuplicateDefinition { .. }))
        );
    }

    /// Same-scope let shadowing is legal and does not produce duplicate diagnostics.
    #[test]
    fn same_scope_let_shadowing_has_no_duplicate_diagnostic() {
        use baml_compiler2_hir::diagnostic::Hir2Diagnostic;

        let mut db = make_db();
        let file = db.add_file(
            "shadow_let.baml",
            "function foo() -> int {\n  let x = 1;\n  let x = 2;\n  return x;\n}",
        );

        let index = file_semantic_index(&db, file);
        let diags = index.diagnostics();

        assert!(!diags.iter().any(
            |d| matches!(d, Hir2Diagnostic::DuplicateDefinition { name, .. } if name == &Name::new("x"))
        ));
    }

    #[test]
    fn duplicate_array_pattern_bindings_produce_hir_diagnostic() {
        use baml_compiler2_hir::diagnostic::Hir2Diagnostic;

        let mut db = make_db();
        let file = db.add_file(
            "dup_array_pattern.baml",
            r#"
function foo(xs: int[]) -> int {
  let [let x, let x] = xs else { return 0 };
  x
}
"#,
        );

        let index = file_semantic_index(&db, file);
        let diags = index.diagnostics();
        assert!(diags.iter().any(
            |d| matches!(d, Hir2Diagnostic::DuplicatePatternBinding { name, .. } if name == &Name::new("x"))
        ));
    }

    #[test]
    fn duplicate_class_pattern_bindings_produce_hir_diagnostic() {
        use baml_compiler2_hir::diagnostic::Hir2Diagnostic;

        let mut db = make_db();
        let file = db.add_file(
            "dup_class_pattern.baml",
            r#"
class User {
  a int
  b int
}

function foo(user: User) -> int {
  let User { a: let x, b: let x } = user;
  x
}
"#,
        );

        let index = file_semantic_index(&db, file);
        let diags = index.diagnostics();
        assert!(diags.iter().any(
            |d| matches!(d, Hir2Diagnostic::DuplicatePatternBinding { name, .. } if name == &Name::new("x"))
        ));
    }

    #[test]
    fn duplicate_chain_alias_and_inner_binding_produce_hir_diagnostic() {
        use baml_compiler2_hir::diagnostic::Hir2Diagnostic;

        let mut db = make_db();
        let file = db.add_file(
            "dup_chain_pattern.baml",
            r#"
class User {
  name string
}

function foo(user: User) -> string {
  let x: User { name: let x } = user;
  x
}
"#,
        );

        let index = file_semantic_index(&db, file);
        let diags = index.diagnostics();
        assert!(diags.iter().any(
            |d| matches!(d, Hir2Diagnostic::DuplicatePatternBinding { name, .. } if name == &Name::new("x"))
        ));
    }

    #[test]
    fn shadowing_initializer_resolves_previous_binding() {
        use baml_compiler2_hir::scope::ScopeKind;
        use text_size::TextSize;

        let mut db = make_db();
        let file = db.add_file(
            "initializer_shadow.baml",
            "function foo() -> int {\n  let x = 1;\n  let x = x + 1;\n  x\n}",
        );

        let index = file_semantic_index(&db, file);
        let function_scope = index
            .scopes
            .iter()
            .enumerate()
            .find_map(|(idx, scope)| {
                matches!(scope.kind, ScopeKind::Function)
                    .then_some(baml_compiler2_hir::scope::FileScopeId::new(idx as u32))
            })
            .expect("function scope");
        let x_bindings = index.scope_bindings[function_scope.index() as usize]
            .bindings
            .iter()
            .filter(|binding| binding.name == Name::new("x"))
            .collect::<Vec<_>>();
        assert_eq!(x_bindings.len(), 2);

        let text = file.text(&db);
        let init_x_offset = TextSize::from(text.find("x + 1").expect("initializer x") as u32);
        let use_scope = index.scope_at_offset(init_x_offset, Some(&Name::new("foo")));
        let resolved = index
            .visible_binding_at(use_scope, init_x_offset, &Name::new("x"))
            .expect("initializer x should resolve");

        assert_eq!(
            index
                .local_binding(resolved)
                .expect("initializer x should resolve to a local binding")
                .site,
            x_bindings[0].site
        );
    }

    #[test]
    fn lambda_does_not_capture_its_own_nested_block_binding() {
        use baml_compiler2_hir::scope::ScopeKind;

        let mut db = make_db();
        let file = db.add_file(
            "lambda_local_block.baml",
            "function foo() -> int {\n  let f = () -> int {\n    { let x = 1; x }\n  };\n  f()\n}",
        );

        let index = file_semantic_index(&db, file);
        let lambda_scope = index
            .scopes
            .iter()
            .enumerate()
            .find_map(|(idx, scope)| {
                matches!(scope.kind, ScopeKind::Lambda)
                    .then_some(baml_compiler2_hir::scope::FileScopeId::new(idx as u32))
            })
            .expect("lambda scope");

        assert!(
            index.scope_bindings[lambda_scope.index() as usize]
                .captures
                .is_empty(),
            "lambda-local block binding should not be recorded as a capture"
        );
    }

    #[test]
    fn lambda_default_capture_is_owned_by_lambda_scope() {
        use baml_compiler2_hir::scope::ScopeKind;

        let mut db = make_db();
        let file = db.add_file(
            "lambda_default_capture.baml",
            "function foo() -> int {\n  let seed = 1;\n  let f = (x: int = seed) -> int { x };\n  f()\n}",
        );

        let index = file_semantic_index(&db, file);
        let lambda_scope = index
            .scopes
            .iter()
            .enumerate()
            .find_map(|(idx, scope)| {
                matches!(scope.kind, ScopeKind::Lambda)
                    .then_some(baml_compiler2_hir::scope::FileScopeId::new(idx as u32))
            })
            .expect("lambda scope");

        let captures = &index.scope_bindings[lambda_scope.index() as usize].captures;
        assert!(
            captures.iter().any(|(name, _)| name == &Name::new("seed")),
            "lambda default should record `seed` as a lambda capture, got: {captures:?}"
        );
    }

    /// A `let` inside a while body must not share the enclosing function's
    /// scope. The body is an `Expr::Block` which pushes its own scope, so
    /// the inner `let x = 99` must register in that scope, not the function
    /// scope.
    ///
    /// This test pins the desired invariant. Without `Stmt::While` walking the
    /// body inside its own block scope, find-references / rename / capture
    /// analysis would walk ancestors from the wrong starting scope.
    #[test]
    fn while_body_let_lives_in_inner_scope() {
        use baml_compiler2_hir::scope::ScopeKind;
        use text_size::TextSize;

        let mut db = make_db();
        let file = db.add_file(
            "while_scope.baml",
            "function foo() -> int {\n  let x = 1;\n  let once = true;\n  while (once) {\n    let x = 99;\n    once = false;\n  };\n  x\n}",
        );

        let index = file_semantic_index(&db, file);

        // Locate the function scope.
        let function_scope = index
            .scopes
            .iter()
            .enumerate()
            .find_map(|(idx, scope)| {
                matches!(scope.kind, ScopeKind::Function)
                    .then_some(baml_compiler2_hir::scope::FileScopeId::new(idx as u32))
            })
            .expect("function scope");

        // Find the offset of the inner `x = 99` token.
        let text = file.text(&db);
        let inner_x_decl = text.find("x = 99").expect("inner x decl");
        let inner_x_offset = TextSize::from(inner_x_decl as u32);

        let scope_at_inner = index.scope_at_offset(inner_x_offset, Some(&Name::new("foo")));
        assert_ne!(
            scope_at_inner, function_scope,
            "inner `let x = 99` inside while body must resolve to a non-function scope; got function scope"
        );

        // The inner binding must register in some descendant of the function
        // scope, not the function scope itself.
        let inner_bindings = index.scope_bindings[scope_at_inner.index() as usize]
            .bindings
            .iter()
            .filter(|b| b.name == Name::new("x"))
            .count();
        assert!(
            inner_bindings >= 1,
            "inner scope must contain the inner `x` binding"
        );

        // The outer `x = 1` binding must remain in the function scope.
        let outer_x_in_function = index.scope_bindings[function_scope.index() as usize]
            .bindings
            .iter()
            .filter(|b| b.name == Name::new("x"))
            .count();
        assert_eq!(
            outer_x_in_function, 1,
            "outer `let x = 1` must stay in function scope; got {} `x` bindings",
            outer_x_in_function
        );
    }

    /// Verify the HIR scope tree contains Block, MatchArm, CatchClause, and
    /// CatchArm scope kinds nested under a Function. A future regression
    /// that drops one of these kinds (e.g. a refactor that name-keys some
    /// lookup and "doesn't need" the explicit scope) will fail this test
    /// loudly.
    #[test]
    fn scope_tree_includes_block_match_catch_kinds() {
        use baml_compiler2_hir::scope::ScopeKind;

        let mut db = make_db();
        let file = db.add_file(
            "scope_kinds.baml",
            r#"function f(x: int) -> int {
  let _local = 1
  {
    let _block_local = 2
  }
  let _matched = match (x) {
    n => n
    _ => 0
  }
  try {
    1
  } catch (e) {
    _ => 2
  }
}"#,
        );

        let index = file_semantic_index(&db, file);

        // Each kind must appear at least once.
        let kinds: Vec<&ScopeKind> = index.scopes.iter().map(|s| &s.kind).collect();
        let has_kind = |kind: ScopeKind| {
            kinds
                .iter()
                .any(|k| std::mem::discriminant(*k) == std::mem::discriminant(&kind))
        };

        assert!(
            has_kind(ScopeKind::Function),
            "scope tree missing Function scope; kinds = {:?}",
            kinds
        );
        assert!(
            has_kind(ScopeKind::Block),
            "scope tree missing Block scope; kinds = {:?}",
            kinds
        );
        assert!(
            has_kind(ScopeKind::MatchArm),
            "scope tree missing MatchArm scope; kinds = {:?}",
            kinds
        );
        assert!(
            has_kind(ScopeKind::CatchClause),
            "scope tree missing CatchClause scope; kinds = {:?}",
            kinds
        );
        assert!(
            has_kind(ScopeKind::CatchArm),
            "scope tree missing CatchArm scope; kinds = {:?}",
            kinds
        );
    }

    /// A field and a method with the same name in a class produce a cross-kind diagnostic.
    #[test]
    fn field_method_same_name_produces_cross_kind_diagnostic() {
        use baml_compiler2_hir::{contributions::DefinitionKind, diagnostic::Hir2Diagnostic};

        let mut db = make_db();
        let file = db.add_file(
            "cross_kind.baml",
            "class Foo {\n  bar string\n  function bar(self) -> string { client C\nprompt #\"hi\"# }\n}",
        );

        let index = file_semantic_index(&db, file);
        let diags = index.diagnostics();

        let dups: Vec<_> = diags
            .iter()
            .filter(|d| matches!(d, Hir2Diagnostic::DuplicateDefinition { name, .. } if name == &Name::new("bar")))
            .collect();
        assert_eq!(dups.len(), 1, "Expected cross-kind duplicate for 'bar'");

        let Hir2Diagnostic::DuplicateDefinition { scope, sites, .. } = dups[0] else {
            panic!("expected DuplicateDefinition diagnostic");
        };
        assert_eq!(scope.as_ref().unwrap(), &Name::new("Foo"));
        assert_eq!(sites.len(), 2);
        let kinds: Vec<_> = sites.iter().map(|s| s.kind).collect();
        assert!(kinds.contains(&DefinitionKind::Field));
        assert!(kinds.contains(&DefinitionKind::Method));
    }

    #[test]
    fn builtin_only_rust_function_is_rejected_in_user_file() {
        use baml_compiler2_hir::diagnostic::Hir2Diagnostic;

        let mut db = make_db();
        let file = db.add_file(
            "user_rust_fn.baml",
            "function deep_copy<T>(value: T) -> T {\n  $rust_function\n}",
        );

        let index = file_semantic_index(&db, file);
        assert!(index.diagnostics().iter().any(|diag| {
            matches!(
                diag,
                Hir2Diagnostic::BuiltinOnlySyntax { feature, .. } if feature == "$rust_function"
            )
        }));
    }

    #[test]
    fn builtin_only_internal_attribute_is_rejected_in_user_file() {
        use baml_compiler2_hir::diagnostic::Hir2Diagnostic;

        let mut db = make_db();
        let file = db.add_file(
            "user_internal_attr.baml",
            "@@internal.uses(vm)\nfunction helper(value: string) -> string {\n  value\n}",
        );

        let index = file_semantic_index(&db, file);
        assert!(index.diagnostics().iter().any(|diag| {
            matches!(
                diag,
                Hir2Diagnostic::BuiltinOnlySyntax { feature, .. } if feature == "@@internal.uses"
            )
        }));
    }

    #[test]
    fn builtin_only_rust_type_is_rejected_in_user_file() {
        use baml_compiler2_hir::diagnostic::Hir2Diagnostic;

        let mut db = make_db();
        let file = db.add_file(
            "user_rust_type.baml",
            "class Response {\n  _body $rust_type\n}",
        );

        let index = file_semantic_index(&db, file);
        assert!(index.diagnostics().iter().any(|diag| {
            matches!(
                diag,
                Hir2Diagnostic::BuiltinOnlySyntax { feature, .. } if feature == "$rust_type"
            )
        }));
    }

    #[test]
    fn builtin_http_file_has_no_phase1_contract_diagnostics() {
        let db = make_db();
        let builtin = baml_compiler2_hir::compiler2_all_files(&db)
            .into_iter()
            .find(|file| file.path(&db) == std::path::Path::new("<builtin>/baml/ns_http/http.baml"))
            .expect("expected builtin http file");

        let index = file_semantic_index(&db, builtin);
        assert!(
            index.diagnostics().is_empty(),
            "expected builtin http file to be phase1-clean, got: {:?}",
            index.diagnostics()
        );
    }

    // ── Namespace derivation from ns_* folders ─────────────────────────────

    /// ns_llm/client.baml → namespace_path = ["llm"]
    #[test]
    fn file_package_ns_folder_creates_namespace() {
        use baml_compiler2_hir::file_package::file_package;

        let mut db = make_db();
        let file = db.add_file("ns_llm/client.baml", "class Foo {}");

        let pkg_info = file_package(&db, file);
        assert_eq!(pkg_info.package.as_str(), "user");
        assert_eq!(
            pkg_info.namespace_path,
            vec![Name::new("llm")],
            "ns_llm/ should create namespace ['llm']"
        );
    }

    /// ns_llm/helpers/utils.baml → namespace_path = ["llm"] (plain subfolder skipped)
    #[test]
    fn file_package_plain_subfolder_skipped() {
        use baml_compiler2_hir::file_package::file_package;

        let mut db = make_db();
        let file = db.add_file("ns_llm/helpers/utils.baml", "class Bar {}");

        let pkg_info = file_package(&db, file);
        assert_eq!(pkg_info.package.as_str(), "user");
        assert_eq!(
            pkg_info.namespace_path,
            vec![Name::new("llm")],
            "plain helpers/ subfolder should be skipped"
        );
    }

    /// ns_llm/ns_openai/client.baml → namespace_path = ["llm", "openai"]
    #[test]
    fn file_package_nested_ns_folders() {
        use baml_compiler2_hir::file_package::file_package;

        let mut db = make_db();
        let file = db.add_file("ns_llm/ns_openai/client.baml", "class Baz {}");

        let pkg_info = file_package(&db, file);
        assert_eq!(pkg_info.package.as_str(), "user");
        assert_eq!(
            pkg_info.namespace_path,
            vec![Name::new("llm"), Name::new("openai")],
            "nested ns_ folders should create ['llm', 'openai']"
        );
    }

    /// plain/folder/file.baml → namespace_path = [] (no ns_ folders)
    #[test]
    fn file_package_plain_folder_no_namespace() {
        use baml_compiler2_hir::file_package::file_package;

        let mut db = make_db();
        let file = db.add_file("plain/folder/file.baml", "class Qux {}");

        let pkg_info = file_package(&db, file);
        assert_eq!(pkg_info.package.as_str(), "user");
        assert!(
            pkg_info.namespace_path.is_empty(),
            "plain folders should not create namespaces"
        );
    }

    /// Flat file (no folder) → namespace_path = [] (unchanged behavior)
    #[test]
    fn file_package_flat_file_unchanged() {
        use baml_compiler2_hir::file_package::file_package;

        let mut db = make_db();
        let file = db.add_file("main.baml", "class Root {}");

        let pkg_info = file_package(&db, file);
        assert_eq!(pkg_info.package.as_str(), "user");
        assert!(
            pkg_info.namespace_path.is_empty(),
            "flat files should have empty namespace_path"
        );
    }

    /// ns_ with invalid identifier suffix is skipped (ns_123bad/)
    #[test]
    fn file_package_invalid_ns_name_skipped() {
        use baml_compiler2_hir::file_package::file_package;

        let mut db = make_db();
        let file = db.add_file("ns_123bad/file.baml", "class Bad {}");

        let pkg_info = file_package(&db, file);
        assert_eq!(pkg_info.package.as_str(), "user");
        assert!(
            pkg_info.namespace_path.is_empty(),
            "ns_ with non-identifier suffix should be skipped"
        );
    }

    /// Files in ns_llm/ are in a separate namespace from root files.
    #[test]
    fn namespace_items_separate_for_ns_folder() {
        let mut db = make_db();
        let _root_file = db.add_file("main.baml", "class Config { key string }");
        let _ns_file = db.add_file("ns_llm/models.baml", "class Response { text string }");

        let user_pkg_id = PackageId::new(&db, Name::new("user"));
        let items = package_items(&db, user_pkg_id);

        // Root namespace should have Config but not Response
        let root_ns = items.namespaces.get(&vec![]).expect("root namespace");
        assert!(root_ns.types.contains_key(&Name::new("Config")));
        assert!(!root_ns.types.contains_key(&Name::new("Response")));

        // "llm" namespace should have Response but not Config
        let llm_ns = items
            .namespaces
            .get(&vec![Name::new("llm")])
            .expect("llm namespace");
        assert!(llm_ns.types.contains_key(&Name::new("Response")));
        assert!(!llm_ns.types.contains_key(&Name::new("Config")));
    }

    /// Same symbol name in different namespaces does NOT conflict.
    #[test]
    fn same_name_different_namespaces_no_conflict() {
        let mut db = make_db();
        let _f1 = db.add_file("ns_llm/types.baml", "class Response { text string }");
        let _f2 = db.add_file("ns_http/types.baml", "class Response { status int }");

        let user_pkg_id = PackageId::new(&db, Name::new("user"));
        let items = package_items(&db, user_pkg_id);

        // No conflicts — different namespaces
        assert!(
            items.conflicts().is_empty(),
            "Same name in different namespaces should not conflict"
        );

        // Both namespaces have their own Response
        let llm_ns = items
            .namespaces
            .get(&vec![Name::new("llm")])
            .expect("llm namespace");
        assert!(llm_ns.types.contains_key(&Name::new("Response")));

        let http_ns = items
            .namespaces
            .get(&vec![Name::new("http")])
            .expect("http namespace");
        assert!(http_ns.types.contains_key(&Name::new("Response")));
    }

    /// Namespace name shadowing a root declaration is detected.
    #[test]
    fn namespace_shadows_root_declaration() {
        use baml_compiler2_hir::package::package_items;

        let mut db = make_db();
        let _root = db.add_file("main.baml", "class foo { x int }");
        let _ns = db.add_file("ns_foo/stuff.baml", "class Bar { y string }");

        let user_pkg_id = PackageId::new(&db, Name::new("user"));
        let items = package_items(&db, user_pkg_id);

        assert_eq!(
            items.shadows().len(),
            1,
            "Expected 1 shadow, got: {:?}",
            items.shadows()
        );
        assert_eq!(items.shadows()[0].ns_name, Name::new("foo"));
    }

    /// No shadow when namespace name doesn't collide with root declarations.
    #[test]
    fn no_shadow_when_names_distinct() {
        use baml_compiler2_hir::package::package_items;

        let mut db = make_db();
        let _root = db.add_file("main.baml", "class Config { x int }");
        let _ns = db.add_file("ns_llm/stuff.baml", "class Model { y string }");

        let user_pkg_id = PackageId::new(&db, Name::new("user"));
        let items = package_items(&db, user_pkg_id);

        assert!(
            items.shadows().is_empty(),
            "No shadow expected when names are distinct"
        );
    }

    // ── 9. Function signatures ──────────────────────────────────────────────

    #[test]
    fn function_signature_tracks_default_presence_not_default_expression() {
        let mut db = make_db();
        let file = db.add_file(
            "defaults.baml",
            "function f(required: string, optional: int = 41) -> string { return required; }",
        );

        let loc = find_function_loc(&db, file, "f");
        let sig_before = function_signature(&db, loc);
        assert!(!sig_before.params[0].has_default);
        assert!(sig_before.params[1].has_default);

        let defaults_before = function_parameter_defaults(&db, loc);
        assert!(defaults_before.param_default(0).is_none());
        let default_before = defaults_before
            .param_default(1)
            .expect("optional parameter default");
        assert_eq!(
            defaults_before
                .defaults
                .exprs
                .display_expr(default_before.expr.expr()),
            "41"
        );

        file.set_text(&mut db).to(
            "function f(required: string, optional: int = 42) -> string { return required; }"
                .to_string(),
        );

        let loc = find_function_loc(&db, file, "f");
        let sig_after = function_signature(&db, loc);
        assert_eq!(sig_before, sig_after);

        let defaults_after = function_parameter_defaults(&db, loc);
        let default_after = defaults_after
            .param_default(1)
            .expect("optional parameter default");
        assert_eq!(
            defaults_after
                .defaults
                .exprs
                .display_expr(default_after.expr.expr()),
            "42"
        );
    }

    // ── 10. Elaborated function signatures ──────────────────────────────────

    #[test]
    fn function_type_throws_immediate_callback_param_opens() {
        let mut db = make_db();
        let file = db.add_file(
            "callback.baml",
            "function direct(cb: (value: int) -> string) -> string { return \"ok\"; }",
        );

        let sig = elaborated_function_signature(&db, find_function_loc(&db, file, "direct"));

        assert!(sig.user_generic_params.is_empty());
        assert_eq!(
            sig.synthetic_effect_params,
            vec![Name::new("__effect_param_0")]
        );
        assert_eq!(
            sig.params[0].ty.to_string(),
            "(value: int) -> string throws __effect_param_0"
        );
    }

    #[test]
    fn function_type_throws_alias_hidden_callback_stays_closed() {
        let mut db = make_db();
        let file = db.add_file(
            "alias_hidden.baml",
            "type Handler = (value: int) -> string\nfunction use_alias(cb: Handler) -> string { return \"ok\"; }",
        );

        let sig = elaborated_function_signature(&db, find_function_loc(&db, file, "use_alias"));

        assert!(sig.synthetic_effect_params.is_empty());
        assert_eq!(sig.params[0].ty.to_string(), "Handler");
    }

    #[test]
    fn function_type_throws_nested_callback_position_stays_closed() {
        let mut db = make_db();
        let file = db.add_file(
            "nested.baml",
            "function nested(cb: ((value: int) -> string) -> string) -> string { return \"ok\"; }",
        );

        let sig = elaborated_function_signature(&db, find_function_loc(&db, file, "nested"));

        assert_eq!(
            sig.synthetic_effect_params,
            vec![Name::new("__effect_param_0")]
        );
        assert_eq!(
            sig.params[0].ty.to_string(),
            "((value: int) -> string throws never) -> string throws __effect_param_0"
        );
    }

    #[test]
    fn function_type_throws_return_position_stays_closed() {
        let mut db = make_db();
        let file = db.add_file(
            "returns_fn.baml",
            "function returns_handler() -> (value: int) -> string { return \"ok\"; }",
        );

        let sig =
            elaborated_function_signature(&db, find_function_loc(&db, file, "returns_handler"));

        assert!(sig.synthetic_effect_params.is_empty());
        assert_eq!(
            sig.return_type.as_ref().expect("return type").to_string(),
            "(value: int) -> string throws never"
        );
    }

    #[test]
    fn function_type_throws_return_position_opens_immediate_callback_surface() {
        let mut db = make_db();
        let file = db.add_file(
            "returns_wrapper.baml",
            "function returns_wrapper() -> ((value: int) -> string) -> string { return \"ok\"; }",
        );

        let sig =
            elaborated_function_signature(&db, find_function_loc(&db, file, "returns_wrapper"));

        assert_eq!(
            sig.synthetic_effect_params,
            vec![Name::new("__effect_param_0")]
        );
        assert_eq!(
            sig.return_type.as_ref().expect("return type").to_string(),
            "((value: int) -> string throws __effect_param_0) -> string throws __effect_param_0"
        );
    }

    #[test]
    fn function_type_throws_return_position_preserves_explicit_callback_throws() {
        let mut db = make_db();
        let file = db.add_file(
            "returns_explicit_wrapper.baml",
            "function returns_explicit_wrapper() -> ((value: int) -> string throws string) -> string { return \"ok\"; }",
        );

        let sig = elaborated_function_signature(
            &db,
            find_function_loc(&db, file, "returns_explicit_wrapper"),
        );

        assert!(sig.synthetic_effect_params.is_empty());
        assert_eq!(
            sig.return_type.as_ref().expect("return type").to_string(),
            "((value: int) -> string throws string) -> string throws string"
        );
    }

    #[test]
    fn function_type_throws_method_immediate_callback_param_opens() {
        let mut db = make_db();
        let file = db.add_file(
            "method_callback.baml",
            "class Box<T> {\n  value T\n  function run(cb: (value: T) -> string) -> string { return \"ok\"; }\n}",
        );

        let sig = elaborated_function_signature(&db, find_method_loc(&db, file, "Box", "run"));

        assert!(sig.user_generic_params.is_empty());
        assert_eq!(
            sig.synthetic_effect_params,
            vec![Name::new("__effect_param_0")]
        );
        assert_eq!(
            sig.params[0].ty.to_string(),
            "(value: T) -> string throws __effect_param_0"
        );
    }

    // ── 10. Early-cutoff: comment-only change ─────────────────────────────────

    /// Changing a comment in a file re-runs `file_semantic_index` (no_eq) and
    /// `namespace_items` (since it depends on file data), but because
    /// `namespace_items` produces the same result (PartialEq), `package_items`
    /// should NOT re-run — that's the Salsa early-cutoff.
    ///
    /// Query chain:
    ///   file_semantic_index (no_eq, always re-runs)
    ///     → namespace_items (re-runs, same result → early cutoff fires)
    ///       → package_items (skipped — no change detected upstream)
    #[test]
    fn comment_change_early_cutoff_skips_package_items() {
        use std::sync::{Arc, Mutex};

        let events = Arc::new(Mutex::new(Vec::<salsa::Event>::new()));
        let mut db = {
            let events = events.clone();
            let mut db = ProjectDatabase::new_with_event_callback(Box::new(move |e| {
                events.lock().unwrap().push(e);
            }));
            db.set_project_root(std::path::Path::new("."));
            db
        };

        let file = db.add_file("comment.baml", "class Foo {}");

        // First run: prime all caches.
        {
            let pkg_id = PackageId::new(&db, Name::new("user"));
            let _ = package_items(&db, pkg_id);
        }

        // Add a comment — semantic symbol content unchanged.
        file.set_text(&mut db)
            .to("// a comment\nclass Foo {}".to_string());

        // Second run: collect executed queries.
        events.lock().unwrap().clear();
        {
            let pkg_id = PackageId::new(&db, Name::new("user"));
            let _ = package_items(&db, pkg_id);
        }

        let executed: Vec<String> = {
            let guard = events.lock().unwrap();
            guard
                .iter()
                .filter_map(|e| {
                    if let salsa::EventKind::WillExecute { database_key } = &e.kind {
                        let name = (&db as &dyn salsa::Database)
                            .ingredient_debug_name(database_key.ingredient_index());
                        Some(name.to_string())
                    } else {
                        None
                    }
                })
                .collect()
        };

        // file_semantic_index must re-run (it's no_eq — always re-runs on change)
        assert!(
            executed.iter().any(|s| s.contains("file_semantic_index")),
            "file_semantic_index should re-run after file change. Got: {:?}",
            executed
        );

        // namespace_items re-runs because it depends on file_semantic_index indirectly,
        // but it produces the same result → triggers early-cutoff for dependents.

        // package_items should NOT re-run — early-cutoff from namespace_items's PartialEq.
        assert!(
            !executed.iter().any(|s| s.contains("package_items")),
            "package_items should NOT re-run on comment-only change (early cutoff). Got: {:?}",
            executed
        );
    }
}
