use indexmap::{IndexMap, IndexSet};
use once_cell::sync::Lazy;
use std::hash::Hash;
use std::sync::Mutex;
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};
use uuid::Uuid;

use baml_types::tracing::events::{
    FunctionEnd, FunctionId, FunctionStart, HTTPRequest, HTTPResponse, LoggedLLMRequest,
    LoggedLLMResponse, TraceData, TraceEvent,
};

use crate::tracingv2::publisher::publisher::PublisherMessage;

use super::super::publisher::PUBLISHING_CHANNEL;

/// Global (singleton) trace storage.
pub static BAML_TRACER: Lazy<Mutex<TraceStorage>> =
    Lazy::new(|| Mutex::new(TraceStorage::default()));

#[derive(Debug, Clone)]
pub struct FunctionLog {
    id: FunctionId,
    inner: Option<FunctionLogInner>, // Property to store the lazily evaluated function log
}

impl Hash for FunctionLog {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

impl Eq for FunctionLog {}

impl PartialEq for FunctionLog {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl FunctionLog {
    pub fn new(id: FunctionId) -> Self {
        BAML_TRACER.lock().unwrap().inc_function_id(&id);
        Self { id, inner: None }
    }

    // Private helper to get inner reference, building if needed
    fn get_inner(&mut self) -> &FunctionLogInner {
        if self.inner.is_none() {
            self.inner = Some(
                build_function_log(&BAML_TRACER.lock().unwrap(), &self.id)
                    .expect("Function log expected to be present"),
            );
        }
        self.inner.as_ref().unwrap()
    }

    pub fn id(&self) -> FunctionId {
        self.id.clone()
    }

    pub fn function_name(&mut self) -> &str {
        &self.get_inner().function_name
    }

    pub fn r#type(&mut self) -> &str {
        &self.get_inner().r#type
    }

    pub fn timing(&mut self) -> &Timing {
        &self.get_inner().timing
    }

    pub fn usage(&mut self) -> &Usage {
        &self.get_inner().usage
    }

    pub fn calls(&mut self) -> &[LLMCallKind] {
        &self.get_inner().calls
    }

    pub fn raw_llm_response(&mut self) -> Option<&str> {
        self.get_inner().raw_llm_response.as_deref()
    }

    pub fn metadata(&mut self) -> &HashMap<String, serde_json::Value> {
        &self.get_inner().metadata
    }
}

///
/// Represents a single function call's log. This implements the "FunctionLog" interface
/// from the prompt's "API Reference."
///
#[derive(Debug, Clone)]
pub struct FunctionLogInner {
    // "id": The id of the request.
    pub id: String,

    // "function_name": The name of the function.
    pub function_name: String,

    // "type": "call" | "stream" (the manner in which the function was called).
    pub r#type: String,

    pub timing: Timing,

