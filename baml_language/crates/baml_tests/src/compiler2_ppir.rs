//! Focused regressions for PPIR package aggregation.

#[cfg(test)]
mod tests {
    use baml_artifact::ArtifactKind;
    use baml_base::Name;
    use baml_compiler2_hir::package::PackageId;
    use baml_compiler2_hir_ty::package_interface::PackageInterface;
    use baml_db::ProjectDatabase;

    use crate::engine::TestDbExt;

    fn namespace_iteration_order(files: &[&str]) -> Vec<Vec<String>> {
        let mut db = ProjectDatabase::new();
        db.workspace(std::path::Path::new("."));

        for (index, namespace) in files.iter().enumerate() {
            db.file(
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

    fn first_seen_namespace_order(files: &[&str]) -> Vec<Vec<String>> {
        let mut expected = Vec::new();
        for namespace in files {
            let path = vec![namespace.to_string()];
            if !expected.contains(&path) {
                expected.push(path);
            }
        }
        expected
    }

    #[test]
    fn package_namespace_iteration_preserves_file_discovery_order() {
        let namespaces = [
            "zulu", "alpha", "quebec", "bravo", "papa", "charlie", "oscar", "delta", "november",
            "echo", "mike", "foxtrot", "lima", "golf", "kilo", "hotel", "juliet", "india", "alpha",
            "zulu", "golf",
        ];
        let expected = first_seen_namespace_order(&namespaces);
        assert_eq!(namespace_iteration_order(&namespaces), expected);
        assert_eq!(
            expected.len(),
            18,
            "duplicate namespace paths must be removed"
        );

        let mut reversed = namespaces;
        reversed.reverse();
        assert_eq!(
            namespace_iteration_order(&reversed),
            first_seen_namespace_order(&reversed)
        );

        let mut rotated = namespaces;
        rotated.rotate_left(7);
        assert_eq!(
            namespace_iteration_order(&rotated),
            first_seen_namespace_order(&rotated)
        );
    }

    #[test]
    fn package_interface_artifacts_preserve_declaration_order_and_reject_v1() {
        let mut db = ProjectDatabase::new();
        db.workspace(std::path::Path::new("."));
        db.file(
            "types.baml",
            "class Zulu {}\nclass Alpha {}\nclass Mike {}\n",
        );

        let package_id = PackageId::new(&db, Name::new("user"));
        let interface =
            baml_compiler2_hir_ty::package_interface::package_interface(&db, package_id);
        let artifact = baml_artifact::encode(ArtifactKind::PackageInterface, &*interface)
            .expect("package interface encodes");
        let decoded: PackageInterface =
            baml_artifact::decode(ArtifactKind::PackageInterface, &artifact)
                .expect("current package interface decodes");
        let declaration_order: Vec<_> = decoded.types[&Vec::<Name>::new()]
            .keys()
            .map(Name::as_str)
            .filter(|name| !name.contains('$'))
            .collect();
        assert_eq!(declaration_order, ["Zulu", "Alpha", "Mike"]);

        let legacy = baml_artifact::encode_with_format_for_test(
            1,
            ArtifactKind::PackageInterface,
            &*interface,
        )
        .expect("legacy package interface envelope encodes");
        assert!(matches!(
            baml_artifact::decode::<PackageInterface>(ArtifactKind::PackageInterface, &legacy),
            Err(baml_artifact::Error::Incompatible {
                artifact_format: 1,
                runtime_format: 2,
                ..
            })
        ));
    }

    /// The unified body-owner queries (`body`/`body_source_map`/`body_scope`/
    /// `file_body_owners`) must agree with the per-kind queries they dispatch
    /// to.
    #[test]
    fn body_owner_queries_match_per_kind_queries() {
        use baml_compiler2_hir::body::{BodyOwnerId, FunctionBody, OwnerBody};

        let mut db = ProjectDatabase::new();
        db.workspace(std::path::Path::new("."));
        let file = db.file(
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
