use std::sync::Arc;

use anyhow::Result;
use baml_ids::FunctionCallId;
use baml_rpc::{ast::tops::BamlFunctionId, BamlTypeId};
use baml_types::{type_meta, HasType};
use base64::Engine;

use crate::tracingv2::storage::interface::TraceEventWithMeta;

pub mod blob_storage;
mod errors;
mod trace_data;
pub mod types;

pub use blob_storage::{BlobRefCache, BlobStorage};

pub trait TypeLookup {
    fn type_lookup(&self, name: &str) -> Option<Arc<BamlTypeId>>;
    fn function_lookup(&self, name: &str) -> Option<Arc<BamlFunctionId>>;
    fn baml_src_hash(&self) -> Option<String>;
}

pub trait IRRpcState: TypeLookup + BlobStorage {}

impl<T: TypeLookup + BlobStorage> IRRpcState for T {}

pub(crate) trait IntoRpcEvent<'a, RpcOutputType> {
    fn to_rpc_event(&'a self, lookup: &(impl IRRpcState + ?Sized)) -> RpcOutputType;
}

pub(super) fn to_rpc_event<'a>(
    event: &'a TraceEventWithMeta,
    lookup: &(impl IRRpcState + ?Sized),
) -> baml_rpc::runtime_api::BackendTraceEvent<'a> {
    let timestamp = baml_rpc::EpochMsTimestamp::try_from(event.timestamp)
        .expect("Failed to convert timestamp to EpochMsTimestamp");

    // Convert the content to RPC format
    let mut content = event.content.to_rpc_event(lookup);

    // Extract blobs from the content
    let blob_cache = lookup.blob_cache();
    extract_blobs_from_trace_data(&mut content, blob_cache, &event.call_id.to_string());

    baml_rpc::runtime_api::BackendTraceEvent {
        call_id: event.call_id.clone(),
        function_event_id: event.function_event_id.clone(),
        call_stack: event.call_stack.clone(),
        timestamp,
        content,
    }
}

