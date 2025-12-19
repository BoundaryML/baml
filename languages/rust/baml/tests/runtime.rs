//! Tests for BamlRuntime creation and function calls.

use std::collections::HashMap;

use baml::{BamlRuntime, FunctionArgs};

/// Helper to create environment variables HashMap from current environment
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

        let result = BamlRuntime::new(".", files, env_vars());
        assert!(result.is_ok(), "Runtime creation failed: {:?}", result.err());
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

        let result = BamlRuntime::new(".", files, env_vars());
        assert!(result.is_ok(), "Runtime creation failed: {:?}", result.err());
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

        let result = BamlRuntime::new(".", files, env_vars());
        assert!(result.is_ok(), "Runtime creation failed: {:?}", result.err());
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

        let result = BamlRuntime::new(".", files, env_vars());
        assert!(result.is_ok(), "Runtime creation failed: {:?}", result.err());
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

        let result = BamlRuntime::new(".", files, env_vars());
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

        let result = BamlRuntime::new(".", files, env_vars());
        assert!(result.is_err(), "Expected error for missing client reference");
    }

    #[test]
    fn empty_files_does_not_panic() {
        let files: HashMap<String, String> = HashMap::new();
        let result = BamlRuntime::new(".", files, env_vars());
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

        let result = BamlRuntime::new(".", files, env_vars());
        assert!(result.is_ok(), "Runtime creation failed: {:?}", result.err());
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

        let result = BamlRuntime::new(".", files, env_vars());
        assert!(result.is_ok(), "Runtime creation failed: {:?}", result.err());
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

        let result = BamlRuntime::new(".", files, env_vars());
        assert!(result.is_ok(), "Runtime creation failed: {:?}", result.err());
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

        let result = BamlRuntime::new(".", files, env_vars());
        assert!(result.is_ok(), "Runtime creation failed: {:?}", result.err());
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

        let result = BamlRuntime::new(".", files, env_vars());
        assert!(result.is_ok(), "Runtime creation failed: {:?}", result.err());
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

        let result = BamlRuntime::new(".", files, env_vars());
        assert!(result.is_ok(), "Runtime creation failed: {:?}", result.err());
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

    /// Test that we can attempt to call a function (will fail without valid API key)
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

        let runtime = BamlRuntime::new(".", files, env_vars()).expect("runtime creation failed");
        let args = FunctionArgs::new().arg("name", "World");

        // This should fail because the API key is invalid, but it proves the call path works
        let result: Result<String, _> = runtime.call_function("SayHello", &args);

        // We expect an error (invalid API key), not a panic
        assert!(result.is_err(), "Expected error with invalid API key");
    }

    /// Test successful function call with valid API key (requires OPENAI_API_KEY env var)
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
            BamlRuntime::new(".", files, HashMap::new()).expect("runtime creation failed");
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
        println!("Got response: {}", response);
    }

    /// Test calling with derive macro types and valid API key
    #[test]
    fn call_function_with_derived_types_succeeds() {
        use baml::{BamlDecode, BamlEncode};

        let api_key = require_env!("OPENAI_API_KEY");

        #[derive(Debug, PartialEq, BamlEncode, BamlDecode)]
        #[baml(name = "Person")]
        struct Person {
            name: String,
            age: i64,
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

        // Note: env vars must be passed in FunctionArgs, not just runtime creation
        let runtime =
            BamlRuntime::new(".", files, HashMap::new()).expect("runtime creation failed");
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
        println!("Got person: {:?}", person);
    }

    /// Test streaming function call with valid API key
    #[test]
    fn call_function_stream_succeeds() {
        use baml::{BamlDecode, BamlEncode, StreamEvent};

        let api_key = require_env!("OPENAI_API_KEY");

        // Final type - all fields required
        #[derive(Debug, Clone, BamlEncode, BamlDecode)]
        #[baml(name = "Person")]
        struct Person {
            name: String,
            age: i64,
        }

        // Partial type - BAML auto-constructs this with optional fields during streaming
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
            BamlRuntime::new(".", files, HashMap::new()).expect("runtime creation failed");
        let args = FunctionArgs::new()
            .arg("text", "Alice is 25 years old")
            .with_env("OPENAI_API_KEY", &api_key);

        let stream = runtime
            .call_function_stream::<PartialPerson, Person>("ExtractPerson", &args)
            .expect("stream creation failed");

        let mut partial_count = 0;
        let mut error_count = 0;
        let mut final_result: Option<Person> = None;

        for event in stream {
            match event {
                StreamEvent::Partial(partial) => {
                    partial_count += 1;
                    println!(
                        "Partial {}: name={:?}, age={:?}",
                        partial_count, partial.name, partial.age
                    );
                }
                StreamEvent::Final(person) => {
                    println!("Final: {:?}", person);
                    final_result = Some(person);
                }
                StreamEvent::Error(e) => {
                    // During streaming, partial decode errors can occur as fields
                    // are still being populated. This is expected behavior when
                    // the partial type has required fields that haven't arrived yet.
                    error_count += 1;
                    println!("Stream decode error {} (expected for incomplete partials): {:?}", error_count, e);
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

        let person = final_result.expect("Expected final result");
        assert_eq!(person.name, "Alice");
        assert_eq!(person.age, 25);
        println!(
            "Streaming test passed with {} partial updates",
            partial_count
        );
    }
}
