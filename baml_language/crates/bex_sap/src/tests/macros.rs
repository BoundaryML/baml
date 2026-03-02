/// Tests that raw_string successfully deserializes to the expected JSON value.
///
/// The pipeline is:
/// 1. `jsonish::parse(raw_string, Default::default(), true)` to parse the raw string.
/// 2. `TyResolvedRef::coerce(&ctx, target, &parsed)` to coerce the parsed value to the target type.
/// 3. `serde_json::to_value(&result.value)` to serialize the result.
/// 4. `assert_eq!(json_value, expected)` to compare.
///
/// Arguments:
/// - `$name`: test function name
/// - `$raw_string`: raw LLM output string
/// - `$target_ty`: expression returning a `TyResolved<'_, &str>`
/// - `$db`: expression returning a `TypeRefDb<'_, &str>`
/// - `$($json)+`: expected JSON value (passed to `serde_json::json!`)
macro_rules! test_deserializer {
    ($name:ident, $raw_string:expr, $target_ty:expr, $db:expr, $($json:tt)+) => {
        #[test]
        fn $name() {
            let target_ty = $target_ty;
            let db = $db;
            let parsed = crate::jsonish::parse($raw_string, Default::default(), true)
                .expect("jsonish::parse failed");
            let ctx = crate::deserializer::coercer::ParsingContext::new(target_ty.as_ref(), &db);
            let default_annotations = crate::sap_model::TypeAnnotations::default();
            let target = crate::sap_model::TyWithMeta::new(
                target_ty.as_ref(),
                &default_annotations,
            );
            let result = crate::sap_model::TyResolvedRef::coerce(&ctx, target, &parsed);
            assert!(result.is_ok(), "Failed to parse: {:?}", result);
            let value = result.unwrap();
            assert!(value.is_some(), "Coercion returned None (in_progress=never?)");
            let value = value.unwrap();
            let json_value = serde_json::to_value(&value).unwrap();
            let expected = serde_json::json!($($json)+);
            assert_eq!(json_value, expected);
        }
    };
}

/// Like `test_deserializer!` but expects the deserialization to fail.
macro_rules! test_failing_deserializer {
    ($name:ident, $raw_string:expr, $target_ty:expr, $db:expr) => {
        #[test]
        fn $name() {
            let target_ty = $target_ty;
            let db = $db;
            let parsed = crate::jsonish::parse($raw_string, Default::default(), true)
                .expect("jsonish::parse failed");
            let ctx = crate::deserializer::coercer::ParsingContext::new(target_ty.as_ref(), &db);
            let default_annotations = crate::sap_model::TypeAnnotations::default();
            let target =
                crate::sap_model::TyWithMeta::new(target_ty.as_ref(), &default_annotations);
            let result = crate::sap_model::TyResolvedRef::coerce(&ctx, target, &parsed);
            match result {
                Ok(Some(v)) => {
                    let json = serde_json::to_value(&v).unwrap();
                    panic!("Parsing should have failed, got: {json}");
                }
                Ok(None) => {
                    // This is also acceptable for a "failing" test - coercion returned None
                }
                Err(_) => {
                    // Expected: parsing failed
                }
            }
        }
    };
}

/// Tests partial (streaming) deserialization where `is_done=false`.
///
/// Arguments:
/// - `$name`: test function name
/// - `$raw_string`: partial raw LLM output string
/// - `$target_ty`: expression returning a `TyResolved<'_, &str>`
/// - `$db`: expression returning a `TypeRefDb<'_, &str>`
/// - `$($json)+`: expected JSON value
macro_rules! test_partial_deserializer {
    ($name:ident, $raw_string:expr, $target_ty:expr, $db:expr, $($json:tt)+) => {
        #[test]
        fn $name() {
            let target_ty = $target_ty;
            let db = $db;
            let parsed = crate::jsonish::parse($raw_string, Default::default(), false)
                .expect("jsonish::parse failed");
            let ctx = crate::deserializer::coercer::ParsingContext::new(target_ty.as_ref(), &db);
            let default_annotations = crate::sap_model::TypeAnnotations::default();
            let target = crate::sap_model::TyWithMeta::new(
                target_ty.as_ref(),
                &default_annotations,
            );
            let result = crate::sap_model::TyResolvedRef::coerce(&ctx, target, &parsed);
            assert!(result.is_ok(), "Failed to parse: {:?}", result);
            let value = result.unwrap();
            assert!(value.is_some(), "Coercion returned None (in_progress=never?)");
            let value = value.unwrap();
            let json_value = serde_json::to_value(&value).unwrap();
            let expected = serde_json::json!($($json)+);
            assert_eq!(json_value, expected);
        }
    };
}

/// Tests partial deserialization that is expected to fail.
macro_rules! test_partial_failing_deserializer {
    ($name:ident, $raw_string:expr, $target_ty:expr, $db:expr) => {
        #[test]
        fn $name() {
            let target_ty = $target_ty;
            let db = $db;
            let parsed = crate::jsonish::parse($raw_string, Default::default(), false)
                .expect("jsonish::parse failed");
            let ctx = crate::deserializer::coercer::ParsingContext::new(target_ty.as_ref(), &db);
            let default_annotations = crate::sap_model::TypeAnnotations::default();
            let target =
                crate::sap_model::TyWithMeta::new(target_ty.as_ref(), &default_annotations);
            let result = crate::sap_model::TyResolvedRef::coerce(&ctx, target, &parsed);
            match result {
                Ok(Some(v)) => {
                    let json = serde_json::to_value(&v).unwrap();
                    panic!("Parsing should have failed, got: {json}");
                }
                Ok(None) => {
                    // Acceptable for a failing test
                }
                Err(_) => {
                    // Expected: parsing failed
                }
            }
        }
    };
}
