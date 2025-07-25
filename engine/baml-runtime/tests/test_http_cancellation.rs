#[cfg(feature = "internal")]
#[cfg(not(feature = "skip-integ-tests"))]
mod http_cancellation_tests {
    use std::{
        collections::HashMap,
        sync::{Arc, Mutex},
        time::Duration,
    };

    use baml_runtime::internal::llm_client::{
        primitive::request::{execute_request, RequestBuilder},
        traits::{HttpContext, WithClient},
        ErrorCode, LLMResponse,
    };
    use baml_types::BamlMap;
    use internal_baml_jinja::RenderedChatMessage;
    use reqwest::Client;
    use tokio::time::timeout;
    use tokio_util::sync::CancellationToken;

    // Mock HTTP context for testing
    struct MockHttpContext {
        request_id: baml_ids::HttpRequestId,
        runtime_context: baml_runtime::RuntimeContext,
    }

    impl HttpContext for MockHttpContext {
        fn http_request_id(&self) -> &baml_ids::HttpRequestId {
            &self.request_id
        }

        fn runtime_context(&self) -> &baml_runtime::RuntimeContext {
            &self.runtime_context
        }
    }

    // Mock client for testing
    struct MockClient {
        client: Client,
        request_options: BamlMap<String, serde_json::Value>,
        context: internal_baml_jinja::RenderContext_Client,
    }

    impl WithClient for MockClient {
        fn context(&self) -> &internal_baml_jinja::RenderContext_Client {
            &self.context
        }

        fn model_features(&self) -> &baml_runtime::internal::llm_client::ModelFeatures {
            // Return a default ModelFeatures for testing
            &baml_runtime::internal::llm_client::ModelFeatures::default()
        }
    }

    impl RequestBuilder for MockClient {
        fn http_client(&self) -> &reqwest::Client {
            &self.client
        }

        fn request_options(&self) -> &BamlMap<String, serde_json::Value> {
            &self.request_options
        }

        async fn build_request(
            &self,
            _prompt: either::Either<&String, &[RenderedChatMessage]>,
            _allow_proxy: bool,
            _stream: bool,
            _expose_secrets: bool,
            _cancellation_token: Option<tokio_util::sync::CancellationToken>,
        ) -> anyhow::Result<reqwest::RequestBuilder> {
            // Create a request to a slow endpoint for testing cancellation
            Ok(self.client.get("https://httpbin.org/delay/5"))
        }
    }

