#[cfg(feature = "internal")]
#[cfg(not(feature = "skip-integ-tests"))]
mod integration_cancellation_tests {
    use std::{
        collections::HashMap,
        sync::{Arc, Mutex},
        time::Duration,
    };

    use baml_runtime::{BamlRuntime, RuntimeContextManager};
    use baml_types::BamlValue;
    use tokio::time::timeout;
    use tokio_util::sync::CancellationToken;

    /// Integration test: Full stack cancellation from TypeScript to HTTP
    #[tokio::test]
    async fn test_full_stack_cancellation() {
        // This test simulates the full flow:
        // TypeScript abort() -> Rust FunctionResultStream.cancel() -> HTTP request cancellation

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
                    base_url "https://httpbin.org/delay/10" // Slow endpoint for testing
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

        // Create a stream (simulating TypeScript BamlStream creation)
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

        // Simulate TypeScript abort() call
        let cancellation_token = CancellationToken::new();
        stream.set_cancellation_token(cancellation_token.clone());

        // Cancel after a short delay (simulating user clicking cancel)
        let cancel_token = cancellation_token.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(200)).await;
            cancel_token.cancel(); // This simulates ffiStream.cancel()
        });

        let start_time = std::time::Instant::now();

        // Run the stream (this should be cancelled quickly)
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

        let elapsed = start_time.elapsed();

        // Should complete within timeout due to cancellation
        assert!(result.is_ok(), "Stream should complete due to cancellation");
        
        let (stream_result, _) = result.unwrap();
        
        // Should be cancelled (not a successful completion)
        assert!(stream_result.is_err(), "Stream should be cancelled");
        
        // Should be much faster than the 10-second delay endpoint
        assert!(
            elapsed < Duration::from_secs(2),
            "Cancellation should be fast, took: {:?}",
            elapsed
        );

        let error_msg = stream_result.unwrap_err().to_string();
        assert!(
            error_msg.contains("cancelled") || error_msg.contains("canceled"),
            "Expected cancellation error, got: {}",
            error_msg
        );
    }

    /// Test that cancellation works with multiple concurrent streams
    #[tokio::test]
    async fn test_concurrent_stream_cancellation() {
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
                    base_url "https://httpbin.org/delay/5"
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

        // Create multiple streams
        let mut streams = Vec::new();
        let mut tokens = Vec::new();

        for i in 0..3 {
            let mut stream = runtime
                .stream_function(
                    "TestFunction".to_string(),
                    &[("input".to_string(), BamlValue::String(format!("test{}", i)))]
                        .into_iter()
                        .collect(),
                    &ctx,
                    None,
                    None,
                    None,
                    HashMap::new(),
                )
                .unwrap();

            let token = CancellationToken::new();
            stream.set_cancellation_token(token.clone());
            
            streams.push(stream);
            tokens.push(token);
        }

        // Cancel only the middle stream
        tokens[1].cancel();

        // Run all streams concurrently
        let mut handles = Vec::new();
        for (i, mut stream) in streams.into_iter().enumerate() {
            let ctx_clone = ctx.clone();
            let handle = tokio::spawn(async move {
                let result = timeout(
                    Duration::from_secs(10),
                    stream.run(
                        None::<fn()>,
                        None::<fn(baml_runtime::FunctionResult)>,
                        &ctx_clone,
                        None,
                        None,
                        HashMap::new(),
                    ),
                ).await;
                (i, result)
            });
            handles.push(handle);
        }

        // Wait for all to complete
        let results = futures::future::join_all(handles).await;

        // Check results
        for (handle_result, (stream_index, timeout_result)) in results.into_iter().enumerate() {
            assert!(handle_result.is_ok(), "Task {} should complete", stream_index);
            
            let (stream_result, _) = timeout_result.unwrap().unwrap();
            
            if stream_index == 1 {
                // Middle stream should be cancelled
                assert!(stream_result.is_err(), "Stream {} should be cancelled", stream_index);
                let error_msg = stream_result.unwrap_err().to_string();
                assert!(
                    error_msg.contains("cancelled") || error_msg.contains("canceled"),
                    "Stream {} should have cancellation error, got: {}",
                    stream_index,
                    error_msg
                );
            }
            // Note: Other streams might also fail due to network issues, but they shouldn't be cancelled
        }
    }

    /// Test cancellation with event callbacks
    #[tokio::test]
    async fn test_cancellation_with_event_callbacks() {
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
                    base_url "https://httpbin.org/delay/3"
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

        // Track events
        let event_count = Arc::new(Mutex::new(0));
        let event_count_clone = event_count.clone();

        let on_event = move |_result: baml_runtime::FunctionResult| {
            let mut count = event_count_clone.lock().unwrap();
            *count += 1;
        };

        // Cancel quickly
        let cancel_token = cancellation_token.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(100)).await;
            cancel_token.cancel();
        });

        // Run with event callback
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
        
        // Events should be minimal due to quick cancellation
        let final_count = *event_count.lock().unwrap();
        assert!(
            final_count <= 1,
            "Should have minimal events due to cancellation, got: {}",
            final_count
        );
    }

    /// Test that cancellation properly cleans up resources
    #[tokio::test]
    async fn test_cancellation_resource_cleanup() {
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

        // Create multiple streams and cancel them all
        let mut streams = Vec::new();
        let mut tokens = Vec::new();

        for i in 0..5 {
            let mut stream = runtime
                .stream_function(
                    "TestFunction".to_string(),
                    &[("input".to_string(), BamlValue::String(format!("test{}", i)))]
                        .into_iter()
                        .collect(),
                    &ctx,
                    None,
                    None,
                    None,
                    HashMap::new(),
                )
                .unwrap();

            let token = CancellationToken::new();
            stream.set_cancellation_token(token.clone());
            
            streams.push(stream);
            tokens.push(token);
        }

        // Cancel all streams
        for token in &tokens {
            token.cancel();
        }

        // Run all streams - they should all be cancelled quickly
        let start_time = std::time::Instant::now();
        
        let mut handles = Vec::new();
        for mut stream in streams {
            let ctx_clone = ctx.clone();
            let handle = tokio::spawn(async move {
                stream.run(
                    None::<fn()>,
                    None::<fn(baml_runtime::FunctionResult)>,
                    &ctx_clone,
                    None,
                    None,
                    HashMap::new(),
                ).await
            });
            handles.push(handle);
        }

        let results = futures::future::join_all(handles).await;
        let elapsed = start_time.elapsed();

        // All should complete quickly due to cancellation
        assert!(
            elapsed < Duration::from_secs(1),
            "All streams should be cancelled quickly, took: {:?}",
            elapsed
        );

        // All should be cancelled
        for (i, result) in results.into_iter().enumerate() {
            assert!(result.is_ok(), "Task {} should complete", i);
            let (stream_result, _) = result.unwrap();
            assert!(stream_result.is_err(), "Stream {} should be cancelled", i);
        }
    }
}
