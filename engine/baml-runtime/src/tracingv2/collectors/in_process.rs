mod function_log;
mod function_log_inner;
pub mod models;

use std::sync::Mutex;

use baml_types::tracing::events::FunctionId;
pub use function_log::FunctionLog;
use indexmap::IndexSet;
use models::Usage;

use crate::tracingv2::storage::storage::{FunctionTrackerTrait, BAML_TRACER};

pub use function_log_inner::drop_function_log_inner;

/// A Collector holds references to multiple FunctionIds in order of insertion.
/// When dropped, it decrements the global ref counts for all tracked IDs.
#[derive(Debug)]
pub struct Collector {
    name: String,
    // Using IndexSet to preserve the insertion order of tracked FuncIds
    tracked_ids: Mutex<IndexSet<FunctionId>>,
}

impl FunctionTrackerTrait for Collector {
    fn track_function(&self, fid: FunctionId) {
        log::trace!("Tracking function: {:?}", fid);
        // First increment the global ref count
        BAML_TRACER.lock().unwrap().inc_ref(&fid);

        // Then add to our set (maintaining insertion order)
        let mut guard = self.tracked_ids.lock().unwrap();
        guard.insert(fid);
    }

    fn untrack_function(&self, fid: &FunctionId) {
        let mut guard = self.tracked_ids.lock().unwrap();
        if guard.swap_remove(fid) {
            BAML_TRACER.lock().unwrap().dec_ref(fid);
        }
    }
}

impl Collector {
    pub fn new(name: Option<String>) -> Self {
        Self {
            name: name.unwrap_or("collector".to_string()),
            tracked_ids: Mutex::new(IndexSet::new()),
        }
    }

    pub fn name(&self) -> String {
        self.name.clone()
    }

    pub fn function_logs(&self) -> Vec<FunctionLog> {
        let guard = self.tracked_ids.lock().unwrap();
        guard
            .iter()
            .map(|fid| FunctionLog::new(fid.clone()))
            .collect()
    }

    pub fn take_logs(&self) -> Vec<FunctionLog> {
        let mut guard = self.tracked_ids.lock().unwrap();
        let logs = guard.iter().map(|fid| FunctionLog::new(fid.clone())).collect();
        guard.clear();
        logs
    }

    pub fn last_function_log(&self) -> Option<FunctionLog> {
        let guard = self.tracked_ids.lock().unwrap();
        guard
            .iter()
            .last() // Based on insertion order
            .map(|id| FunctionLog::new(id.clone()))
    }

    pub fn function_log_by_id(&self, fid: &FunctionId) -> Option<FunctionLog> {
        let guard = self.tracked_ids.lock().unwrap();
        guard.get(fid).map(|fid| FunctionLog::new(fid.clone()))
    }

    pub fn usage(&self) -> Usage {
        let guard = self.tracked_ids.lock().unwrap();
        let mut total_usage = Usage::default();
        for fid in guard.iter() {
            let usage = FunctionLog::new(fid.clone()).usage();
            total_usage.accumulate(usage);
        }
        total_usage
    }
}

impl Clone for Collector {
    fn clone(&self) -> Self {
        // log::info!("Cloning collector: {}", self.name);
        // Create a new collector with empty set
        let new_collector = Self::new(Some(format!("{}_clone", self.name)));

        // Get all currently tracked IDs from the original
        let tracked = self.tracked_ids.lock().unwrap();

        // Track each ID in the new collector (this will inc_ref for each)
        for fid in tracked.iter() {
            new_collector.track_function(fid.clone());
        }

        new_collector
    }
}

impl Drop for Collector {
    fn drop(&mut self) {
        // log::info!("Dropping collector: {}", self.name);
        // On drop, we untrack (and thus dec_ref) everything we were tracking
        let mut tracer = BAML_TRACER.lock().unwrap();
        let guard = self.tracked_ids.lock().unwrap();
        for fid in guard.iter() {
            tracer.dec_ref(fid);
        }
    }
}


