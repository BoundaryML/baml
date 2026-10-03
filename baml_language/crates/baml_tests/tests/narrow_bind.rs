use baml_tests::baml_test;
use bex_engine::BexExternalValue;

#[tokio::test]
async fn typed_pattern_tests_and_binds_the_same_value() {
    let output = baml_test!(
        r#"
class Foo { field: int }

function pick(x: Foo | int) -> int {
  match (x) {
    let foo: Foo => foo.field,
    let n: int => n,
  }
}

function main() -> int {
  pick(Foo { field: 1 }) + pick(2)
}
"#
    );

    assert!(
        output.bytecode.contains("narrow_bind"),
        "{}",
        output.bytecode
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(3)));
}

/// A container arm that misses (a `json` map tested against `json[]`, and the
/// reverse) falls through to the next arm; `is` answers the same. The miss is
/// refuted from the outermost constructors without canonicalizing the
/// recursive `json` alias.
#[tokio::test]
async fn json_container_arm_misses_fall_through() {
    let output = baml_test!(
        r#"
function arm(v: baml.json.json) -> int {
  match (v) {
    let items: baml.json.json[] => 1,
    let fields: map<string, baml.json.json> => 2,
    let s: string | int => 3,
    _ => 0,
  }
}

function main() -> int {
  let m: map<string, baml.json.json> = {"a": 1};
  let a: baml.json.json[] = [1];
  let mv: baml.json.json = m;
  let av: baml.json.json = a;
  let checks = [
    arm(mv) == 2,
    arm(av) == 1,
    arm("x") == 3,
    arm(true) == 0,
    !(mv is baml.json.json[]),
    !(av is map<string, baml.json.json>),
    mv is map<string, baml.json.json>,
    av is baml.json.json[],
  ];
  let passed = 0;
  let i = 0;
  while (i < checks.length()) {
    if (checks[i]) { passed += 1; }
    i += 1;
  }
  passed
}
"#
    );

    assert_eq!(output.result, Ok(BexExternalValue::Int(8)));
}
