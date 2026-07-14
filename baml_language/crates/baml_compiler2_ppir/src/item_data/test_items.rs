use baml_base::Name;
use baml_compiler2_hir::{item_tree::TestArgValue, loc::TestLoc};

/// Semantic data for a `test` declaration.
///
/// Carries neither type expressions nor spans, so there is no source-map twin —
/// the data query alone is the firewall.
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
