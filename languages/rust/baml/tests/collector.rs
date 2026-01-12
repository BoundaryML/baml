//! Tests for Collector, `FunctionLog`, Usage, and `LogType`
#![allow(clippy::print_stdout)]

mod collector {
    use std::collections::HashMap;

    use baml::{BamlRuntime, Collector, FunctionArgs, LogType};

    /// Helper to create environment variables `HashMap` from current
    /// environment
    fn env_vars() -> HashMap<String, String> {
        std::env::vars().collect()
    }

    /// Create a minimal runtime for testing collector types
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

    // =========================================================================
    // Collector Creation Tests
    // =========================================================================

    mod creation {
        use super::*;

        #[test]
        fn new_collector_succeeds() {
            let runtime = create_test_runtime();
            let _collector = runtime.new_collector("test_collector");
        }

        #[test]
        fn new_collector_with_empty_name_succeeds() {
            let runtime = create_test_runtime();
            let _collector = runtime.new_collector("");
        }

        #[test]
        fn new_collector_with_special_chars_succeeds() {
            let runtime = create_test_runtime();
            let _collector = runtime.new_collector("test-collector_123!@#");
        }

        #[test]
        fn multiple_collectors_can_be_created() {
            let runtime = create_test_runtime();
            let _c1 = runtime.new_collector("collector1");
            let _c2 = runtime.new_collector("collector2");
            let _c3 = runtime.new_collector("collector3");
        }
    }

    // =========================================================================
    // Collector Property Tests
    // =========================================================================

    mod properties {
        use super::*;

        #[test]
        fn name_returns_collector_name() {
            let runtime = create_test_runtime();
            let collector = runtime.new_collector("my_test_collector");
            assert_eq!(collector.name(), "my_test_collector");
        }

        #[test]
        fn name_returns_empty_string_for_empty_name() {
            let runtime = create_test_runtime();
            let collector = runtime.new_collector("");
            assert_eq!(collector.name(), "");
        }

        #[test]
        fn logs_returns_empty_vec_initially() {
            let runtime = create_test_runtime();
            let collector = runtime.new_collector("test");
            let logs = collector.logs();
            assert!(logs.is_empty(), "Logs should be empty initially");
        }

        #[test]
        fn last_returns_none_initially() {
            let runtime = create_test_runtime();
            let collector = runtime.new_collector("test");
            assert!(
                collector.last().is_none(),
                "last() should return None initially"
            );
        }

        #[test]
        fn get_by_id_returns_none_for_nonexistent_id() {
            let runtime = create_test_runtime();
            let collector = runtime.new_collector("test");
            assert!(
                collector.get_by_id("nonexistent-id").is_none(),
                "get_by_id should return None for nonexistent ID"
            );
        }

        #[test]
        fn clear_returns_zero_when_empty() {
            let runtime = create_test_runtime();
            let collector = runtime.new_collector("test");
            let cleared = collector.clear();
            assert_eq!(cleared, 0, "clear() should return 0 for empty collector");
        }
    }

    // =========================================================================
    // Usage Tests (on empty collector)
    // =========================================================================

    mod usage {
        use super::*;

        #[test]
        fn usage_returns_zero_tokens_initially() {
            let runtime = create_test_runtime();
            let collector = runtime.new_collector("test");
            let usage = collector.usage();

            assert_eq!(
                usage.input_tokens(),
                0,
                "input_tokens should be 0 initially"
            );
            assert_eq!(
                usage.output_tokens(),
                0,
                "output_tokens should be 0 initially"
            );
        }
    }

    // =========================================================================
    // LogType Tests
    // =========================================================================

    mod log_type {
        use super::*;

        #[test]
        fn log_type_call_debug_format() {
            let log_type = LogType::Call;
            assert_eq!(format!("{log_type:?}"), "Call");
        }

        #[test]
        fn log_type_stream_debug_format() {
            let log_type = LogType::Stream;
            assert_eq!(format!("{log_type:?}"), "Stream");
        }

