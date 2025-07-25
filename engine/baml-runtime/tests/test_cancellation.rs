#[cfg(feature = "internal")]
#[cfg(not(feature = "skip-integ-tests"))]
mod cancellation_tests {
    use std::{
        collections::HashMap,
        sync::{Arc, Mutex},
        time::Duration,
    };

    use baml_runtime::{BamlRuntime, FunctionResultStream, RuntimeContextManager};
    use baml_types::BamlValue;
    use tokio::time::timeout;
    use tokio_util::sync::CancellationToken;

    /// Test that cancellation token properly cancels a stream before execution
    #[tokio::test]
    async fn test_stream_cancellation_before_execution() {
        let mut files = HashMap::new();
        files.insert(
            "main.baml",
            r##"
            class TestOutput {
                message string
            }

            function TestFunction(input: string) -> TestOutput {
                client "openai/gpt-4o-mini"
                prompt #"
                    Return a test message: {{ input }}
                "#
            }
            "##,
        );

        let mut env_vars = HashMap::new();
        env_vars.insert("OPENAI_API_KEY".to_string(), "test-key".to_string());

        let runtime = BamlRuntime::from_file_content(".", &files, env_vars).unwrap();
        let ctx = runtime.create_ctx_manager(BamlValue::String("test".to_string()), None);

        // Create a stream
        let mut stream = runtime
            .stream_function(
                "TestFunction".to_string(),
                &[("input".to_string(), BamlValue::String("test".to_string()))]
                    .into_iter()
                    .collect(),
                &ctx,
                None,
                None,
                None,
                HashMap::new(),
            )
            .unwrap();

        // Cancel before execution
        let cancellation_token = CancellationToken::new();
        cancellation_token.cancel();
        stream.set_cancellation_token(cancellation_token);

        // Try to run the stream - should fail immediately
        let (result, _) = stream
            .run(
                None::<fn()>,
                None::<fn(baml_runtime::FunctionResult)>,
                &ctx,
                None,
                None,
                HashMap::new(),
            )
            .await;

        assert!(result.is_err());
        let error_msg = result.unwrap_err().to_string();
        assert!(
            error_msg.contains("cancelled") || error_msg.contains("canceled"),
            "Expected cancellation error, got: {}",
            error_msg
        );
    }

