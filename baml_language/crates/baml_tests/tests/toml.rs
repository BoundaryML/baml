//! Runtime tests for the `baml.toml` standard-library namespace.
//!
//! Two layers are exercised:
//! - The native `baml.toml.Table.parse` (`crates/bex_vm/src/package_baml/toml.rs`),
//!   whose `convert_toml_value` maps every `toml::Value` arm (string / integer /
//!   float / boolean / datetime / array / table) onto a VM value.
//! - The BAML stdlib functions in `ns_toml/toml.baml`: `Table.to_json` /
//!   `Table.from_json`, the `item_to_json` / `item_from_json` helpers, and the
//!   `Datetime` wrapper.
//!
//! Parse results are inspected by stringifying their JSON projection
//! (`baml.json.stringify(table.to_json())`). This is deterministic: `toml = "0.8"`
//! is built without `preserve_order`, so `toml::Table` sorts keys alphabetically,
//! and `serde_json` carries `preserve_order`, so the emitted JSON key order is
//! fixed.

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

/// Assert the most recent BAML execution failed with `UnhandledThrow` whose
/// payload is an `Instance` of `expected_class`. Use this rather than a bare
/// `UnhandledThrow { .. }` match — the latter passes for any uncaught throw,
/// which masks regressions where the wrong error class is raised.
fn assert_throw_class(
    result: &Result<BexExternalValue, bex_engine::EngineError>,
    expected_class: &str,
) {
    let Err(bex_engine::EngineError::UnhandledThrow { value, .. }) = result else {
        panic!("expected UnhandledThrow({expected_class}), got: {result:?}");
    };
    let BexExternalValue::Instance { class_name, .. } = value.as_ref() else {
        panic!("expected throw Instance({expected_class}), got: {value:?}");
    };
    assert_eq!(class_name, expected_class);
}

// ─── Native parse path: convert_toml_value arms ──────────────────────────────

/// Base case: the string / int / float / bool scalar arms, plus a successful
/// `parse` and the `Table.to_json` projection, in one shot.
#[tokio::test]
async fn parse_scalars() {
    let output = baml_test!(
        r#"
        function main() -> string {
            let t = baml.toml.Table.parse("name = \"hi\"\ncount = 3\nratio = 1.5\nok = true\n");
            baml.json.stringify(t.to_json())
        }
    "#
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String(
            r#"{"count":3,"name":"hi","ok":true,"ratio":1.5}"#.to_string().into()
        ))
    );
}

/// The datetime arm — the only arm that allocates a `Datetime` instance — plus
/// `Datetime.to_json` reached through the union-valued map projection.
#[tokio::test]
async fn parse_datetime() {
    let output = baml_test!(
        r#"
        function main() -> string {
            let t = baml.toml.Table.parse("dt = 1979-05-27T07:32:00Z");
            baml.json.stringify(t.to_json())
        }
    "#
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String(
            r#"{"dt":"1979-05-27T07:32:00Z"}"#.to_string().into()
        ))
    );
}

/// The array arm (recursion over elements).
#[tokio::test]
async fn parse_array() {
    let output = baml_test!(
        r#"
        function main() -> string {
            let t = baml.toml.Table.parse("nums = [1, 2, 3]");
            baml.json.stringify(t.to_json())
        }
    "#
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String(r#"{"nums":[1,2,3]}"#.to_string().into()))
    );
}

/// The nested-table arm (recursive `Table` allocation).
#[tokio::test]
async fn parse_nested_table() {
    let output = baml_test!(
        r#"
        function main() -> string {
            let t = baml.toml.Table.parse("[server]\nhost = \"localhost\"");
            baml.json.stringify(t.to_json())
        }
    "#
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String(
            r#"{"server":{"host":"localhost"}}"#.to_string().into()
        ))
    );
}

/// The error branch in `toml.rs`: malformed input throws `TomlParseError`.
#[tokio::test]
async fn parse_invalid_throws() {
    let output = baml_test!(
        r#"
        function main() -> baml.toml.Table {
            baml.toml.Table.parse("= = =")
        }
    "#
    );
    assert_throw_class(&output.result, "baml.toml.TomlParseError");
}

// ─── BAML-level functions ────────────────────────────────────────────────────

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

/// `item_from_json`'s explicit null-rejection arm — the deliberate contrast with
/// `Table.from_json`, which silently skips nulls.
#[tokio::test]
async fn item_from_json_rejects_null() {
    let output = baml_test!(
        r#"
        function main() -> baml.toml.Item {
            baml.toml.item_from_json(null)
        }
    "#
    );
    assert_throw_class(&output.result, "baml.json.JsonDecodeError");
}

/// `item_to_json` is a public helper that `Table.to_json` does not reach (the
/// latter uses the auto-derived map projection), so it is otherwise untested.
/// One call hits its array-recursion, `Datetime`, and scalar arms together.
#[tokio::test]
async fn item_to_json_array_with_datetime() {
    let output = baml_test!(
        r#"
        function main() -> string {
            let item: baml.toml.Item = [baml.toml.Datetime { value: "2020-01-01T00:00:00Z" }, 7];
            baml.json.stringify(baml.toml.item_to_json(item))
        }
    "#
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String(
            r#"["2020-01-01T00:00:00Z",7]"#.to_string().into()
        ))
    );
}
