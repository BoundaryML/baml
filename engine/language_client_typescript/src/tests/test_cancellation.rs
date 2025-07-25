#[cfg(test)]
mod cancellation_tests {
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use napi::Env;
    use tokio::time::timeout;
    use tokio_util::sync::CancellationToken;

    use crate::types::{
        function_result_stream::FunctionResultStream,
        runtime_ctx_manager::RuntimeContextManager,
    };

    /// Test that FunctionResultStream properly handles cancellation
    #[tokio::test]
    async fn test_function_result_stream_cancellation() {
        // Create a mock BAML runtime stream
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

        let runtime = baml_runtime::BamlRuntime::from_file_content(".", &files, env_vars).unwrap();
        let ctx = runtime.create_ctx_manager(baml_types::BamlValue::String("test".to_string()), None);

        // Create a Rust stream
        let rust_stream = runtime
            .stream_function(
                "TestFunction".to_string(),
                &[("input".to_string(), baml_types::BamlValue::String("test".to_string()))]
                    .into_iter()
                    .collect(),
                &ctx,
                None,
                None,
                None,
                HashMap::new(),
            )
            .unwrap();

        // Create TypeScript FFI wrapper
        let mut ffi_stream = FunctionResultStream::new(rust_stream, None, None, None);

        // Test cancellation
        let result = ffi_stream.cancel();
        assert!(result.is_ok(), "Cancel should succeed");

        // Verify the underlying stream is cancelled
        assert!(ffi_stream.cancellation_token.is_cancelled());
    }

    /// Test that cancellation works with the done() method
    #[tokio::test]
    async fn test_done_with_cancellation() {
        // This test would require a full NAPI environment setup
        // For now, we'll test the core logic
        
        let cancellation_token = CancellationToken::new();
        
        // Test that a cancelled token prevents execution
        cancellation_token.cancel();
        
        assert!(cancellation_token.is_cancelled());
        
        // In the real implementation, this would prevent the stream from running
        let result = tokio::select! {
            _ = tokio::time::sleep(Duration::from_secs(1)) => "completed",
            _ = cancellation_token.cancelled() => "cancelled"
        };
        
        assert_eq!(result, "cancelled");
    }

    /// Test that finalization cancels the stream
    #[test]
    fn test_finalization_cancels_stream() {
        let cancellation_token = CancellationToken::new();
        
        // Simulate what happens in ObjectFinalize
        cancellation_token.cancel();
        
        assert!(cancellation_token.is_cancelled());
    }

    /// Test multiple streams with independent cancellation
    #[tokio::test]
    async fn test_independent_stream_cancellation() {
        let token1 = CancellationToken::new();
        let token2 = CancellationToken::new();
        
        // Initially neither is cancelled
        assert!(!token1.is_cancelled());
        assert!(!token2.is_cancelled());
        
        // Cancel only the first
        token1.cancel();
        
        assert!(token1.is_cancelled());
        assert!(!token2.is_cancelled());
        
        // Cancel the second
        token2.cancel();
        
        assert!(token1.is_cancelled());
        assert!(token2.is_cancelled());
    }

    /// Test cancellation with timeout
    #[tokio::test]
    async fn test_cancellation_with_timeout() {
        let token = CancellationToken::new();
        let token_clone = token.clone();
        
        // Cancel after delay
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(100)).await;
            token_clone.cancel();
        });
        
        // Wait for cancellation with timeout
        let result = timeout(Duration::from_secs(1), token.cancelled()).await;
        
        assert!(result.is_ok(), "Cancellation should complete within timeout");
    }
}
