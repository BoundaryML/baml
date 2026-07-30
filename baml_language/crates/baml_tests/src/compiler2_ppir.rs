//! Focused regressions for PPIR package aggregation.

#[cfg(test)]
mod tests {
    use baml_base::Name;
    use baml_compiler2_hir::package::PackageId;
    use baml_project::ProjectDatabase;

    fn namespace_iteration_order(files: &[&str]) -> Vec<Vec<String>> {
        let mut db = ProjectDatabase::new();
        db.set_project_root(std::path::Path::new("."));

        for (index, namespace) in files.iter().enumerate() {
            db.add_file(
                format!("ns_{namespace}/marker_{index}.baml"),
                &format!("class Marker{index} {{}}"),
            );
        }

        let package =
            baml_compiler2_ppir::package_items(&db, PackageId::new(&db, Name::new("user")));
        package
            .namespaces
            .keys()
            .map(|path| path.iter().map(ToString::to_string).collect())
            .collect()
    }

    #[test]
    fn package_namespace_iteration_is_independent_of_file_discovery_order() {
        let namespaces = [
            "zulu", "alpha", "quebec", "bravo", "papa", "charlie", "oscar", "delta", "november",
            "echo", "mike", "foxtrot", "lima", "golf", "kilo", "hotel", "juliet", "india", "alpha",
            "zulu", "golf",
        ];
        let expected = namespace_iteration_order(&namespaces);
        assert_eq!(
            expected.len(),
            18,
            "duplicate namespace paths must be removed"
        );

        let mut reversed = namespaces;
        reversed.reverse();
        assert_eq!(namespace_iteration_order(&reversed), expected);

        let mut rotated = namespaces;
        rotated.rotate_left(7);
        assert_eq!(namespace_iteration_order(&rotated), expected);
    }

    /// The unified body-owner queries (`body`/`body_source_map`/`body_scope`/
    /// `file_body_owners`) must agree with the per-kind queries they dispatch
    /// to.
    #[test]
    fn body_owner_queries_match_per_kind_queries() {
        use baml_compiler2_hir::body::{BodyOwnerId, FunctionBody, OwnerBody};

        let mut db = ProjectDatabase::new();
        db.set_project_root(std::path::Path::new("."));
        let file = db.add_file(
            "test.baml",
            "function f() -> int { 1 }\n\nfunction g() -> int { f() }\n",
        );

        let owners = baml_compiler2_ppir::file_body_owners(&db, file);
        let functions = baml_compiler2_ppir::item_data::file_functions(&db, file);
        assert_eq!(owners.len(), functions.len());

        for (&func_loc, &owner) in functions.iter().zip(owners.iter()) {
            assert_eq!(owner, BodyOwnerId::Function(func_loc));
            assert!(owner.file(&db) == file);

            let unified = baml_compiler2_ppir::body(&db, owner);
            let direct = baml_compiler2_ppir::function_body(&db, func_loc);
            assert!(matches!(direct.as_ref(), FunctionBody::Expr(_)));
            let OwnerBody::Function(unified_body) = &unified else {
                panic!("function owner must dispatch to the function body query");
            };
            assert_eq!(unified_body, &direct);
            assert_eq!(
                unified.expr_body(),
                match direct.as_ref() {
                    FunctionBody::Expr(body) => Some(body),
                    _ => None,
                }
            );

            assert_eq!(
                baml_compiler2_ppir::body_source_map(&db, owner),
                baml_compiler2_ppir::function_body_source_map(&db, func_loc)
            );
            assert!(
                baml_compiler2_ppir::body_scope(&db, owner)
                    == baml_compiler2_ppir::item_data::function_scope(&db, func_loc)
            );
        }
    }
}
