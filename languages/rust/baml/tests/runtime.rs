//! Tests for `BamlRuntime` creation and function calls.
#![allow(clippy::print_stdout, clippy::items_after_statements)]

use std::collections::HashMap;

use baml::{BamlRuntime, FunctionArgs, StreamState, StreamingState};

/// Helper to create environment variables `HashMap` from current environment
fn env_vars() -> HashMap<String, String> {
    std::env::vars().collect()
}

// =============================================================================
// Runtime Creation Tests
// =============================================================================

mod creation {
    use super::*;

    #[test]
    fn minimal_baml_parses_successfully() {
        // Minimal valid BAML that should parse
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

        let result = BamlRuntime::new(".", &files, &env_vars());
        assert!(
            result.is_ok(),
            "Runtime creation failed: {:?}",
            result.err()
        );
    }

    #[test]
    fn runtime_with_function_and_class() {
        let mut files = HashMap::new();
        files.insert(
            "main.baml".to_string(),
            r#####"
            client<llm> GPT4 {
                provider openai
                options {
                    model "gpt-4o"
                    api_key "test-key"
                }
            }

            class User {
                id int
                name string
                email string
            }

            function GetUser(id: int) -> User {
                client GPT4
                prompt #"Get user with id {{id}}"#
            }
            "#####
                .to_string(),
        );

        let result = BamlRuntime::new(".", &files, &env_vars());
        assert!(
            result.is_ok(),
            "Runtime creation failed: {:?}",
            result.err()
        );
    }

    #[test]
    fn runtime_with_enum() {
        let mut files = HashMap::new();
        files.insert(
            "main.baml".to_string(),
            r#####"
            client<llm> GPT4 {
                provider openai
                options {
                    model "gpt-4o"
                    api_key "test-key"
                }
            }

            enum Status {
                Active
                Inactive
                Pending
            }

            function GetStatus() -> Status {
                client GPT4
                prompt #"What is the status?"#
            }
            "#####
                .to_string(),
        );

        let result = BamlRuntime::new(".", &files, &env_vars());
        assert!(
            result.is_ok(),
            "Runtime creation failed: {:?}",
            result.err()
        );
    }

    #[test]
    fn runtime_with_multiple_files() {
        let mut files = HashMap::new();
        files.insert(
            "clients.baml".to_string(),
            r#####"
            client<llm> GPT4 {
                provider openai
                options {
                    model "gpt-4o"
                    api_key "test-key"
                }
            }
            "#####
                .to_string(),
        );
        files.insert(
            "types.baml".to_string(),
            r#####"
            class Person {
                name string
                age int
            }
            "#####
                .to_string(),
        );
        files.insert(
            "functions.baml".to_string(),
            r#####"
            function ExtractPerson(text: string) -> Person {
                client GPT4
                prompt #"Extract person from: {{text}}"#
            }
            "#####
                .to_string(),
        );

        let result = BamlRuntime::new(".", &files, &env_vars());
        assert!(
            result.is_ok(),
            "Runtime creation failed: {:?}",
            result.err()
        );
    }

