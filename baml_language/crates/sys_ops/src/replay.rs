//! Record-replay system for HTTP calls.
//!
//! [`RecordReplay`] is a generic recording/pinning/lookup store.
//! [`ReplayHttp`] wraps any `dyn IoNamespaceHttp` to add transparent recording
//! and replay-hit interception.

use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex, RwLock,
        atomic::{AtomicU64, Ordering},
    },
};

use serde::Serialize;
use sha2::{Digest, Sha256};
use sys_types::{BexHeap, CallId, OpErrorKind, SysOpContext, SysOpOutput};

use crate::io::{IoClassHttpResponse, IoNamespaceHttp, owned};

// ---------------------------------------------------------------------------
// Display info and snapshot types
// ---------------------------------------------------------------------------

/// Human-readable request metadata for popover display.
#[derive(Debug, Clone, Serialize)]
pub struct RequestDisplayInfo {
    pub method: String,
    pub url: String,
    pub body: String,
}

/// Snapshot of a single request group for the popover.
#[derive(Debug, Clone, Serialize)]
pub struct ReplayGroupSnapshot {
    pub key: String,
    pub display: RequestDisplayInfo,
    pub recordings: Vec<RecordingSnapshot>,
    pub pinned_fetch_id: Option<u64>,
}

/// Snapshot of a single recording within a group.
#[derive(Debug, Clone, Serialize)]
pub struct RecordingSnapshot {
    pub fetch_id: u64,
    pub status: i64,
    pub body: String,
    /// Seconds since UNIX epoch when the recording was captured.
    pub recorded_at: u64,
}

// ---------------------------------------------------------------------------
// Generic record-replay store
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct RecordedEntry<V> {
    pub fetch_id: u64,
    pub value: V,
    /// Seconds since UNIX epoch when recorded.
    pub recorded_at: u64,
}

pub struct RecordReplay<K: std::hash::Hash + Eq + Clone, V: Clone> {
    /// Content key to ordered list of recordings (most recent last).
    recordings: HashMap<K, Vec<RecordedEntry<V>>>,
    /// Fetch ID to content key (for resolving pin commands by `fetch_id`).
    fetch_id_to_key: HashMap<u64, K>,
    /// Content key to pinned entry (at most one per key).
    pinned_by_key: HashMap<K, RecordedEntry<V>>,
    /// Human-readable request metadata, one per unique key.
    request_display: HashMap<K, RequestDisplayInfo>,
}

impl<K: std::hash::Hash + Eq + Clone, V: Clone> Default for RecordReplay<K, V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<K: std::hash::Hash + Eq + Clone, V: Clone> RecordReplay<K, V> {
    pub fn new() -> Self {
        Self {
            recordings: HashMap::new(),
            fetch_id_to_key: HashMap::new(),
            pinned_by_key: HashMap::new(),
            request_display: HashMap::new(),
        }
    }