        #[test]
        fn log_type_equality() {
            assert_eq!(LogType::Call, LogType::Call);
            assert_eq!(LogType::Stream, LogType::Stream);
            assert_ne!(LogType::Call, LogType::Stream);
        }

        #[test]
        fn log_type_is_copy() {
            let log_type = LogType::Call;
            let copied = log_type;
            assert_eq!(log_type, copied);
        }

        #[test]
        fn log_type_is_clone() {
            let log_type = LogType::Stream;
            #[allow(clippy::clone_on_copy)]
            let cloned = log_type.clone();
            assert_eq!(log_type, cloned);
        }
    }

    // =========================================================================
    // FunctionArgs Integration Tests
    // =========================================================================

    mod function_args_integration {
        use super::*;

        #[test]
        fn with_collector_accepts_collector_reference() {
            let runtime = create_test_runtime();
            let collector = runtime.new_collector("test");

            let args = FunctionArgs::new()
                .arg("text", "hello")
                .with_collector(&collector);

            // Just verify it compiles and encodes without error
            let encoded = args.encode();
            assert!(encoded.is_ok(), "Failed to encode args with collector");
        }

        #[test]
        fn multiple_collectors_can_be_added() {
            let runtime = create_test_runtime();
            let c1 = runtime.new_collector("collector1");
            let c2 = runtime.new_collector("collector2");

            let args = FunctionArgs::new()
                .arg("text", "hello")
                .with_collector(&c1)
                .with_collector(&c2);

            let encoded = args.encode();
            assert!(
                encoded.is_ok(),
                "Failed to encode args with multiple collectors"
            );
        }

        #[test]
        fn collector_can_be_combined_with_other_args() {
            let runtime = create_test_runtime();
            let collector = runtime.new_collector("test");

            let args = FunctionArgs::new()
                .arg("prompt", "test prompt")
                .arg("count", 42i64)
                .with_env("TEST_VAR", "test_value")
                .with_tag("source", "test")
                .with_collector(&collector);

            let encoded = args.encode();
            assert!(
                encoded.is_ok(),
                "Failed to encode complex args with collector"
            );
        }
    }

    // =========================================================================
    // Thread Safety Tests
    // =========================================================================

    mod thread_safety {
        use std::{sync::Arc, thread};

        use super::*;

        #[test]
        fn collector_is_send() {
            fn assert_send<T: Send>() {}
            assert_send::<Collector>();
        }

        #[test]
        fn collector_is_sync() {
            fn assert_sync<T: Sync>() {}
            assert_sync::<Collector>();
        }

        #[test]
        fn collector_can_be_shared_across_threads() {
            let runtime = create_test_runtime();
            let collector = Arc::new(runtime.new_collector("shared"));

            let handles: Vec<_> = (0..4)
                .map(|i| {
                    let c = Arc::clone(&collector);
                    thread::spawn(move || {
                        // Each thread reads the name
                        let name = c.name();
                        assert_eq!(name, "shared", "Thread {i} got wrong name");
                    })
                })
                .collect();

            for handle in handles {
                handle.join().expect("Thread panicked");
            }
        }
    }

    // =========================================================================
    // Integration Tests with Real Function Calls
    // =========================================================================

    mod function_call_integration {
        use super::*;

        /// Helper macro to skip test if env var is not set
        macro_rules! require_env {
            ($name:expr) => {
                match std::env::var($name) {
                    Ok(val) if !val.is_empty() => val,
                    _ => {
                        eprintln!("Skipping test: {} not set", $name);
                        return;
                    }
                }
            };
        }