    pub usage: Usage,
    // A list of LLM calls (either LLMCall or LLMStreamCall).
    pub calls: Vec<LLMCallKind>,
    // "raw_llm_response": The final best guess textual output for this function, if any.
    pub raw_llm_response: Option<String>,
    // "metadata": Any user-provided metadata, from `TraceEvent.tags` or other sources.
    pub metadata: HashMap<String, serde_json::Value>,
}

impl FunctionLogInner {
    /// Return the "selected" call, or None if none was selected.
    /// In a real system, you might rely on some additional marker to identify
    /// which call was "selected" for parsing. Currently checks if any call is flagged.
    pub fn selected_call(&self) -> Option<&LLMCallKind> {
        self.calls.iter().find(|call| call.selected())
    }
}

///
/// A minimal "Usage" struct holding input and output tokens.
///
#[derive(Debug, Default, Clone, Hash, Eq, PartialEq)]
pub struct Usage {
    pub input_tokens: i64,
    pub output_tokens: i64,
}

///
/// Basic timing data for a function or call record.
///
#[derive(Debug, Default, Clone, Hash, Eq, PartialEq)]
pub struct Timing {
    pub start_time_utc_ms: i64,
    pub duration_ms: i64,
    pub time_to_first_parsed_ms: i64,
}

///
/// Specialized timing for streaming calls.
///
#[derive(Debug, Default, Clone, Hash, Eq, PartialEq)]
pub struct StreamTiming {
    pub start_time_utc_ms: i64,
    pub duration_ms: i64,
    pub time_to_first_parsed_ms: i64,
    pub time_to_first_token_ms: i64,
}

#[derive(Debug, Clone)]
pub enum LLMCallKind {
    Basic(LLMCall),
    Stream(LLMStreamCall),
}

impl LLMCallKind {
    /// Returns whether this call is selected.
    pub fn selected(&self) -> bool {
        match self {
            LLMCallKind::Basic(c) => c.selected,
            LLMCallKind::Stream(c) => c.selected,
        }
    }
}

///
/// Represents a single "LLM call" (non-streaming).
///
#[derive(Debug, Default, Clone)]
pub struct LLMCall {
    pub client_name: String,
    pub provider: String,
    pub timing: Timing,
    pub request: Option<Arc<HTTPRequest>>,
    pub response: Option<Arc<HTTPResponse>>,
    pub usage: Option<Usage>,
    pub selected: bool,
}

///
/// Represents a single streaming LLM call.
///
#[derive(Debug, Default, Clone)]
pub struct LLMStreamCall {
    pub client_name: String,
    pub provider: String,
    pub timing: StreamTiming,
    pub request: Option<Arc<HTTPRequest>>,
    pub response: Option<Arc<HTTPResponse>>,
    pub usage: Option<Usage>,
    pub selected: bool,

    /// Each chunk of data from the LLM (e.g. streaming tokens).
    pub chunks: Vec<serde_json::Value>,
}

impl Drop for FunctionLog {
    fn drop(&mut self) {
        log::info!("Dropping function log: {}", self.id().0);
        BAML_TRACER.lock().unwrap().dec_function_id(&self.id());
    }
}

impl Clone for Collector {
    fn clone(&self) -> Self {
        // Increment the function reference count for the function logs
        let function_logs_clone = self.function_logs.clone();
        Self {
            id: self.id.clone(),
            function_logs: function_logs_clone,
        }
    }
}

pub struct Collector {
    id: String,
    function_logs: Arc<Mutex<IndexSet<Arc<FunctionLog>>>>,
}

impl Collector {
    pub fn new(id: String) -> Self {
        Self {
            id,
            function_logs: Arc::new(Mutex::new(IndexSet::new())),
        }
    }

    pub fn id(&self) -> String {
        self.id.clone()
    }

    pub fn events(&self) -> Vec<Arc<TraceEvent>> {
        let mut all_events = Vec::new();

        // Acquire the global tracer lock in a blocking manner.
        let tracer = BAML_TRACER.lock().unwrap();

        // For each subscribed span, grab its events under a blocking lock.
        let function_logs_guard = self.function_logs.lock().unwrap();
        for span_id in function_logs_guard.iter() {
            if let Some(events) = tracer.get_events(&span_id.id) {
                let guard = events.lock().unwrap();
                all_events.extend(guard.clone());
            }
        }

        all_events
    }

    pub fn function_logs(&self) -> Vec<Arc<FunctionLog>> {
        let function_logs_guard = self.function_logs.lock().unwrap();
        function_logs_guard
            .iter()
            .map(|id| Arc::new(FunctionLog::new(id.id.clone())))
            .collect()
    }

