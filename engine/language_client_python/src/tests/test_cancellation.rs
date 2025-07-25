#[cfg(test)]
mod python_cancellation_tests {
    use std::collections::HashMap;
    use std::time::Duration;

    use pyo3::prelude::*;
    use tokio::time::timeout;
    use tokio_util::sync::CancellationToken;

    use crate::types::function_result_stream::{FunctionResultStream, SyncFunctionResultStream};

    /// Test that Python FunctionResultStream properly handles cancellation
    #[tokio::test]
    async fn test_python_function_result_stream_cancellation() {
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

        // Create Python FFI wrapper
        let ffi_stream = FunctionResultStream::new(rust_stream, None, None, None, HashMap::new());

        // Test cancellation
        Python::with_gil(|py| {
            let result = ffi_stream.cancel();
            assert!(result.is_ok(), "Cancel should succeed");

            // Verify the stream is cancelled
            assert!(ffi_stream.is_cancelled());
        });
    }

    /// Test that sync Python stream handles cancellation
    #[test]
    fn test_python_sync_stream_cancellation() {
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

        // Create sync Python FFI wrapper
        let ffi_stream = SyncFunctionResultStream::new(rust_stream, None, None, None, HashMap::new());

        // Test cancellation
        Python::with_gil(|py| {
            let result = ffi_stream.cancel();
            assert!(result.is_ok(), "Cancel should succeed");

            // Verify the stream is cancelled
            assert!(ffi_stream.is_cancelled());
        });
    }

    /// Test cancellation token functionality
    #[tokio::test]
    async fn test_cancellation_token_functionality() {
        let token = CancellationToken::new();
        
        // Initially not cancelled
        assert!(!token.is_cancelled());
        
        // Cancel the token
        token.cancel();
        
        // Now should be cancelled
        assert!(token.is_cancelled());
        
        // cancelled() future should complete immediately
        let result = timeout(Duration::from_millis(100), token.cancelled()).await;
        assert!(result.is_ok(), "cancelled() future should complete immediately");
    }

    /// Test that cancellation works with tokio::select!
    #[tokio::test]
    async fn test_python_cancellation_with_select() {
        let token = CancellationToken::new();
        let token_clone = token.clone();
        
        // Spawn a task that cancels after delay
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(100)).await;
            token_clone.cancel();
        });
        
        // Use select to race between work and cancellation
        let result = tokio::select! {
            _ = tokio::time::sleep(Duration::from_secs(1)) => {
                "work_completed"
            }
            _ = token.cancelled() => {
                "cancelled"
            }
        };
        
        assert_eq!(result, "cancelled");
    }
}
