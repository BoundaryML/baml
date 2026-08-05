//! Export-IR tests: a full-package snapshot over the smallest builtin
//! package, cross-link probes over `baml`, and byte determinism.

use baml_project::ProjectDatabase;

use crate::{Package, export_package};

fn make_db() -> ProjectDatabase {
    let mut db = ProjectDatabase::new();
    db.set_project_root(std::path::Path::new("."));
    db
}

/// The whole `assert` package, pretty-printed — small enough to review, and
/// it exercises functions, signatures, and throws end to end.
#[test]
fn assert_package_exports_fully() {
    let db = make_db();
    let export = export_package(&db, Package::named(&db, "assert"));
    // Without this the test still passes if `assert` stops resolving: the
    // export becomes an empty document and the snapshot is accepted as-is.
    assert!(
        !export.items.is_empty(),
        "the assert package resolves and exports items"
    );
    insta::assert_snapshot!(serde_json::to_string_pretty(&export).unwrap());
}

#[test]
fn export_is_byte_deterministic() {
    let db = make_db();
    let a = serde_json::to_string(&export_package(&db, Package::named(&db, "baml"))).unwrap();
    let b = serde_json::to_string(&export_package(&db, Package::named(&db, "baml"))).unwrap();
    assert_eq!(a, b);
}

#[test]
fn baml_package_export_cross_links() {
    let db = make_db();
    let export = export_package(&db, Package::named(&db, "baml"));
    let json = serde_json::to_value(&export).unwrap();
    let items = json["items"].as_array().unwrap();

    let find = |id: &str| {
        items
            .iter()
            .find(|item| item["id"] == id)
            .unwrap_or_else(|| panic!("missing item {id}"))
    };

    // The original false-gap, now a cross-link: baml.Int's impl list includes
    // the Comparable block, and that block's export carries `compare`.
    let int = find("T:baml.Int");
    let int_impls: Vec<&str> = int["impls"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    let comparable_impl = int_impls
        .iter()
        .find(|id| id.contains("baml.Comparable for int"))
        .unwrap_or_else(|| panic!("Int lists its Comparable impl: {int_impls:?}"));
    let impls = json["impls"].as_array().unwrap();
    let block = impls
        .iter()
        .find(|imp| imp["id"] == **comparable_impl)
        .expect("Comparable-for-int block is exported");
    assert!(
        block["methods"]
            .as_array()
            .unwrap()
            .iter()
            .any(|m| m["name"] == "compare"),
        "compare is listed"
    );

    // The generic Sortable impl attaches to Array with its symbolic binding.
    let array = find("T:baml.Array");
    let array_impls: Vec<&str> = array["impls"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert!(
        array_impls
            .iter()
            .any(|id| id.contains("baml.Sortable for T[]")),
        "Sortable attaches to Array: {array_impls:?}"
    );

    // Interface records list their implementors.
    let comparable = find("T:baml.Comparable");
    assert!(
        comparable["implementors"].as_array().unwrap().len() >= 4,
        "Comparable lists implementors"
    );
    // Required-method signature: Self stays symbolic in the export.
    let required = comparable["required_methods"].as_array().unwrap();
    let compare = required
        .iter()
        .find(|m| m["name"] == "compare")
        .expect("Comparable::compare is required");
    assert_eq!(
        compare["signature"]["throws"]["display"],
        "(Self as baml.Comparable).CompareError"
    );

    // Synthetic companions are present and flagged, never dropped.
    assert!(
        items
            .iter()
            .any(|item| item["synthetic"] == true
                && item["id"].as_str().unwrap().contains("$stream")),
        "synthetic $stream companions are listed and flagged"
    );

    // Docstrings survive.
    let string = find("T:baml.String");
    assert!(
        string["docstring"]
            .as_str()
            .unwrap()
            .contains("UTF-8 encoded string")
    );
}