        /// Create a runtime with a simple function for testing collectors
        fn create_runtime_with_function() -> BamlRuntime {
            let mut files = HashMap::new();
            files.insert(
                "main.baml".to_string(),
                r##"
                client<llm> GPT4 {
                    provider openai
                    options {
                        model "gpt-4o-mini"
                        api_key env.OPENAI_API_KEY
                    }
                }

                function SayHello(name: string) -> string {
                    client GPT4
                    prompt #"Say hello to {{name}} in exactly 3 words."#
                }
                "##
                .to_string(),
            );

            BamlRuntime::new(".", &files, &HashMap::new()).expect("runtime creation failed")
        }

        #[test]
        fn collector_captures_function_call_telemetry() {
            let api_key = require_env!("OPENAI_API_KEY");

            let runtime = create_runtime_with_function();
            let collector = runtime.new_collector("test_collector");

            // Verify collector is empty before the call
            assert!(
                collector.logs().is_empty(),
                "Collector should be empty before call"
            );
            assert!(
                collector.last().is_none(),
                "last() should be None before call"
            );

            // Make a function call with the collector
            let args = FunctionArgs::new()
                .arg("name", "Alice")
                .with_env("OPENAI_API_KEY", &api_key)
                .with_collector(&collector);

            let result: Result<String, _> = runtime.call_function("SayHello", &args);
            assert!(result.is_ok(), "Function call failed: {:?}", result.err());

            let response = result.unwrap();
            println!("Got response: {response}");

            // Verify collector captured the call
            let logs = collector.logs();
            assert_eq!(logs.len(), 1, "Collector should have exactly 1 log");

            let log = collector.last().expect("last() should return a log");
            assert_eq!(log.function_name(), "SayHello");
            assert_eq!(log.log_type(), LogType::Call);

            // Verify the log has an ID
            let log_id = log.id();
            assert!(!log_id.is_empty(), "Log ID should not be empty");

            // Verify we can look up the log by ID
            let found_log = collector.get_by_id(&log_id);
            assert!(found_log.is_some(), "Should find log by ID");
            assert_eq!(found_log.unwrap().id(), log_id);
        }

        #[test]
        fn collector_tracks_token_usage() {
            let api_key = require_env!("OPENAI_API_KEY");

            let runtime = create_runtime_with_function();
            let collector = runtime.new_collector("usage_test");

            // Verify zero usage before the call
            let usage_before = collector.usage();
            assert_eq!(usage_before.input_tokens(), 0);
            assert_eq!(usage_before.output_tokens(), 0);

            // Make a function call
            let args = FunctionArgs::new()
                .arg("name", "Bob")
                .with_env("OPENAI_API_KEY", &api_key)
                .with_collector(&collector);

            let result: Result<String, _> = runtime.call_function("SayHello", &args);
            assert!(result.is_ok(), "Function call failed: {:?}", result.err());

            // Verify usage was tracked
            let usage = collector.usage();
            println!(
                "Token usage - input: {}, output: {}",
                usage.input_tokens(),
                usage.output_tokens()
            );

            assert!(usage.input_tokens() > 0, "Should have input tokens");
            assert!(usage.output_tokens() > 0, "Should have output tokens");

            // Verify per-log usage
            let log = collector.last().expect("Should have a log");
            let log_usage = log.usage();
            assert!(log_usage.input_tokens() > 0, "Log should have input tokens");
            assert!(
                log_usage.output_tokens() > 0,
                "Log should have output tokens"
            );
        }

        #[test]
        fn collector_captures_tags() {
            let api_key = require_env!("OPENAI_API_KEY");

            let runtime = create_runtime_with_function();
            let collector = runtime.new_collector("tags_test");

            // Make a call with tags
            let args = FunctionArgs::new()
                .arg("name", "Charlie")
                .with_env("OPENAI_API_KEY", &api_key)
                .with_tag("environment", "test")
                .with_tag("version", "1.0")
                .with_collector(&collector);

            let result: Result<String, _> = runtime.call_function("SayHello", &args);
            assert!(result.is_ok(), "Function call failed: {:?}", result.err());

            // Verify tags were captured
            let log = collector.last().expect("Should have a log");
            let tags = log.tags();
            println!("Captured tags: {tags:?}");

            assert_eq!(tags.get("environment"), Some(&"test".to_string()));
            assert_eq!(tags.get("version"), Some(&"1.0".to_string()));
        }