    #[test]
    fn invalid_baml_returns_error() {
        let mut files = HashMap::new();
        files.insert(
            "main.baml".to_string(),
            r#####"
            this is not valid baml syntax {{{
            "#####
                .to_string(),
        );

        let result = BamlRuntime::new(".", &files, &env_vars());
        assert!(result.is_err(), "Expected error for invalid BAML");
    }

    #[test]
    fn missing_client_reference_returns_error() {
        let mut files = HashMap::new();
        files.insert(
            "main.baml".to_string(),
            r#####"
            function Test() -> string {
                client NonExistentClient
                prompt #"test"#
            }
            "#####
                .to_string(),
        );

        let result = BamlRuntime::new(".", &files, &env_vars());
        assert!(
            result.is_err(),
            "Expected error for missing client reference"
        );
    }

    #[test]
    fn empty_files_does_not_panic() {
        let files: HashMap<String, String> = HashMap::new();
        let result = BamlRuntime::new(".", &files, &env_vars());
        // Empty project should fail but not panic
        let _ = result;
    }
}

// =============================================================================
// FunctionArgs Tests
// =============================================================================

mod function_args {
    use super::*;

    #[test]
    fn builder_pattern_is_ergonomic() {
        let args = FunctionArgs::new()
            .arg("name", "Alice")
            .arg("age", 30i64)
            .arg("active", true)
            .arg("score", 95.5f64)
            .with_env("API_KEY", "secret")
            .with_tag("source", "test");

        let encoded = args.encode();
        assert!(encoded.is_ok());
        assert!(!encoded.unwrap().is_empty());
    }

    #[test]
    fn args_with_vec() {
        let args = FunctionArgs::new().arg("items", vec!["a".to_string(), "b".to_string()]);

        let encoded = args.encode();
        assert!(encoded.is_ok());
    }

    #[test]
    fn args_with_optional() {
        let some_value: Option<String> = Some("present".to_string());
        let none_value: Option<String> = None;

        let args = FunctionArgs::new()
            .arg("with_value", some_value)
            .arg("without_value", none_value);

        let encoded = args.encode();
        assert!(encoded.is_ok());
    }

    #[test]
    fn args_with_nested_data() {
        // Test with HashMap (map type)
        let mut metadata: HashMap<String, String> = HashMap::new();
        metadata.insert("key1".to_string(), "value1".to_string());
        metadata.insert("key2".to_string(), "value2".to_string());

        let args = FunctionArgs::new().arg("metadata", metadata);

        let encoded = args.encode();
        assert!(encoded.is_ok());
    }

    #[test]
    fn multiple_env_overrides() {
        let args = FunctionArgs::new()
            .with_env("OPENAI_API_KEY", "sk-test1")
            .with_env("ANTHROPIC_API_KEY", "sk-ant-test")
            .with_env("CUSTOM_VAR", "custom_value");

        let encoded = args.encode();
        assert!(encoded.is_ok());
    }

    #[test]
    fn multiple_tags() {
        let args = FunctionArgs::new()
            .with_tag("user_id", "user123")
            .with_tag("request_id", "req456")
            .with_tag("priority", 1i64);

        let encoded = args.encode();
        assert!(encoded.is_ok());
    }
}

// =============================================================================
// Complex BAML Scenarios
// =============================================================================

mod complex_scenarios {
    use super::*;

    #[test]
    fn runtime_with_nested_classes() {
        let mut files = HashMap::new();
        files.insert(
            "main.baml".to_string(),
            r#####"
            client<llm> GPT4 {
                provider openai
                options {
                    model "gpt-4o"
                    api_key "test-key"
                }
            }

            class Address {
                street string
                city string
                country string
            }

            class Person {
                name string
                age int
                address Address
            }

            function ExtractPerson(text: string) -> Person {
                client GPT4
                prompt #"Extract person from: {{text}}"#
            }
            "#####
                .to_string(),
        );

        let result = BamlRuntime::new(".", &files, &env_vars());
        assert!(
            result.is_ok(),
            "Runtime creation failed: {:?}",
            result.err()
        );
    }

    #[test]
    fn runtime_with_optional_fields() {
        let mut files = HashMap::new();
        files.insert(
            "main.baml".to_string(),
            r#####"
            client<llm> GPT4 {
                provider openai
                options {
                    model "gpt-4o"
                    api_key "test-key"
                }
            }

            class UserProfile {
                name string
                email string?
                phone string?
                age int?
            }

            function GetProfile(id: string) -> UserProfile {
                client GPT4
                prompt #"Get profile for {{id}}"#
            }
            "#####
                .to_string(),
        );

        let result = BamlRuntime::new(".", &files, &env_vars());
        assert!(
            result.is_ok(),
            "Runtime creation failed: {:?}",
            result.err()
        );
    }

    #[test]
    fn runtime_with_list_return_type() {
        let mut files = HashMap::new();
        files.insert(
            "main.baml".to_string(),
            r#####"
            client<llm> GPT4 {
                provider openai
                options {
                    model "gpt-4o"
                    api_key "test-key"
                }
            }

            class Item {
                name string
                price float
            }

            function ExtractItems(text: string) -> Item[] {
                client GPT4
                prompt #"Extract items from: {{text}}"#
            }
            "#####
                .to_string(),
        );

        let result = BamlRuntime::new(".", &files, &env_vars());
        assert!(
            result.is_ok(),
            "Runtime creation failed: {:?}",
            result.err()
        );
    }

    #[test]
    fn runtime_with_map_type() {
        let mut files = HashMap::new();
        files.insert(
            "main.baml".to_string(),
            r#####"
            client<llm> GPT4 {
                provider openai
                options {
                    model "gpt-4o"
                    api_key "test-key"
                }
            }

            function ExtractMetadata(text: string) -> map<string, string> {
                client GPT4
                prompt #"Extract key-value pairs from: {{text}}"#
            }
            "#####
                .to_string(),
        );

        let result = BamlRuntime::new(".", &files, &env_vars());
        assert!(
            result.is_ok(),
            "Runtime creation failed: {:?}",
            result.err()
        );
    }

    #[test]
    fn runtime_with_union_type() {
        let mut files = HashMap::new();
        files.insert(
            "main.baml".to_string(),
            r#####"
            client<llm> GPT4 {
                provider openai
                options {
                    model "gpt-4o"
                    api_key "test-key"
                }
            }

            class Cat {
                name string
                meows bool
            }

            class Dog {
                name string
                barks bool
            }

            function ClassifyPet(description: string) -> Cat | Dog {
                client GPT4
                prompt #"Classify this pet: {{description}}"#
            }
            "#####
                .to_string(),
        );

        let result = BamlRuntime::new(".", &files, &env_vars());
        assert!(
            result.is_ok(),
            "Runtime creation failed: {:?}",
            result.err()
        );
    }

    #[test]
    fn runtime_with_retry_policy() {
        let mut files = HashMap::new();
        files.insert(
            "main.baml".to_string(),
            r#####"
            retry_policy ExponentialBackoff {
                max_retries 3
                strategy {
                    type exponential_backoff
                }
            }

            client<llm> GPT4 {
                provider openai
                retry_policy ExponentialBackoff
                options {
                    model "gpt-4o"
                    api_key "test-key"
                }
            }

            function ReliableCall() -> string {
                client GPT4
                prompt #"Hello"#
            }
            "#####
                .to_string(),
        );

        let result = BamlRuntime::new(".", &files, &env_vars());
        assert!(
            result.is_ok(),
            "Runtime creation failed: {:?}",
            result.err()
        );
    }
}

// =============================================================================
// Function Call Tests (require API keys to actually run)
// =============================================================================

mod function_calls {
    use super::*;

    /// Helper macro to skip tests when an environment variable is not set.
    /// Returns early from the test with a clear skip message.
    macro_rules! require_env {
        ($var:expr) => {
            match std::env::var($var) {
                Ok(val) if !val.is_empty() => val,
                _ => {
                    eprintln!("SKIPPED: {} not set", $var);
                    return;
                }
            }
        };
    }

    /// Test that we can attempt to call a function (will fail without valid API
    /// key)
    #[test]
    fn call_function_returns_error_without_api_key() {
        let mut files = HashMap::new();
        files.insert(
            "main.baml".to_string(),
            r##"
            client<llm> GPT4 {
                provider openai
                options {
                    model "gpt-4o"
                    api_key "invalid-key"
                }
            }

            function SayHello(name: string) -> string {
                client GPT4
                prompt #"Say hello to {{name}}"#
            }
            "##
            .to_string(),
        );

        let runtime = BamlRuntime::new(".", &files, &env_vars()).expect("runtime creation failed");
        let args = FunctionArgs::new().arg("name", "World");

        // This should fail because the API key is invalid, but it proves the call path
        // works
        let result: Result<String, _> = runtime.call_function("SayHello", &args);

        // We expect an error (invalid API key), not a panic
        assert!(result.is_err(), "Expected error with invalid API key");
    }

    /// Test successful function call with valid API key (requires
    /// `OPENAI_API_KEY` env var)
    #[test]
    fn call_function_succeeds_with_valid_api_key() {
        let api_key = require_env!("OPENAI_API_KEY");

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
                prompt #"Say hello to {{name}} in exactly 5 words."#
            }
            "##
            .to_string(),
        );

        // Note: env vars must be passed in FunctionArgs, not just runtime creation
        let runtime =
            BamlRuntime::new(".", &files, &HashMap::new()).expect("runtime creation failed");
        let args = FunctionArgs::new()
            .arg("name", "World")
            .with_env("OPENAI_API_KEY", &api_key);

        let result: Result<String, _> = runtime.call_function("SayHello", &args);

        assert!(
            result.is_ok(),
            "Expected success but got: {:?}",
            result.err()
        );
        let response = result.unwrap();
        assert!(!response.is_empty(), "Response should not be empty");
        println!("Got response: {response}");
    }

    /// Test calling with derive macro types and valid API key
    #[test]
    fn call_function_with_derived_types_succeeds() {
        use baml::{BamlDecode, BamlEncode};

        #[derive(Debug, PartialEq, BamlEncode, BamlDecode)]
        #[baml(name = "Person")]
        struct Person {
            name: String,
            age: i64,
        }

        let api_key = require_env!("OPENAI_API_KEY");

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

            class Person {
                name string
                age int
            }

            function ExtractPerson(text: string) -> Person {
                client GPT4
                prompt #"Extract the person's name and age from: {{text}}

                Return a JSON object with 'name' and 'age' fields."#
            }
            "##
            .to_string(),
        );

        // Note: env vars must be passed in FunctionArgs, not just runtime creation
        let runtime =
            BamlRuntime::new(".", &files, &HashMap::new()).expect("runtime creation failed");
        let args = FunctionArgs::new()
            .arg("text", "John is 30 years old")
            .with_env("OPENAI_API_KEY", &api_key);

        let result: Result<Person, _> = runtime.call_function("ExtractPerson", &args);

        assert!(
            result.is_ok(),
            "Expected success but got: {:?}",
            result.err()
        );
        let person = result.unwrap();
        assert_eq!(person.name, "John");
        assert_eq!(person.age, 30);
        println!("Got person: {person:?}");
    }