// watch out when running all cargo tests in the project -- as they could mess with the global tracer state if you don't add the #[serial]. Perhaps we need #[tokio::test]
#[cfg(test)]
mod tests {
    use super::*;
    use baml_types::tracing::events::{
        ContentId, FunctionEnd, FunctionId, FunctionStart, HttpRequestId, LoggedLLMRequest, LoggedLLMResponse, TraceData, TraceEvent, TraceLevel
    };
    use core::time::Duration;
    use std::sync::Arc;
    use serial_test::serial;
    use tokio::runtime::Runtime;

    #[test]
    #[serial]
    fn test_reference_count_lifecycle() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            // Clear and check initial state
            {
                let mut tracer = BAML_TRACER.lock().unwrap();
                tracer.clear();
            }

            let f_id = FunctionId("func_abc".to_string());

            // Initially, no references
            {
                let tracer = BAML_TRACER.lock().unwrap();
                assert_eq!(tracer.ref_count_for(&f_id), 0);
            }

            // Create a collector to track the function ID
            let collector = Collector::new(Some("test_collector".to_string()));
            collector.track_function(f_id.clone());
            {
                let tracer = BAML_TRACER.lock().unwrap();
                assert_eq!(tracer.ref_count_for(&f_id), 1);
            }

            // Put an event
            let event = Arc::new(TraceEvent {
                span_id: f_id.clone(),
                event_id: ContentId("event_abc".to_string()),
                span_chain: vec![],
                timestamp: web_time::SystemTime::now(),
                callsite: "test_event".into(),
                verbosity: TraceLevel::Info,
                content: TraceData::LogMessage {
                    msg: "test_event1".into(),
                },
                tags: Default::default(),
            });
            {
                let mut tracer = BAML_TRACER.lock().unwrap();
                tracer.put(event.clone());
            }

            // Check events exist
            {
                let tracer = BAML_TRACER.lock().unwrap();
                let maybe_events = tracer.get_events(&f_id);
                assert!(maybe_events.is_some());
                assert_eq!(maybe_events.unwrap().len(), 1);
            }

