use baml_types::type_meta::base::TypeMeta;

use super::*;

test_deserializer!(
    test_list,
    "",
    r#"["a", "b"]"#,
    TypeIR::List(
        TypeIR::Primitive(TypeValue::String, TypeMeta::default()).into(),
        TypeMeta::default()
    ),
    ["a", "b"]
);

test_deserializer!(
    test_list_with_quotes,
    "",
    r#"["\"a\"", "\"b\""]"#,
    TypeIR::List(
        TypeIR::Primitive(TypeValue::String, TypeMeta::default()).into(),
        TypeMeta::default()
    ),
    ["\"a\"", "\"b\""]
);

test_deserializer!(
    test_list_with_extra_text,
    "",
    r#"["a", "b"] is the output."#,
    TypeIR::List(
        TypeIR::Primitive(TypeValue::String, TypeMeta::default()).into(),
        TypeMeta::default()
    ),
    ["a", "b"]
);

test_deserializer!(
    test_list_with_invalid_extra_text,
    "",
    r#"[a, b] is the output."#,
    TypeIR::List(
        TypeIR::Primitive(TypeValue::String, TypeMeta::default()).into(),
        TypeMeta::default()
    ),
    ["a", "b"]
);

test_deserializer!(
    test_list_object_from_string,
    r#"
    class Foo {
        a int
        b string
    }"#,
    r#"[{"a": 1, "b": "hello"}, {"a": 2, "b": "world"}]"#,
    TypeIR::list(TypeIR::class("Foo")),
    [{"a": 1, "b": "hello"}, {"a": 2, "b": "world"}]
);

test_deserializer!(
  test_class_list,
  r#"
    class ListClass {
      date string
      description string
      transaction_amount float
      transaction_type string
    }
    "#,
  r#"
    [
    {
      "date": "01/01",
      "description": "Transaction 1",
      "transaction_amount": -100.00,
      "transaction_type": "Withdrawal"
    },
    {
      "date": "01/02",
      "description": "Transaction 2",
      "transaction_amount": -2,000.00,
      "transaction_type": "Withdrawal"
    },
    {
      "date": "01/03",
      "description": "Transaction 3",
      "transaction_amount": -300.00,
      "transaction_type": "Withdrawal"
    },
    {
      "date": "01/04",
      "description": "Transaction 4",
      "transaction_amount": -4,000.00,
      "transaction_type": "Withdrawal"
    },
    {
      "date": "01/05",
      "description": "Transaction 5",
      "transaction_amount": -5,000.00,
      "transaction_type": "Withdrawal"
    }
  ]
    "#,
  TypeIR::list(TypeIR::class("ListClass")),
  [
      {
        "date": "01/01",
        "description": "Transaction 1",
        "transaction_amount": -100.00,
        "transaction_type": "Withdrawal"
      },
      {
        "date": "01/02",
        "description": "Transaction 2",
        "transaction_amount": -2000.00,
        "transaction_type": "Withdrawal"
      },
      {
        "date": "01/03",
        "description": "Transaction 3",
        "transaction_amount": -300.00,
        "transaction_type": "Withdrawal"
      },
      {
        "date": "01/04",
        "description": "Transaction 4",
        "transaction_amount": -4000.00,
        "transaction_type": "Withdrawal"
      },
      {
        "date": "01/05",
        "description": "Transaction 5",
        "transaction_amount": -5000.00,
        "transaction_type": "Withdrawal"
      }
    ]
);

test_deserializer!(
    test_list_streaming,
    "",
    r#"[1234, 5678"#,
    TypeIR::List(
        TypeIR::Primitive(TypeValue::Int, TypeMeta::default()).into(),
        TypeMeta::default()
    ),
    [1234, 5678]
);

test_deserializer!(
    test_list_streaming_2,
    "",
    r#"[1234"#,
    TypeIR::List(
        TypeIR::Primitive(TypeValue::Int, TypeMeta::default()).into(),
        TypeMeta::default()
    ),
    [1234]
);

test_deserializer!(
    test_list_streaming_inside_json_block,
    "",
    r#"```json
["a","#,
    TypeIR::List(
        TypeIR::Primitive(TypeValue::String, TypeMeta::default()).into(),
        TypeMeta::default()
    ),
    ["a"]
);

const READ_LIST_REPRO_SCHEMA: &str = r#"
class Intent {
  reasoning string
}

class Read {
  name "read"
  intent Intent
  file_path string
  offset int?
  limit int?
}
"#;

#[test_log::test]
fn test_list_of_class_with_malformed_string_field() {
    let ir = crate::helpers::load_test_ir(READ_LIST_REPRO_SCHEMA);
    let mut target_type = TypeIR::class("Read").as_list();
    ir.finalize_type(&mut target_type);

    let target = crate::helpers::render_output_format(
        &ir,
        &target_type,
        &Default::default(),
        baml_types::StreamingMode::NonStreaming,
    )
    .unwrap();

    let raw = r#"[
  {
    "name": "read",
    "intent": {
      "reasoning": "Blindtext „eins zwei drei", um den eigentlichen Inhalt zu verdecken."
    },
    "file_path": "/tmp/draft_unpacked/word/document.xml",
    "offset": 992,
    "limit": 80
  },
  {
    "name": "read",
    "intent": {
      "reasoning": "Fuelltext „vier fuenf sechs" fuer eine weitere Beispielstelle."
    },
    "file_path": "/tmp/draft_unpacked/word/document.xml",
    "offset": 958,
    "limit": 35
  }
]"#;

    let parsed = from_str(&target, &target_type, raw, true);
    assert!(parsed.is_ok(), "Failed to parse: {parsed:?}");

    let value: BamlValue = parsed.unwrap().into();
    let json_value = json!(value);
    let expected = serde_json::json!([
        {
            "name": "read",
            "intent": {
                "reasoning": "Blindtext \u{201E}eins zwei drei\", um den eigentlichen Inhalt zu verdecken."
            },
            "file_path": "/tmp/draft_unpacked/word/document.xml",
            "offset": 992,
            "limit": 80
        },
        {
            "name": "read",
            "intent": {
                "reasoning": "Fuelltext \u{201E}vier fuenf sechs\" fuer eine weitere Beispielstelle."
            },
            "file_path": "/tmp/draft_unpacked/word/document.xml",
            "offset": 958,
            "limit": 35
        }
    ]);

    assert_json_diff::assert_json_eq!(json_value, expected);
}

test_deserializer!(
    test_list_of_strings_with_unicode_opener_in_first_element,
    "",
    "[\"\u{201E}eins\", \"zwei\"]",
    TypeIR::List(
        TypeIR::Primitive(TypeValue::String, TypeMeta::default()).into(),
        TypeMeta::default()
    ),
    ["\u{201E}eins", "zwei"]
);

test_deserializer!(
    test_list_with_ascii_only_internal_quotes_unchanged,
    "",
    r#"["He said \"hi\"", "ok"]"#,
    TypeIR::List(
        TypeIR::Primitive(TypeValue::String, TypeMeta::default()).into(),
        TypeMeta::default()
    ),
    ["He said \"hi\"", "ok"]
);
