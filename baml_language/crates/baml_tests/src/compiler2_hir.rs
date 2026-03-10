//! Phase 2 tests for `baml_compiler2_hir`.
//!
//! Covers:
//! - Multi-file `package_items` aggregation
//! - Targeted unit tests: cross-file symbol merging
//! - Early-cutoff: comment-only changes don't re-run `namespace_items`

#[cfg(test)]
mod tests {
    use baml_base::Name;
    use baml_compiler2_hir::{namespace::NamespaceId, package::PackageId};
    use baml_compiler2_ppir::{file_semantic_index, package_items};
    use baml_project::ProjectDatabase;
    use salsa::Setter;

    // ── Helper ────────────────────────────────────────────────────────────────

    /// Create a minimal test database with a project root at ".".
    fn make_db() -> ProjectDatabase {
        let mut db = ProjectDatabase::new();
        db.set_project_root(std::path::Path::new("."));
        db
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

        let point_path = vec![Name::new("Point")];
        let dir_path = vec![Name::new("Dir")];
        let missing_path = vec![Name::new("Missing")];

        assert!(
            items.lookup_type(&point_path).is_some(),
            "Point should resolve"
        );
        assert!(items.lookup_type(&dir_path).is_some(), "Dir should resolve");
        assert!(
            items.lookup_type(&missing_path).is_none(),
            "Missing should not resolve"
        );
    }

    // ── 2. namespace_items query ──────────────────────────────────────────────

    /// namespace_items for a specific NamespaceId returns the right symbols.
    #[test]
    fn namespace_items_for_user_root() {
        let mut db = make_db();
        let _f = db.add_file("ns.baml", "class Widget {}");

        let ns_id = NamespaceId::new(&db, Name::new("user"), vec![]);
        let ns = baml_compiler2_ppir::namespace_items(&db, ns_id);

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

        let item_tree = baml_compiler2_ppir::file_item_tree(&db, file);

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
            let bindings2 = baml_compiler2_ppir::scope_bindings_query(&db, scope_id);
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
        let ns = baml_compiler2_ppir::namespace_items(&db, ns_id);

        // First wins (a.baml < b.baml alphabetically)
        assert!(ns.types.contains_key(&Name::new("Foo")));

        // Two conflicts: "Foo" and "stream_Foo" (both defined in a.baml and b.baml)
        assert_eq!(
            ns.conflicts().len(),
            2,
            "Expected 2 conflicts (Foo + stream_Foo), got: {:?}",
            ns.conflicts()
        );
        let foo_conflict = ns
            .conflicts()
            .iter()
            .find(|c| c.name == Name::new("Foo"))
            .expect("Foo conflict");
        assert_eq!(foo_conflict.entries.len(), 2);
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
        let ns = baml_compiler2_ppir::namespace_items(&db, ns_id);

        // First wins
        assert!(ns.values.contains_key(&Name::new("greet")));

        // One conflict with 3 definitions
        assert_eq!(ns.conflicts().len(), 1);
        assert_eq!(ns.conflicts()[0].entries.len(), 3);
    }

    /// Different item kinds competing for the same type name (class vs enum).
    #[test]
    fn different_kinds_same_name_produces_conflict() {
        let mut db = make_db();
        let _file_a = db.add_file("a.baml", "class Thing { x int }");
        let _file_b = db.add_file("b.baml", "enum Thing { A\nB }");

        let ns_id = NamespaceId::new(&db, Name::new("user"), vec![]);
        let ns = baml_compiler2_ppir::namespace_items(&db, ns_id);

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
        let ns = baml_compiler2_ppir::namespace_items(&db, ns_id);

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

        // 2 conflicts: "Dup" and "stream_Dup" (both defined in a.baml and b.baml)
        assert_eq!(items.conflicts().len(), 2);
        let dup_conflict = items
            .conflicts()
            .iter()
            .find(|c| c.name == Name::new("Dup"))
            .expect("Dup conflict");
        assert_eq!(dup_conflict.name, Name::new("Dup"));

        // Resolution still works (first wins)
        let resolved = items.lookup_type(&[Name::new("Dup")]);
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
        let ns = baml_compiler2_ppir::namespace_items(&db, ns_id);

        // 2 conflicts: "Widget" and "stream_Widget"
        assert_eq!(ns.conflicts().len(), 2);
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
        let ns = baml_compiler2_ppir::namespace_items(&db, ns_id);

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

        let Hir2Diagnostic::DuplicateDefinition { name, scope, sites } = dups[0];
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

        // Filter for duplicate "name" field within "Foo" (not "stream_Foo")
        let dups: Vec<_> = diags
            .iter()
            .filter(|d| matches!(d, Hir2Diagnostic::DuplicateDefinition { name, scope, .. } if name == &Name::new("name") && scope.as_ref().is_some_and(|s| s == &Name::new("Foo"))))
            .collect();
        assert_eq!(dups.len(), 1);

        let Hir2Diagnostic::DuplicateDefinition { scope, sites, .. } = dups[0];
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

        let Hir2Diagnostic::DuplicateDefinition { scope, sites, .. } = dups[0];
        assert_eq!(scope.as_ref().unwrap(), &Name::new("Color"));
        assert_eq!(sites.len(), 2);
        assert!(sites.iter().all(|s| s.kind == DefinitionKind::Variant));
    }

    /// Duplicate let-bindings in the same function produce a DuplicateDefinition diagnostic.
    #[test]
    fn duplicate_let_binding_produces_diagnostic() {
        use baml_compiler2_hir::{contributions::DefinitionKind, diagnostic::Hir2Diagnostic};

        let mut db = make_db();
        let file = db.add_file(
            "dup_let.baml",
            "function foo() -> int {\n  let x = 1;\n  let x = 2;\n  return x;\n}",
        );

        let index = file_semantic_index(&db, file);
        let diags = index.diagnostics();

        let dups: Vec<_> = diags
            .iter()
            .filter(|d| matches!(d, Hir2Diagnostic::DuplicateDefinition { name, .. } if name == &Name::new("x")))
            .collect();
        assert_eq!(dups.len(), 1);

        let Hir2Diagnostic::DuplicateDefinition { scope, sites, .. } = dups[0];
        assert_eq!(scope.as_ref().unwrap(), &Name::new("foo"));
        assert_eq!(sites.len(), 2);
        assert!(sites.iter().all(|s| s.kind == DefinitionKind::Binding));
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

        let Hir2Diagnostic::DuplicateDefinition { scope, sites, .. } = dups[0];
        assert_eq!(scope.as_ref().unwrap(), &Name::new("Foo"));
        assert_eq!(sites.len(), 2);
        let kinds: Vec<_> = sites.iter().map(|s| s.kind).collect();
        assert!(kinds.contains(&DefinitionKind::Field));
        assert!(kinds.contains(&DefinitionKind::Method));
    }

    // ── 9. Early-cutoff: comment-only change ──────────────────────────────────

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
