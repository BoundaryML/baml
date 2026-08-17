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
    // See handles_tests::with_snapshot_dir: file snapshots need a real
    // directory when the binary was compiled in a build sandbox.
    crate::handles_tests::with_snapshot_dir(|| {
        insta::assert_snapshot!(serde_json::to_string_pretty(&export).unwrap());
    });
}

/// Every `id` in the document addresses exactly one *symbol*.
///
/// Consumers key on ids — a report diffs on them, a cache blesses on them — so
/// a collision is not a cosmetic flaw but a wrong answer about a different
/// symbol. The pressure is entirely on impl blocks: an inherited default is
/// re-listed by every implementor (13 impls inherit `baml.iter.Iterator.chain`),
/// which is why an impl entry is addressed through its block and keeps the
/// declaration in `declared_by`.
///
/// One symbol may legitimately appear twice, and the invariant is stated on
/// records rather than on strings for that reason: a method written in a class
/// body's `implements` block is both a method of the class and a method of the
/// block, and both views list it. What must never happen is one id covering two
/// *different* records, so equal ids are required to carry equal content.
#[test]
fn every_exported_id_is_unique() {
    let db = make_db();
    let json = serde_json::to_value(export_package(&db, Package::named(&db, "baml"))).unwrap();

    let mut ids: Vec<String> = Vec::new();
    let mut records: std::collections::HashMap<String, serde_json::Value> =
        std::collections::HashMap::new();
    let mut conflicts: Vec<String> = Vec::new();
    let mut collect = |value: &serde_json::Value| {
        if let Some(id) = value.get("id").and_then(serde_json::Value::as_str) {
            ids.push(id.to_string());
            match records.get(id) {
                Some(existing) if existing != value => conflicts.push(id.to_string()),
                Some(_) => {}
                None => {
                    records.insert(id.to_string(), value.clone());
                }
            }
        }
    };
    for item in json["items"].as_array().unwrap() {
        collect(item);
        for key in [
            "fields",
            "methods",
            "variants",
            "assoc_types",
            "required_methods",
            // An interface's defaults are a member list like any other, and
            // every entry carries an id. Omitting the key left a whole class of
            // member outside the invariant this test exists to state.
            "default_methods",
        ] {
            for member in item
                .get(key)
                .and_then(serde_json::Value::as_array)
                .into_iter()
                .flatten()
            {
                collect(member);
            }
        }
    }
    for block in json["impls"].as_array().unwrap() {
        collect(block);
        for method in block["methods"].as_array().unwrap() {
            collect(method);
        }
    }

    assert!(ids.len() > 1000, "the census actually walked the document");
    conflicts.sort();
    conflicts.dedup();
    assert!(
        conflicts.is_empty(),
        "{} id(s) cover more than one record, e.g. {:?}",
        conflicts.len(),
        &conflicts[..conflicts.len().min(5)]
    );

    // No `::` anywhere: it is Rust's path separator and appears nowhere in
    // BAML's grammar, so an id containing one can be neither pasted into
    // `describe` nor written in source. Impl entries were once addressed
    // `<block id>::<name>`.
    let rustish: Vec<&String> = ids.iter().filter(|id| id.contains("::")).collect();
    assert!(
        rustish.is_empty(),
        "{} id(s) are spelled with `::`, e.g. {:?}",
        rustish.len(),
        &rustish[..rustish.len().min(5)]
    );
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

    // An interface exports the parameters it declares, and only those. The
    // in-scope view leads with the implicit `Self`, which belongs to every
    // interface and so describes none of them; exporting it would read as
    // `interface Add<Self, Rhs>`.
    let add = find("T:baml.ops.Add");
    let add_generics: Vec<&str> = add["generics"]
        .as_array()
        .unwrap()
        .iter()
        .map(|g| g["name"].as_str().unwrap())
        .collect();
    assert_eq!(add_generics, ["Rhs"], "Add declares Rhs alone");
    assert_eq!(
        add["generics"][0]["bounds"][0], "baml.Concrete",
        "the parameter's bound comes with it"
    );
    assert!(
        comparable["generics"].as_array().is_none_or(Vec::is_empty),
        "an interface with no declared parameters exports none"
    );
    // Associated types are exported as members of the interface that owns them.
    let sortable = find("T:baml.Sortable");
    assert!(
        sortable["assoc_types"]
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a["name"] == "SortError"),
        "Sortable carries SortError"
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
