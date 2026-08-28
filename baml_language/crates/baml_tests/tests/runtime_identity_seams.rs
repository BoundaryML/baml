//! B-1605 bug 9: every inbound type-bearing value arm must resolve through
//! the call-local runtime declaration overlay, including empty containers.

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

#[tokio::test]
async fn runtime_declarations_anchor_through_every_container_seam() {
    let output = baml_test!(
        r###"
client TestClient = openai.ResponsesClient.new(
  model = "unused",
  api_key = "unused",
)

function Extract<T>() -> T {
  client: TestClient
  prompt: `${ctx.output_format()}`
}

function Check<T>(expected: reflect.Type, document: string) -> bool throws unknown {
  let direct = baml.sap.parse<T>(document)
  let companion = Extract@spec<T>().parse(document)
  reflect.Type.of_value(direct) == expected
    && reflect.Type.of_value(companion) == expected
    && baml.json.to_string(direct) == baml.json.to_string(companion)
}

class StaticInner {
  x string
}

function main() -> bool throws unknown {
  let inner = reflect.class.new("Inner", { "x": reflect.Type.of<string>() })
  let inner_array = inner.as_type().array().as_type()
  let holder_array = reflect.class.new("HolderArray", { "bs": inner_array }).as_type()
  let holder_map = reflect.class.new("HolderMap", {
    "by_name": reflect.map.new(reflect.Type.of<string>(), inner.as_type()).as_type(),
  }).as_type()
  let minted_enum = reflect.enum.new("MintedState", ["Ready", "Done"])
  let enum_array = minted_enum.as_type().array().as_type()

  let compiled_package = reflect.Package.compile({
    "compiled.baml": "class CompiledInner { x string }",
  })
  let compiled_inner = compiled_package.get_class("root.CompiledInner")
    ?? throw "missing CompiledInner"
  let compiled_array = compiled_inner.as_type().array().as_type()
  let compiled_holder_array = reflect.class.new("CompiledHolderArray", {
    "bs": compiled_array,
  }).as_type()
  let compiled_holder_map = reflect.class.new("CompiledHolderMap", {
    "by_name": reflect.map.new(reflect.Type.of<string>(), compiled_inner.as_type()).as_type(),
  }).as_type()
  let static_holder = reflect.class.new("StaticHolder", {
    "bs": reflect.Type.of<StaticInner[]>(),
  }).as_type()

  let array_ok = {
    type T = unreflect(holder_array)
    Check<T>(holder_array, `{"bs":[{"x":"one"}]}`)
  }
  let empty_array_ok = {
    type T = unreflect(holder_array)
    Check<T>(holder_array, `{"bs":[]}`)
  }
  let map_ok = {
    type T = unreflect(holder_map)
    Check<T>(holder_map, `{"by_name":{"one":{"x":"one"}}}`)
  }
  let enum_ok = {
    type T = unreflect(enum_array)
    Check<T>(enum_array, `["Ready","Done"]`)
  }
  let top_level_array_ok = {
    type T = unreflect(inner_array)
    Check<T>(inner_array, `[{"x":"one"}]`)
  }
  let compiled_array_ok = {
    type T = unreflect(compiled_holder_array)
    Check<T>(compiled_holder_array, `{"bs":[{"x":"one"}]}`)
  }
  let compiled_map_ok = {
    type T = unreflect(compiled_holder_map)
    Check<T>(compiled_holder_map, `{"by_name":{"one":{"x":"one"}}}`)
  }
  let static_control_ok = {
    type T = unreflect(static_holder)
    Check<T>(static_holder, `{"bs":[{"x":"one"}]}`)
  }

  array_ok && empty_array_ok && map_ok && enum_ok && top_level_array_ok
    && compiled_array_ok && compiled_map_ok && static_control_ok
}
"###
    );

    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}