        #[test]
        fn collector_clear_removes_all_logs() {
            let api_key = require_env!("OPENAI_API_KEY");

            let runtime = create_runtime_with_function();
            let collector = runtime.new_collector("clear_test");

            // Make a function call
            let args = FunctionArgs::new()
                .arg("name", "Dave")
                .with_env("OPENAI_API_KEY", &api_key)
                .with_collector(&collector);

            let result: Result<String, _> = runtime.call_function("SayHello", &args);
            assert!(result.is_ok(), "Function call failed: {:?}", result.err());

            // Verify we have logs
            assert_eq!(collector.logs().len(), 1);

            // Clear and verify
            let cleared = collector.clear();
            assert_eq!(cleared, 1, "Should have cleared 1 log");
            assert!(
                collector.logs().is_empty(),
                "Logs should be empty after clear"
            );
            assert!(
                collector.last().is_none(),
                "last() should be None after clear"
            );

            // Usage should be reset too
            let usage = collector.usage();
            assert_eq!(
                usage.input_tokens(),
                0,
                "Input tokens should be 0 after clear"
            );
            assert_eq!(
                usage.output_tokens(),
                0,
                "Output tokens should be 0 after clear"
            );
        }

        #[test]
        fn multiple_calls_accumulate_in_collector() {
            let api_key = require_env!("OPENAI_API_KEY");

            let runtime = create_runtime_with_function();
            let collector = runtime.new_collector("multi_call_test");

            // Make multiple calls
            for name in ["Eve", "Frank", "Grace"] {
                let args = FunctionArgs::new()
                    .arg("name", name)
                    .with_env("OPENAI_API_KEY", &api_key)
                    .with_collector(&collector);

                let result: Result<String, _> = runtime.call_function("SayHello", &args);
                assert!(
                    result.is_ok(),
                    "Call for {} failed: {:?}",
                    name,
                    result.err()
                );
            }

            // Verify all calls were logged
            let logs = collector.logs();
            assert_eq!(logs.len(), 3, "Should have 3 logs");

            // Verify last() returns the most recent
            let last = collector.last().expect("Should have last log");
            assert_eq!(last.function_name(), "SayHello");

            // Verify usage accumulated
            let usage = collector.usage();
            println!(
                "Total usage - input: {}, output: {}",
                usage.input_tokens(),
                usage.output_tokens()
            );
            assert!(usage.input_tokens() > 0);
            assert!(usage.output_tokens() > 0);
        }

        #[test]
        fn separate_collectors_track_independently() {
            let api_key = require_env!("OPENAI_API_KEY");

            let runtime = create_runtime_with_function();
            let collector1 = runtime.new_collector("collector_1");
            let collector2 = runtime.new_collector("collector_2");

            // Make a call with collector1
            let args1 = FunctionArgs::new()
                .arg("name", "Henry")
                .with_env("OPENAI_API_KEY", &api_key)
                .with_collector(&collector1);

            let result1: Result<String, _> = runtime.call_function("SayHello", &args1);
            assert!(result1.is_ok());

            // Make a call with collector2
            let args2 = FunctionArgs::new()
                .arg("name", "Ivy")
                .with_env("OPENAI_API_KEY", &api_key)
                .with_collector(&collector2);

            let result2: Result<String, _> = runtime.call_function("SayHello", &args2);
            assert!(result2.is_ok());

            // Verify each collector only has its own logs
            assert_eq!(collector1.logs().len(), 1);
            assert_eq!(collector2.logs().len(), 1);

            // Verify logs are different
            let log1 = collector1.last().unwrap();
            let log2 = collector2.last().unwrap();
            assert_ne!(log1.id(), log2.id(), "Logs should have different IDs");
        }
    }

    // =========================================================================
    // Timing Tests
    // =========================================================================

