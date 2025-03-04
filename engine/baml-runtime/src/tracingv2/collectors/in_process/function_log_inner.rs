use std::{collections::HashMap, sync::Arc};

use baml_types::tracing::events::{
    FunctionEnd, FunctionId, FunctionStart, HTTPRequest, HTTPResponse, LoggedLLMRequest,
    LoggedLLMResponse, TraceData,
};
use once_cell::sync::Lazy;
use std::sync::Mutex;

use super::models::*;
use crate::tracingv2::storage::TraceStorage;

/// Global (singleton) trace storage.
static FUNCTION_LOG_INNER_CACHE: Lazy<Mutex<FunctionLogInnerCache>> =
    Lazy::new(|| Mutex::new(FunctionLogInnerCache::default()));

/// Our main storage struct. Holds:
/// 1) A map of FunctionId -> list of events (Vec<Arc<TraceEvent>>).
/// 2) A map of FunctionId -> reference count (how many "owners" are tracking it).
/// 3) A cache of FunctionId -> Arc<Mutex<FunctionLogInner>> to avoid rebuilding
///    the same FunctionLogInner multiple times.
#[derive(Default)]
struct FunctionLogInnerCache {
    /// Cache of built FunctionLogInner objects, so multiple calls to build_function_log
    /// for the same FunctionId share the same Arc. Because we may need to modify this
    /// while holding only an &TraceStorage, we wrap it in a Mutex for interior mutability.
    cache: Mutex<HashMap<FunctionId, Arc<Mutex<FunctionLogInner>>>>,
    storage: Arc<Mutex<TraceStorage>>,
}

pub fn drop_function_log_inner(id: &FunctionId) {
    FUNCTION_LOG_INNER_CACHE
        .lock()
        .unwrap()
        .drop_function_id(id);
}

/// Convert a `web_time::SystemTime` to i64 milliseconds since UNIX epoch.
fn system_time_to_utc_ms(st: &web_time::SystemTime) -> i64 {
    let dur = st
        .duration_since(web_time::SystemTime::UNIX_EPOCH)
        .unwrap_or_else(|_| std::time::Duration::from_secs(0));
    dur.as_millis() as i64
}

fn parse_llm_client_and_provider(req: Option<&Arc<LoggedLLMRequest>>) -> (String, String) {
    match req {
        Some(r) => (r.client_name.clone(), r.client_provider.clone()),
        None => ("".into(), "".into()),
    }
}

/// A helper structure for building an LLM call from multiple events sharing the same request_id.
#[derive(Default, Debug)]
struct CallAccumulator {
    pub llm_request: Option<Arc<LoggedLLMRequest>>,
    pub llm_response: Option<Arc<LoggedLLMResponse>>,
    pub http_request: Option<Arc<HTTPRequest>>,
    pub http_responses: Vec<Arc<HTTPResponse>>,
    pub usage: Option<Usage>,
    pub timestamp_first_seen: Option<i64>,
    pub timestamp_last_seen: Option<i64>,
}

impl FunctionLogInnerCache {
    pub fn drop_function_id(&self, id: &FunctionId) {
        self.cache.lock().unwrap().remove(id);
    }

    pub fn clear(&self) {
        self.cache.lock().unwrap().clear();
    }

