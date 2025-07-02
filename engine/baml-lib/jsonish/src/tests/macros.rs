macro_rules! test_failing_deserializer {
    ($name:ident, $file_content:expr, $raw_string:expr, $target_type:expr) => {
        #[test_log::test]
        fn $name() {
            let ir = crate::helpers::load_test_ir($file_content);
            let mut target_type = $target_type;
            ir.finalize_type(&mut target_type);
            let target = crate::helpers::render_output_format(
                &ir,
                &target_type,
                &Default::default(),
                baml_types::StreamingMode::NonStreaming,
            )
            .unwrap();

            log::info!("target: {target}");
            log::info!("target_type: {target_type}");

            let result = from_str(&target, &target_type, $raw_string, true);

            match result {
                Ok(v) => {
                    let value: BamlValue = v.into();
                    assert!(false, "Parsing should have failed: {value}");
                }
                Err(e) => {}
            }
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
            let mut target_type = $target_type;
            ir.finalize_type(&mut target_type);
            let target = crate::helpers::render_output_format(&ir, &target_type, &Default::default(), baml_types::StreamingMode::NonStreaming).unwrap();

            let result = from_str(
                &target,
                &target_type,
                $raw_string,
                true,
            );

            assert!(result.is_ok(), "Failed to parse: {:?}", result);

            let value = result.unwrap();
            log::trace!("Score: {}", value.score());
            assert_eq!(value.field_type(), &target_type);
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
            let mut target_type = $target_type;
            ir.finalize_type(&mut target_type);
            let target = crate::helpers::render_output_format(
                &ir,
                &target_type,
                &Default::default(),
                baml_types::StreamingMode::NonStreaming,
            )
            .unwrap();

            let result = from_str(&target, &target_type, $raw_string, true);

            assert!(result.is_ok(), "Failed to parse: {:?}", result);

            let value = result.unwrap();
            assert_eq!(value.field_type(), &target_type);
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
            let mut target_type = $target_type;
            ir.finalize_type(&mut target_type);
            let target_type = target_type.to_streaming_type(&ir).to_ir_type();
            let target = crate::helpers::render_output_format(&ir, &target_type, &Default::default(), baml_types::StreamingMode::Streaming).unwrap();
            log::info!("target: {target}");
            log::info!("target_type: {target_type}");

            let result = from_str(
                &target,
                &target_type,
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

macro_rules! test_partial_deserializer_streaming {
    ($name:ident, $file_content:expr, $raw_string:expr, $target_type:expr, $($json:tt)+) => {
        #[test_log::test]
        fn $name() {
            let ir = crate::helpers::load_test_ir($file_content);
            let mut target_type = $target_type;
            ir.finalize_type(&mut target_type);
            let target_type = target_type.to_streaming_type(&ir).to_ir_type();
            let target = crate::helpers::render_output_format(&ir, &target_type, &Default::default(), baml_types::StreamingMode::Streaming).unwrap();
            log::debug!("target: {target}");
            log::debug!("--------------------------------");
            log::debug!("target_type: {target_type}");
            log::debug!("--------------------------------");

            let parsed = from_str(
                &target,
                &target_type,
                $raw_string,
                false,
            );

            // dbg!(&target);
            // dbg!(&$target_type);
            // dbg!(&parsed);

            assert!(parsed.is_ok(), "Failed to parse: {:?}", parsed);

            let result = crate::helpers::parsed_value_to_response(&ir, parsed.unwrap(), baml_types::StreamingMode::Streaming).unwrap();

            // dbg!(&result);

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
            let ir = crate::helpers::load_test_ir($file_content);
            let mut target_type = $target_type;
            ir.finalize_type(&mut target_type);
            let target_type = target_type.to_streaming_type(&ir).to_ir_type();
            let target = crate::helpers::render_output_format(
                &ir,
                &target_type,
                &Default::default(),
                baml_types::StreamingMode::Streaming,
            )
            .unwrap();

            let parsed = from_str(&target, &target_type, $raw_string, false);

            assert!(parsed.is_err(), "Got a result: {:?}", parsed);

            // let result = crate::helpers::parsed_value_to_response(
            //     &ir,
            //     parsed.unwrap(),
            //     baml_types::StreamingMode::Streaming,
            // );

            // assert!(
            //     result.is_err(),
            //     "Failed not to parse: {}",
            //     json!(result.unwrap().serialize_partial())
            // );
        }
    };
}