    /// Record a response. Called after every successful HTTP round-trip.
    pub fn record(&mut self, key: K, fetch_id: u64, value: V, display: RequestDisplayInfo) {
        self.fetch_id_to_key.insert(fetch_id, key.clone());
        // Store display info only on first recording for this key.
        self.request_display.entry(key.clone()).or_insert(display);
        let recorded_at = web_time::SystemTime::now()
            .duration_since(web_time::SystemTime::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        self.recordings.entry(key).or_default().push(RecordedEntry {
            fetch_id,
            value,
            recorded_at,
        });
    }

    /// Look up a pinned entry by content key.
    pub fn get_pinned(&self, key: &K) -> Option<&RecordedEntry<V>> {
        self.pinned_by_key.get(key)
    }

    /// Pin or unpin a recording identified by its `fetch_id`.
    /// Returns true if the operation succeeded (`fetch_id` was found).
    pub fn set_pinned(&mut self, fetch_id: u64, pinned: bool) -> bool {
        let Some(key) = self.fetch_id_to_key.get(&fetch_id).cloned() else {
            return false;
        };
        if pinned {
            if let Some(entries) = self.recordings.get(&key) {
                if let Some(entry) = entries.iter().find(|e| e.fetch_id == fetch_id) {
                    self.pinned_by_key.insert(key, entry.clone());
                    return true;
                }
            }
            false
        } else {
            self.pinned_by_key.remove(&key);
            true
        }
    }
}

impl RecordReplay<RequestKey, RecordedResponse> {
    /// Return a full snapshot of the store for the popover UI.
    pub fn snapshot(&self) -> Vec<ReplayGroupSnapshot> {
        self.recordings
            .iter()
            .filter_map(|(key, entries)| {
                let display = self.request_display.get(key)?.clone();
                let pinned_fetch_id = self.pinned_by_key.get(key).map(|e| e.fetch_id);
                let recordings = entries
                    .iter()
                    .map(|e| RecordingSnapshot {
                        fetch_id: e.fetch_id,
                        status: e.value.status,
                        body: e.value.body.clone(),
                        recorded_at: e.recorded_at,
                    })
                    .collect();
                Some(ReplayGroupSnapshot {
                    key: hex::encode(key.0),
                    display,
                    recordings,
                    pinned_fetch_id,
                })
            })
            .collect()
    }
}

// ---------------------------------------------------------------------------
// HTTP-specific types
// ---------------------------------------------------------------------------

/// SHA-256 hash of (method, url, sorted headers, body).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RequestKey([u8; 32]);

impl RequestKey {
    pub fn from_request(request: &owned::http::Request) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(request.method.as_bytes());
        hasher.update(b"\0");
        hasher.update(request.url.as_bytes());
        hasher.update(b"\0");
        let mut sorted_headers: Vec<_> = request.headers.iter().collect();
        sorted_headers.sort_by_key(|(k, _)| k.as_str());
        for (k, v) in &sorted_headers {
            hasher.update(k.as_bytes());
            hasher.update(b":");
            hasher.update(v.as_bytes());
            hasher.update(b"\0");
        }
        hasher.update(request.body.as_bytes());
        RequestKey(hasher.finalize().into())
    }
}

#[derive(Debug, Clone)]
pub struct RecordedResponse {
    pub status: i64,
    pub headers: indexmap::IndexMap<String, String>,
    pub body: String,
    pub url: String,
}

/// Synthetic response body stored in `_body` for replay hits.
/// Follows the existing `Option::take` single-consumption pattern.
pub struct ReplayResponseBody(pub Mutex<Option<String>>);

/// Event emitted when a replay hit occurs (used for fetch log broadcasting).
pub struct ReplayFetchEvent {
    pub call_id: u64,
    pub fetch_id: u64,
    pub method: String,
    pub url: String,
    pub request_headers: HashMap<String, String>,
    pub request_body: String,
    pub status: i64,
    pub response_headers: HashMap<String, String>,
    pub response_body: String,
    pub duration_ms: u64,
}

// ---------------------------------------------------------------------------
// ReplayHttp wrapper
// ---------------------------------------------------------------------------

pub struct ReplayHttp {
    inner: Arc<dyn IoNamespaceHttp + Send + Sync>,
    store: Arc<RwLock<RecordReplay<RequestKey, RecordedResponse>>>,
    fetch_id_allocator: Arc<AtomicU64>,
    /// Callback for replay-hit fetch log events.
    on_replay: Option<Arc<dyn Fn(ReplayFetchEvent) + Send + Sync>>,
    /// Correlates `send()` to `text()` via response body pointer identity.
    #[allow(clippy::type_complexity)]
    pending_recordings: Arc<Mutex<HashMap<usize, (RequestKey, RequestDisplayInfo, u64)>>>,
}

