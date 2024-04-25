// use serde_json::json;

// use crate::from_str;

// macro_rules! valided_parses {
//     ($raw:expr, $expected:expr, $schema:expr) => {
//         let res = from_str($raw, $schema.to_string());
//         match res {
//             Ok(v) => assert_eq!(v, $expected),
//             Err(e) => assert!(false, "Failed to parse: {:?}", e),
//         }
//     };
// }

// #[test]
// fn test_null() {
//     let expectation = json!(null);
//     let schema = json!({
//         "type": "null",
//     });
//     valided_parses!("null", expectation, schema);
//     valided_parses!("NULL", expectation, schema);
//     valided_parses!(" \tnull  ", expectation, schema);
//     valided_parses!(" NULL ", expectation, schema);
// }

// #[test]
// fn test_string() {
//     let expectation = json!("hello");
//     let schema = json!({
//         "type": "string",
//     });
//     // Quoted json strings
//     valided_parses!(r#""hello""#, expectation, schema);
// }

// #[test]
// fn test_unquoted_string() {
//     let schema = json!({
//         "type": "string",
//     });
//     // Single word strings don't get treated differently
//     valided_parses!(" hello ", json!(" hello "), schema);
// }

// #[test]
// fn test_multi_unquoted_string() {
//     let expectation = json!("hello world");
//     let schema = json!({
//         "type": "string",
//     });
//     // Multi word strings
//     valided_parses!(" hello world ", expectation, schema);
// }

// // TODO: Fix this.
// // This test should panic because we don't yet handle single quote strings.
// fn test_single_quote_string() {
//     let expectation = json!("hello");
//     let schema = json!({
//         "type": "string",
//     });
//     valided_parses!("'hello'", expectation, schema);
//     valided_parses!(" 'hello' ", expectation, schema);
// }

// #[test]
// fn test_bool() {
//     let schema = json!({
//         "type": "bool",
//     });
//     valided_parses!("true", json!(true), schema);
//     valided_parses!("false", json!(false), schema);
//     valided_parses!(" true ", json!(true), schema);
//     valided_parses!(" false ", json!(false), schema);
//     // Test case insensitivity
//     valided_parses!("True", json!(true), schema);
//     valided_parses!("False", json!(false), schema);
// }

// #[test]
// fn test_float() {
//     let schema = json!({
//         "type": "float",
//     });
//     valided_parses!("123", json!(123.0), schema);
//     valided_parses!("-123", json!(-123.0), schema);
//     valided_parses!("123.45", json!(123.45), schema);
// }

// #[test]
// fn test_int() {
//     let schema = json!({
//         "type": "int",
//     });
//     valided_parses!("123", json!(123), schema);
//     valided_parses!("-123", json!(-123), schema);
//     valided_parses!(" 123 ", json!(123), schema);
//     valided_parses!("1.222", json!(1), schema);
//     // Always rounds
//     valided_parses!("1.55", json!(2), schema);
// }

// #[test]
// fn test_array() {
//     let schema = json!({
//         "type": "array",
//         "inner": {
//             "type": "int",
//         },
//     });
//     valided_parses!("[1, 2, 3]", json!([1, 2, 3]), schema);
// }
