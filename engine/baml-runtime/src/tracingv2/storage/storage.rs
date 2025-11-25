//! A full implementation of a manually reference-counted trace storage system,
//! including a global tracer, FunctionLog, Collector, and all related data types.
//!
//! This version ensures we don't allocate multiple copies of the same FunctionLogInner
//! for a single FunctionCallId, even if multiple Collectors or FunctionLogs want it.
//! It uses manual reference counting (`inc_ref` / `dec_ref`) to free memory for
//! a FunctionCallId as soon as there are no more "owners."
use std::{
    collections::{HashMap, HashSet},
    fmt,
    hash::Hash,
    ops::DerefMut,
    sync::{Arc, Mutex},
};

use baml_ids::{FunctionCallId, HttpRequestId};
use baml_types::{
    tracing::events::{
        FunctionEnd, FunctionStart, HTTPRequest, HTTPResponse, HTTPResponseStream,
        LoggedLLMRequest, LoggedLLMResponse, SSEEvent, TraceData, TraceEvent,
    },
    HasType,
};
use indexmap::{IndexMap, IndexSet};
use once_cell::sync::Lazy;
use serde::Serialize;
use uuid::Uuid;

use super::interface::TraceEventWithMeta;

/// Global (singleton) trace storage.
pub static BAML_TRACER: Lazy<Mutex<TraceStorage>> =
    Lazy::new(|| Mutex::new(TraceStorage::default()));

/// Our main storage struct. Holds:
/// 1) A map of FunctionCallId -> list of events (Vec<Arc<TraceEvent>>).
/// 2) A map of FunctionCallId -> reference count (how many "owners" are tracking it).
/// 3) A cache of FunctionCallId -> Arc<Mutex<FunctionLogInner>> to avoid rebuilding
///    the same FunctionLogInner multiple times.
#[derive(Default)]
pub struct TraceStorage {
    /// For each function (call), we keep a vector of TraceEvents.
    /// This data is only kept while ref_count > 0.
    call_map: HashMap<FunctionCallId, Vec<Arc<TraceEventWithMeta>>>,
    /// Manual reference count for each function ID. If it hits 0, we remove that ID's data.
    ref_counts: HashMap<FunctionCallId, usize>,

    /// Cache of built FunctionLogInner objects, so multiple calls to build_function_log
    /// for the same FunctionCallId share the same Arc. Because we may need to modify this
    /// while holding only an &TraceStorage, we wrap it in a Mutex for interior mutability.
    function_inners: Mutex<HashMap<FunctionCallId, Arc<Mutex<FunctionLogInner>>>>,
}

impl fmt::Debug for TraceStorage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "TraceStorage {{ ref_counts: {:#?}, function_call_count: {:#?} }}",
            self.ref_counts,
            self.function_call_count()
        )
    }
}

impl TraceStorage {
    /// Increase the reference count for the given FunctionCallId.
    /// If there's no entry yet, create one (with an empty Vec of events).
    pub fn inc_ref(&mut self, function_id: &FunctionCallId) {
        let count = self.ref_counts.entry(function_id.clone()).or_insert(0);
        *count += 1;

        // Ensure call_map has an entry for the ID; create if not present.
        self.call_map.entry(function_id.clone()).or_default();
    }

    /// Decrease the reference count for the given FunctionCallId,
    /// and if it hits zero, remove from memory (both events and cached FunctionLogInner).
    pub fn dec_ref(&mut self, function_id: &FunctionCallId) {
        match self.ref_counts.get_mut(function_id) {
            Some(rc) => {
                if *rc == 0 {
                    panic!("Attempted to decrement ref below 0 for FunctionID {function_id:?}");
                }
                *rc -= 1;
                // If refcount hits 0, remove from both maps
                if *rc == 0 {
                    self.ref_counts.remove(function_id);
                    self.call_map.remove(function_id);

                    // Remove the cached FunctionLogInner
                    let mut lock = self.function_inners.lock().unwrap();
                    lock.remove(function_id);
                }
            }
            None => {
                panic!("Attempted to decrement ref for FunctionID {function_id:?} (not found)");
            }
        }
    }

    /// Append a new event for the given function ID, but only if ref_count > 0.
    pub fn put(&mut self, event: Arc<TraceEventWithMeta>) {
        // log::debug!(
        //     "#####################   Putting event: {} ############\n{}\n\n",
        //     event.call_id,
        //     event.content.type_name()
        // );
        if let Err(e) = crate::tracingv2::publisher::publish_trace_event(event.clone()) {
            log::warn!("Failed to publish trace event: {e:?}");
        }

        let Some(&count) = self.ref_counts.get(&event.call_id) else {
            // Note -- this happens on python functions since there's no collector for them so we never 'track' these.
            // meaning we can just disacrd the data after publishing.
            // If no references exist, skip or handle otherwise
            // log::trace!("No references for FunctionID {:?} -- dropping events", event.call_id);

            return;
        };
        if count > 0 {
            if let Some(events_vec) = self.call_map.get_mut(&event.call_id) {
                events_vec.push(event);
            }
        }
    }

