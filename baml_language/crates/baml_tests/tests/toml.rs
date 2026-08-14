//! Runtime tests for the `baml.toml` standard-library namespace live in
//! `baml_src/ns_toml/toml.baml` as pure-BAML tests. This file holds only the
//! one test that cannot move there: it needs `#[ignore]` (pending the VM
//! `continue`/`break` bug), and BAML test blocks have no ignore mechanism.

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

/// `Table.from_json` base path plus its documented lossy null-skip; transitively
/// covers `item_from_json`'s scalar and nested-map arms and the round-trip back
/// through `to_json`. The `"b": null` entry must be dropped.
///
/// Ignored: `from_json` skips nulls with `continue` inside the key loop, and
/// `continue`/`break` are currently broken in the VM — the loop body surfaces a
/// spurious `TypeError { expected: Map, got: Any }` rather than skipping. Drop
/// the `#[ignore]` once that VM bug is fixed; the assertion below is the intended
/// behavior.
#[tokio::test]
#[ignore = "blocked on VM continue/break bug; toml.baml from_json skips nulls via `continue`"]
async fn from_json_skips_null() {
    let output = baml_test!(
        r#"
        function main() -> string {
            let j: baml.json.json = baml.json.parse("{\"a\":1,\"b\":null,\"c\":{\"d\":\"x\"}}");
            baml.json.stringify(baml.toml.Table.from_json(j).to_json())
        }
    "#
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String(
            r#"{"a":1,"c":{"d":"x"}}"#.to_string().into()
        ))
    );
}