    pub fn track_function(&self, span_id: FunctionId) {
        let function_log = FunctionLog::new(span_id);
        let mut function_logs_guard = self.function_logs.lock().unwrap();
        function_logs_guard.insert(Arc::new(function_log));
    }
}

impl Drop for Collector {
    fn drop(&mut self) {
        log::info!("Dropping collector: {}", self.id);
        let function_logs_guard = self.function_logs.lock().unwrap();
        for function_log in function_logs_guard.iter() {
            BAML_TRACER
                .lock()
                .unwrap()
                .dec_function_id(&function_log.id);
        }
    }
}

/// Build a single [FunctionLog] from all events corresponding to `function_id`.
///
/// If there is no data for the given function ID (or function_start is missing),
/// returns None.
fn build_function_log(
    storage: &TraceStorage,
    function_id: &FunctionId,
) -> Option<FunctionLogInner> {
    let events = storage.get_events(function_id)?;
    let guard = events.lock().unwrap();

    let mut function_start: Option<&FunctionStart> = None;
    let mut function_end: Option<&FunctionEnd> = None;

    let mut function_start_time: Option<i64> = None;
    let mut function_end_time: Option<i64> = None;

    // We will parse usage from LLM responses
    let mut usage = Usage::default();

    let mut combined_metadata = HashMap::new();

    // We must group requests by request_id for LLM calls.
    // request_id -> (HTTPRequest optional, Vec<HTTPResponse>, LoggedLLMRequest?, LoggedLLMResponse?)
    let mut calls_map: HashMap<String, CallAccumulator> = HashMap::new();

    // We'll parse each event in chronological order (they may not be strictly sorted by time, though).
    // In a real system, you'd want to sort by event timestamp, but here we'll just iterate in stored order.
    for event in guard.iter() {
        // We can parse the timestamp as ms since epoch from the event.
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

                // If there's a usage dimension from the end result (not typical),
                // you could parse it here from end.result.
            }

            // LLM adjacency
            TraceData::LLMRequest(llm_req) => {
                // No request_id in LLMRequest itself. Could store a "virtual" request_id from the event_id if desired.
                let rid = format!("llm_req_{}", event.event_id.0);
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
                        input_tokens: usage_info.input_tokens.unwrap_or(0) as i64,
                        output_tokens: usage_info.output_tokens.unwrap_or(0) as i64,
                    });
                }
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
            // "Parsed" is ignored for this structure (the user can parse if desired).
            TraceData::Parsed(_) => {}
            TraceData::LogMessage { .. } => {} // We do not store "FunctionEnd(...) again" or other fields here.
        }
    }

    // If we never found a FunctionStart, we skip building a log.
    let start_ev = function_start.as_ref()?;
    let fname = start_ev.name.clone();

    // Build the top-level FunctionLog
    let start_ms = function_start_time.unwrap_or(0);
    let end_ms = function_end_time.unwrap_or(start_ms);
    let duration = end_ms.saturating_sub(start_ms);

    // build each LLMCall or LLMStreamCall from calls_map.
    let mut calls = Vec::new();
    for (rid, call_acc) in calls_map {
        let (client, provider) = parse_llm_client_and_provider(call_acc.llm_request.as_ref());
        let start_t = call_acc.timestamp_first_seen.unwrap_or(start_ms);
        let end_t = call_acc.timestamp_last_seen.unwrap_or(start_t);
        let partial_duration = end_t.saturating_sub(start_t);

        // If we suspect streaming, we might look at the presence of multiple HTTP responses or chunk data.
        // For simplicity, let's call it streaming if there's more than one response or if it's a known streaming route.
        let is_stream = call_acc.http_responses.len() > 1;

        // Merge local usage from the call:
        let local_usage = call_acc.usage.unwrap_or_else(|| Usage {
            input_tokens: 0,
            output_tokens: 0,
        });
        usage.input_tokens += local_usage.input_tokens;
        usage.output_tokens += local_usage.output_tokens;

        // Build the request/response as JSON
        // let request_json = if let Some(r) = &call_acc.http_request {
        //     serde_json::json!({
        //         "url": r.url,
        //         "method": r.method,
        //         "headers": r.headers,
        //         "body": r.body,
        //     })
        // } else if let Some(llm_req) = &call_acc.llm_request {
        //     // fallback to storing the LLMRequest as is
        //     serde_json::to_value(llm_req).unwrap_or_else(|_| serde_json::json!({}))
        // } else {
        //     serde_json::json!({})
        // };

        // let response_json = if call_acc.http_responses.is_empty() {
        //     // Maybe store the LLMResponse if no raw response was found
        //     if let Some(resp) = &call_acc.llm_response {
        //         serde_json::to_value(resp).ok()
        //     } else {
        //         None
        //     }
        // } else {
        //     // for multiple responses, store them in an array
        //     Some(serde_json::json!(call_acc
        //         .http_responses
        //         .iter()
        //         .map(|r| {
        //             serde_json::json!({
        //                 "status": r.status,
        //                 "headers": r.headers,
        //                 "body": r.body,
        //             })
        //         })
        //         .collect::<Vec<_>>()))
        // };

        if !is_stream {
            // Basic LLMCall
            calls.push(LLMCallKind::Basic(LLMCall {
                client_name: client,
                provider,
                timing: Timing {
                    start_time_utc_ms: start_t,
                    duration_ms: partial_duration,
                    time_to_first_parsed_ms: 0, // not computed
                },
                request: call_acc.http_request.clone(),
                response: call_acc.http_responses.first().cloned(),
                usage: Some(local_usage),
                selected: false, // you could add logic to mark which one is selected
            }));
        } else {
            // Streaming call
            calls.push(LLMCallKind::Stream(LLMStreamCall {
                client_name: client,
                provider,
                timing: StreamTiming {
                    start_time_utc_ms: start_t,
                    duration_ms: partial_duration,
                    time_to_first_parsed_ms: 0, // not computed
                    time_to_first_token_ms: 0,  // not computed
                },
                request: call_acc.http_request.clone(),
                response: call_acc.http_responses.first().cloned(),
                usage: Some(local_usage),
                selected: false,
                chunks: Vec::new(), // user could store partial chunk messages
            }));
        }
    }

    // Possibly guess "call" vs "stream". We'll mark "stream" if there's at least one LLMStreamCall.
    let is_stream_fn = calls.iter().any(|c| matches!(c, LLMCallKind::Stream(_)));

    let function_log = FunctionLogInner {
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
            time_to_first_parsed_ms: 0, // not computed
        },
        usage,
        calls,
        raw_llm_response: None, // could store a best guess from LLMResponse
        metadata: combined_metadata,
    };

    Some(function_log)
}