    /// Test function call that returns Checked<T> with @check constraints
    #[test]
    fn call_function_with_checked_type_succeeds() {
        use baml::{CheckStatus, Checked};

        let api_key = require_env!("OPENAI_API_KEY");

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

            // Return type with @check constraint - returns Checked<int>
            function PredictAge(name: string) -> int @check(reasonable_age, {{ this > 0 and this < 150 }}) {
                client GPT4
                prompt #"Guess the age of someone named {{name}}. Return only a number."#
            }
            "##
            .to_string(),
        );

        let runtime =
            BamlRuntime::new(".", &files, &HashMap::new()).expect("runtime creation failed");
        let args = FunctionArgs::new()
            .arg("name", "Alice")
            .with_env("OPENAI_API_KEY", &api_key);

        let result: Result<Checked<i64>, _> = runtime.call_function("PredictAge", &args);

        assert!(
            result.is_ok(),
            "Expected success but got: {:?}",
            result.err()
        );

        let checked = result.unwrap();
        println!("Got Checked value: {}", checked.value);
        println!("Checks: {:?}", checked.checks);

        // The value should be a reasonable age
        assert!(checked.value > 0, "Age should be positive");

        // Should have exactly one check named "reasonable_age"
        assert_eq!(checked.checks.len(), 1, "Should have one check");
        let check = checked
            .get_check("reasonable_age")
            .expect("Should have 'reasonable_age' check");
        assert_eq!(check.name, "reasonable_age");