    /// Retrieve events for a particular function (call).
    /// Returns None if the function isn't being tracked (or was removed).
    pub fn get_events(
        &self,
        function_id: &FunctionCallId,
    ) -> Option<&Vec<Arc<TraceEventWithMeta>>> {
        self.call_map.get(function_id)
    }

    /// Returns how many references a given function currently has.
    pub fn ref_count_for(&self, function_id: &FunctionCallId) -> usize {
        self.ref_counts.get(function_id).copied().unwrap_or(0)
    }

    pub fn function_call_count(&self) -> usize {
        self.call_map.len()
    }

    /// For debugging – return a copy of all events in memory.
    pub fn events(&self) -> HashMap<FunctionCallId, Vec<Arc<TraceEventWithMeta>>> {
        self.call_map.clone()
    }

    pub fn clear(&mut self) {
        self.call_map.clear();
        self.ref_counts.clear();
        self.function_inners.lock().unwrap().clear();
    }
}

///
/// Build a single [FunctionLogInner] from all events corresponding to `function_id`, or
/// return it from the cache. If there is no data for the given function ID
/// (or FunctionStart is missing), returns None.
///
fn build_function_log(
    storage: &TraceStorage,
    function_id: &FunctionCallId,
) -> Option<Arc<Mutex<FunctionLogInner>>> {
    // First, check if there's already a cached FunctionLogInner.
    {
        let lock = storage.function_inners.lock().unwrap();
        if let Some(existing) = lock.get(function_id) {
            // Already built, just return a clone
            return Some(existing.clone());
        }
    }

    // If no cached version, fetch events to build from scratch.
    let events = storage.get_events(function_id)?;
    let guard = events; // A reference to the vector.

    let mut function_start: Option<&FunctionStart<_>> = None;
    // let mut function_end: Option<&FunctionEnd<_>> = None;

    let mut function_start_time: Option<i64> = None;
    let mut function_end_time: Option<i64> = None;

    let mut usage = Usage::default();
    let mut combined_metadata = HashMap::new();
    let mut raw_llm_response: Option<String> = None;

    // We must group requests by request_id for LLM calls.
    let mut calls_map: HashMap<HttpRequestId, CallAccumulator> = HashMap::new();

    // TODO sort events by timestamp:
    for event in guard.iter() {
        let time_ms = system_time_to_utc_ms(&event.timestamp);

        match &event.content {
            // Function lifecycle
            TraceData::FunctionStart(start) => {
                function_start = Some(start);
                function_start_time = Some(time_ms);
                for (k, v) in start.options.tags.iter() {
                    combined_metadata.insert(k.clone(), v.clone());
                }
            }
            TraceData::FunctionEnd(end) => {
                // function_end = Some(end);
                function_end_time = Some(time_ms);
            }

            // LLM adjacency
            TraceData::LLMRequest(llm_req) => {
                // TODO: request_id must match
                let rid = llm_req.request_id.clone();
                let entry = calls_map.entry(rid).or_default();
                entry.llm_request = Some(llm_req.clone());
                entry.timestamp_first_seen = Some(time_ms);
            }
            TraceData::LLMResponse(llm_res) => {
                let rid = llm_res.request_id.clone();
                let entry = calls_map.entry(rid).or_default();
                entry.llm_response = Some(llm_res.clone());
                entry.timestamp_last_seen = Some(time_ms);

                // Attempt usage from here:
                if let Some(usage_info) = &llm_res.usage {
                    entry.usage = Some(Usage {
                        input_tokens: usage_info.input_tokens.map(|t| t as i64),
                        output_tokens: usage_info.output_tokens.map(|t| t as i64),
                        cached_input_tokens: usage_info.cached_input_tokens.map(|t| t as i64),
                    });
                }

                // TODO: zero copy?
                raw_llm_response = llm_res.raw_text_output.clone();
            }

            // Raw requests and responses
            TraceData::RawLLMRequest(http_req) => {
                let rid = http_req.id.clone();
                let entry = calls_map.entry(rid).or_default();
                entry.http_request = Some(http_req.clone());
                entry.timestamp_first_seen = Some(time_ms);
            }
            TraceData::RawLLMResponse(http_res) => {
                let rid = http_res.request_id.clone();
                let entry = calls_map.entry(rid).or_default();
                entry.http_response = Some(http_res.clone());
                entry.timestamp_last_seen = Some(time_ms);
            }
            TraceData::RawLLMResponseStream(http_res_stream) => {
                let rid = http_res_stream.request_id.clone();
                let entry = calls_map.entry(rid.clone()).or_default();

                // find or insert the event
                match &mut entry.http_response_stream {
                    Some(stream) => {
                        stream.lock().unwrap().push(http_res_stream.clone());
                    }
                    None => {
                        entry.http_response_stream =
                            Some(Arc::new(Mutex::new(vec![http_res_stream.clone()])));
                    }
                }

                entry.timestamp_last_seen = Some(time_ms);
            }
            TraceData::SetTags(tags) => {
                for (k, v) in tags.iter() {
                    combined_metadata.insert(k.clone(), v.clone());
                }
            }
        }
    }

    // If we never found a FunctionStart, skip building a log.
    let start_ev = function_start.as_ref()?;
    let fname = start_ev.name.clone();

    let start_ms = function_start_time.unwrap_or(0);
    let end_ms = function_end_time;
    let duration = end_ms.map(|end| end.saturating_sub(start_ms));

    // Build each LLM call candidate first so we can compute the selected one by timestamp
    struct CallCandidate {
        request_id: HttpRequestId,
        is_stream: bool,
        client: String,
        provider: String,
        start_t: i64,
        end_t: i64,
        partial_duration: i64,
        http_request: Option<Arc<HTTPRequest>>,
        http_response: Option<Arc<HTTPResponse>>,
        http_response_stream: Option<Arc<Mutex<Vec<Arc<HTTPResponseStream>>>>>,
        local_usage: Usage,
        is_success: bool,
    }

    let mut candidates: Vec<CallCandidate> = Vec::new();

    for (rid, call_acc) in calls_map {
        let (client, provider) = parse_llm_client_and_provider(call_acc.llm_request.as_ref());
        let start_t = call_acc.timestamp_first_seen.unwrap_or(start_ms);
        let end_t = call_acc.timestamp_last_seen.unwrap_or(start_t);
        let partial_duration = end_t.saturating_sub(start_t);

        let is_stream = call_acc.http_response_stream.is_some();

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
        usage.cached_input_tokens =
            match (usage.cached_input_tokens, local_usage.cached_input_tokens) {
                (Some(i), Some(j)) => Some(i + j),
                (None, None) => None,
                (Some(i), None) => Some(i),
                (None, Some(j)) => Some(j),
            };

        let is_success = call_acc
            .llm_response
            .as_ref()
            .map(|resp| resp.error_message.is_none())
            .unwrap_or(false);

        candidates.push(CallCandidate {
            request_id: rid.clone(),
            is_stream,
            client,
            provider,
            start_t,
            end_t,
            partial_duration,
            http_request: call_acc.http_request.clone(),
            http_response: call_acc.http_response.clone(),
            http_response_stream: call_acc.http_response_stream.clone(),
            local_usage,
            is_success,
        });
    }

    // Determine which candidate should be marked selected
    let mut selected_idx: Option<usize> = None;
    if !candidates.is_empty() {
        // Filter successful candidates
        let mut successful_calls: Vec<(usize, &CallCandidate)> = candidates
            .iter()
            .enumerate()
            .filter(|(_, c)| c.is_success)
            .collect();

        if !successful_calls.is_empty() {
            // Sort successful calls by lexicographic order of request_id (ULID UUID)
            successful_calls
                .sort_by(|(_, a), (_, b)| a.request_id.to_string().cmp(&b.request_id.to_string()));

            // Pick the first (earliest lexicographically)
            selected_idx = Some(successful_calls[0].0);
        }
    }

    // Build final calls vector, marking only the selected one as selected
    let mut calls = Vec::new();
    for (i, c) in candidates.into_iter().enumerate() {
        let is_selected = matches!(selected_idx, Some(sel) if sel == i);
        if !c.is_stream {
            calls.push(LLMCallKind::Basic(LLMCall {
                client_name: c.client,
                provider: c.provider,
                timing: Timing {
                    start_time_utc_ms: c.start_t,
                    duration_ms: Some(c.partial_duration),
                },
                request: c.http_request,
                response: c.http_response,
                usage: Some(c.local_usage),
                selected: is_selected,
            }));
        } else {
            let sse_chunks = c.http_response_stream.and_then(|chunks| {
                let chunks = chunks.lock().unwrap();
                let request_id = chunks.first().map(|e| e.request_id.clone())?;
                Some(Arc::new(LLMHTTPStreamResponse {
                    request_id,
                    event: chunks.iter().map(|e| e.event.clone()).collect::<Vec<_>>(),
                }))
            });
            calls.push(LLMCallKind::Stream(LLMStreamCall {
                llm_call: LLMCall {
                    client_name: c.client,
                    provider: c.provider,
                    timing: Timing {
                        start_time_utc_ms: c.start_t,
                        duration_ms: Some(c.partial_duration),
                    },
                    request: c.http_request,
                    response: c.http_response,
                    usage: Some(c.local_usage),
                    selected: is_selected,
                },
                timing: StreamTiming {
                    start_time_utc_ms: c.start_t,
                    duration_ms: Some(c.partial_duration),
                },
                sse_chunks,
            }));
        }
    }

    // If there's at least one streaming call, we mark the FunctionLogInner's type as "stream".
    let is_stream_fn = calls.iter().any(|c| matches!(c, LLMCallKind::Stream(_)));

    let function_log_inner = FunctionLogInner {
        id: function_id.clone(),
        function_name: fname,
        r#type: if is_stream_fn {
            "stream".into()
        } else {
            "call".into()
        },
        timing: Timing {
            start_time_utc_ms: start_ms,
            duration_ms: duration,
        },
        usage,
        calls,
        raw_llm_response,
        metadata: combined_metadata,
    };

    let new_arc = Arc::new(Mutex::new(function_log_inner));
    // only cache if we we've finished the function
    if function_end_time.is_some() {
        // Insert into the cache
        let mut lock = storage.function_inners.lock().unwrap();
        lock.insert(function_id.clone(), new_arc.clone());
    }

    Some(new_arc)
}