    pub fn get_or_create(&self, function_id: &FunctionId) -> Option<Arc<Mutex<FunctionLogInner>>> {
        let cache = self.cache.lock().unwrap();
        if let Some(existing) = cache.get(function_id) {
            // Already built, just return a clone
            return Some(existing.clone());
        }

        let storage = self.storage.lock().unwrap();

        // If no cached version, fetch events to build from scratch.
        let events = storage.get_events(function_id)?;
        let guard = events; // A reference to the vector.

        let mut function_start: Option<&FunctionStart> = None;
        let mut function_end: Option<&FunctionEnd> = None;

        let mut function_start_time: Option<i64> = None;
        let mut function_end_time: Option<i64> = None;

        let mut usage = Usage::default();
        let mut combined_metadata = HashMap::new();
        let mut raw_llm_response: Option<String> = None;

        // We must group requests by request_id for LLM calls.
        let mut calls_map: HashMap<String, CallAccumulator> = HashMap::new();

        // TODO sort events by timestamp:
        for event in guard.iter() {
            let time_ms = system_time_to_utc_ms(&event.timestamp);

            // Merge event tags into metadata (if any).
            for (k, v) in event.tags.iter() {
                combined_metadata.insert(k.clone(), v.clone());
            }

            match &event.content {
                // Function lifecycle
                TraceData::FunctionStart(start) => {
                    function_start = Some(start);
                    function_start_time = Some(time_ms);
                }
                TraceData::FunctionEnd(end) => {
                    function_end = Some(end);
                    function_end_time = Some(time_ms);
                }

                // LLM adjacency
                TraceData::LLMRequest(llm_req) => {
                    // TODO: request_id must match
                    let rid = llm_req.request_id.0.clone();
                    let entry = calls_map.entry(rid).or_default();
                    entry.llm_request = Some(llm_req.clone());
                    entry.timestamp_first_seen = Some(time_ms);
                }
                TraceData::LLMResponse(llm_res) => {
                    let rid = llm_res.request_id.0.clone();
                    let entry = calls_map.entry(rid).or_default();
                    entry.llm_response = Some(llm_res.clone());
                    entry.timestamp_last_seen = Some(time_ms);

                    // Attempt usage from here:
                    if let Some(usage_info) = &llm_res.usage {
                        entry.usage = Some(Usage {
                            input_tokens: usage_info.input_tokens.map(|t| t as i64),
                            output_tokens: usage_info.output_tokens.map(|t| t as i64),
                        });
                    }

                    // TODO: zero copy?
                    raw_llm_response = llm_res.raw_text_output.clone();
                }

                // Raw requests and responses
                TraceData::RawLLMRequest(http_req) => {
                    let rid = http_req.request_id.0.clone();
                    let entry = calls_map.entry(rid).or_default();
                    entry.http_request = Some(http_req.clone());
                    entry.timestamp_first_seen = Some(time_ms);
                }
                TraceData::RawLLMResponse(http_res) => {
                    let rid = http_res.request_id.0.clone();
                    let entry = calls_map.entry(rid).or_default();
                    entry.http_responses.push(http_res.clone());
                    entry.timestamp_last_seen = Some(time_ms);
                }

                // Possibly streaming or partial events
                TraceData::Parsed(_) => {}
                TraceData::LogMessage { .. } => {}
            }
        }

        // If we never found a FunctionStart, skip building a log.
        let start_ev = function_start.as_ref()?;
        let fname = start_ev.function_display_name.clone();

        let start_ms = function_start_time.unwrap_or(0);
        let end_ms = function_end_time;
        let duration = end_ms.map(|end| end.saturating_sub(start_ms));

        // Build each LLMCall or LLMStreamCall
        let mut calls = Vec::new();
        for (_rid, call_acc) in calls_map {
            let (client, provider) = parse_llm_client_and_provider(call_acc.llm_request.as_ref());
            let start_t = call_acc.timestamp_first_seen.unwrap_or(start_ms);
            let end_t = call_acc.timestamp_last_seen.unwrap_or(start_t);
            let partial_duration = end_t.saturating_sub(start_t);

            let is_stream = call_acc.http_responses.len() > 1;

            let local_usage = call_acc.usage.unwrap_or_default();
            usage.input_tokens = match (usage.input_tokens, local_usage.input_tokens) {
                (Some(i), Some(j)) => Some(i + j),
                (None, None) => None,
                (Some(i), None) => Some(i),
                (None, Some(j)) => Some(j),
            };
            usage.output_tokens = match (usage.output_tokens, local_usage.output_tokens) {
                (Some(i), Some(j)) => Some(i + j),
                (None, None) => None,
                (Some(i), None) => Some(i),
                (None, Some(j)) => Some(j),
            };

            if !is_stream {
                // Basic LLMCall
                calls.push(LLMCallKind::Basic(LLMCall {
                    client_name: client,
                    provider,
                    timing: Timing {
                        start_time_utc_ms: start_t,
                        duration_ms: Some(partial_duration),
                        time_to_first_parsed_ms: None,
                    },
                    request: call_acc.http_request.clone(),
                    response: call_acc.http_responses.first().cloned(),
                    usage: Some(local_usage),
                    selected: call_acc.llm_response.is_some(),
                }));
            } else {
                // Streaming call
                calls.push(LLMCallKind::Stream(LLMStreamCall {
                    client_name: client,
                    provider,
                    timing: StreamTiming {
                        start_time_utc_ms: start_t,
                        duration_ms: Some(partial_duration),
                        time_to_first_parsed_ms: None,
                        time_to_first_token_ms: None,
                    },
                    request: call_acc.http_request.clone(),
                    response: call_acc.http_responses.first().cloned(),
                    usage: Some(local_usage),
                    selected: call_acc.llm_response.is_some(),
                    chunks: Vec::new(),
                }));
            }
        }

        // If there's at least one streaming call, we mark the FunctionLogInner's type as "stream".
        let is_stream_fn = calls.iter().any(|c| matches!(c, LLMCallKind::Stream(_)));

        let function_log_inner = FunctionLogInner {
            id: function_id.0.clone(),
            function_name: fname,
            r#type: if is_stream_fn {
                "stream".into()
            } else {
                "call".into()
            },
            timing: Timing {
                start_time_utc_ms: start_ms,
                duration_ms: duration,
                time_to_first_parsed_ms: None,
            },
            usage,
            calls,
            raw_llm_response,
            metadata: combined_metadata,
        };

        let new_arc = Arc::new(Mutex::new(function_log_inner));
        {
            // Insert into the cache
            let mut lock = self.cache.lock().unwrap();
            lock.insert(function_id.clone(), new_arc.clone());
        }

        Some(new_arc)
    }
}

///
/// Represents the "inner" data for a single function call
/// (the real set of usage/calls/timing, etc.).
///
#[derive(Debug, Clone)]
pub struct FunctionLogInner {
    pub id: String,
    pub function_name: String,
    pub r#type: String,
    pub timing: Timing,
    pub usage: Usage,
    pub calls: Vec<LLMCallKind>,
    pub raw_llm_response: Option<String>,
    pub metadata: HashMap<String, serde_json::Value>,
}

impl FunctionLogInner {
    /// Return the "selected" call, or None if none was selected.
    pub fn selected_call(&self) -> Option<&LLMCallKind> {
        self.calls.iter().find(|call| call.selected())
    }
    ///
    /// Build a single [FunctionLogInner] from all events corresponding to `function_id`, or
    /// return it from the cache. If there is no data for the given function ID
    /// (or FunctionStart is missing), returns None.
    ///
    pub fn get_or_create(id: &FunctionId) -> Option<Arc<Mutex<FunctionLogInner>>> {
        FUNCTION_LOG_INNER_CACHE.lock().unwrap().get_or_create(id)
    }
}
