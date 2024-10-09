use super::*;

test_deserializer!(
    test_list,
    "",
    r#"["a", "b"]"#,
    FieldType::List(FieldType::Primitive(TypeValue::String).into()),
    ["a", "b"]
);

test_deserializer!(
    test_list_with_quotes,
    "",
    r#"["\"a\"", "\"b\""]"#,
    FieldType::List(FieldType::Primitive(TypeValue::String).into()),
    ["\"a\"", "\"b\""]
);

test_deserializer!(
    test_list_with_extra_text,
    "",
    r#"["a", "b"] is the output."#,
    FieldType::List(FieldType::Primitive(TypeValue::String).into()),
    ["a", "b"]
);

test_deserializer!(
    test_list_with_invalid_extra_text,
    "",
    r#"[a, b] is the output."#,
    FieldType::List(FieldType::Primitive(TypeValue::String).into()),
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
    FieldType::List(FieldType::Class("Foo".to_string()).into()),
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
  FieldType::List(FieldType::Class("ListClass".to_string()).into()),
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
