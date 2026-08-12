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
}