// ─────────────────────────────────────────────────────────────────────────────
//  Utility / Parsing / Accumulators
// ─────────────────────────────────────────────────────────────────────────────

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

fn parse_llm_client_and_provider(req: Option<&Arc<LoggedLLMRequest>>) -> (String, String) {
    match req {
        Some(r) => match &r.client {
            baml_types::tracing::events::LLMClient::Ref(name) => (name.clone(), "".into()),
            baml_types::tracing::events::LLMClient::ShortHand(provider, name) => {
                (name.clone(), provider.clone())
            }
        },
        None => ("".into(), "".into()),
    }
}

/// Convert a `web_time::SystemTime` to i64 milliseconds since UNIX epoch.
fn system_time_to_utc_ms(st: &web_time::SystemTime) -> i64 {
    let dur = st
        .duration_since(web_time::SystemTime::UNIX_EPOCH)
        .unwrap_or_else(|_| std::time::Duration::from_secs(0));
    dur.as_millis() as i64
}

/// Our main storage struct. Holds:
/// 1) A map of FunctionId -> list of events (wrapped in an Arc<Mutex<Vec<Arc<TraceEvent>>>>).
/// 2) A map of FunctionId -> reference count (how many "subscribers" are listening).
#[derive(Default)]
pub struct TraceStorage {
    /// For each function (span), we keep an Arc<Mutex<Vec<Arc<TraceEvent>>>>>.
    /// This lets us append events asynchronously from multiple tasks.
    span_map: HashMap<FunctionId, Arc<Mutex<Vec<Arc<TraceEvent>>>>>,
    /// For each function (span), how many "subscribers" currently have it referenced?
    ref_counts: HashMap<FunctionId, Arc<AtomicUsize>>,
}

impl TraceStorage {
    pub fn new() -> Self {
        Self {
            span_map: HashMap::new(),
            ref_counts: HashMap::new(),
        }
    }

    /// Retrieve events for a particular function (span).
    /// Returns None if the function isn't being tracked (or was dropped).
    pub fn get_events(&self, function_id: &FunctionId) -> Option<Arc<Mutex<Vec<Arc<TraceEvent>>>>> {
        self.span_map.get(function_id).cloned()
    }

