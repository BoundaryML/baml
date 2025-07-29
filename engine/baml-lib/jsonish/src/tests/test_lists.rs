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