        // If the LLM returned a reasonable age (1-149), the check should pass
        if checked.value > 0 && checked.value < 150 {
            assert_eq!(
                check.status,
                CheckStatus::Succeeded,
                "Check should pass for reasonable age"
            );
            assert!(checked.all_passed());
        }
    }

    /// Test function call that returns Checked<Option<T>> with @check
    /// constraints on optional type
    #[test]
    fn call_function_with_checked_optional_type_succeeds() {
        use baml::{CheckStatus, Checked};

        let api_key = require_env!("OPENAI_API_KEY");

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

            // Return type with @check constraint on optional type - returns Checked<int?>
            function MaybeGetAge(name: string) -> int? @check(if_present_reasonable, {{ this == null or (this > 0 and this < 150) }}) {
                client GPT4
                prompt #"If you know the age of someone named {{name}}, return just the number. If you don't know, return null."#
            }
            "##
            .to_string(),
        );

        let runtime =
            BamlRuntime::new(".", &files, &HashMap::new()).expect("runtime creation failed");
        let args = FunctionArgs::new()
            .arg("name", "Alice")
            .with_env("OPENAI_API_KEY", &api_key);

        let result: Result<Checked<Option<i64>>, _> = runtime.call_function("MaybeGetAge", &args);

        assert!(
            result.is_ok(),
            "Expected success but got: {:?}",
            result.err()
        );

        let checked = result.unwrap();
        println!("Got Checked<Option<i64>> value: {:?}", checked.value);
        println!("Checks: {:?}", checked.checks);

        // Should have exactly one check named "if_present_reasonable"
        assert_eq!(checked.checks.len(), 1, "Should have one check");
        let check = checked
            .get_check("if_present_reasonable")
            .expect("Should have 'if_present_reasonable' check");
        assert_eq!(check.name, "if_present_reasonable");

        // The check should pass whether it's Some(reasonable_age) or None
        match checked.value {
            Some(age) => {
                println!("Got age: {age}");
                if age > 0 && age < 150 {
                    assert_eq!(
                        check.status,
                        CheckStatus::Succeeded,
                        "Check should pass for reasonable age"
                    );
                }
            }
            None => {
                println!("Got null (no age)");
                assert_eq!(
                    check.status,
                    CheckStatus::Succeeded,
                    "Check should pass for null"
                );
            }
        }
    }

    /// Test function call that returns Option<Checked<T>> - i.e., (int @check)?
    #[test]
    fn call_function_with_optional_checked_type_succeeds() {
        use baml::{CheckStatus, Checked};

        let api_key = require_env!("OPENAI_API_KEY");

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

            // Return type (int @check)? - the whole checked value is optional
            function MaybeGetCheckedAge(name: string) -> (int @check(reasonable_age, {{ this > 0 and this < 150 }}))? {
                client GPT4
                prompt #"If you know the age of someone named {{name}}, return just the number. If you don't know, return null."#
            }
            "##
            .to_string(),
        );

        let runtime =
            BamlRuntime::new(".", &files, &HashMap::new()).expect("runtime creation failed");
        let args = FunctionArgs::new()
            .arg("name", "Bob")
            .with_env("OPENAI_API_KEY", &api_key);

        let result: Result<Option<Checked<i64>>, _> =
            runtime.call_function("MaybeGetCheckedAge", &args);

        assert!(
            result.is_ok(),
            "Expected success but got: {:?}",
            result.err()
        );

        let maybe_checked = result.unwrap();
        println!(
            "Got Option<Checked<i64>>: {:?}",
            maybe_checked.as_ref().map(|c| c.value)
        );

        match maybe_checked {
            Some(checked) => {
                println!("Got Checked value: {}", checked.value);
                println!("Checks: {:?}", checked.checks);

                // Should have exactly one check named "reasonable_age"
                assert_eq!(checked.checks.len(), 1, "Should have one check");
                let check = checked
                    .get_check("reasonable_age")
                    .expect("Should have 'reasonable_age' check");
                assert_eq!(check.name, "reasonable_age");

                // If the LLM returned a reasonable age (1-149), the check should pass
                if checked.value > 0 && checked.value < 150 {
                    assert_eq!(
                        check.status,
                        CheckStatus::Succeeded,
                        "Check should pass for reasonable age"
                    );
                    assert!(checked.all_passed());
                }
            }
            None => {
                println!("Got None (null response)");
                // This is valid - the whole checked value is optional
            }
        }
    }

    /// Test streaming function call that returns `StreamState`<T> with
    /// @`stream.with_state`
    #[test]
    fn call_function_stream_with_state_succeeds() {
        use baml::BamlDecode;

        let api_key = require_env!("OPENAI_API_KEY");

        // Partial type with StreamState field - only needs BamlDecode since it's only
        // received For `string @stream.with_state`, the streaming type is
        // `StreamState<Option<String>>` The outer Option is because the field
        // might not exist yet during streaming
        #[derive(Debug, Clone, BamlDecode)]
        #[baml(name = "MessageWithState")]
        struct PartialMessageWithState {
            content: Option<StreamState<Option<String>>>,
        }

        // Final type - all fields required, no StreamState wrapper
        #[derive(Debug, Clone, BamlDecode)]
        #[baml(name = "MessageWithState")]
        struct MessageWithState {
            content: String,
        }

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

            class MessageWithState {
                content string @stream.with_state
            }

            function GenerateMessage(topic: string) -> MessageWithState {
                client GPT4
                prompt #"Write a short message about {{topic}}."#
            }
            "##
            .to_string(),
        );

        let runtime =
            BamlRuntime::new(".", &files, &HashMap::new()).expect("runtime creation failed");
        let args = FunctionArgs::new()
            .arg("topic", "rust programming")
            .with_env("OPENAI_API_KEY", &api_key);

        let stream = runtime
            .call_function_stream::<PartialMessageWithState, MessageWithState>(
                "GenerateMessage",
                &args,
            )
            .expect("stream creation failed");

        let mut saw_started = false;
        let mut saw_done = false;
        let mut partial_count = 0;

        let mut stream = stream;
        for event in stream.partials() {
            match event {
                Ok(partial) => {
                    partial_count += 1;
                    if let Some(ref state) = partial.content {
                        // state.value is Option<String> since it's StreamState<Option<String>>
                        println!(
                            "Partial {}: state={:?}, value={:?}",
                            partial_count, state.state, state.value
                        );
                        match state.state {
                            StreamingState::Pending => {}
                            StreamingState::Started => saw_started = true,
                            StreamingState::Done => saw_done = true,
                        }
                    }
                }
                Err(e) => {
                    println!("Stream error (may be expected for early partials): {e:?}");
                }
            }
        }

        // We should get at least some partial updates
        assert!(partial_count > 0, "Expected at least one partial result");

        // Should have seen at least Started and Done states (Pending might be missed if
        // streaming is fast)
        assert!(
            saw_started || saw_done,
            "Expected to see Started or Done streaming state"
        );

        let msg = stream.get_final_response().expect("Expected final result");
        assert!(!msg.content.is_empty(), "Content should not be empty");
        println!("Streaming with state test passed with {partial_count} partial updates");
    }

    /// Test streaming function call with valid API key
    #[test]
    fn call_function_stream_succeeds() {
        use baml::{BamlDecode, BamlEncode};

        let api_key = require_env!("OPENAI_API_KEY");

        // Final type - all fields required
        #[derive(Debug, Clone, BamlEncode, BamlDecode)]
        #[baml(name = "Person")]
        struct Person {
            name: String,
            age: i64,
        }

        // Partial type - BAML auto-constructs this with optional fields during
        // streaming
        #[derive(Debug, Clone, BamlEncode, BamlDecode)]
        #[baml(name = "Person")]
        struct PartialPerson {
            name: Option<String>,
            age: Option<i64>,
        }

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

            class Person {
                name string
                age int
            }

            function ExtractPerson(text: string) -> Person {
                client GPT4
                prompt #"Extract the person's name and age from: {{text}}

                Return a JSON object with 'name' and 'age' fields."#
            }
            "##
            .to_string(),
        );

        let runtime =
            BamlRuntime::new(".", &files, &HashMap::new()).expect("runtime creation failed");
        let args = FunctionArgs::new()
            .arg("text", "Alice is 25 years old")
            .with_env("OPENAI_API_KEY", &api_key);

        let mut stream = runtime
            .call_function_stream::<PartialPerson, Person>("ExtractPerson", &args)
            .expect("stream creation failed");

        let mut partial_count = 0;
        let mut error_count = 0;

        for event in stream.partials() {
            match event {
                Ok(partial) => {
                    partial_count += 1;
                    println!(
                        "Partial {}: name={:?}, age={:?}",
                        partial_count, partial.name, partial.age
                    );
                }
                Err(e) => {
                    // During streaming, partial decode errors can occur as fields
                    // are still being populated. This is expected behavior when
                    // the partial type has required fields that haven't arrived yet.
                    error_count += 1;
                    println!(
                        "Stream decode error {error_count} (expected for incomplete partials): {e:?}"
                    );
                }
            }
        }

        // We should get at least one successful partial decode
        assert!(
            partial_count > 0,
            "Expected at least one partial result during streaming"
        );

        // Note: Early partials may fail to decode because fields are still null.
        // This is expected behavior - the streaming protocol sends incomplete data.

        let person = stream.get_final_response().expect("Expected final result");
        assert_eq!(person.name, "Alice");
        assert_eq!(person.age, 25);
        println!("Streaming test passed with {partial_count} partial updates");
    }
}
