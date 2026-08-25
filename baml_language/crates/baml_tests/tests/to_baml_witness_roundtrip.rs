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
    "display_name": reflect.Type.of<string>(),
  }, implementations = [witness])
  let source = original.as_type().to_baml()
  let checked_source = source + `

function as_named(value: Person) -> Named<Value = string> {
  value
}
`
  let compiled_package = reflect.Package.compile({
    "contract.baml": "interface Named {\n  type Value\n  name Self.Value\n}",
    "person.baml": checked_source,
  })
  let compiled = compiled_package.get_class("root.Person")
    ?? throw "missing compiled Person"

  original.as_type().implements(reflect.Type.of<Named<Value = string>>())
    && original.as_type() != compiled.as_type()
    && compiled.fields()[0].name == "display_name"
    && compiled_package.functions().get("root.as_named") != null
}
"###
    );

    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn same_named_runtime_declarations_round_trip_with_unique_source_names() {
    let output = baml_test!(
        r###"
function main() -> bool throws unknown {
  let first = reflect.class.new("Choice", { "left": reflect.Type.of<string>() })
  let second = reflect.class.new("Choice", { "right": reflect.Type.of<int>() })
  let combined = reflect.union.new([first.as_type(), second.as_type()])
  let source = combined.as_type().to_baml()
  let compiled = reflect.Package.compile({ "choices.baml": source })
  compiled.get_class("root.Choice") != null
    && compiled.get_class("root.Choice_2") != null
}
"###
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn minted_and_compiled_same_named_declarations_round_trip() {
    let output = baml_test!(
        r###"
function main() -> bool throws unknown {
  let package = reflect.Package.compile({
    "original.baml": "class Choice { compiled string }",
  })
  let compiled_choice = package.get_class("root.Choice") ?? throw "missing Choice"
  let minted_choice = reflect.class.new("Choice", { "minted": reflect.Type.of<int>() })
  let combined = reflect.union.new([compiled_choice.as_type(), minted_choice.as_type()])
  let round_trip = reflect.Package.compile({ "choices.baml": combined.as_type().to_baml() })
  round_trip.get_class("root.Choice") != null
    && round_trip.get_class("root.Choice_2") != null
}
"###
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn runtime_class_static_dependencies_are_declared_in_round_trip_source() {
    let output = baml_test!(
        r###"
class StaticChild {
  value string
}

function main() -> bool throws unknown {
  let holder = reflect.class.new("RuntimeHolder", {
    "child": reflect.Type.of<StaticChild>(),
  })
  let source = holder.as_type().to_baml()
  let round_trip = reflect.Package.compile({ "holder.baml": source })
  round_trip.get_class("root.RuntimeHolder") != null
    && round_trip.get_class("root.StaticChild") != null
}
"###
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}
