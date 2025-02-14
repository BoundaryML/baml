macro_rules! test_failing_deserializer {
    ($name:ident, $file_content:expr, $raw_string:expr, $target_type:expr) => {
        #[test_log::test]
        fn $name() {
            let ir = crate::helpers::load_test_ir($file_content);
            let target =
                crate::helpers::render_output_format(&ir, &$target_type, &Default::default())
                    .unwrap();

            let result = from_str(&target, &$target_type, $raw_string, false);

            assert!(
                result.is_err(),
                "Failed not to parse: {:?}",
                result.unwrap()
            );
        }
    };
}

/// Arguments
///
/// - `name`: The name of the test function to generate.
/// - `file_content`: A BAML schema used for the test.
/// - `raw_string`: An example payload coming from an LLM to parse.
/// - `target_type`: The type to try to parse `raw_string` into.
/// - `json`: The expected JSON encoding that the parser should return.
///
/// Example
///
/// ```ignore
/// test_deserializer!(
///     my_test,
///     "schema_content",
///     "raw_payload",
///     MyType,
///     { "expected": "json" }
/// );
/// ```
macro_rules! test_deserializer {
    ($name:ident, $file_content:expr, $raw_string:expr, $target_type:expr, $($json:tt)+) => {
        #[test_log::test]
        fn $name() {
            let ir = crate::helpers::load_test_ir($file_content);
            let target = crate::helpers::render_output_format(&ir, &$target_type, &Default::default()).unwrap();

            let result = from_str(
                &target,
                &$target_type,
                $raw_string,
                false,
            );

            assert!(result.is_ok(), "Failed to parse: {:?}", result);

            let value = result.unwrap();
            log::trace!("Score: {}", value.score());
            let value: BamlValue = value.into();
            log::info!("{}", value);
            let json_value = json!(value);

            let expected = serde_json::json!($($json)+);

            assert_json_diff::assert_json_eq!(json_value, expected);
        }
    };
}

macro_rules! test_deserializer_with_expected_score {
    ($name:ident, $file_content:expr, $raw_string:expr, $target_type:expr, $target_score:expr) => {
        #[test_log::test]
        fn $name() {
            let ir = crate::helpers::load_test_ir($file_content);
            let target =
                crate::helpers::render_output_format(&ir, &$target_type, &Default::default())
                    .unwrap();

            let result = from_str(&target, &$target_type, $raw_string, false);

            assert!(result.is_ok(), "Failed to parse: {:?}", result);

            let value = result.unwrap();
            dbg!(&value);
            log::trace!("Score: {}", value.score());
            assert_eq!(value.score(), $target_score);
        }
    };
}

macro_rules! test_partial_deserializer {
    ($name:ident, $file_content:expr, $raw_string:expr, $target_type:expr, $($json:tt)+) => {
        #[test_log::test]
        fn $name() {
            let ir = crate::helpers::load_test_ir($file_content);
            let target = crate::helpers::render_output_format(&ir, &$target_type, &Default::default()).unwrap();

            let result = from_str(
                &target,
                &$target_type,
                $raw_string,
                true,
            );

            assert!(result.is_ok(), "Failed to parse: {:?}", result);

            let value = result.unwrap();
            log::trace!("Score: {}", value.score());
            let value: BamlValue = value.into();
            log::info!("{}", value);
            let json_value = json!(value);

            let expected = serde_json::json!($($json)+);

            assert_json_diff::assert_json_eq!(json_value, expected);
        }
    };
}

macro_rules! test_partial_deserializer_streaming {
    ($name:ident, $file_content:expr, $raw_string:expr, $target_type:expr, $($json:tt)+) => {
        #[test_log::test]
        fn $name() {
            let ir = crate::helpers::load_test_ir($file_content);
            let target = crate::helpers::render_output_format(&ir, &$target_type, &Default::default()).unwrap();

            let parsed = from_str(
                &target,
                &$target_type,
                $raw_string,
                true,
            );

            // dbg!(&target);
            // dbg!(&$target_type);
            dbg!(&parsed);

            assert!(parsed.is_ok(), "Failed to parse: {:?}", parsed);

            let result = crate::helpers::parsed_value_to_response(&ir, parsed.unwrap(), &$target_type, true).unwrap();

            dbg!(&result);

            let value = result;
            log::trace!("Score: {}", value.score());
            let json_value = json!(value.serialize_partial());

            let expected = serde_json::json!($($json)+);

            assert_json_diff::assert_json_eq!(json_value, expected);
        }
    };
}

macro_rules! test_partial_deserializer_streaming_failure {
    ($name:ident, $file_content:expr, $raw_string:expr, $target_type:expr) => {
        #[test_log::test]
        fn $name() {
            let ir = load_test_ir($file_content);
            let target =
                crate::helpers::render_output_format(&ir, &$target_type, &Default::default())
                    .unwrap();

            let parsed = from_str(&target, &$target_type, $raw_string, true);

            dbg!(&target);
            dbg!(&$target_type);

            assert!(parsed.is_ok(), "Failed to parse: {:?}", parsed);

            let result =
                crate::helpers::parsed_value_to_response(&ir, parsed.unwrap(), &$target_type, true);

            assert!(
                result.is_err(),
                "Failed not to parse: {:?}",
                result.unwrap()
            );
        }
    };
}