    /// Increments the reference count for the given function ID.
    /// Also ensures the underlying events map entry exists.
    pub fn inc_function_id(&mut self, function_id: &FunctionId) {
        // Ensure we have an event vector for that function ID
        self.span_map
            .entry(function_id.clone())
            .or_insert_with(|| Arc::new(Mutex::new(Vec::new())));

        // Ensure we have a reference count, then increment it.
        let arc_counter = self
            .ref_counts
            .entry(function_id.clone())
            .or_insert_with(|| Arc::new(AtomicUsize::new(0)));
        arc_counter.fetch_add(1, Ordering::SeqCst);
    }

    /// Decrements the reference count. If it hits zero, we remove the span's events from memory.
    pub fn dec_function_id(&mut self, function_id: &FunctionId) {
        if let Some(arc_counter) = self.ref_counts.get(function_id) {
            let new_val = arc_counter.fetch_sub(1, Ordering::SeqCst);
            if new_val == 1 {
                // That means we just went from 1 to 0 – remove from memory.
                self.ref_counts.remove(function_id);
                self.span_map.remove(function_id);
            }
        }
        let event_count = self
            .span_map
            .values()
            .map(|arc| arc.lock().unwrap().len())
            .sum::<usize>();
        let function_ids_left = self.span_map.len();
        log::info!("Number of function IDs left: {}", function_ids_left);
        log::info!("Number of events left: {}", event_count);
    }

    /// Insert / record a new event for the given function (span).
    /// If the reference count is 0 (no subscribers), we skip storing it.
    ///
    /// Regardless, we also publish the event so that external systems can receive it.
    pub fn put(&mut self, event: Arc<TraceEvent>) {
        let span_id = event.span_id.clone();

        // Check how many references are present for this span
        let count = self
            .ref_counts
            .get(&span_id)
            .map(|arc| arc.load(Ordering::SeqCst))
            .unwrap_or(0);

        // Always attempt to publish
        if let Err(e) = PUBLISHING_CHANNEL.send(PublisherMessage::Trace(event.clone())) {
            log::error!("Failed to send event to publisher: {:?}", e);
        }

        // If nobody's referencing this span, no need to store it locally
        if count == 0 {
            return;
        }

        // If count > 0, store the event locally in the span_map.
        #[cfg(not(target_arch = "wasm32"))]
        {
            let event_clone = event.clone();
            let span_map = self.span_map.get(&span_id).cloned();
            tokio::spawn(async move {
                if let Some(ev) = span_map {
                    let mut guard = ev.lock().unwrap();
                    guard.push(event_clone);
                }
            });
        }

        #[cfg(target_arch = "wasm32")]
        {
            let event_clone = event.clone();
            let span_map = self.span_map.get(&span_id).cloned();
            wasm_bindgen_futures::spawn_local(async move {
                if let Some(ev) = span_map {
                    let mut guard = ev.lock().unwrap();
                    guard.push(event_clone);
                }
            });
        }
    }

    /// For testing or debugging – return the entire map of events.
    /// (Use with caution in a real system, as it might be huge.)
    pub fn events(&self) -> HashMap<FunctionId, Arc<Mutex<Vec<Arc<TraceEvent>>>>> {
        self.span_map.clone()
    }

    /// For testing or debugging – return how many references a given function currently has.
    pub fn ref_count_for(&self, function_id: &FunctionId) -> usize {
        self.ref_counts
            .get(function_id)
            .map(|arc| arc.load(Ordering::SeqCst))
            .unwrap_or(0)
    }
}

// ----------------- TESTS -----------------
#[cfg(test)]
mod tests {
    use super::*;
    use baml_types::tracing::events::{
        ContentId, FunctionEnd, FunctionId, FunctionStart, TraceData, TraceEvent, TraceLevel,
    };
    use core::time::Duration;
    use tokio::runtime::Runtime;