#[derive(Debug, Serialize)]
pub enum HTTPResponseOrStream {
    Response(Arc<HTTPResponse>),
    Stream(Arc<HTTPResonseStreamCollection>),
}

impl HTTPResponseOrStream {
    pub fn response(&self) -> Option<&HTTPResponse> {
        match self {
            HTTPResponseOrStream::Response(resp) => Some(resp),
            HTTPResponseOrStream::Stream(_) => None,
        }
    }

    pub fn stream(&self) -> Option<&HTTPResonseStreamCollection> {
        match self {
            HTTPResponseOrStream::Response(_) => None,
            HTTPResponseOrStream::Stream(stream) => Some(stream),
        }
    }
}

/// A helper structure for building an LLM call from multiple events sharing the same request_id.
#[derive(Default, Debug)]
struct CallAccumulator {
    pub llm_request: Option<Arc<LoggedLLMRequest>>,
    pub llm_response: Option<Arc<LoggedLLMResponse>>,
    pub http_request: Option<Arc<HTTPRequest>>,
    pub http_response: Option<Arc<HTTPResponse>>,
    pub http_response_stream: Option<Arc<Mutex<Vec<Arc<HTTPResponseStream>>>>>,
    pub usage: Option<Usage>,
    pub timestamp_first_seen: Option<i64>,
    pub timestamp_last_seen: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct HTTPResonseStreamCollection {
    pub request_id: HttpRequestId,
    pub event: Mutex<Vec<Arc<HTTPResponseStream>>>,
}

fn parse_llm_client_and_provider(req: Option<&Arc<LoggedLLMRequest>>) -> (String, String) {
    match req {
        Some(r) => (r.client_name.clone(), r.client_provider.clone()),
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

///
/// Represents a single function call's log.
///
#[derive(Debug)]
pub struct FunctionLog {
    id: FunctionCallId,
    /// We store an optional Arc<Mutex<FunctionLogInner>> so that we only load it lazily.
    inner: Option<Arc<Mutex<FunctionLogInner>>>,
    instance_id: String,
}

impl Clone for FunctionLog {
    fn clone(&self) -> Self {
        // Creating a new FunctionLog will inc_ref again:
        Self::new(self.id.clone())
    }
}

impl FunctionLog {
    pub fn new(id: FunctionCallId) -> Self {
        // Manually increment the global reference count
        BAML_TRACER.lock().unwrap().inc_ref(&id);
        let instance_id = Uuid::new_v4().to_string();

        Self {
            id,
            inner: None,
            instance_id,
        }
    }

    // Private helper to get or build the inner reference
    fn get_inner(&mut self) -> &Arc<Mutex<FunctionLogInner>> {
        if self.inner.is_none() {
            // We attempt to build or retrieve from the global tracer
            let maybe_arc = {
                let tracer = BAML_TRACER.lock().unwrap();
                build_function_log(&tracer, &self.id)
                    .expect("Function log expected to be present (no FunctionStart event?). Did you forget to track_function()?")
            };
            self.inner = Some(maybe_arc);
        }
        self.inner.as_ref().unwrap()
    }

    pub fn id(&self) -> FunctionCallId {
        self.id.clone()
    }

    // The methods below clone from the underlying data (no references).
    pub fn function_name(&mut self) -> String {
        self.get_inner().lock().unwrap().function_name.clone()
    }

    pub fn log_type(&mut self) -> String {
        self.get_inner().lock().unwrap().r#type.clone()
    }

    pub fn timing(&mut self) -> Timing {
        self.get_inner().lock().unwrap().timing.clone()
    }

    pub fn usage(&mut self) -> Usage {
        self.get_inner().lock().unwrap().usage.clone()
    }

    pub fn calls(&mut self) -> Vec<LLMCallKind> {
        self.get_inner().lock().unwrap().calls.clone()
    }

    pub fn raw_llm_response(&mut self) -> Option<String> {
        self.get_inner().lock().unwrap().raw_llm_response.clone()
    }

    pub fn metadata(&mut self) -> HashMap<String, serde_json::Value> {
        self.get_inner().lock().unwrap().metadata.clone()
    }

    /// Backwards-compatible alias for metadata used by some language clients as "tags"
    pub fn tags(&mut self) -> HashMap<String, serde_json::Value> {
        self.get_inner().lock().unwrap().metadata.clone()
    }
}

impl Drop for FunctionLog {
    fn drop(&mut self) {
        // Manually decrement the global ref count
        BAML_TRACER.lock().unwrap().dec_ref(&self.id);
    }
}

///
/// Represents the "inner" data for a single function call
/// (the real set of usage/calls/timing, etc.).
///
#[derive(Debug, Clone, Serialize)]
pub struct FunctionLogInner {
    pub id: FunctionCallId,
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
}

#[derive(Debug, Default, Clone, Hash, Eq, PartialEq, Serialize)]
pub struct Usage {
    pub input_tokens: Option<i64>,
    pub output_tokens: Option<i64>,
    pub cached_input_tokens: Option<i64>,
}

#[derive(Debug, Default, Clone, Hash, Eq, PartialEq, Serialize)]
pub struct Timing {
    pub start_time_utc_ms: i64,
    pub duration_ms: Option<i64>,
}

#[derive(Debug, Default, Clone, Hash, Eq, PartialEq, Serialize)]
pub struct StreamTiming {
    pub start_time_utc_ms: i64,
    pub duration_ms: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum LLMCallKind {
    Basic(LLMCall),
    Stream(LLMStreamCall),
}

impl LLMCallKind {
    /// Returns whether this call is selected.
    pub fn selected(&self) -> bool {
        match self {
            LLMCallKind::Basic(c) => c.selected,
            LLMCallKind::Stream(c) => c.llm_call.selected,
        }
    }

    pub fn as_request(&self) -> Option<&LLMCall> {
        match self {
            LLMCallKind::Basic(c) => Some(c),
            LLMCallKind::Stream(c) => None,
        }
    }

    pub fn as_stream(&self) -> Option<&LLMStreamCall> {
        match self {
            LLMCallKind::Basic(c) => None,
            LLMCallKind::Stream(c) => Some(c),
        }
    }
}

#[derive(Debug, Default, Clone, Serialize)]
pub struct LLMCall {
    pub client_name: String,
    pub provider: String,
    pub timing: Timing,
    pub request: Option<Arc<HTTPRequest>>,
    pub response: Option<Arc<HTTPResponse>>,
    pub usage: Option<Usage>,
    pub selected: bool,
}

#[derive(Debug, Default, Clone, Serialize)]
pub struct LLMStreamCall {
    pub llm_call: LLMCall,
    pub timing: StreamTiming,
    pub sse_chunks: Option<Arc<LLMHTTPStreamResponse>>,
}

#[derive(Debug, Default, Clone, Serialize)]
pub struct LLMHTTPStreamResponse {
    pub request_id: HttpRequestId,
    pub event: Vec<Arc<SSEEvent>>,
}

/// A Collector holds references to multiple FunctionIds in order of insertion.
/// When dropped, it decrements the global ref counts for all tracked IDs.
#[derive(Debug)]
pub struct Collector {
    name: String,
    // Using IndexSet to preserve the insertion order of tracked FuncIds
    tracked_ids: Mutex<IndexSet<FunctionCallId>>,
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

    pub fn track_function(&self, fid: FunctionCallId) {
        log::debug!("Tracking function: {fid:?}");

        // Then add to our set (maintaining insertion order)
        let mut guard = self.tracked_ids.lock().unwrap();
        if guard.insert(fid.clone()) {
            // First increment the global ref count
            BAML_TRACER.lock().unwrap().inc_ref(&fid);
        }
    }
    pub fn untrack_function(&self, fid: &FunctionCallId) {
        let mut guard = self.tracked_ids.lock().unwrap();
        if guard.swap_remove(fid) {
            BAML_TRACER.lock().unwrap().dec_ref(fid);
        }
    }

    pub fn clear(&self) -> usize {
        let mut guard = self.tracked_ids.lock().unwrap();
        for fid in guard.iter() {
            BAML_TRACER.lock().unwrap().dec_ref(fid);
        }
        let len = guard.len();
        guard.clear();
        len
    }

    pub fn function_logs(&self) -> Vec<FunctionLog> {
        let guard = self.tracked_ids.lock().unwrap();
        guard
            .iter()
            .map(|fid| FunctionLog::new(fid.clone()))
            .collect()
    }

    pub fn last_function_log(&self) -> Option<FunctionLog> {
        let guard = self.tracked_ids.lock().unwrap();
        guard
            .iter()
            .last() // Based on insertion order
            .map(|id| FunctionLog::new(id.clone()))
    }

    pub fn function_log_by_id(&self, fid: &FunctionCallId) -> Option<FunctionLog> {
        let guard = self.tracked_ids.lock().unwrap();
        guard.get(fid).map(|fid| FunctionLog::new(fid.clone()))
    }

    pub fn usage(&self) -> Usage {
        let guard = self.tracked_ids.lock().unwrap();
        let mut total_usage = Usage::default();
        for fid in guard.iter() {
            let mut log = FunctionLog::new(fid.clone());
            let usage = log.usage();
            total_usage.input_tokens = match (total_usage.input_tokens, usage.input_tokens) {
                (Some(a), Some(b)) => Some(a + b),
                (None, Some(b)) => Some(b),
                (Some(a), None) => Some(a),
                (None, None) => None,
            };
            total_usage.output_tokens = match (total_usage.output_tokens, usage.output_tokens) {
                (Some(a), Some(b)) => Some(a + b),
                (None, Some(b)) => Some(b),
                (Some(a), None) => Some(a),
                (None, None) => None,
            };
            total_usage.cached_input_tokens =
                match (total_usage.cached_input_tokens, usage.cached_input_tokens) {
                    (Some(a), Some(b)) => Some(a + b),
                    (None, Some(b)) => Some(b),
                    (Some(a), None) => Some(a),
                    (None, None) => None,
                };
        }
        total_usage
    }
}

impl Clone for Collector {
    fn clone(&self) -> Self {
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
        // On drop, we untrack (and thus dec_ref) everything we were tracking
        let mut tracer = BAML_TRACER.lock().unwrap();
        let guard = self.tracked_ids.lock().unwrap();
        for fid in guard.iter() {
            tracer.dec_ref(fid);
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
//  Tests
// ─────────────────────────────────────────────────────────────────────────────

// watch out when running all cargo tests in the project -- as they could mess with the global tracer state if you don't add the #[serial]. Perhaps we need #[tokio::test]
#[cfg(test)]
mod tests {
    use core::time::Duration;

    use baml_ids::{FunctionCallId, FunctionEventId, HttpRequestId};
    use baml_types::{
        ir_type::TypeNonStreaming,
        tracing::events::{
            EvaluationContext, FunctionEnd, FunctionStart, FunctionType, LLMChatMessage,
            LLMChatMessagePart, LLMUsage, LoggedLLMRequest, LoggedLLMResponse, TraceData,
            TraceEvent,
        },
    };
    use indexmap::IndexMap;
    use serial_test::serial;
    use tokio::runtime::Runtime;

    use super::*;

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

            let f_id = FunctionCallId::new();

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

            // Put a simple SetTags event
            let event: TraceEventWithMeta =
                TraceEvent::new_set_tags(vec![f_id.clone()], Default::default());
            let event = Arc::new(event);
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
            let f_id = FunctionCallId::new();
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

            // Put a simple SetTags event
            let event: TraceEventWithMeta =
                TraceEvent::new_set_tags(vec![f_id.clone()], Default::default());
            let event = Arc::new(event);
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
            let f_id = FunctionCallId::new();
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

            // Put a simple SetTags event
            let event: TraceEventWithMeta =
                TraceEvent::new_set_tags(vec![f_id.clone()], Default::default());
            let event = Arc::new(event);
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
            let f_id = FunctionCallId::new();

            // Clear global state
            {
                let mut tracer = BAML_TRACER.lock().unwrap();
                tracer.clear();
            }

            // Create a collector to track the function ID
            let collector = Collector::new(Some("test_collector".to_string()));
            collector.track_function(f_id.clone());

            // Create and insert start event
            let start_event: TraceEventWithMeta = TraceEvent::new_function_start(
                vec![f_id.clone()],
                "test_function".into(),
                vec![],
                EvaluationContext {
                    tags: Default::default(),
                },
                FunctionType::Native,
                false,
            );
            let start_event = Arc::new(start_event);
            {
                let mut tracer = BAML_TRACER.lock().unwrap();
                tracer.put(start_event.clone());
            }

            // Create and insert end event
            let end_event: TraceEventWithMeta = TraceEvent::new_function_end(
                vec![f_id.clone()],
                Ok(baml_types::BamlValueWithMeta::Null(TypeNonStreaming::null())),
                baml_types::tracing::events::FunctionType::BamlLlm,
            );
            let end_event = Arc::new(end_event);
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
            let f_id = FunctionCallId::new();

            // Clear global state
            {
                let mut tracer = BAML_TRACER.lock().unwrap();
                tracer.clear();
            }

            // Create a collector to track the function ID
            let collector = Collector::new(Some("test_collector".to_string()));
            collector.track_function(f_id.clone());

            let mut tags_map = serde_json::Map::new();
            tags_map.insert(
                "foo".to_string(),
                serde_json::Value::String("bar".to_string()),
            );
            tags_map.insert(
                "some_number".to_string(),
                serde_json::Value::Number(42.into()),
            );

            let start_event: TraceEventWithMeta = TraceEvent::new_function_start(
                vec![f_id.clone()],
                "test_function_meta".into(),
                vec![],
                EvaluationContext { tags: tags_map },
                FunctionType::Native,
                false,
            );
            let start_event = Arc::new(start_event);
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
            let f_id = FunctionCallId::new();
            let collector = Collector::new(Some("test_collector".to_string()));
            collector.track_function(f_id.clone());
            let start_time = web_time::SystemTime::now();
            // Create start event
            let start_event = Arc::new(TraceEvent {
                call_id: f_id.clone(),
                function_event_id: FunctionEventId::new(),
                content: TraceData::FunctionStart(FunctionStart {
                    name: "test_function_timing".into(),
                    function_type: FunctionType::Native,
                    is_stream: false,
                    args: vec![],
                    options: EvaluationContext {
                        tags: Default::default(),
                    },
                }),
                call_stack: vec![f_id.clone()],
                timestamp: start_time,
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
                call_id: f_id.clone(),
                function_event_id: FunctionEventId::new(),
                content: TraceData::FunctionEnd(FunctionEnd::Success {
                    value: baml_types::BamlValueWithMeta::Null(TypeNonStreaming::null()),
                    function_type: baml_types::tracing::events::FunctionType::BamlLlm,
                }),
                call_stack: vec![f_id.clone()],
                timestamp: end_time,
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
        f_id: &FunctionCallId,
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
        let start_event: TraceEventWithMeta = TraceEvent::new_function_start(
            vec![f_id.clone()],
            function_name.into(),
            vec![],
            EvaluationContext {
                tags: Default::default(),
            },
            FunctionType::Native,
            false,
        );
        let start_event = Arc::new(start_event);
        {
            let mut tracer = BAML_TRACER.lock().unwrap();
            tracer.put(start_event);
        }

        // Insert LLM requests and responses
        for (i, (req, resp)) in llm_calls.into_iter().enumerate() {
            // Put the request
            let event_req: TraceEventWithMeta =
                TraceEvent::new_llm_request(vec![f_id.clone()], Arc::new(req));
            let event_req = Arc::new(event_req);
            {
                let mut tracer = BAML_TRACER.lock().unwrap();
                tracer.put(event_req);
            }

            // Put the response
            let event_resp: TraceEventWithMeta =
                TraceEvent::new_llm_response(vec![f_id.clone()], Arc::new(resp));
            let event_resp = Arc::new(event_resp);
            {
                let mut tracer = BAML_TRACER.lock().unwrap();
                tracer.put(event_resp);
            }
        }

        // Insert the function end event
        let end_event: TraceEventWithMeta = TraceEvent::new_function_end(
            vec![f_id.clone()],
            Ok(baml_types::BamlValueWithMeta::Null(TypeNonStreaming::null())),
            baml_types::tracing::events::FunctionType::BamlLlm,
        );
        let end_event = Arc::new(end_event);
        {
            let mut tracer = BAML_TRACER.lock().unwrap();
            tracer.put(end_event);
        }

        collector
    }

    #[test]
    #[serial]
    fn test_selected_call_prefers_success_over_failure() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let f_id = FunctionCallId::new();

            // Create one failed response and one successful response
            let rid_fail = HttpRequestId::new();
            let rid_success = HttpRequestId::new();

            let failed_req = LoggedLLMRequest {
                request_id: rid_fail.clone(),
                client_name: "client_a".into(),
                client_provider: "provider_a".into(),
                params: IndexMap::new(),
                prompt: vec![LLMChatMessage {
                    role: "user".into(),
                    content: vec![LLMChatMessagePart::Text("hi".into())],
                }],
            };
            let failed_resp = LoggedLLMResponse::new_failure(
                rid_fail.clone(),
                "boom".into(),
                Some("m1".into()),
                Some("error".into()),
                vec![],
            );

            let ok_req = LoggedLLMRequest {
                request_id: rid_success.clone(),
                client_name: "client_b".into(),
                client_provider: "provider_b".into(),
                params: IndexMap::new(),
                prompt: vec![LLMChatMessage {
                    role: "user".into(),
                    content: vec![LLMChatMessagePart::Text("hello".into())],
                }],
            };
            let ok_resp = LoggedLLMResponse::new_success(
                rid_success.clone(),
                "m2".into(),
                Some("stop".into()),
                LLMUsage {
                    input_tokens: Some(1),
                    output_tokens: Some(2),
                    total_tokens: Some(3),
                    cached_input_tokens: Some(0),
                },
                "ok".into(),
                vec![],
            );

            let collector = inject_test_events(
                &f_id,
                "test_selected_call",
                vec![(failed_req, failed_resp), (ok_req, ok_resp)],
            )
            .await;

            let mut flog = FunctionLog::new(f_id.clone());
            let calls = flog.calls();
            assert_eq!(calls.len(), 2);

            // Exactly one should be marked selected, and it should be the success
            let selected: Vec<_> = calls.iter().filter(|c| c.selected()).collect();
            assert_eq!(selected.len(), 1);
            let sel = selected[0];
            match sel {
                LLMCallKind::Basic(c) => {
                    assert_eq!(c.client_name, "client_b");
                    assert!(c.selected);
                }
                LLMCallKind::Stream(s) => {
                    assert_eq!(s.llm_call.client_name, "client_b");
                    assert!(s.llm_call.selected);
                }
            }

            drop(flog);
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
    fn test_selected_call_chooses_earlier_success_if_last_failed() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let f_id = FunctionCallId::new();

            // First a successful call, then a failed call (latest is failed)
            let rid_success = HttpRequestId::new();
            let rid_fail = HttpRequestId::new();

            let ok_req = LoggedLLMRequest {
                request_id: rid_success.clone(),
                client_name: "client_ok".into(),
                client_provider: "provider_ok".into(),
                params: IndexMap::new(),
                prompt: vec![LLMChatMessage {
                    role: "user".into(),
                    content: vec![LLMChatMessagePart::Text("hello".into())],
                }],
            };
            let ok_resp = LoggedLLMResponse::new_success(
                rid_success.clone(),
                "m2".into(),
                Some("stop".into()),
                LLMUsage {
                    input_tokens: Some(1),
                    output_tokens: Some(2),
                    total_tokens: Some(3),
                    cached_input_tokens: Some(0),
                },
                "ok".into(),
                vec![],
            );

            let failed_req = LoggedLLMRequest {
                request_id: rid_fail.clone(),
                client_name: "client_fail".into(),
                client_provider: "provider_fail".into(),
                params: IndexMap::new(),
                prompt: vec![LLMChatMessage {
                    role: "user".into(),
                    content: vec![LLMChatMessagePart::Text("hi".into())],
                }],
            };
            let failed_resp = LoggedLLMResponse::new_failure(
                rid_fail.clone(),
                "boom".into(),
                Some("m1".into()),
                Some("error".into()),
                vec![],
            );

            // Inject in order: success first, failure second (so failure is latest by timestamp)
            let collector = inject_test_events(
                &f_id,
                "test_selected_call_last_failed",
                vec![(ok_req, ok_resp), (failed_req, failed_resp)],
            )
            .await;

            let mut flog = FunctionLog::new(f_id.clone());
            let calls = flog.calls();
            assert_eq!(calls.len(), 2);

            // Latest failed, we expect selected to be the successful earlier call
            let selected: Vec<_> = calls.iter().filter(|c| c.selected()).collect();
            assert_eq!(selected.len(), 1);
            let sel = selected[0];
            match sel {
                LLMCallKind::Basic(c) => {
                    assert_eq!(c.client_name, "client_ok");
                    assert!(c.selected);
                }
                LLMCallKind::Stream(s) => {
                    assert_eq!(s.llm_call.client_name, "client_ok");
                    assert!(s.llm_call.selected);
                }
            }

            drop(flog);
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
    fn test_usage_accumulation_within_function_log_retries() {
        use baml_types::tracing::events::{LLMUsage, LoggedLLMRequest, LoggedLLMResponse};

        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let f_id = FunctionCallId::new();

            let id1 = HttpRequestId::new();
            let id2 = HttpRequestId::new();
            let llm_calls = vec![
                (
                    LoggedLLMRequest {
                        request_id: id1.clone(),
                        client_name: "my_client".into(),
                        client_provider: "my_provider".into(),
                        params: {
                            let mut m: IndexMap<String, serde_json::Value> = IndexMap::new();
                            m.insert("temperature".to_string(), serde_json::json!(0.7));
                            m
                        },
                        prompt: vec![LLMChatMessage {
                            role: "user".to_string(),
                            content: vec![LLMChatMessagePart::Text("Hello world".to_string())],
                        }],
                    },
                    LoggedLLMResponse {
                        request_id: id1,
                        client_stack: vec!["MyOpenai".to_string()],
                        model: Some("test-model-v1".into()),
                        finish_reason: Some("stop".into()),
                        usage: Some(LLMUsage {
                            input_tokens: Some(12),
                            output_tokens: Some(8),
                            total_tokens: Some(20),
                            cached_input_tokens: Some(0),
                        }),
                        raw_text_output: Some("Hello back".into()),
                        error_message: None,
                    },
                ),
                (
                    LoggedLLMRequest {
                        request_id: id2.clone(),
                        client_name: "my_client".into(),
                        client_provider: "my_provider".into(),
                        params: {
                            let mut m: IndexMap<String, serde_json::Value> = IndexMap::new();
                            m.insert("temperature".to_string(), serde_json::json!(0.9));
                            m
                        },
                        prompt: vec![LLMChatMessage {
                            role: "user".to_string(),
                            content: vec![LLMChatMessagePart::Text("Next message".to_string())],
                        }],
                    },
                    LoggedLLMResponse {
                        request_id: id2,
                        client_stack: vec!["MyOpenai".to_string()],
                        model: Some("test-model-v2".into()),
                        finish_reason: Some("length".into()),
                        usage: Some(LLMUsage {
                            input_tokens: Some(10),
                            output_tokens: Some(30),
                            total_tokens: Some(40),
                            cached_input_tokens: Some(0),
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