    /// Test that cancellation token properly cancels a stream during execution
    #[tokio::test]
    async fn test_stream_cancellation_during_execution() {
        let mut files = HashMap::new();
        files.insert(
            "main.baml",
            r##"
            class TestOutput {
                message string
            }

            function TestFunction(input: string) -> TestOutput {
                client "openai/gpt-4o-mini"
                prompt #"
                    Return a test message: {{ input }}
                "#
            }
            "##,
        );

        let mut env_vars = HashMap::new();
        env_vars.insert("OPENAI_API_KEY".to_string(), "test-key".to_string());

        let runtime = BamlRuntime::from_file_content(".", &files, env_vars).unwrap();
        let ctx = runtime.create_ctx_manager(BamlValue::String("test".to_string()), None);

        // Create a stream
        let mut stream = runtime
            .stream_function(
                "TestFunction".to_string(),
                &[("input".to_string(), BamlValue::String("test".to_string()))]
                    .into_iter()
                    .collect(),
                &ctx,
                None,
                None,
                None,
                HashMap::new(),
            )
            .unwrap();

        let cancellation_token = CancellationToken::new();
        stream.set_cancellation_token(cancellation_token.clone());

        // Cancel after a short delay
        let cancel_token = cancellation_token.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(100)).await;
            cancel_token.cancel();
        });

        // Try to run the stream - should be cancelled
        let result = timeout(
            Duration::from_secs(5),
            stream.run(
                None::<fn()>,
                None::<fn(baml_runtime::FunctionResult)>,
                &ctx,
                None,
                None,
                HashMap::new(),
            ),
        )
        .await;

        assert!(result.is_ok(), "Stream should complete (with cancellation)");
        let (stream_result, _) = result.unwrap();
        
        // Should be cancelled
        assert!(stream_result.is_err());
        let error_msg = stream_result.unwrap_err().to_string();
        assert!(
            error_msg.contains("cancelled") || error_msg.contains("canceled"),
            "Expected cancellation error, got: {}",
            error_msg
        );
    }

    /// Test that cancellation token is properly checked during orchestration
    #[tokio::test]
    async fn test_orchestration_cancellation() {
        let mut files = HashMap::new();
        files.insert(
            "main.baml",
            r##"
            class TestOutput {
                message string
            }

            client<llm> TestClient {
                provider "openai"
                options {
                    api_key "test-key"
                    model "gpt-4o-mini"
                }
            }

            function TestFunction(input: string) -> TestOutput {
                client TestClient
                prompt #"
                    Return a test message: {{ input }}
                "#
            }
            "##,
        );

        let runtime = BamlRuntime::from_file_content(".", &files, HashMap::new()).unwrap();
        let ctx = runtime.create_ctx_manager(BamlValue::String("test".to_string()), None);

        // Create a stream
        let mut stream = runtime
            .stream_function(
                "TestFunction".to_string(),
                &[("input".to_string(), BamlValue::String("test".to_string()))]
                    .into_iter()
                    .collect(),
                &ctx,
                None,
                None,
                None,
                HashMap::new(),
            )
            .unwrap();

        // Test that the stream has cancellation support
        let cancellation_token = CancellationToken::new();
        stream.set_cancellation_token(cancellation_token.clone());

        // Verify the token is not cancelled initially
        assert!(!stream.is_cancelled());

        // Cancel the token
        cancellation_token.cancel();

        // Verify the stream detects cancellation
        assert!(stream.is_cancelled());
    }

    /// Test that multiple streams can be cancelled independently
    #[tokio::test]
    async fn test_multiple_stream_cancellation() {
        let mut files = HashMap::new();
        files.insert(
            "main.baml",
            r##"
            class TestOutput {
                message string
            }

            function TestFunction(input: string) -> TestOutput {
                client "openai/gpt-4o-mini"
                prompt #"
                    Return a test message: {{ input }}
                "#
            }
            "##,
        );

        let mut env_vars = HashMap::new();
        env_vars.insert("OPENAI_API_KEY".to_string(), "test-key".to_string());

        let runtime = BamlRuntime::from_file_content(".", &files, env_vars).unwrap();
        let ctx = runtime.create_ctx_manager(BamlValue::String("test".to_string()), None);

        // Create two streams
        let mut stream1 = runtime
            .stream_function(
                "TestFunction".to_string(),
                &[("input".to_string(), BamlValue::String("test1".to_string()))]
                    .into_iter()
                    .collect(),
                &ctx,
                None,
                None,
                None,
                HashMap::new(),
            )
            .unwrap();

        let mut stream2 = runtime
            .stream_function(
                "TestFunction".to_string(),
                &[("input".to_string(), BamlValue::String("test2".to_string()))]
                    .into_iter()
                    .collect(),
                &ctx,
                None,
                None,
                None,
                HashMap::new(),
            )
            .unwrap();

        // Create separate cancellation tokens
        let token1 = CancellationToken::new();
        let token2 = CancellationToken::new();

        stream1.set_cancellation_token(token1.clone());
        stream2.set_cancellation_token(token2.clone());

        // Cancel only the first stream
        token1.cancel();

        // Verify only stream1 is cancelled
        assert!(stream1.is_cancelled());
        assert!(!stream2.is_cancelled());

        // Cancel the second stream
        token2.cancel();

        // Verify both streams are now cancelled
        assert!(stream1.is_cancelled());
        assert!(stream2.is_cancelled());
    }

    /// Test cancellation with event callbacks
    #[tokio::test]
    async fn test_cancellation_with_callbacks() {
        let mut files = HashMap::new();
        files.insert(
            "main.baml",
            r##"
            class TestOutput {
                message string
            }

            function TestFunction(input: string) -> TestOutput {
                client "openai/gpt-4o-mini"
                prompt #"
                    Return a test message: {{ input }}
                "#
            }
            "##,
        );

        let mut env_vars = HashMap::new();
        env_vars.insert("OPENAI_API_KEY".to_string(), "test-key".to_string());

        let runtime = BamlRuntime::from_file_content(".", &files, env_vars).unwrap();
        let ctx = runtime.create_ctx_manager(BamlValue::String("test".to_string()), None);

        // Create a stream
        let mut stream = runtime
            .stream_function(
                "TestFunction".to_string(),
                &[("input".to_string(), BamlValue::String("test".to_string()))]
                    .into_iter()
                    .collect(),
                &ctx,
                None,
                None,
                None,
                HashMap::new(),
            )
            .unwrap();

        let cancellation_token = CancellationToken::new();
        stream.set_cancellation_token(cancellation_token.clone());

        // Track callback invocations
        let callback_count = Arc::new(Mutex::new(0));
        let callback_count_clone = callback_count.clone();

        let on_event = move |_result: baml_runtime::FunctionResult| {
            let mut count = callback_count_clone.lock().unwrap();
            *count += 1;
        };

        // Cancel immediately
        cancellation_token.cancel();

        // Run the stream with callback
        let (result, _) = stream
            .run(
                None::<fn()>,
                Some(on_event),
                &ctx,
                None,
                None,
                HashMap::new(),
            )
            .await;

        // Should be cancelled
        assert!(result.is_err());
        
        // Callback should not have been called due to immediate cancellation
        let final_count = *callback_count.lock().unwrap();
        assert_eq!(final_count, 0, "Callbacks should not be invoked after cancellation");
    }

    /// Test that cancellation works with sync runtime
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn test_sync_stream_cancellation() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        
        rt.block_on(async {
            let mut files = HashMap::new();
            files.insert(
                "main.baml",
                r##"
                class TestOutput {
                    message string
                }

                function TestFunction(input: string) -> TestOutput {
                    client "openai/gpt-4o-mini"
                    prompt #"
                        Return a test message: {{ input }}
                    "#
                }
                "##,
            );

            let mut env_vars = HashMap::new();
            env_vars.insert("OPENAI_API_KEY".to_string(), "test-key".to_string());

            let runtime = BamlRuntime::from_file_content(".", &files, env_vars).unwrap();
            let ctx = runtime.create_ctx_manager(BamlValue::String("test".to_string()), None);

            // Create a stream
            let mut stream = runtime
                .stream_function(
                    "TestFunction".to_string(),
                    &[("input".to_string(), BamlValue::String("test".to_string()))]
                        .into_iter()
                        .collect(),
                    &ctx,
                    None,
                    None,
                    None,
                    HashMap::new(),
                )
                .unwrap();

            let cancellation_token = CancellationToken::new();
            stream.set_cancellation_token(cancellation_token.clone());

            // Cancel the token
            cancellation_token.cancel();

            // Run sync version
            let (result, _) = stream.run_sync(
                None::<fn()>,
                None::<fn(baml_runtime::FunctionResult)>,
                &ctx,
                None,
                None,
                HashMap::new(),
            );

            // Should be cancelled
            assert!(result.is_err());
            let error_msg = result.unwrap_err().to_string();
            assert!(
                error_msg.contains("cancelled") || error_msg.contains("canceled"),
                "Expected cancellation error, got: {}",
                error_msg
            );
        });
    }
}

