use crate::{baml_db, baml_tyannotated};

test_deserializer!(
    test_list,
    r#"["a", "b"]"#,
    baml_tyannotated!([string]),
    baml_db! {},
    ["a", "b"]
);

test_deserializer!(
    test_list_with_quotes,
    r#"["\"a\"", "\"b\""]"#,
    baml_tyannotated!([string]),
    baml_db! {},
    ["\"a\"", "\"b\""]
);

test_deserializer!(
    test_list_with_extra_text,
    r#"["a", "b"] is the output."#,
    baml_tyannotated!([string]),
    baml_db! {},
    ["a", "b"]
);

test_deserializer!(
    test_list_with_invalid_extra_text,
    r#"[a, b] is the output."#,
    baml_tyannotated!([string]),
    baml_db! {},
    ["a", "b"]
);

test_deserializer!(
    test_list_object_from_string,
    r#"[{"a": 1, "b": "hello"}, {"a": 2, "b": "world"}]"#,
    baml_tyannotated!([Foo]),
    baml_db!{
        class Foo {
            a: int,
            b: string,
        }
    },
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
    baml_tyannotated!([ListClass]),
    baml_db!{
        class ListClass {
            date: string,
            description: string,
            transaction_amount: float,
            transaction_type: string,
        }
    },
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

test_partial_deserializer!(
    test_list_streaming,
    r#"[1234, 5678"#,
    baml_tyannotated!([int]),
    baml_db! {},
    [1234, 5678]
);

test_partial_deserializer!(
    test_list_streaming_2,
    r#"[1234"#,
    baml_tyannotated!([int]),
    baml_db! {},
    [1234]
);

test_partial_deserializer!(
    test_list_streaming_inside_json_block,
    r#"```json
["a","#,
    baml_tyannotated!([string]),
    baml_db! {},
    ["a"]
);

// ============================================================================
// Array parse_as tests
// ============================================================================

// Array parse_as with narrower element type
test_deserializer!(
    test_array_parse_as_narrower_element,
    r#"[1, 2, 3]"#,
    baml_tyannotated!([(int | null) @parse_without_null]),
    baml_db! {},
    [1, 2, 3]
);

// Partial array through parse_as
test_partial_deserializer!(
    test_array_parse_as_partial,
    r#"[1, 2"#,
    baml_tyannotated!([(int | null) @parse_without_null]),
    baml_db! {},
    [1, 2]
);

// null element rejected by parse_as([int]) - array drops the failed element
test_deserializer!(
    test_array_parse_as_rejects_null_element,
    r#"[1, null, 3]"#,
    baml_tyannotated!([(int | null) @parse_without_null]),
    baml_db! {},
    [1, 3]
);
