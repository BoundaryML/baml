use super::*;

test_deserializer!(
    test_list,
    r#"["a", "b"]"#,
    array_of(annotated(string_ty())),
    empty_db(),
    ["a", "b"]
);

test_deserializer!(
    test_list_with_quotes,
    r#"["\"a\"", "\"b\""]"#,
    array_of(annotated(string_ty())),
    empty_db(),
    ["\"a\"", "\"b\""]
);

test_deserializer!(
    test_list_with_extra_text,
    r#"["a", "b"] is the output."#,
    array_of(annotated(string_ty())),
    empty_db(),
    ["a", "b"]
);

test_deserializer!(
    test_list_with_invalid_extra_text,
    r#"[a, b] is the output."#,
    array_of(annotated(string_ty())),
    empty_db(),
    ["a", "b"]
);

test_deserializer!(
    test_list_object_from_string,
    r#"[{"a": 1, "b": "hello"}, {"a": 2, "b": "world"}]"#,
    array_of(annotated(class_ty("Foo", vec![
        field("a", int_ty()),
        field("b", string_ty()),
    ]))),
    empty_db(),
    [{"a": 1, "b": "hello"}, {"a": 2, "b": "world"}]
);

test_deserializer!(
    test_class_list,
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
    array_of(annotated(class_ty("ListClass", vec![
        field("date", string_ty()),
        field("description", string_ty()),
        field("transaction_amount", float_ty()),
        field("transaction_type", string_ty()),
    ]))),
    empty_db(),
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
    r#"[1234, 5678"#,
    array_of(annotated(int_ty())),
    empty_db(),
    [1234, 5678]
);

test_deserializer!(
    test_list_streaming_2,
    r#"[1234"#,
    array_of(annotated(int_ty())),
    empty_db(),
    [1234]
);

test_deserializer!(
    test_list_streaming_inside_json_block,
    r#"```json
["a","#,
    array_of(annotated(string_ty())),
    empty_db(),
    ["a"]
);