#[cfg(feature = "internal")]
mod unit_tests {
    use super::*;
    use tokio_util::sync::CancellationToken;

    /// Test CancellationToken basic functionality
    #[tokio::test]
    async fn test_cancellation_token_basic() {
        let token = CancellationToken::new();
        
        // Initially not cancelled
        assert!(!token.is_cancelled());
        
        // Cancel the token
        token.cancel();
        
        // Now should be cancelled
        assert!(token.is_cancelled());
        
        // cancelled() future should complete immediately
        let result = tokio::time::timeout(
            std::time::Duration::from_millis(100),
            token.cancelled()
        ).await;
        
        assert!(result.is_ok(), "cancelled() future should complete immediately");
    }

    /// Test CancellationToken with tokio::select!
    #[tokio::test]
    async fn test_cancellation_with_select() {
        let token = CancellationToken::new();
        let token_clone = token.clone();
        
        // Spawn a task that cancels after delay
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            token_clone.cancel();
        });
        
        // Use select to race between work and cancellation
        let result = tokio::select! {
            _ = tokio::time::sleep(std::time::Duration::from_secs(1)) => {
                "work_completed"
            }
            _ = token.cancelled() => {
                "cancelled"
            }
        };
        
        assert_eq!(result, "cancelled");
    }

    /// Test that child tokens are cancelled when parent is cancelled
    #[tokio::test]
    async fn test_child_token_cancellation() {
        let parent_token = CancellationToken::new();
        let child_token = parent_token.child_token();
        
        // Initially neither is cancelled
        assert!(!parent_token.is_cancelled());
        assert!(!child_token.is_cancelled());
        
        // Cancel parent
        parent_token.cancel();
        
        // Both should be cancelled
        assert!(parent_token.is_cancelled());
        assert!(child_token.is_cancelled());
    }
}
