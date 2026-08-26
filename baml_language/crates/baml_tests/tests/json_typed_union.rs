//! Runtime regressions for JSON serialization of structural union values.

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

#[tokio::test]
async fn typed_union_preserves_class_enum_and_media_values() {
    let output = baml_test!(
        r#"
class Record {
  name string
}

enum Phase {
  Ready
}

type Payload = Record | Phase | image

function serialize(value: Payload) -> string {
  baml.json.to_string(value)
}

function main() -> string {
  serialize(Record { name: "Ada" })
    + "|" + serialize(Phase.Ready)
    + "|" + serialize(image.from_url("https://example.com/a.png", "image/png"))
}
"#
    );

    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::String(
            concat!(
                r#"{"name":"Ada"}"#,
                r#"|"Ready""#,
                r#"|{"kind":"image","source":"url","value":"https://example.com/a.png","mime":"image/png"}"#,
            )
            .into()
        )
    );
}

#[tokio::test]
async fn typed_class_and_union_serialization_are_equivalent() {
    let output = baml_test!(
        r#"
class Detail {
  label string
}

class Record {
  name string
  detail Detail
}

type Payload = Record | string

function main() -> string {
  let record = Record {
    name: "Ada",
    detail: Detail { label: "nested" },
  }
  baml.json.to_string(record)
    + "|" + baml.json.to_string(record)
}
"#
    );

    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::String(
            concat!(
                r#"{"name":"Ada","detail":{"label":"nested"}}"#,
                r#"|{"name":"Ada","detail":{"label":"nested"}}"#,
            )
            .into()
        )
    );
}

#[tokio::test]
async fn typed_union_preserves_uint8array_serialization_error() {
    let output = baml_test!(
        r#"
type Payload = uint8array | string

function main() -> string {
  {
    let _ = baml.json.to_string(baml.Uint8Array.from_hex("00ff"))
    "unexpected success"
  } catch (e) {
    baml.json.JsonSerializationError => "serialization error"
  }
}
"#
    );

    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::String("serialization error".into())
    );
}