impl ReplayHttp {
    pub fn new(
        inner: Arc<dyn IoNamespaceHttp + Send + Sync>,
        store: Arc<RwLock<RecordReplay<RequestKey, RecordedResponse>>>,
        fetch_id_allocator: Arc<AtomicU64>,
        on_replay: Option<Arc<dyn Fn(ReplayFetchEvent) + Send + Sync>>,
    ) -> Self {
        Self {
            inner,
            store,
            fetch_id_allocator,
            on_replay,
            pending_recordings: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn store(&self) -> &Arc<RwLock<RecordReplay<RequestKey, RecordedResponse>>> {
        &self.store
    }

    fn response_body_key(resp: &owned::http::Response) -> usize {
        Arc::as_ptr(&resp._body).cast::<()>() as usize
    }
}

impl IoClassHttpResponse for ReplayHttp {
    fn text(
        &self,
        heap: &Arc<BexHeap>,
        call_id: CallId,
        response: owned::http::Response,
        ctx: &SysOpContext,
    ) -> SysOpOutput<String> {
        // Try replay downcast first.
        if let Some(replay_body) = response._body.downcast_ref::<ReplayResponseBody>() {
            let text = replay_body.0.lock().unwrap().take();
            return match text {
                Some(t) => SysOpOutput::ok(t),
                None => SysOpOutput::err(OpErrorKind::Other(
                    "Replayed response body has already been consumed".into(),
                )),
            };
        }

        // Look up correlation from `send()`.
        let body_ptr = Self::response_body_key(&response);
        let pending_entry = self.pending_recordings.lock().unwrap().remove(&body_ptr);

        // Capture response metadata before delegating.
        let resp_status = response.status_code;
        let resp_headers = response.headers.clone();
        let resp_url = response.url.clone();

        let inner_result = self.inner.text(heap, call_id, response, ctx);
        let store = self.store.clone();

        match pending_entry {
            Some((key, display, fetch_id)) => match inner_result {
                SysOpOutput::Async(fut) => SysOpOutput::async_op(async move {
                    let text = fut.await?;
                    store.write().unwrap().record(
                        key,
                        fetch_id,
                        RecordedResponse {
                            status: resp_status,
                            headers: resp_headers,
                            body: text.clone(),
                            url: resp_url,
                        },
                        display,
                    );
                    Ok(text)
                }),
                SysOpOutput::Ready(Ok(text)) => {
                    store.write().unwrap().record(
                        key,
                        fetch_id,
                        RecordedResponse {
                            status: resp_status,
                            headers: resp_headers,
                            body: text.clone(),
                            url: resp_url,
                        },
                        display,
                    );
                    SysOpOutput::ok(text)
                }
                SysOpOutput::Ready(Err(err)) => SysOpOutput::Ready(Err(err)),
            },
            None => inner_result,
        }
    }
}

impl IoNamespaceHttp for ReplayHttp {
    fn send(
        &self,
        heap: &Arc<BexHeap>,
        call_id: CallId,
        request: owned::http::Request,
        ctx: &SysOpContext,
    ) -> SysOpOutput<owned::http::Response> {
        let request_key = RequestKey::from_request(&request);

        // Check for a pinned replay entry.
        if let Some(entry) = self.store.read().unwrap().get_pinned(&request_key) {
            let recorded = entry.value.clone();
            let fetch_id = self.fetch_id_allocator.fetch_add(1, Ordering::Relaxed);

            // Emit replay fetch log events via callback.
            if let Some(ref on_replay) = self.on_replay {
                on_replay(ReplayFetchEvent {
                    call_id: call_id.0,
                    fetch_id,
                    method: request.method,
                    url: request.url,
                    request_headers: request.headers.into_iter().collect(),
                    request_body: request.body,
                    status: recorded.status,
                    response_headers: recorded
                        .headers
                        .iter()
                        .map(|(k, v)| (k.clone(), v.clone()))
                        .collect(),
                    response_body: recorded.body.clone(),
                    duration_ms: 0,
                });
            }

            // Return synthetic response with ReplayResponseBody.
            let body: Arc<dyn std::any::Any + Send + Sync> =
                Arc::new(ReplayResponseBody(Mutex::new(Some(recorded.body.clone()))));
            return SysOpOutput::ok(owned::http::Response {
                status_code: recorded.status,
                headers: recorded.headers,
                url: recorded.url,
                _body: body,
            });
        }

        // No replay hit — delegate to inner.
        let display = RequestDisplayInfo {
            method: request.method.clone(),
            url: request.url.clone(),
            body: request.body.clone(),
        };
        // Pre-allocate a unique fetch_id now so it's deterministic (not racy).
        let fetch_id = self.fetch_id_allocator.fetch_add(1, Ordering::Relaxed);
        let inner_result = self.inner.send(heap, call_id, request, ctx);
        let pending = self.pending_recordings.clone();
        let key = request_key;

        match inner_result {
            SysOpOutput::Async(fut) => SysOpOutput::async_op(async move {
                let resp = fut.await?;
                let body_ptr = Arc::as_ptr(&resp._body).cast::<()>() as usize;
                pending
                    .lock()
                    .unwrap()
                    .insert(body_ptr, (key, display, fetch_id));
                Ok(resp)
            }),
            SysOpOutput::Ready(Ok(resp)) => {
                let body_ptr = Arc::as_ptr(&resp._body).cast::<()>() as usize;
                self.pending_recordings
                    .lock()
                    .unwrap()
                    .insert(body_ptr, (key, display, fetch_id));
                SysOpOutput::Ready(Ok(resp))
            }
            other @ SysOpOutput::Ready(Err(_)) => other,
        }
    }

    fn fetch(
        &self,
        heap: &Arc<BexHeap>,
        call_id: CallId,
        url: String,
        ctx: &SysOpContext,
    ) -> SysOpOutput<owned::http::Response> {
        let req = owned::http::Request {
            method: "GET".to_string(),
            url,
            headers: indexmap::IndexMap::new(),
            body: String::new(),
        };
        self.send(heap, call_id, req, ctx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_display() -> RequestDisplayInfo {
        RequestDisplayInfo {
            method: "GET".to_string(),
            url: "https://example.com".to_string(),
            body: String::new(),
        }
    }

    #[test]
    fn test_record_and_lookup() {
        let mut store = RecordReplay::new();
        let key = RequestKey([0u8; 32]);
        store.record(key.clone(), 1, "response body".to_string(), dummy_display());

        assert!(store.get_pinned(&key).is_none());

        assert!(store.set_pinned(1, true));
        let pinned = store.get_pinned(&key).unwrap();
        assert_eq!(pinned.value, "response body");
        assert_eq!(pinned.fetch_id, 1);

        assert!(store.set_pinned(1, false));
        assert!(store.get_pinned(&key).is_none());
    }

    #[test]
    fn test_multiple_recordings_per_key() {
        let mut store = RecordReplay::new();
        let key = RequestKey([0u8; 32]);
        store.record(key.clone(), 1, "first".to_string(), dummy_display());
        store.record(key.clone(), 2, "second".to_string(), dummy_display());

        assert!(store.set_pinned(2, true));
        let pinned = store.get_pinned(&key).unwrap();
        assert_eq!(pinned.value, "second");

        assert!(store.set_pinned(1, true));
        let pinned = store.get_pinned(&key).unwrap();
        assert_eq!(pinned.value, "first");
    }

    #[test]
    fn test_request_key_deterministic() {
        let req = owned::http::Request {
            method: "POST".to_string(),
            url: "https://api.openai.com/v1/chat".to_string(),
            headers: indexmap::IndexMap::from([
                ("content-type".to_string(), "application/json".to_string()),
                ("authorization".to_string(), "Bearer sk-xxx".to_string()),
            ]),
            body: r#"{"model":"gpt-4","messages":[]}"#.to_string(),
        };
        let key1 = RequestKey::from_request(&req);
        let key2 = RequestKey::from_request(&req);
        assert_eq!(key1, key2);
    }

    #[test]
    fn test_request_key_header_order_independent() {
        let req1 = owned::http::Request {
            method: "POST".to_string(),
            url: "https://api.example.com".to_string(),
            headers: indexmap::IndexMap::from([
                ("a".to_string(), "1".to_string()),
                ("b".to_string(), "2".to_string()),
            ]),
            body: "body".to_string(),
        };
        let req2 = owned::http::Request {
            method: "POST".to_string(),
            url: "https://api.example.com".to_string(),
            headers: indexmap::IndexMap::from([
                ("b".to_string(), "2".to_string()),
                ("a".to_string(), "1".to_string()),
            ]),
            body: "body".to_string(),
        };
        assert_eq!(
            RequestKey::from_request(&req1),
            RequestKey::from_request(&req2)
        );
    }

    #[test]
    fn test_request_key_different_body() {
        let req1 = owned::http::Request {
            method: "POST".to_string(),
            url: "https://api.example.com".to_string(),
            headers: indexmap::IndexMap::new(),
            body: "body1".to_string(),
        };
        let req2 = owned::http::Request {
            method: "POST".to_string(),
            url: "https://api.example.com".to_string(),
            headers: indexmap::IndexMap::new(),
            body: "body2".to_string(),
        };
        assert_ne!(
            RequestKey::from_request(&req1),
            RequestKey::from_request(&req2)
        );
    }

    #[test]
    fn test_set_pinned_unknown_fetch_id() {
        let mut store: RecordReplay<RequestKey, String> = RecordReplay::new();
        assert!(!store.set_pinned(999, true));
    }

    // ---------------------------------------------------------------------------
    // snapshot() tests
    // ---------------------------------------------------------------------------

    fn make_key(s: &str) -> RequestKey {
        let mut hasher = sha2::Sha256::new();
        hasher.update(s.as_bytes());
        RequestKey(hasher.finalize().into())
    }

    fn make_display(method: &str, url: &str) -> RequestDisplayInfo {
        RequestDisplayInfo {
            method: method.to_string(),
            url: url.to_string(),
            body: String::new(),
        }
    }

    fn make_response(status: i64, body: &str) -> RecordedResponse {
        RecordedResponse {
            status,
            headers: indexmap::IndexMap::new(),
            body: body.to_string(),
            url: "https://example.com".to_string(),
        }
    }

    #[test]
    fn snapshot_returns_all_groups_with_display_info() {
        let mut store = RecordReplay::new();
        let key_a = make_key("request-a");
        let key_b = make_key("request-b");

        store.record(
            key_a.clone(),
            1,
            make_response(200, "resp1"),
            make_display("POST", "https://a.com"),
        );
        store.record(
            key_a,
            2,
            make_response(200, "resp2"),
            make_display("POST", "https://a.com"),
        );
        store.record(
            key_b,
            3,
            make_response(404, "not found"),
            make_display("GET", "https://b.com"),
        );

        let snap = store.snapshot();
        assert_eq!(snap.len(), 2);

        let group_a = snap
            .iter()
            .find(|g| g.display.url == "https://a.com")
            .unwrap();
        assert_eq!(group_a.display.method, "POST");
        assert_eq!(group_a.recordings.len(), 2);
        assert_eq!(group_a.pinned_fetch_id, None);

        let group_b = snap
            .iter()
            .find(|g| g.display.url == "https://b.com")
            .unwrap();
        assert_eq!(group_b.recordings.len(), 1);
        assert_eq!(group_b.recordings[0].status, 404);
    }

    #[test]
    fn snapshot_shows_pinned_fetch_id() {
        let mut store = RecordReplay::new();
        let key = make_key("req");
        store.record(
            key.clone(),
            10,
            make_response(200, "ok"),
            make_display("GET", "https://x.com"),
        );
        store.record(
            key,
            11,
            make_response(200, "ok2"),
            make_display("GET", "https://x.com"),
        );
        store.set_pinned(11, true);

        let snap = store.snapshot();
        assert_eq!(snap.len(), 1);
        assert_eq!(snap[0].pinned_fetch_id, Some(11));
    }

    #[test]
    fn display_info_stored_only_on_first_record() {
        let mut store = RecordReplay::new();
        let key = make_key("req");
        store.record(
            key.clone(),
            1,
            make_response(200, "a"),
            make_display("POST", "https://first.com"),
        );
        store.record(
            key,
            2,
            make_response(200, "b"),
            make_display("POST", "https://second.com"),
        );

        let snap = store.snapshot();
        // Display info should be from the first recording, not overwritten.
        assert_eq!(snap[0].display.url, "https://first.com");
    }
}
