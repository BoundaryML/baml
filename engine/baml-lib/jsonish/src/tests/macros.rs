#[macro_use]
macro_rules! test_failing_deserializer {
    ($name:ident, $file_content:expr, $raw_string:expr, $target_type:expr) => {
        #[test_log::test]
        fn $name() {
            let ir = load_test_ir($file_content);
            let target = render_output_format(&ir, &$target_type, &Default::default()).unwrap();

            let result = from_str(&target, &$target_type, $raw_string, false);

            assert!(result.is_err(), "Failed to parse: {:?}", result);
        }
    };
}

#[macro_use]
macro_rules! test_deserializer {
    ($name:ident, $file_content:expr, $raw_string:expr, $target_type:expr, $($json:tt)+) => {
        #[test_log::test]
        fn $name() {
            let ir = load_test_ir($file_content);
            let target = render_output_format(&ir, &$target_type, &Default::default()).unwrap();

            let result = from_str(
                &target,
                &$target_type,
                $raw_string,
                false,
            );

            assert!(result.is_ok(), "Failed to parse: {:?}", result);

            let value = result.unwrap();
            // log::info!("{}", value);
            log::info!("Score: {}", value.score());
            let value: BamlValue = value.into();
            let json_value = json!(value);

            let expected = serde_json::json!($($json)+);

            assert_eq!(json_value, expected, "Expected: {:#}, got: {:#?}", expected, value);
        }
    };
}

#[macro_use]
macro_rules! test_partial_deserializer {
    ($name:ident, $file_content:expr, $raw_string:expr, $target_type:expr, $($json:tt)+) => {
        #[test_log::test]
        fn $name() {
            let ir = load_test_ir($file_content);
            let target = render_output_format(&ir, &$target_type, &Default::default()).unwrap();

            let result = from_str(
                &target,
                &$target_type,
                $raw_string,
                true,
            );

            assert!(result.is_ok(), "Failed to parse: {:?}", result);

            let value = result.unwrap();
            println!("{}", value);
            let value: BamlValue = value.into();
            let json_value = json!(value);

            let expected = serde_json::json!($($json)+);

            assert_eq!(json_value, expected, "Expected: {:#}, got: {:#?}", expected, value);
        }
    };
}