    mod timing_tests {
        use super::*;

        /// Helper macro to skip test if env var is not set
        macro_rules! require_env {
            ($name:expr) => {
                match std::env::var($name) {
                    Ok(val) if !val.is_empty() => val,
                    _ => {
                        eprintln!("Skipping test: {} not set", $name);
                        return;
                    }
                }
            };
        }

        /// Create a runtime with a simple function for testing
        fn create_runtime_with_function() -> BamlRuntime {
            let mut files = HashMap::new();
            files.insert(
                "main.baml".to_string(),
                r##"
                client<llm> GPT4 {
                    provider openai
                    options {
                        model "gpt-4o-mini"
                        api_key env.OPENAI_API_KEY
                    }
                }

                function SayHello(name: string) -> string {
                    client GPT4
                    prompt #"Say hello to {{name}} in exactly 3 words."#
                }
                "##
                .to_string(),
            );

            BamlRuntime::new(".", &files, &HashMap::new()).expect("runtime creation failed")
        }

        #[test]
        fn timing_has_start_time_after_call() {
            let api_key = require_env!("OPENAI_API_KEY");

            let runtime = create_runtime_with_function();
            let collector = runtime.new_collector("timing_test");

            let args = FunctionArgs::new()
                .arg("name", "Test")
                .with_env("OPENAI_API_KEY", &api_key)
                .with_collector(&collector);

            let result: Result<String, _> = runtime.call_function("SayHello", &args);
            assert!(result.is_ok());

            let log = collector.last().expect("Should have a log");
            let timing = log.timing();

            assert!(
                timing.start_time_utc_ms() > 0,
                "start_time should be positive"
            );
            assert!(timing.duration_ms().is_some(), "duration should be set");
            assert!(
                timing.duration_ms().unwrap() > 0,
                "duration should be positive"
            );
        }
    }

    // =========================================================================
    // LLMCall Tests
    // =========================================================================

    mod llm_call_tests {
        use super::*;

        /// Helper macro to skip test if env var is not set
        macro_rules! require_env {
            ($name:expr) => {
                match std::env::var($name) {
                    Ok(val) if !val.is_empty() => val,
                    _ => {
                        eprintln!("Skipping test: {} not set", $name);
                        return;
                    }
                }
            };
        }

        /// Create a runtime with a simple function for testing
        fn create_runtime_with_function() -> BamlRuntime {
            let mut files = HashMap::new();
            files.insert(
                "main.baml".to_string(),
                r##"
                client<llm> GPT4 {
                    provider openai
                    options {
                        model "gpt-4o-mini"
                        api_key env.OPENAI_API_KEY
                    }
                }

                function SayHello(name: string) -> string {
                    client GPT4
                    prompt #"Say hello to {{name}} in exactly 3 words."#
                }
                "##
                .to_string(),
            );

            BamlRuntime::new(".", &files, &HashMap::new()).expect("runtime creation failed")
        }

        #[test]
        fn calls_returns_llm_calls_after_function_call() {
            let api_key = require_env!("OPENAI_API_KEY");

            let runtime = create_runtime_with_function();
            let collector = runtime.new_collector("calls_test");

            let args = FunctionArgs::new()
                .arg("name", "Test")
                .with_env("OPENAI_API_KEY", &api_key)
                .with_collector(&collector);

            let result: Result<String, _> = runtime.call_function("SayHello", &args);
            assert!(result.is_ok());

            let log = collector.last().expect("Should have a log");
            let calls = log.calls();

            assert_eq!(calls.len(), 1, "Should have exactly 1 LLM call");

            let call = &calls[0];
            assert_eq!(call.provider(), "openai");
            assert!(call.selected(), "The call should be selected");
        }