    #[test]
    fn test_reference_count_lifecycle() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let mut storage = TraceStorage::new();
            let f_id = FunctionId("func_abc".to_string());

            // Initially, no references
            assert_eq!(storage.ref_count_for(&f_id), 0);

            // Increment reference count
            storage.inc_function_id(&f_id);
            assert_eq!(storage.ref_count_for(&f_id), 1);

            // Put an event – it should be stored in memory (since ref > 0)
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
            storage.put(event.clone());

            // Wait for the async insert to complete
            tokio::time::sleep(Duration::from_millis(10)).await;

            let maybe_events = storage.get_events(&f_id);
            assert!(maybe_events.is_some());

            // Store Arc<Mutex<Vec<Arc<TraceEvent>>>>> in a variable before locking
            let arc_mutex = maybe_events.unwrap();
            let event_list = arc_mutex.lock().unwrap();

            assert_eq!(event_list.len(), 1);
            match &event_list[0].content {
                TraceData::LogMessage { msg } => assert_eq!(msg, "test_event1"),
                _ => panic!("Expected a LogMessage content."),
            }

            // Decrement reference count => it should remove the events if count hits 0
            storage.dec_function_id(&f_id);
            assert_eq!(storage.ref_count_for(&f_id), 0);

            // That should also remove the events, so get_events should return None
            assert!(storage.get_events(&f_id).is_none());
        });
    }

    //
    // Tests for FunctionLog
    //

    #[test]
    fn test_function_log_basic() {
        // This test ensures that a FunctionLog correctly loads FunctionStart/End
        // and associated metadata from the global BAML_TRACER.

        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let f_id = FunctionId("test_function_log_basic".to_string());

            // Ensure a clean slate for our global tracer reference counting.
            {
                let mut tracer = BAML_TRACER.lock().unwrap();
                tracer.inc_function_id(&f_id);
            }

            // Create a FunctionStart event
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

            // Create a FunctionEnd event
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

            // Insert them into the global tracer
            {
                let mut tracer = BAML_TRACER.lock().unwrap();
                tracer.put(start_event.clone());
                tracer.put(end_event.clone());
            }

            // Wait a bit for async insertion
            tokio::time::sleep(Duration::from_millis(50)).await;

            // Create a new FunctionLog, which should load the data from the tracer
            let mut func_log = FunctionLog::new(f_id.clone());
            assert_eq!(func_log.id(), f_id);

            // These fields are pulled from the events we added
            assert_eq!(func_log.function_name(), "test_function");
            assert!(func_log.r#type() == "call" || func_log.r#type() == "stream");

            // By default, usage is zero unless an LLM was triggered
            assert_eq!(func_log.usage().input_tokens, 0);
            assert_eq!(func_log.usage().output_tokens, 0);

            // Double-check that there are no LLM calls
            assert_eq!(func_log.calls().len(), 0);

            // We didn't provide a raw_llm_response, so it should be None
            assert!(func_log.raw_llm_response().is_none());

            // We have no additional metadata in tags, so it should be empty
            assert!(func_log.metadata().is_empty());

            // Cleanup reference count
            {
                let mut tracer = BAML_TRACER.lock().unwrap();
                tracer.dec_function_id(&f_id);
            }
        });
    }

    #[test]
    fn test_function_log_with_metadata() {
        // Tests that metadata from the Event tags is merged properly.

        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let f_id = FunctionId("test_function_log_with_metadata".to_string());

            {
                let mut tracer = BAML_TRACER.lock().unwrap();
                tracer.inc_function_id(&f_id);
            }

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

            // Wait a bit for async insertion
            tokio::time::sleep(Duration::from_millis(50)).await;

            // Create a new FunctionLog and check that metadata is there
            let mut func_log = FunctionLog::new(f_id.clone());
            let meta = func_log.metadata();
            assert_eq!(meta.get("foo").unwrap(), "bar");
            assert_eq!(meta.get("some_number").unwrap(), 42);

            // Cleanup reference count
            {
                let mut tracer = BAML_TRACER.lock().unwrap();
                tracer.dec_function_id(&f_id);
            }
        });
    }
}
