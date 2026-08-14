use baml_base::Name;
use baml_compiler2_hir::{
    item_tree::{ItemSpans, TestArgValue},
    loc::TestLoc,
};

/// Semantic data for a `test` declaration.
///
/// Carries no type expressions, so there is no `TypeRefStore`; spans live in
/// the [`test_source_map`] twin.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestData {
    pub name: Name,
    /// The function(s) this test exercises.
    pub function_refs: Vec<Name>,
    pub args: Vec<(Name, TestArgValue)>,
}

#[salsa::tracked(returns(ref))]
pub fn test_data<'db>(db: &'db dyn crate::Db, test: TestLoc<'db>) -> TestData {
    let item_tree = crate::file_item_tree(db, test.file(db));
    let data = &item_tree[test.id(db)];

    TestData {
        name: data.name.clone(),
        function_refs: data.function_refs.clone(),
        args: data.args.clone(),
    }
}

/// Declaration and name-token spans for one test. Kept separate from
/// [`test_data`] so a whitespace-only edit invalidates this but not the
/// semantic data.
#[salsa::tracked(returns(ref))]
pub fn test_source_map<'db>(db: &'db dyn crate::Db, test: TestLoc<'db>) -> ItemSpans {
    let item_source_map = crate::file_item_tree_source_map(db, test.file(db));
    item_source_map
        .test_spans
        .get(&test.id(db))
        .copied()
        .unwrap_or_else(|| unreachable!("spans recorded at allocation"))
}
