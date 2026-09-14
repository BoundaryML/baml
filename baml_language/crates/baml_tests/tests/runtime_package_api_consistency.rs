//! BEP-066 Package reflection API consistency regressions.

use baml_compiler_diagnostics::Severity;
use baml_db::{ProjectDatabase, collect_diagnostics};
use baml_tests::{baml_test, engine::TestDbExt};
use bex_engine::BexExternalValue;

fn diagnostics(source: &str) -> Vec<(String, String)> {
    let mut db = ProjectDatabase::new();
    db.workspace(std::path::Path::new("."));
    db.file("static_probe.baml", source);
    collect_diagnostics(&db)
        .into_iter()
        .filter(|diagnostic| diagnostic.severity == Severity::Error)
        .map(|diagnostic| (diagnostic.code().to_string(), diagnostic.message))
        .collect()
}

#[test]
fn p8_requires_all_precise_type_finders_and_enumerators() {
    let source = r#"
function probe(pkg: reflect.Package) -> bool {
  pkg.get_enum("root.E") == null
    && pkg.get_interface("root.I") == null
    && pkg.enums().length() == 0
    && pkg.interfaces().length() == 0
}
"#;
    let mut db = ProjectDatabase::new();
    db.workspace(std::path::Path::new("."));
    db.file("probe.baml", source);
    let errors = collect_diagnostics(&db)
        .into_iter()
        .filter(|diagnostic| diagnostic.severity == Severity::Error)
        .map(|diagnostic| diagnostic.message)
        .collect::<Vec<_>>();
    assert!(
        errors.is_empty(),
        "P-8 required Package methods are absent: {errors:#?}"
    );
}

#[test]
fn e6_requires_kind_specific_nullable_finders() {
    let errors = diagnostics(
        r#"
function probe(pkg: reflect.Package) -> bool {
  pkg.get_class("root.E") == null
    && pkg.get_enum("root.E") == null
    && pkg.get_interface("root.I") == null
}
"#,
    );
    assert!(
        errors.is_empty(),
        "E-6 required finders are absent: {errors:#?}"
    );
}

#[tokio::test]
async fn package_type_finders_and_enumerators_are_precise_by_kind() {
    let output = baml_test!(
        r###"
function main() -> bool {
  let source = `
class C { value string }
enum E { A }
interface I {}
type Alias = C
function F() -> null { null }
`
  let first = reflect.Package.compile({ "types.baml": source })
  let second = reflect.Package.compile({ "types.baml": source })

  let class_t = first.get_class("root.C") ?? throw "missing class"
  let enum_t = first.get_enum("root.E") ?? throw "missing enum"
  let interface_t = first.get_interface("root.I") ?? throw "missing interface"
  let listed_class = first.classes().get("root.C") ?? throw "class not enumerated"
  let listed_enum = first.enums().get("root.E") ?? throw "enum not enumerated"
  let listed_interface = first.interfaces().get("root.I") ?? throw "interface not enumerated"
  let second_enum = second.get_enum("root.E") ?? throw "missing second enum"
  let second_interface = second.get_interface("root.I") ?? throw "missing second interface"

  return first.get_class("root.E") == null
    && first.get_enum("root.C") == null
    && first.get_interface("root.C") == null
    && first.get_enum("root.I") == null
    && first.get_class("root.Alias") == null
    && first.get_enum("root.Missing") == null
    && first.classes().length() == 1
    && first.enums().length() == 1
    && first.interfaces().length() == 1
    && first.functions().length() == 1
    && class_t.as_type() == listed_class.as_type()
    && enum_t.as_type() == listed_enum.as_type()
    && interface_t.as_type() == listed_interface.as_type()
    && enum_t.values()[0].name == "A"
    && enum_t.as_type() != second_enum.as_type()
    && interface_t.as_type() != second_interface.as_type()
}

"###
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn package_class_enumeration_preserves_declaration_order() {
    let output = baml_test!(
        r###"
class PurchaseOrder { id string }
class OrderedItem { sku string }

function main() -> string {
  let current = reflect.Package.current().classes().keys().join("|")
  let compiled = reflect.Package.compile({ "types.baml": `
class PurchaseOrder { id string }
class OrderedItem { sku string }
` }).classes().keys().join("|")
  `${current}~~${compiled}`
}
"###
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String(
            "root.PurchaseOrder|root.OrderedItem~~root.PurchaseOrder|root.OrderedItem".into()
        ))
    );
}