    /// Test that HTTP requests are properly cancelled
    #[tokio::test]
    async fn test_http_request_cancellation() {
        let client = MockClient {
            client: Client::new(),
            request_options: BamlMap::new(),
            context: internal_baml_jinja::RenderContext_Client {
                name: "test_client".to_string(),
                provider: "test".to_string(),
                options: serde_json::Map::new(),
            },
        };

        let ctx = MockHttpContext {
            request_id: baml_ids::HttpRequestId::new(),
            runtime_context: baml_runtime::RuntimeContext::new(
                vec![],
                None,
                None,
                None,
                HashMap::new(),
            ),
        };

        // Build a request
        let request_builder = client
            .build_request(
                either::Left(&"test".to_string()),
                false,
                false,
                false,
                None,
            )
            .await
            .unwrap();
        let request = request_builder.build().unwrap();

        let cancellation_token = CancellationToken::new();

        // Cancel after a short delay
        let cancel_token = cancellation_token.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(100)).await;
            cancel_token.cancel();
        });

        let start_time = web_time::SystemTime::now();
        let instant_start = web_time::Instant::now();

        // Execute request with cancellation
        let result = execute_request(
            &client,
            request,
            either::Left(&"test".to_string()),
            start_time,
            instant_start,
            &ctx,
            true,
            Some(cancellation_token),
        )
        .await;

        // Should be cancelled
        assert!(result.is_err());
        
        if let Err(LLMResponse::LLMFailure(error)) = result {
            assert_eq!(error.code, ErrorCode::Other(499)); // Client Closed Request
            assert!(error.message.contains("cancelled"));
        } else {
            panic!("Expected LLMFailure with cancellation error");
        }
    }

    /// Test that HTTP requests complete normally without cancellation
    #[tokio::test]
    async fn test_http_request_without_cancellation() {
        let client = MockClient {
            client: Client::new(),
            request_options: BamlMap::new(),
            context: internal_baml_jinja::RenderContext_Client {
                name: "test_client".to_string(),
                provider: "test".to_string(),
                options: serde_json::Map::new(),
            },
        };

        let ctx = MockHttpContext {
            request_id: baml_ids::HttpRequestId::new(),
            runtime_context: baml_runtime::RuntimeContext::new(
                vec![],
                None,
                None,
                None,
                HashMap::new(),
            ),
        };

        // Build a request to a fast endpoint
        let request = client.client.get("https://httpbin.org/get").build().unwrap();

        let start_time = web_time::SystemTime::now();
        let instant_start = web_time::Instant::now();

        // Execute request without cancellation
        let result = timeout(
            Duration::from_secs(10),
            execute_request(
                &client,
                request,
                either::Left(&"test".to_string()),
                start_time,
                instant_start,
                &ctx,
                true,
                None, // No cancellation token
            ),
        )
        .await;

        // Should complete successfully (or fail due to network, but not cancellation)
        assert!(result.is_ok(), "Request should complete within timeout");
        
        let execute_result = result.unwrap();
        // The result might be an error due to network issues, but it shouldn't be a cancellation error
        if let Err(LLMResponse::LLMFailure(error)) = execute_result {
            assert_ne!(error.code, ErrorCode::Other(499), "Should not be a cancellation error");
        }
    }

    /// Test that cancelled token prevents request execution
    #[tokio::test]
    async fn test_pre_cancelled_token() {
        let client = MockClient {
            client: Client::new(),
            request_options: BamlMap::new(),
            context: internal_baml_jinja::RenderContext_Client {
                name: "test_client".to_string(),
                provider: "test".to_string(),
                options: serde_json::Map::new(),
            },
        };

        let ctx = MockHttpContext {
            request_id: baml_ids::HttpRequestId::new(),
            runtime_context: baml_runtime::RuntimeContext::new(
                vec![],
                None,
                None,
                None,
                HashMap::new(),
            ),
        };

        // Build a request
        let request_builder = client
            .build_request(
                either::Left(&"test".to_string()),
                false,
                false,
                false,
                None,
            )
            .await
            .unwrap();
        let request = request_builder.build().unwrap();

        // Pre-cancel the token
        let cancellation_token = CancellationToken::new();
        cancellation_token.cancel();

        let start_time = web_time::SystemTime::now();
        let instant_start = web_time::Instant::now();

        // Execute request with pre-cancelled token
        let result = execute_request(
            &client,
            request,
            either::Left(&"test".to_string()),
            start_time,
            instant_start,
            &ctx,
            true,
            Some(cancellation_token),
        )
        .await;

        // Should be cancelled immediately
        assert!(result.is_err());
        
        if let Err(LLMResponse::LLMFailure(error)) = result {
            assert_eq!(error.code, ErrorCode::Other(499)); // Client Closed Request
            assert!(error.message.contains("cancelled"));
        } else {
            panic!("Expected LLMFailure with cancellation error");
        }
    }

    /// Test cancellation timing
    #[tokio::test]
    async fn test_cancellation_timing() {
        let client = MockClient {
            client: Client::new(),
            request_options: BamlMap::new(),
            context: internal_baml_jinja::RenderContext_Client {
                name: "test_client".to_string(),
                provider: "test".to_string(),
                options: serde_json::Map::new(),
            },
        };

        let ctx = MockHttpContext {
            request_id: baml_ids::HttpRequestId::new(),
            runtime_context: baml_runtime::RuntimeContext::new(
                vec![],
                None,
                None,
                None,
                HashMap::new(),
            ),
        };

        // Build a request to a slow endpoint
        let request_builder = client
            .build_request(
                either::Left(&"test".to_string()),
                false,
                false,
                false,
                None,
            )
            .await
            .unwrap();
        let request = request_builder.build().unwrap();

        let cancellation_token = CancellationToken::new();

        // Cancel after 200ms
        let cancel_token = cancellation_token.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(200)).await;
            cancel_token.cancel();
        });

        let start_time = web_time::Instant::now();
        let system_start = web_time::SystemTime::now();

        // Execute request with cancellation
        let result = execute_request(
            &client,
            request,
            either::Left(&"test".to_string()),
            system_start,
            start_time,
            &ctx,
            true,
            Some(cancellation_token),
        )
        .await;

        let elapsed = start_time.elapsed();

        // Should be cancelled and should take less than the full 5 seconds the endpoint would take
        assert!(result.is_err());
        assert!(elapsed < Duration::from_secs(1), "Request should be cancelled quickly, took: {:?}", elapsed);
        
        if let Err(LLMResponse::LLMFailure(error)) = result {
            assert_eq!(error.code, ErrorCode::Other(499)); // Client Closed Request
            assert!(error.message.contains("cancelled"));
        } else {
            panic!("Expected LLMFailure with cancellation error");
        }
    }
}
