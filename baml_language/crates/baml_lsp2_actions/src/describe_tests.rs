//! Snapshot tests for `describe()`.

use crate::testing::ProjectTest;

fn make_multi_ns_project() -> ProjectTest {
    let mut builder = ProjectTest::builder();
    builder.source(
        "types.baml",
        r#"
class Point {
    x int
    y int
}
"#,
    );
    builder.source(
        "ns_llm/models.baml",
        r#"
class Config {
    model string
    temperature float
}
"#,
    );
    builder.build()
}

#[test]
fn describe_by_definition_class_in_namespace() {
    let project = make_multi_ns_project();
    let pkg_id =
        baml_compiler2_hir::package::PackageId::new(&project.db, baml_base::Name::new("user"));
    let pkg = baml_compiler2_hir::package::package_items(&project.db, pkg_id);

    let ns_path = vec![baml_base::Name::new("llm")];
    let item_name = baml_base::Name::new("Config");
    let def = pkg.lookup_type(&ns_path, &item_name).unwrap();

    let files = baml_compiler2_hir::compiler2_all_files(&project.db);
    let desc = crate::describe::describe_by_definition(&project.db, &files, def);
    assert!(desc.is_some());
    let desc = desc.unwrap();
    assert_eq!(desc.name, "Config");
    assert_eq!(desc.kind, crate::DefinitionKind::Class);
}

#[test]
fn describe_item_member_field() {
    let project = make_multi_ns_project();
    let pkg_id =
        baml_compiler2_hir::package::PackageId::new(&project.db, baml_base::Name::new("user"));
    let pkg = baml_compiler2_hir::package::package_items(&project.db, pkg_id);

    let root_ns: Vec<baml_base::Name> = vec![];
    let item_name = baml_base::Name::new("Point");
    let def = pkg.lookup_type(&root_ns, &item_name).unwrap();

    let files = baml_compiler2_hir::compiler2_all_files(&project.db);
    let desc = crate::describe::describe_item_member(&project.db, &files, def, "x");
    assert!(desc.is_some());
    let desc = desc.unwrap();
    assert_eq!(desc.name, "x");
    assert_eq!(desc.kind, crate::DefinitionKind::Field);
}

#[test]
fn describe_item_member_nonexistent() {
    let project = make_multi_ns_project();
    let pkg_id =
        baml_compiler2_hir::package::PackageId::new(&project.db, baml_base::Name::new("user"));
    let pkg = baml_compiler2_hir::package::package_items(&project.db, pkg_id);

    let root_ns: Vec<baml_base::Name> = vec![];
    let item_name = baml_base::Name::new("Point");
    let def = pkg.lookup_type(&root_ns, &item_name).unwrap();

    let files = baml_compiler2_hir::compiler2_all_files(&project.db);
    let desc = crate::describe::describe_item_member(&project.db, &files, def, "nonexistent");
    assert!(desc.is_none());
}

fn make_project() -> ProjectTest {
    let mut builder = ProjectTest::builder();
    builder.source(
        "types.baml",
        r#"
class Point {
    x int
    y int
}

class Person {
    name string
    age int
}

enum Color {
    Red,
    Green,
    Blue,
}
"#,
    );
    builder.source(
        "funcs.baml",
        r#"
/// Extract a point from text.
function ExtractPoint(text: string) -> Point {
    let result = Point { x: 0, y: 0 };
    return result;
}

function MakePerson(n: string, a: int) -> Person {
    return Person { name: n, age: a };
}

function UseColor(c: Color) -> string {
    match (c) {
        Red => "red"
        Green => "green"
        Blue => "blue"
    }
}
"#,
    );
    builder.build()
}

#[test]
fn describe_class() {
    let project = make_project();
    let descs = project.describe("Point");
    assert_eq!(descs.len(), 1);
    insta::assert_snapshot!(project.format_description(&descs[0]));
}

#[test]
fn describe_class_with_refs() {
    let project = make_project();
    let descs = project.describe("Person");
    assert_eq!(descs.len(), 1);
    insta::assert_snapshot!(project.format_description(&descs[0]));
}

#[test]
fn describe_enum() {
    let project = make_project();
    let descs = project.describe("Color");
    assert_eq!(descs.len(), 1);
    insta::assert_snapshot!(project.format_description(&descs[0]));
}

#[test]
fn describe_interface() {
    let mut builder = ProjectTest::builder();
    builder.source(
        "interfaces.baml",
        r#"
interface Named {
    name: string
    function label(self) -> string
}

class Person {
    name: string
    implements Named {
        function label(self) -> string {
            return self.name
        }
    }
}
"#,
    );
    let project = builder.build();

    let descs = project.describe("Named");
    assert_eq!(descs.len(), 1);
    insta::assert_snapshot!(project.format_description(&descs[0]));
}

#[test]
fn describe_function() {
    let project = make_project();
    let descs = project.describe("ExtractPoint");
    assert_eq!(descs.len(), 1);
    insta::assert_snapshot!(project.format_description(&descs[0]));
}

#[test]
fn describe_function_with_enum_param() {
    let project = make_project();
    let descs = project.describe("UseColor");
    assert_eq!(descs.len(), 1);
    insta::assert_snapshot!(project.format_description(&descs[0]));
}

#[test]
fn describe_nonexistent() {
    let project = make_project();
    let descs = project.describe("DoesNotExist");
    assert!(descs.is_empty());
}

#[test]
fn describe_builtin_string_with_compiler2_visible_files() {
    let project = make_project();
    let descs = project.describe_compiler2_visible("String");

    assert_eq!(descs.len(), 1);
    assert_eq!(descs[0].name, "String");
    insta::assert_snapshot!(project.format_description(&descs[0]));
}

#[test]
fn describe_builtin_deep_copy_with_compiler2_visible_files() {
    let project = make_project();
    let descs = project.describe_compiler2_visible("deep_copy");

    assert_eq!(descs.len(), 1);
    assert_eq!(descs[0].name, "deep_copy");
    insta::assert_snapshot!(project.format_description(&descs[0]));
}

#[test]
fn user_only_describe_still_does_not_search_builtins() {
    let project = make_project();
    let descs = project.describe("String");

    assert!(descs.is_empty());
}

#[test]
fn search_symbols_compiler2_visible_includes_selected_builtins() {
    let project = make_project();
    let files = baml_compiler2_hir::compiler2_all_files(&project.db);
    let symbols = crate::search::search_symbols(&project.db, &files, "");

    assert!(symbols.iter().any(|sym| sym.name == "String"));
    assert!(symbols.iter().any(|sym| sym.name == "Array"));
    assert!(symbols.iter().any(|sym| sym.name == "deep_copy"));
}

#[test]
fn describe_is_case_sensitive() {
    let project = make_project();
    // "point" should not match "Point"
    let descs = project.describe("point");
    assert!(descs.is_empty());
}

#[test]
fn describe_lambda_local_binding_uses_lambda_body() {
    let mut builder = ProjectTest::builder();
    builder.source(
        "lambda.baml",
        r#"
function LambdaLocalDescribe() -> string {
    let f = () -> string {
        let ignored = 1
        let target = "lambda"
        target
    }
    f()
}
"#,
    );
    let project = builder.build();

    let descs = project.describe("target");

    assert_eq!(descs.len(), 1);
    assert_eq!(descs[0].shape, "let target: string");
    assert_eq!(descs[0].resolved_type.as_deref(), Some("string"));
}