            // Drop the collector => reference count goes to 0
            drop(collector);
            {
                let tracer = BAML_TRACER.lock().unwrap();
                assert_eq!(tracer.ref_count_for(&f_id), 0);
                assert!(tracer.get_events(&f_id).is_none());
            }
        });
    }

    #[test]
    #[serial]
    fn test_collector_clone_reference_counts() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let f_id = FunctionId("func_abc".to_string());
            // Clear global state
            {
                let mut tracer = BAML_TRACER.lock().unwrap();
                tracer.clear();
            }

            // Create original collector and track function
            let collector1 = Collector::new(Some("test_collector1".to_string()));
            collector1.track_function(f_id.clone());

            // Check initial reference count is 1
            {
                let tracer = BAML_TRACER.lock().unwrap();
                assert_eq!(tracer.ref_count_for(&f_id), 1);
            }

            // Clone collector and verify ref count increases
            let collector2 = collector1.clone();
            {
                let tracer = BAML_TRACER.lock().unwrap();
                assert_eq!(tracer.ref_count_for(&f_id), 2);
            }

            // Put an event
            let event = Arc::new(TraceEvent {
                span_id: f_id.clone(),
                event_id: ContentId("event_abc".to_string()),
                span_chain: vec![],
                timestamp: web_time::SystemTime::now(),
                callsite: "test_event".into(),
                verbosity: TraceLevel::Info,
                content: TraceData::LogMessage {
                    msg: "test_event1".into(),
                },
                tags: Default::default(),
            });
            {
                let mut tracer = BAML_TRACER.lock().unwrap();
                tracer.put(event.clone());
            }

            // Verify events exist
            {
                let tracer = BAML_TRACER.lock().unwrap();
                let maybe_events = tracer.get_events(&f_id);
                assert!(maybe_events.is_some());
                assert_eq!(maybe_events.unwrap().len(), 1);
            }

            // Drop first collector, verify ref count decreases but events remain
            drop(collector1);
            {
                let tracer = BAML_TRACER.lock().unwrap();
                assert_eq!(tracer.ref_count_for(&f_id), 1);
                assert!(tracer.get_events(&f_id).is_some());
            }

            // Drop second collector, verify everything is cleaned up
            drop(collector2);
            {
                let tracer = BAML_TRACER.lock().unwrap();
                assert_eq!(tracer.ref_count_for(&f_id), 0);
                assert!(tracer.get_events(&f_id).is_none());
            }
        });
    }

    #[test]
    #[serial]
    fn test_collector_and_function_log_clone_reference_counts() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let f_id = FunctionId("func_abc".to_string());
            // Clear global state
            {
                let mut tracer = BAML_TRACER.lock().unwrap();
                tracer.clear();
            }

            // Create original collector and track function
            let collector1 = Collector::new(Some("test_collector1".to_string()));
            collector1.track_function(f_id.clone());

            // Check initial reference count is 1
            {
                let tracer = BAML_TRACER.lock().unwrap();
                assert_eq!(tracer.ref_count_for(&f_id), 1);
            }

            // Clone collector and verify ref count increases
            let collector2 = collector1.clone();
            {
                let tracer = BAML_TRACER.lock().unwrap();
                assert_eq!(tracer.ref_count_for(&f_id), 2);
            }

            // Create a function log and clone it
            let func_log1 = collector1.function_log_by_id(&f_id).unwrap();
            let func_log2 = func_log1.clone();
            {
                let tracer = BAML_TRACER.lock().unwrap();
                assert_eq!(tracer.ref_count_for(&f_id), 4);
            }

            // Put an event
            let event = Arc::new(TraceEvent {
                span_id: f_id.clone(),
                event_id: ContentId("event_abc".to_string()),
                span_chain: vec![],
                timestamp: web_time::SystemTime::now(),
                callsite: "test_event".into(),
                verbosity: TraceLevel::Info,
                content: TraceData::LogMessage {
                    msg: "test_event1".into(),
                },
                tags: Default::default(),
            });
            {
                let mut tracer = BAML_TRACER.lock().unwrap();
                tracer.put(event.clone());
            }

            // Verify events exist
            {
                let tracer = BAML_TRACER.lock().unwrap();
                let maybe_events = tracer.get_events(&f_id);
                assert!(maybe_events.is_some());
                assert_eq!(maybe_events.unwrap().len(), 1);
            }

            // Drop first function log, verify ref count decreases but events remain
            drop(func_log1);
            {
                let tracer = BAML_TRACER.lock().unwrap();
                assert_eq!(tracer.ref_count_for(&f_id), 3);
                assert!(tracer.get_events(&f_id).is_some());
            }

            // Drop second function log
            drop(func_log2);
            {
                let tracer = BAML_TRACER.lock().unwrap();
                assert_eq!(tracer.ref_count_for(&f_id), 2);
                assert!(tracer.get_events(&f_id).is_some());
            }

            // Drop first collector, verify ref count decreases but events remain
            drop(collector1);
            {
                let tracer = BAML_TRACER.lock().unwrap();
                assert_eq!(tracer.ref_count_for(&f_id), 1);
                assert!(tracer.get_events(&f_id).is_some());
            }

            // Drop second collector, verify everything is cleaned up
            drop(collector2);
            {
                let tracer = BAML_TRACER.lock().unwrap();
                assert_eq!(tracer.ref_count_for(&f_id), 0);
                assert!(tracer.get_events(&f_id).is_none());
            }
        });
    }

    #[test]
    #[serial]
    fn test_function_log_basic() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let f_id = FunctionId("test_function_log_basic".to_string());

            // Clear global state
            {
                let mut tracer = BAML_TRACER.lock().unwrap();
                tracer.clear();
            }

            // Create a collector to track the function ID
            let collector = Collector::new(Some("test_collector".to_string()));
            collector.track_function(f_id.clone());

            // Create and insert start event
            let start_event = Arc::new(TraceEvent {
                span_id: f_id.clone(),
                event_id: ContentId("start_event".to_string()),
                span_chain: vec![],
                timestamp: web_time::SystemTime::now(),
                callsite: "unit_test_start".into(),
                verbosity: TraceLevel::Info,
                content: TraceData::FunctionStart(FunctionStart {
                    name: "test_function".into(),
                    args: vec![],
                    options: baml_types::tracing::events::BamlOptions {
                        type_builder: None,
                        client_registry: None,
                    },
                }),
                tags: Default::default(),
            });
            {
                let mut tracer = BAML_TRACER.lock().unwrap();
                tracer.put(start_event.clone());
            }

            // Create and insert end event
            let end_event = Arc::new(TraceEvent {
                span_id: f_id.clone(),
                event_id: ContentId("end_event".to_string()),
                span_chain: vec![],
                timestamp: web_time::SystemTime::now(),
                callsite: "unit_test_end".into(),
                verbosity: TraceLevel::Info,
                content: TraceData::FunctionEnd(FunctionEnd {
                    result: Ok(baml_types::BamlValue::Null),
                }),
                tags: Default::default(),
            });
            {
                let mut tracer = BAML_TRACER.lock().unwrap();
                tracer.put(end_event.clone());
            }

            let mut func_log = FunctionLog::new(f_id.clone());
            assert_eq!(func_log.id(), f_id);

            assert_eq!(func_log.function_name(), "test_function");
            let tpe = func_log.log_type();
            assert!(tpe == "call" || tpe == "stream");

            assert_eq!(func_log.usage().input_tokens, None);
            assert_eq!(func_log.usage().output_tokens, None);
            assert_eq!(func_log.calls().len(), 0);
            assert!(func_log.raw_llm_response().is_none());
            assert!(func_log.metadata().is_empty());

            // Clean up by dropping both the collector and function_log
            drop(collector);
            drop(func_log);

            // Verify everything is cleaned up
            {
                let tracer = BAML_TRACER.lock().unwrap();
                assert_eq!(tracer.ref_count_for(&f_id), 0);
                assert!(tracer.get_events(&f_id).is_none());
            }
        });
    }

    #[test]
    #[serial]
    fn test_function_log_with_metadata() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let f_id = FunctionId("test_function_log_with_metadata".to_string());

            // Clear global state
            {
                let mut tracer = BAML_TRACER.lock().unwrap();
                tracer.clear();
            }

            // Create a collector to track the function ID
            let collector = Collector::new(Some("test_collector".to_string()));
            collector.track_function(f_id.clone());

            let mut tags = serde_json::Map::new();
            tags.insert(
                "foo".to_string(),
                serde_json::Value::String("bar".to_string()),
            );
            tags.insert(
                "some_number".to_string(),
                serde_json::Value::Number(42.into()),
            );

            let start_event = Arc::new(TraceEvent {
                span_id: f_id.clone(),
                event_id: ContentId("start_event".to_string()),
                span_chain: vec![],
                timestamp: web_time::SystemTime::now(),
                callsite: "unit_test_start".into(),
                verbosity: TraceLevel::Info,
                content: TraceData::FunctionStart(FunctionStart {
                    name: "test_function_meta".into(),
                    args: vec![],
                    options: baml_types::tracing::events::BamlOptions {
                        type_builder: None,
                        client_registry: None,
                    },
                }),
                tags,
            });
            {
                let mut tracer = BAML_TRACER.lock().unwrap();
                tracer.put(start_event.clone());
            }

            let mut func_log = FunctionLog::new(f_id.clone());
            let meta = func_log.metadata();
            assert_eq!(meta.get("foo").unwrap(), "bar");
            assert_eq!(meta.get("some_number").unwrap(), 42);

            // Clean up by dropping both the collector and function_log
            drop(collector);
            drop(func_log);

            // Verify everything is cleaned up
            {
                let tracer = BAML_TRACER.lock().unwrap();
                assert_eq!(tracer.ref_count_for(&f_id), 0);
                assert!(tracer.get_events(&f_id).is_none());
            }
        });
    }

    #[test]
    #[serial]
    fn test_timing_calculations() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let f_id = FunctionId("test_timing".to_string());
            let collector = Collector::new(Some("test_collector".to_string()));
            collector.track_function(f_id.clone());
            let start_time = web_time::SystemTime::now();
            // Create start event
            let start_event = Arc::new(TraceEvent {
                span_id: f_id.clone(),
                event_id: ContentId("start_event".to_string()),
                span_chain: vec![],
                timestamp: start_time,
                callsite: "unit_test_start".into(),
                verbosity: TraceLevel::Info,
                content: TraceData::FunctionStart(FunctionStart {
                    name: "test_function_timing".into(),
                    args: vec![],
                    options: baml_types::tracing::events::BamlOptions {
                        type_builder: None,
                        client_registry: None,
                    },
                }),
                tags: Default::default(),
            });

            // Add start event
            {
                let mut tracer = BAML_TRACER.lock().unwrap();
                tracer.put(start_event.clone());
            }

            // Sleep to create measurable duration
            tokio::time::sleep(Duration::from_millis(100)).await;
            let end_time = web_time::SystemTime::now();

            // Create end event
            let end_event = Arc::new(TraceEvent {
                span_id: f_id.clone(),
                event_id: ContentId("end_event".to_string()),
                span_chain: vec![],
                timestamp: end_time,
                callsite: "unit_test_end".into(),
                verbosity: TraceLevel::Info,
                content: TraceData::FunctionEnd(FunctionEnd {
                    result: Ok(baml_types::BamlValue::Null),
                }),
                tags: Default::default(),
            });

            // Add end event
            {
                let mut tracer = BAML_TRACER.lock().unwrap();
                tracer.put(end_event.clone());
            }

            let mut func_log = FunctionLog::new(f_id.clone());
            let timing = func_log.timing();
            let duration = end_time.duration_since(start_time).unwrap();

            assert!(
                // leeway since test is a bit flaky -- maybe due to web_time crate
                (duration.as_millis() as i64 - func_log.timing().duration_ms.unwrap()).abs() <= 5
            );

            // Start time should be valid (non-zero)
            assert!(timing.start_time_utc_ms > 0);

            // Clean up
            drop(collector);
            drop(func_log);

            {
                let tracer = BAML_TRACER.lock().unwrap();
                assert_eq!(tracer.ref_count_for(&f_id), 0);
                assert!(tracer.get_events(&f_id).is_none());
            }
        });
    }
    /// Helper function to inject a sequence of events for testing
    async fn inject_test_events(
        f_id: &FunctionId,
        function_name: &str,
        llm_calls: Vec<(LoggedLLMRequest, LoggedLLMResponse)>,
    ) -> Collector {
        // Clear out the global tracer first
        {
            let mut tracer = BAML_TRACER.lock().unwrap();
            tracer.clear();
        }

        // Create a collector and track our function
        let collector = Collector::new(Some("test_collector".to_string()));
        collector.track_function(f_id.clone());

        // Insert a FunctionStart event
        let start_event = Arc::new(TraceEvent {
            span_id: f_id.clone(),
            event_id: ContentId("start_id".to_string()),
            span_chain: vec![],
            timestamp: web_time::SystemTime::now(),
            callsite: "test_start".into(),
            verbosity: TraceLevel::Info,
            content: TraceData::FunctionStart(FunctionStart {
                name: function_name.into(),
                args: vec![],
                options: baml_types::tracing::events::BamlOptions {
                    type_builder: None,
                    client_registry: None,
                },
            }),
            tags: Default::default(),
        });
        {
            let mut tracer = BAML_TRACER.lock().unwrap();
            tracer.put(start_event);
        }

        // Insert LLM requests and responses
        for (i, (req, resp)) in llm_calls.into_iter().enumerate() {
            let req = Arc::new(req);
            let resp = Arc::new(resp);

            // Put the request
            let event_req = Arc::new(TraceEvent {
                span_id: f_id.clone(),
                event_id: ContentId(format!("request_{}", i)),
                span_chain: vec![],
                timestamp: web_time::SystemTime::now(),
                callsite: format!("llm_request_{}", i).into(),
                verbosity: TraceLevel::Info,
                content: TraceData::LLMRequest(req),
                tags: Default::default(),
            });
            {
                let mut tracer = BAML_TRACER.lock().unwrap();
                tracer.put(event_req);
            }

            // Put the response
            let event_resp = Arc::new(TraceEvent {
                span_id: f_id.clone(),
                event_id: ContentId(format!("response_{}", i)),
                span_chain: vec![],
                timestamp: web_time::SystemTime::now(),
                callsite: format!("llm_response_{}", i).into(),
                verbosity: TraceLevel::Info,
                content: TraceData::LLMResponse(resp),
                tags: Default::default(),
            });
            {
                let mut tracer = BAML_TRACER.lock().unwrap();
                tracer.put(event_resp);
            }
        }

        // Insert the function end event
        let end_event = Arc::new(TraceEvent {
            span_id: f_id.clone(),
            event_id: ContentId("end_event".to_string()),
            span_chain: vec![],
            timestamp: web_time::SystemTime::now(),
            callsite: "test_end".into(),
            verbosity: TraceLevel::Info,
            content: TraceData::FunctionEnd(FunctionEnd {
                result: Ok(baml_types::BamlValue::Null),
            }),
            tags: Default::default(),
        });
        {
            let mut tracer = BAML_TRACER.lock().unwrap();
            tracer.put(end_event);
        }

        collector
    }

    #[test]
    #[serial]
    fn test_usage_accumulation_within_function_log_retries() {
        use baml_types::tracing::events::{
            ContentId, FunctionEnd, FunctionId, FunctionStart, LLMUsage, LoggedLLMRequest,
            LoggedLLMResponse, TraceData, TraceEvent, TraceLevel,
        };
        use std::time::Duration;

        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let f_id = FunctionId("test_usage_accumulation".to_string());

            let llm_calls = vec![
                (
                    LoggedLLMRequest {
                        request_id: HttpRequestId("req_1".to_string()),
                        client_name: "my_client".into(),
                        client_provider: "my_provider".into(),
                        params: serde_json::json!({ "temperature": 0.7 }),
                        prompt: serde_json::json!(["Hello world"]),
                    },
                    LoggedLLMResponse {
                        request_id: HttpRequestId("req_1".to_string()),
                        model: Some("test-model-v1".into()),
                        finish_reason: Some("stop".into()),
                        usage: Some(LLMUsage {
                            input_tokens: Some(12),
                            output_tokens: Some(8),
                            total_tokens: Some(20),
                        }),
                        raw_text_output: Some("Hello back".into()),
                        error_message: None,
                    },
                ),
                (
                    LoggedLLMRequest {
                        request_id: HttpRequestId("req_2".to_string()),
                        client_name: "my_client".into(),
                        client_provider: "my_provider".into(),
                        params: serde_json::json!({ "temperature": 0.9 }),
                        prompt: serde_json::json!(["Next message"]),
                    },
                    LoggedLLMResponse {
                        request_id: HttpRequestId("req_2".to_string()),
                        model: Some("test-model-v2".into()),
                        finish_reason: Some("length".into()),
                        usage: Some(LLMUsage {
                            input_tokens: Some(10),
                            output_tokens: Some(30),
                            total_tokens: Some(40),
                        }),
                        raw_text_output: Some("Super long response".into()),
                        error_message: None,
                    },
                ),
            ];

            let collector = inject_test_events(&f_id, "test_usage_func", llm_calls).await;

            // Now create a FunctionLog and check the usage
            let mut func_log = FunctionLog::new(f_id.clone());
            let usage = func_log.usage();
            assert_eq!(usage.input_tokens, Some(12 + 10));
            assert_eq!(usage.output_tokens, Some(8 + 30));

            // Verify the calls
            println!("calls: {:#?}", func_log.calls());
            let calls = func_log.calls();
            assert_eq!(calls.len(), 2);

            // Clean up
            drop(func_log);
            drop(collector);

            // Ensure everything is cleaned
            {
                let tracer = BAML_TRACER.lock().unwrap();
                assert_eq!(tracer.ref_count_for(&f_id), 0);
                assert!(tracer.get_events(&f_id).is_none());
            }
        });
    }

    // TODO: validate http request body and response body are serde objects
    // but need to inject these events in as well.
    //  let calls = func_log.calls();
    //  for call in calls {
    //      if let LLMCallKind::Basic(req) = call.clone() {
    //          match &req.request.as_ref().unwrap().body {
    //              serde_json::Value::Object(_) => {}
    //              _ => panic!("HTTP request body should be a serde object"),
    //          };
    //          match &req.response.as_ref().unwrap().body {
    //              serde_json::Value::Object(_) => {}
    //              _ => panic!("HTTP response body should be a serde object"),
    //          };
    //      }
    //      if let LLMCallKind::Stream(resp) = call.clone() {
    //          match &resp.request.as_ref().unwrap().body {
    //              serde_json::Value::Object(_) => {}
    //              _ => panic!("HTTP request body should be a serde object"),
    //          };
    //          match &resp.response.as_ref().unwrap().body {
    //              serde_json::Value::Object(_) => {}
    //              _ => panic!("HTTP response body should be a serde object"),
    //          };
    //      }
    //  }
}