// Helper function to extract blobs from TraceData
fn extract_blobs_from_trace_data<'a>(
    trace_data: &mut baml_rpc::runtime_api::TraceData<'a>,
    blob_cache: &BlobRefCache,
    call_id: &str,
) {
    use baml_rpc::runtime_api::TraceData;

    // Get all blob replacements for this function call upfront
    let blob_replacements = blob_cache.get_blobs_for_function(call_id);
    // log::info!("Blob replacements: {:?}", blob_replacements.len());

    match trace_data {
        TraceData::FunctionStart { args, .. } => {
            // Mark this function call as started in the blob cache
            blob_cache.start_function_call(call_id);

            // Extract blobs from all arguments
            for (_, arg_value) in args.iter_mut() {
                blob_storage::extract_blobs_from_baml_value(arg_value, blob_cache, call_id);
            }
        }
        TraceData::FunctionEnd { .. } => {
            // Clean up blobs for this function call
            blob_cache.end_function_call(call_id);
        }
        TraceData::Intermediate(intermediate_data) => {
            use baml_rpc::runtime_api::{HTTPBody, IntermediateData};

            match intermediate_data {
                IntermediateData::LLMRequest { prompt, .. } => {
                    // Extract blobs from LLM request messages
                    // Process each message in the prompt
                    for message in prompt.iter_mut() {
                        for part in message.content.iter_mut() {
                            match part {
                                baml_rpc::runtime_api::LLMChatMessagePart::Media(media) => {
                                    // Extract blobs from Base64 media content
                                    if let baml_rpc::runtime_api::baml_value::MediaValue::Base64(
                                        base64_str,
                                    ) = &media.value
                                    {
                                        // Find the blob hash for this base64 content from our precomputed replacements
                                        if let Some((_, blob_hash)) =
                                            blob_replacements.iter().find(|(base64_content, _)| {
                                                base64_content == base64_str.as_ref()
                                            })
                                        {
                                            // Replace the Base64 variant with BlobRef containing the hash
                                            media.value =
                                                baml_rpc::runtime_api::baml_value::MediaValue::BlobRef(
                                                    std::borrow::Cow::Owned(blob_hash.clone()),
                                                );
                                        } else {
                                            // If not found in cache, store it now
                                            let blob_hash = blob_cache.store_blob(
                                                call_id,
                                                base64_str.as_ref(),
                                                media.mime_type.clone(),
                                            );
                                            media.value =
                                                baml_rpc::runtime_api::baml_value::MediaValue::BlobRef(
                                                    std::borrow::Cow::Owned(blob_hash),
                                                );
                                        }
                                    }
                                }
                                // Text parts shouldnt have images, so dont process this.
                                // baml_rpc::runtime_api::LLMChatMessagePart::Text(text) => {
                                //     // Extract blobs from text content that might contain base64
                                //     let mut processed_text = text.as_ref().to_string();
                                //     for (base64_content, blob_hash) in &blob_replacements {
                                //         if processed_text.contains(base64_content) {
                                //             processed_text =
                                //                 processed_text.replace(base64_content, blob_hash);
                                //         }
                                //     }
                                //     if processed_text != text.as_ref() {
                                //         *text = std::borrow::Cow::Owned(processed_text);
                                //     }
                                // }
                                baml_rpc::runtime_api::LLMChatMessagePart::WithMeta(
                                    inner_part,
                                    _,
                                ) => {
                                    // Recursively process the inner part
                                    match inner_part.as_mut() {
                                        baml_rpc::runtime_api::LLMChatMessagePart::Media(media) => {
                                            if let baml_rpc::runtime_api::baml_value::MediaValue::Base64(base64_str) = &media.value {
                                                // Find the blob hash for this base64 content from our precomputed replacements
                                                if let Some((_, blob_hash)) = blob_replacements
                                                    .iter()
                                                    .find(|(base64_content, _)| base64_content == base64_str.as_ref())
                                                {
                                                    media.value = baml_rpc::runtime_api::baml_value::MediaValue::BlobRef(
                                                        std::borrow::Cow::Owned(blob_hash.clone())
                                                    );
                                                } else {
                                                    // If not found in cache, store it now
                                                    let blob_hash = blob_cache.store_blob(
                                                        call_id,
                                                        base64_str.as_ref(),
                                                        media.mime_type.clone(),
                                                    );
                                                    media.value = baml_rpc::runtime_api::baml_value::MediaValue::BlobRef(
                                                        std::borrow::Cow::Owned(blob_hash)
                                                    );
                                                }
                                            }
                                        }
                                        baml_rpc::runtime_api::LLMChatMessagePart::Text(text) => {
                                            let mut processed_text = text.as_ref().to_string();
                                            for (base64_content, blob_hash) in &blob_replacements {
                                                if processed_text.contains(base64_content) {
                                                    processed_text = processed_text
                                                        .replace(base64_content, blob_hash);
                                                }
                                            }
                                            if processed_text != text.as_ref() {
                                                *text = std::borrow::Cow::Owned(processed_text);
                                            }
                                        }
                                        _ => {}
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                }
                IntermediateData::RawLLMRequest { body, .. } => {
                    // Extract blobs from raw request body
                    // HTTPBody has a raw field containing bytes
                    if let Ok(text) = std::str::from_utf8(&body.raw) {
                        let mut processed_text = text.to_string();
                        for (base64_content, blob_hash) in &blob_replacements {
                            if processed_text.contains(base64_content) {
                                processed_text = processed_text.replace(base64_content, blob_hash);
                            }
                        }
                        if processed_text != text {
                            body.raw = std::borrow::Cow::Owned(processed_text.into_bytes());
                        }
                    }
                }
                _ => {}
            }
        }
    }
}

impl<'a, T: std::fmt::Debug + HasType<type_meta::NonStreaming>>
    IntoRpcEvent<'a, baml_rpc::runtime_api::TraceData<'a>>
    for baml_types::tracing::events::TraceData<'a, T>
{
    fn to_rpc_event(
        &'a self,
        lookup: &(impl IRRpcState + ?Sized),
    ) -> baml_rpc::runtime_api::TraceData<'a> {
        use baml_types::tracing::events::TraceData;

        match self {
            TraceData::FunctionStart(function_start) => function_start.to_rpc_event(lookup),
            TraceData::FunctionEnd(function_end) => function_end.to_rpc_event(lookup),
            TraceData::LLMRequest(logged_llmrequest) => {
                baml_rpc::runtime_api::TraceData::Intermediate(
                    logged_llmrequest.to_rpc_event(lookup),
                )
            }
            TraceData::RawLLMRequest(httprequest) => {
                baml_rpc::runtime_api::TraceData::Intermediate(httprequest.to_rpc_event(lookup))
            }
            TraceData::RawLLMResponse(httpresponse) => {
                baml_rpc::runtime_api::TraceData::Intermediate(httpresponse.to_rpc_event(lookup))
            }
            TraceData::LLMResponse(logged_llmresponse) => {
                baml_rpc::runtime_api::TraceData::Intermediate(
                    logged_llmresponse.to_rpc_event(lookup),
                )
            }
            TraceData::RawLLMResponseStream(httpresponse) => {
                baml_rpc::runtime_api::TraceData::Intermediate(httpresponse.to_rpc_event(lookup))
            }
            TraceData::SetTags(tags) => baml_rpc::runtime_api::TraceData::Intermediate(
                baml_rpc::runtime_api::IntermediateData::SetTags(
                    tags.clone().into_iter().collect(),
                ),
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{borrow::Cow, collections::HashMap};

    use baml_rpc::{
        ast::type_reference::TypeReference,
        runtime_api::baml_value::{
            BamlValue, Media, MediaValue, TypeIndex, ValueContent, ValueMetadata,
        },
        RpcClientDetails,
    };
    use indexmap::IndexMap;

    use super::*;

    #[test]
    fn test_extract_blobs_from_media_value() {
        let cache = blob_storage::BlobRefCache::new();
        let function_call_id = "test-call-123";

        // Create a BamlValue with Base64 media content
        let mut baml_value = BamlValue {
            metadata: ValueMetadata {
                type_ref: TypeReference::string(), // Simplified
                type_index: TypeIndex::NotUnion,
                check_results: None,
            },
            value: ValueContent::Media(Media {
                mime_type: Some("image/png".to_string()),
                value: MediaValue::Base64(Cow::Borrowed("aGVsbG8gd29ybGQ=")), // "hello world" in base64
            }),
        };

        // Extract blobs - should replace Base64 with BlobRef
        blob_storage::extract_blobs_from_baml_value(&mut baml_value, &cache, function_call_id);

        // Check that the value was replaced with BlobRef
        if let ValueContent::Media(media) = &baml_value.value {
            match &media.value {
                MediaValue::BlobRef(hash) => {
                    // Should be a valid hash string
                    assert!(!hash.is_empty());

                    // Should have stored the blob in cache
                    assert_eq!(cache.blob_count(), 1);
                    assert!(cache.has_blob(hash.as_ref()));
                    assert_eq!(
                        cache.get_blob_content(hash.as_ref()).unwrap(),
                        b"aGVsbG8gd29ybGQ="
                    ); // "hello world" base64
                    assert!(cache.blob_has_ref(hash.as_ref(), function_call_id));
                }
                _ => panic!("Expected BlobRef, got {:?}", media.value),
            }
        } else {
            panic!("Expected Media content");
        }
    }

    #[test]
    fn test_extract_blobs_from_nested_baml_value() {
        let cache = blob_storage::BlobRefCache::new();
        let function_call_id = "test-call-456";

        // Create a nested BamlValue with Base64 media in a list
        let mut baml_value = BamlValue {
            metadata: ValueMetadata {
                type_ref: TypeReference::string(),
                type_index: TypeIndex::NotUnion,
                check_results: None,
            },
            value: ValueContent::List(vec![BamlValue {
                metadata: ValueMetadata {
                    type_ref: TypeReference::string(),
                    type_index: TypeIndex::NotUnion,
                    check_results: None,
                },
                value: ValueContent::Media(Media {
                    mime_type: Some("image/jpeg".to_string()),
                    value: MediaValue::Base64(Cow::Borrowed("dGVzdCBpbWFnZQ==")), // "test image" in base64
                }),
            }]),
        };

        // Extract blobs - should process nested values
        blob_storage::extract_blobs_from_baml_value(&mut baml_value, &cache, function_call_id);

        // Check that nested value was processed
        if let ValueContent::List(items) = &baml_value.value {
            if let ValueContent::Media(media) = &items[0].value {
                match &media.value {
                    MediaValue::BlobRef(_) => {
                        // Success - blob was extracted
                        assert_eq!(cache.blob_count(), 1);
                        // We can't easily verify the exact content without the hash, but we know a blob was stored
                    }
                    _ => panic!("Expected BlobRef in nested value"),
                }
            }
        }
    }

    #[test]
    fn test_extract_blobs_from_class_fields() {
        let cache = blob_storage::BlobRefCache::new();
        let function_call_id = "test-call-789";

        // Create a BamlValue with Base64 media in class fields
        let mut fields = IndexMap::new();
        fields.insert(
            "image".to_string(),
            BamlValue {
                metadata: ValueMetadata {
                    type_ref: TypeReference::string(),
                    type_index: TypeIndex::NotUnion,
                    check_results: None,
                },
                value: ValueContent::Media(Media {
                    mime_type: Some("image/png".to_string()),
                    value: MediaValue::Base64(Cow::Borrowed("Y2xhc3MgZmllbGQ=")), // "class field" in base64
                }),
            },
        );

        let mut baml_value = BamlValue {
            metadata: ValueMetadata {
                type_ref: TypeReference::string(),
                type_index: TypeIndex::NotUnion,
                check_results: None,
            },
            value: ValueContent::Class { fields },
        };

        // Extract blobs - should process class fields
        blob_storage::extract_blobs_from_baml_value(&mut baml_value, &cache, function_call_id);

        // Check that class field was processed
        if let ValueContent::Class { fields } = &baml_value.value {
            if let Some(image_value) = fields.get("image") {
                if let ValueContent::Media(media) = &image_value.value {
                    match &media.value {
                        MediaValue::BlobRef(_) => {
                            // Success - blob was extracted from class field
                            assert_eq!(cache.blob_count(), 1);
                            // We can't easily verify the exact content without the hash, but we know a blob was stored
                        }
                        _ => panic!("Expected BlobRef in class field"),
                    }
                }
            }
        }
    }

    #[test]
    fn test_blob_deduplication_in_baml_values() {
        let cache = blob_storage::BlobRefCache::new();
        let function_call_id = "test-call-dedup";

        // Create two identical BamlValues with same Base64 content
        let base64_content = "aWRlbnRpY2FsIGNvbnRlbnQ="; // "identical content" in base64

        let mut baml_value1 = BamlValue {
            metadata: ValueMetadata {
                type_ref: TypeReference::string(),
                type_index: TypeIndex::NotUnion,
                check_results: None,
            },
            value: ValueContent::Media(Media {
                mime_type: Some("image/png".to_string()),
                value: MediaValue::Base64(Cow::Borrowed(base64_content)),
            }),
        };

        let mut baml_value2 = BamlValue {
            metadata: ValueMetadata {
                type_ref: TypeReference::string(),
                type_index: TypeIndex::NotUnion,
                check_results: None,
            },
            value: ValueContent::Media(Media {
                mime_type: Some("image/png".to_string()),
                value: MediaValue::Base64(Cow::Borrowed(base64_content)),
            }),
        };

        // Extract blobs from both values
        blob_storage::extract_blobs_from_baml_value(&mut baml_value1, &cache, function_call_id);
        blob_storage::extract_blobs_from_baml_value(&mut baml_value2, &cache, function_call_id);

        // Should only have one blob stored (deduplication)
        assert_eq!(cache.blob_count(), 1);

        // Both values should have the same hash
        let hash1 = if let ValueContent::Media(media) = &baml_value1.value {
            if let MediaValue::BlobRef(hash) = &media.value {
                hash.to_string()
            } else {
                panic!("Expected BlobRef")
            }
        } else {
            panic!("Expected Media")
        };

        let hash2 = if let ValueContent::Media(media) = &baml_value2.value {
            if let MediaValue::BlobRef(hash) = &media.value {
                hash.to_string()
            } else {
                panic!("Expected BlobRef")
            }
        } else {
            panic!("Expected Media")
        };

        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_extract_blobs_from_llm_request_string() {
        let cache = blob_storage::BlobRefCache::new();
        let function_call_id = "test-llm-call";

        // First, store a blob in the cache (this would happen when processing BamlValue)
        let base64_content = "aGVsbG8gd29ybGQ="; // "hello world" in base64
        let blob_hash = cache.store_blob(
            function_call_id,
            base64_content,
            Some("image/png".to_string()),
        );

        // Test string content that contains the base64 of the stored blob
        let base64_content = "aGVsbG8gd29ybGQ="; // "hello world" in base64
        let input_content = format!("Here is an image: {base64_content} in the prompt");
        let result =
            blob_storage::extract_blobs_from_string(&input_content, &cache, function_call_id);

        // Should replace the base64 string with the blob hash
        assert!(result.contains(&blob_hash));
        assert!(!result.contains(base64_content));
        assert!(result.contains("Here is an image:"));
        assert!(result.contains("in the prompt"));

        // Should have the blob stored
        assert_eq!(cache.blob_count(), 1);
        assert!(cache.has_blob(&blob_hash));
        assert_eq!(
            cache.get_blob_content(&blob_hash).unwrap(),
            base64_content.as_bytes()
        );
        assert!(cache.blob_has_ref(&blob_hash, function_call_id));
    }

    #[test]
    fn test_extract_blobs_from_http_request_body() {
        let cache = blob_storage::BlobRefCache::new();
        let function_call_id = "test-http-call";

        // First, store blobs in the cache (this would happen when processing BamlValues)
        let base64_1 = "dGVzdCBpbWFnZSAx"; // "test image 1" in base64
        let base64_2 = "dGVzdCBpbWFnZSAy"; // "test image 2" in base64
        let blob1_hash =
            cache.store_blob(function_call_id, base64_1, Some("image/png".to_string()));
        let blob2_hash =
            cache.store_blob(function_call_id, base64_2, Some("image/jpeg".to_string()));

        // Test HTTP request body with base64 content that matches our stored blobs
        let base64_1 = "dGVzdCBpbWFnZSAx"; // "test image 1" in base64
        let base64_2 = "dGVzdCBpbWFnZSAy"; // "test image 2" in base64
        let body_content = format!(
            r#"{{
            "messages": [
                {{
                    "role": "user", 
                    "content": "Analyze this image: {base64_1}"
                }},
                {{
                    "role": "user",
                    "content": "And compare with: {base64_2}"
                }}
            ]
        }}"#
        );

        let result =
            blob_storage::extract_blobs_from_string(&body_content, &cache, function_call_id);

        // Should replace both base64 strings with blob hashes
        assert!(!result.contains(base64_1));
        assert!(!result.contains(base64_2));
        assert!(result.contains(&blob1_hash));
        assert!(result.contains(&blob2_hash));
        assert!(result.contains("Analyze this image:"));
        assert!(result.contains("And compare with:"));

        // Should have both blobs stored
        assert_eq!(cache.blob_count(), 2);

        // Check both blobs were stored correctly
        assert!(cache.has_blob(&blob1_hash));
        assert!(cache.has_blob(&blob2_hash));
        assert_eq!(
            cache.get_blob_content(&blob1_hash).unwrap(),
            base64_1.as_bytes()
        );
        assert_eq!(
            cache.get_blob_content(&blob2_hash).unwrap(),
            base64_2.as_bytes()
        );
        assert!(cache.blob_has_ref(&blob1_hash, function_call_id));
        assert!(cache.blob_has_ref(&blob2_hash, function_call_id));
    }

    #[test]
    fn test_extract_blobs_only_replaces_stored_content() {
        let cache = blob_storage::BlobRefCache::new();
        let function_call_id = "test-preserve-call";

        // Store only one specific blob
        let stored_base64 = "c3RvcmVkIGRhdGE="; // "stored data" in base64
        let blob_hash = cache.store_blob(function_call_id, stored_base64, None);

        // Test content with the stored base64 and some other base64 content
        let stored_base64 = "c3RvcmVkIGRhdGE="; // "stored data" in base64
        let other_base64 = "b3RoZXIgZGF0YQ=="; // "other data" in base64
        let input_content = format!("Stored: {stored_base64} Other: {other_base64} More text");

        let result =
            blob_storage::extract_blobs_from_string(&input_content, &cache, function_call_id);

        // Should only replace the stored base64 content
        assert!(result.contains(&blob_hash));
        assert!(!result.contains(stored_base64));
        assert!(result.contains(other_base64)); // This should remain unchanged
        assert!(result.contains("More text"));

        // Should only have the one stored blob
        assert_eq!(cache.blob_count(), 1);
        assert!(cache.has_blob(&blob_hash));
        assert_eq!(
            cache.get_blob_content(&blob_hash).unwrap(),
            stored_base64.as_bytes()
        );
        assert!(cache.blob_has_ref(&blob_hash, function_call_id));
    }

    #[test]
    fn test_extract_blobs_integration_with_trace_data_processing() {
        use std::borrow::Cow;

        use baml_rpc::runtime_api::{HTTPBody, IntermediateData, TraceData};

        let cache = blob_storage::BlobRefCache::new();
        let function_call_id = "integration-test-call";

        // First, store a blob in the cache (this would happen during BamlValue processing)
        let base64_content = "aW50ZWdyYXRpb24gdGVzdA=="; // "integration test" in base64
        let blob_hash = cache.store_blob(
            function_call_id,
            base64_content,
            Some("image/png".to_string()),
        );

        // Test the actual trace data processing pipeline with base64 that matches our stored blob
        let base64_content = "aW50ZWdyYXRpb24gdGVzdA=="; // "integration test" in base64
        let original_body = format!(r#"{{"image": "{base64_content}"}}"#);
        let mut trace_data = TraceData::Intermediate(IntermediateData::RawLLMRequest {
            http_request_id: "req-123".to_string(),
            url: "https://api.example.com/chat".to_string(),
            method: "POST".to_string(),
            headers: std::collections::HashMap::new(),
            body: HTTPBody {
                raw: Cow::Borrowed(original_body.as_bytes()),
            },
            client_details: RpcClientDetails {
                name: "test-client".to_string(),
                provider: "openai".to_string(),
                options: IndexMap::new(),
            },
        });

        // Process the trace data (simulating the actual pipeline)
        extract_blobs_from_trace_data(&mut trace_data, &cache, function_call_id);

        // Verify the body was processed
        if let TraceData::Intermediate(IntermediateData::RawLLMRequest { body, .. }) = &trace_data {
            let processed_body = std::str::from_utf8(&body.raw).unwrap();
            assert!(processed_body.contains(&blob_hash));
            assert!(!processed_body.contains(base64_content));
            assert!(processed_body.contains(r#"{"image": ""#));
        } else {
            panic!("Expected RawLLMRequest");
        }

        // Should have stored the blob
        assert_eq!(cache.blob_count(), 1);
        assert!(cache.has_blob(&blob_hash));
        assert_eq!(
            cache.get_blob_content(&blob_hash).unwrap(),
            base64_content.as_bytes()
        );
        assert!(cache.blob_has_ref(&blob_hash, function_call_id));
    }

    #[test]
    fn test_extract_blobs_from_llm_request_media() {
        use std::borrow::Cow;

        use baml_rpc::runtime_api::{
            baml_value::{Media, MediaValue},
            IntermediateData, LLMChatMessage, LLMChatMessagePart, TraceData,
        };

        let cache = blob_storage::BlobRefCache::new();
        let function_call_id = "llm-media-test-call";

        // First, simulate a FunctionStart event that would establish blobs in the cache
        let base64_content = "dGVzdCBpbWFnZSBkYXRh"; // "test image data" in base64
        let mut function_start_data = TraceData::FunctionStart {
            function_display_name: "test_function".to_string(),
            function_type: baml_rpc::runtime_api::FunctionType::Native,
            is_stream: false,
            tags: HashMap::new(),
            baml_function_content: None,
            args: vec![(
                "image_arg".to_string(),
                BamlValue {
                    metadata: ValueMetadata {
                        type_ref: TypeReference::string(),
                        type_index: TypeIndex::NotUnion,
                        check_results: None,
                    },
                    value: ValueContent::Media(Media {
                        mime_type: Some("image/png".to_string()),
                        value: MediaValue::Base64(Cow::Borrowed(base64_content)),
                    }),
                },
            )],
        };

        // Process the FunctionStart first to establish the blob in cache
        extract_blobs_from_trace_data(&mut function_start_data, &cache, function_call_id);

        // Create an LLMRequest with Media content containing the same Base64 data
        let mut trace_data = TraceData::Intermediate(IntermediateData::LLMRequest {
            client_name: "test-client".to_string(),
            client_provider: "openai".to_string(),
            params: std::collections::HashMap::new(),
            prompt: vec![LLMChatMessage {
                role: "user".to_string(),
                content: vec![
                    LLMChatMessagePart::Text(Cow::Borrowed("Here's an image:")),
                    LLMChatMessagePart::Media(Media {
                        mime_type: Some("image/png".to_string()),
                        value: MediaValue::Base64(Cow::Borrowed(base64_content)),
                    }),
                    LLMChatMessagePart::Text(Cow::Borrowed("What do you see?")),
                ],
            }],
        });

        // Process the trace data (simulating the actual pipeline)
        extract_blobs_from_trace_data(&mut trace_data, &cache, function_call_id);

        // Verify the media was processed and Base64 was replaced with BlobRef
        if let TraceData::Intermediate(IntermediateData::LLMRequest { prompt, .. }) = &trace_data {
            let message = &prompt[0];
            let media_part = &message.content[1];

            if let LLMChatMessagePart::Media(media) = media_part {
                match &media.value {
                    MediaValue::BlobRef(blob_hash) => {
                        // Should have stored the blob and replaced with hash
                        assert!(!blob_hash.is_empty());

                        // Verify the blob was stored in cache
                        assert_eq!(cache.blob_count(), 1);
                        assert!(cache.has_blob(blob_hash.as_ref()));
                        assert_eq!(
                            cache.get_blob_content(blob_hash.as_ref()).unwrap(),
                            b"dGVzdCBpbWFnZSBkYXRh"
                        );
                        assert!(cache.blob_has_ref(blob_hash.as_ref(), function_call_id));
                    }
                    _ => panic!("Expected BlobRef, got {:?}", media.value),
                }
            } else {
                panic!("Expected Media part");
            }

            // Text parts should remain unchanged
            if let LLMChatMessagePart::Text(text) = &message.content[0] {
                assert_eq!(text.as_ref(), "Here's an image:");
            }
            if let LLMChatMessagePart::Text(text) = &message.content[2] {
                assert_eq!(text.as_ref(), "What do you see?");
            }
        } else {
            panic!("Expected LLMRequest");
        }
    }

    #[test]
    fn test_extract_blobs_from_llm_request_text_with_base64() {
        use std::borrow::Cow;

        use baml_rpc::runtime_api::{
            IntermediateData, LLMChatMessage, LLMChatMessagePart, TraceData,
        };

        let cache = blob_storage::BlobRefCache::new();
        let function_call_id = "llm-text-test-call";

        // First, store a blob in the cache (this would happen during BamlValue processing)
        let base64_content = "ZW1iZWRkZWQgaW1hZ2U="; // "embedded image" in base64
        let blob_hash = cache.store_blob(
            function_call_id,
            base64_content,
            Some("image/jpeg".to_string()),
        );

        // Create an LLMRequest with text that contains the base64 of our stored blob
        let base64_content = "ZW1iZWRkZWQgaW1hZ2U="; // "embedded image" in base64
        let text_with_base64 = format!("Please analyze this data: {base64_content}");
        let mut trace_data = TraceData::Intermediate(IntermediateData::LLMRequest {
            client_name: "test-client".to_string(),
            client_provider: "anthropic".to_string(),
            params: std::collections::HashMap::new(),
            prompt: vec![LLMChatMessage {
                role: "user".to_string(),
                content: vec![LLMChatMessagePart::Text(Cow::Borrowed(&text_with_base64))],
            }],
        });

        // Process the trace data (simulating the actual pipeline)
        extract_blobs_from_trace_data(&mut trace_data, &cache, function_call_id);

        // Verify the text was processed and base64 was replaced with blob hash
        if let TraceData::Intermediate(IntermediateData::LLMRequest { prompt, .. }) = &trace_data {
            let message = &prompt[0];

            if let LLMChatMessagePart::Text(processed_text) = &message.content[0] {
                // Regular text parts are not processed for blob replacement by design
                // (only WithMeta wrapped text parts are processed)
                assert!(!processed_text.contains(&blob_hash));
                assert!(processed_text.contains(base64_content)); // Original should remain
                assert!(processed_text.contains("Please analyze this data:"));
            } else {
                panic!("Expected Text part");
            }
        } else {
            panic!("Expected LLMRequest");
        }

        // Should still have the blob stored
        assert_eq!(cache.blob_count(), 1);
        assert!(cache.has_blob(&blob_hash));
        assert_eq!(
            cache.get_blob_content(&blob_hash).unwrap(),
            base64_content.as_bytes()
        );
        assert!(cache.blob_has_ref(&blob_hash, function_call_id));
    }
}
