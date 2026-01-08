//! Tests for media types (Image, Audio, Pdf, Video)
//!
//! Each media type has the same interface, so we use a macro to generate tests
//! for all four.

use std::collections::HashMap;

use baml::BamlRuntime;

/// Helper to create environment variables `HashMap` from current environment
fn env_vars() -> HashMap<String, String> {
    std::env::vars().collect()
}

/// Create a minimal runtime for testing media types
fn create_test_runtime() -> BamlRuntime {
    let mut files = HashMap::new();
    files.insert(
        "main.baml".to_string(),
        r#####"
        client<llm> TestClient {
            provider openai
            options {
                model "gpt-4o"
                api_key "test-key"
            }
        }
        "#####
            .to_string(),
    );

    BamlRuntime::new(".", &files, &env_vars()).expect("Failed to create test runtime")
}

// =============================================================================
// Macro to generate tests for each media type
// =============================================================================

macro_rules! media_type_tests {
    ($mod_name:ident, $from_url:ident, $from_base64:ident) => {
        mod $mod_name {
            use super::*;

            // -----------------------------------------------------------------
            // Constructor tests
            // -----------------------------------------------------------------

            #[test]
            fn from_url_succeeds() {
                let runtime = create_test_runtime();
                let _media = runtime.$from_url("https://example.com/test.png", None);
            }

            #[test]
            fn from_url_with_mime_type_succeeds() {
                let runtime = create_test_runtime();
                let _media = runtime.$from_url("https://example.com/test.png", Some("image/png"));
            }

            #[test]
            fn from_base64_succeeds() {
                let runtime = create_test_runtime();
                let _media = runtime.$from_base64("SGVsbG8gV29ybGQ=", None);
            }

            #[test]
            fn from_base64_with_mime_type_succeeds() {
                let runtime = create_test_runtime();
                let _media = runtime.$from_base64("SGVsbG8gV29ybGQ=", Some("image/png"));
            }

            // -----------------------------------------------------------------
            // Method tests - URL-based media
            // -----------------------------------------------------------------

            #[test]
            fn is_url_returns_true_for_url_media() {
                let runtime = create_test_runtime();
                let media = runtime.$from_url("https://example.com/test.png", None);
                assert!(media.is_url(), "is_url should return true for URL media");
            }

            #[test]
            fn is_base64_returns_false_for_url_media() {
                let runtime = create_test_runtime();
                let media = runtime.$from_url("https://example.com/test.png", None);
                assert!(
                    !media.is_base64(),
                    "is_base64 should return false for URL media"
                );
            }

            #[test]
            fn as_url_returns_url_for_url_media() {
                let runtime = create_test_runtime();
                let url = "https://example.com/test.png";
                let media = runtime.$from_url(url, None);
                assert_eq!(media.as_url(), Some(url.to_string()));
            }

            #[test]
            fn as_base64_returns_none_for_url_media() {
                let runtime = create_test_runtime();
                let media = runtime.$from_url("https://example.com/test.png", None);
                assert_eq!(
                    media.as_base64(),
                    None,
                    "as_base64 should return None for URL media"
                );
            }

            #[test]
            fn mime_type_returns_none_when_not_provided_for_url() {
                let runtime = create_test_runtime();
                let media = runtime.$from_url("https://example.com/test.png", None);
                assert_eq!(media.mime_type(), None);
            }

            #[test]
            fn mime_type_returns_value_when_provided_for_url() {
                let runtime = create_test_runtime();
                let media = runtime.$from_url("https://example.com/test.png", Some("image/png"));
                assert_eq!(media.mime_type(), Some("image/png".to_string()));
            }

            // -----------------------------------------------------------------
            // Method tests - base64-based media
            // -----------------------------------------------------------------

            #[test]
            fn is_url_returns_false_for_base64_media() {
                let runtime = create_test_runtime();
                let media = runtime.$from_base64("SGVsbG8gV29ybGQ=", None);
                assert!(
                    !media.is_url(),
                    "is_url should return false for base64 media"
                );
            }

            #[test]
            fn is_base64_returns_true_for_base64_media() {
                let runtime = create_test_runtime();
                let media = runtime.$from_base64("SGVsbG8gV29ybGQ=", None);
                assert!(
                    media.is_base64(),
                    "is_base64 should return true for base64 media"
                );
            }

            #[test]
            fn as_url_returns_none_for_base64_media() {
                let runtime = create_test_runtime();
                let media = runtime.$from_base64("SGVsbG8gV29ybGQ=", None);
                assert_eq!(
                    media.as_url(),
                    None,
                    "as_url should return None for base64 media"
                );
            }

            #[test]
            fn as_base64_returns_base64_for_base64_media() {
                let runtime = create_test_runtime();
                let base64 = "SGVsbG8gV29ybGQ=";
                let media = runtime.$from_base64(base64, None);
                assert_eq!(media.as_base64(), Some(base64.to_string()));
            }

            #[test]
            fn mime_type_returns_none_when_not_provided_for_base64() {
                let runtime = create_test_runtime();
                let media = runtime.$from_base64("SGVsbG8gV29ybGQ=", None);
                assert_eq!(media.mime_type(), None);
            }

            #[test]
            fn mime_type_returns_value_when_provided_for_base64() {
                let runtime = create_test_runtime();
                let media = runtime.$from_base64("SGVsbG8gV29ybGQ=", Some("image/png"));
                assert_eq!(media.mime_type(), Some("image/png".to_string()));
            }
        }
    };
}

// =============================================================================
// Generate tests for each media type
// =============================================================================

media_type_tests!(media_image, new_image_from_url, new_image_from_base64);
media_type_tests!(media_audio, new_audio_from_url, new_audio_from_base64);
media_type_tests!(media_pdf, new_pdf_from_url, new_pdf_from_base64);
media_type_tests!(media_video, new_video_from_url, new_video_from_base64);