        #[test]
        fn llm_call_has_http_request() {
            let api_key = require_env!("OPENAI_API_KEY");

            let runtime = create_runtime_with_function();
            let collector = runtime.new_collector("http_test");

            let args = FunctionArgs::new()
                .arg("name", "Test")
                .with_env("OPENAI_API_KEY", &api_key)
                .with_collector(&collector);

            let result: Result<String, _> = runtime.call_function("SayHello", &args);
            assert!(result.is_ok());

            let log = collector.last().expect("Should have a log");
            let calls = log.calls();
            let call = &calls[0];

            let request = call.http_request().expect("Should have HTTP request");
            assert!(!request.url().is_empty(), "URL should not be empty");
            assert_eq!(request.method(), "POST");

            // Check body is valid JSON
            let body_json = request.body().json();
            assert!(body_json.is_ok(), "Body should be valid JSON");
        }

        #[test]
        fn llm_call_has_usage() {
            let api_key = require_env!("OPENAI_API_KEY");

            let runtime = create_runtime_with_function();
            let collector = runtime.new_collector("usage_test");

            let args = FunctionArgs::new()
                .arg("name", "Test")
                .with_env("OPENAI_API_KEY", &api_key)
                .with_collector(&collector);

            let result: Result<String, _> = runtime.call_function("SayHello", &args);
            assert!(result.is_ok());

            let log = collector.last().expect("Should have a log");
            let calls = log.calls();
            let call = &calls[0];

            let usage = call.usage().expect("Should have usage");
            assert!(usage.input_tokens() > 0);
            assert!(usage.output_tokens() > 0);
        }

        #[test]
        fn selected_call_returns_selected_llm_call() {
            let api_key = require_env!("OPENAI_API_KEY");

            let runtime = create_runtime_with_function();
            let collector = runtime.new_collector("selected_test");

            let args = FunctionArgs::new()
                .arg("name", "Test")
                .with_env("OPENAI_API_KEY", &api_key)
                .with_collector(&collector);

            let result: Result<String, _> = runtime.call_function("SayHello", &args);
            assert!(result.is_ok());

            let log = collector.last().expect("Should have a log");
            let selected = log.selected_call().expect("Should have a selected call");

            assert!(selected.selected());
            assert_eq!(selected.provider(), "openai");
        }
    }

    // =========================================================================
    // Cached Input Tokens Tests
    // =========================================================================

    mod cached_tokens_tests {
        use super::*;

        /// Helper macro to skip test if env var is not set
        macro_rules! require_env {
            ($name:expr) => {
                match std::env::var($name) {
                    Ok(val) if !val.is_empty() => val,
                    _ => {
                        eprintln!("Skipping test: {} not set", $name);
                        return;
                    }
                }
            };
        }

        /// Create a runtime with a simple function for testing
        fn create_runtime_with_function() -> BamlRuntime {
            let mut files = HashMap::new();
            files.insert(
                "main.baml".to_string(),
                r##"
                client<llm> GPT4 {
                    provider openai
                    options {
                        model "gpt-4o-mini"
                        api_key env.OPENAI_API_KEY
                    }
                }

                function SayHello(name: string) -> string {
                    client GPT4
                    prompt #"Say hello to {{name}} in exactly 3 words."#
                }
                "##
                .to_string(),
            );

            BamlRuntime::new(".", &files, &HashMap::new()).expect("runtime creation failed")
        }

        #[test]
        fn usage_has_cached_input_tokens_field() {
            let api_key = require_env!("OPENAI_API_KEY");

            let runtime = create_runtime_with_function();
            let collector = runtime.new_collector("cached_test");

            let args = FunctionArgs::new()
                .arg("name", "Test")
                .with_env("OPENAI_API_KEY", &api_key)
                .with_collector(&collector);

            let result: Result<String, _> = runtime.call_function("SayHello", &args);
            assert!(result.is_ok());

            let usage = collector.usage();
            // cached_input_tokens may be None or Some(0) for non-cached calls
            // The important thing is that the method exists and doesn't panic
            let cached = usage.cached_input_tokens();
            println!("Cached input tokens: {cached:?}");
        }
    }
}
