//! BEP-066 I-2: declarations in a compiled Package have one created-once identity.

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

#[tokio::test]
async fn compiled_package_declarations_share_one_identity_across_all_access_paths() {
    let output = baml_test!(
        r####"
function main() -> bool throws unknown {
  let pkg = reflect.Package.compile({ "pkg.baml": #"
class Item { value int }
enum State { Ready }
function ReflectedItem() -> reflect.Type { reflect.Type.of<Item>() }
function ReflectedState() -> reflect.Type { reflect.Type.of<State>() }
"# })

  let item = pkg.get_class("root.Item") ?? throw "missing Item"
  let state = pkg.get_enum("root.State") ?? throw "missing State"
  let listed_item = pkg.classes().get("root.Item") ?? throw "Item not enumerated"
  let listed_state = pkg.enums().get("root.State") ?? throw "State not enumerated"
  let reflected_item = pkg.get_function<() -> reflect.Type>("root.ReflectedItem")
    ?? throw "missing ReflectedItem"
  let reflected_state = pkg.get_function<() -> reflect.Type>("root.ReflectedState")
    ?? throw "missing ReflectedState"

  item.as_type() == reflected_item()
    && item.as_type() == listed_item.as_type()
    && state.as_type() == reflected_state()
    && state.as_type() == listed_state.as_type()
}
"####
    );

    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}
