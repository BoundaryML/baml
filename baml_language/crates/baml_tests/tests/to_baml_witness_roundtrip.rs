//! BEP-066 I-11 regression: canonical source must retain witness-backed
//! conformance across `to_baml()` and `reflect.Package.compile`.

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

#[tokio::test]
async fn witnessed_to_baml_round_trip_preserves_the_definition_tuple() {
    let output = baml_test!(
        r###"
interface Named {
  type Value
  name Self.Value
}

function main() -> bool throws unknown {
  let witness = reflect.interface.implementation<Named<Value = string>>()
    .field("name", class_field = "display_name")
  let original = reflect.class.new("Person", {
    "display_name": type.of<string>(),
  }, implementations = [witness])
  let source = original.as_type().to_baml()
  let checked_source = source + #"

function as_named(value: Person) -> Named<Value = string> {
  value
}
"#
  let compiled_package = reflect.Package.compile({
    "contract.baml": "interface Named {\n  type Value\n  name Self.Value\n}",
    "person.baml": checked_source,
  })
  let compiled = compiled_package.get_class("root.Person")
    ?? throw "missing compiled Person"

  original.as_type().implements(type.of<Named<Value = string>>())
    && original.as_type() != compiled.as_type()
    && compiled.fields()[0].name == "display_name"
    && compiled_package.functions().get("root.as_named") != null
}
"###
    );

    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}
