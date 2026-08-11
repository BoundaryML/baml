//! §8 value plane: a run's captured values, listed and hydrated.
//!
//! Backing stores (§8.2 ValueSet): the run's `.bamlvalue` capture roots +
//! the project CAS (`store/packs`) for canonical-DAG bodies, with the
//! legacy inline/blob `BamlOutboundValue` body as the fallback codec.
//! Call→function names join through an EXACT source only. Preferred:
//! the capture record's own `function_id` (stamped at capture time, same
//! id space as the run dictionary — no raw firehose needed). Fallback for
//! records written before captures carried ids: the raw firehose of the
//! bound session, when one exists (§8.2: aggregates are always available;
//! exact instances only where an exact source covers the scope). Neither
//! source → the `fn` column is honestly absent, with a remedy note, never
//! guessed.

use std::io;
use std::path::{Path, PathBuf};

#[cfg(not(target_arch = "wasm32"))]
use bex_events::store::Store;
use bex_events::store::canon;
use bex_events::value::{
    BlobRef, DagRef, ValueCaptureKind, ValueFileRecord, read_bamlvalue_from_bytes,
};
use rustc_hash::FxHashMap;

/// One captured value of a run (a §8.2 ValueSet row).
#[derive(Debug, Clone)]
pub struct ValueRow {
    /// `value_N` — stable per boundary, never renumbered.
    pub value_id: String,
    /// `input` | `output` | `error` | `log` (role view of the kind).
    pub role: &'static str,
    /// The precise capture kind (`rootInput`, `callOutput`, …).
    pub kind: &'static str,
    pub thread_id: u64,
    pub call_id: u64,
    /// Resolved from the capture's own `function_id` when it carries one,
    /// else joined from the raw firehose; `None` = no exact source.
    pub fn_name: Option<String>,
    /// Canonical-DAG root (`bamlv_1_…`) when the §7.4 dual-write ran.
    pub cid: Option<[u8; 32]>,
    pub original_bytes: u64,
    /// §7.2 trigger promotion: the trigger id that made a staged capture
    /// durable.
    pub promoted_by: Option<String>,
    body: BodySource,
}

#[derive(Debug, Clone)]
enum BodySource {
    /// Canonical DAG in the project CAS.
    Dag(DagRef),
    /// Inline `BamlOutboundValue` bytes in the record.
    Inline(Vec<u8>),
    /// Externalized sha256 blob under the boundary dir.
    Blob(BlobRef),
    /// Nothing retained (capture loss / missing).
    None,
}

/// How the call→function join went.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FnJoin {
    /// Names resolved exactly: capture-carried function ids and/or the
    /// session's raw firehose cover every row.
    Exact,
    /// No exact source covers this run — some `fn_name`s are absent.
    NoExactSource,
}

#[derive(Debug)]
pub struct RunValues {
    pub rows: Vec<ValueRow>,
    pub fn_join: FnJoin,
    /// True when any `.bamlvalue` segment ended mid-record (crash tail).
    pub truncated: bool,
}

fn role_of(kind: ValueCaptureKind) -> (&'static str, &'static str) {
    match kind {
        ValueCaptureKind::RootInput => ("input", "rootInput"),
        ValueCaptureKind::CallInput => ("input", "callInput"),
        ValueCaptureKind::RootOutput => ("output", "rootOutput"),
        ValueCaptureKind::CallOutput => ("output", "callOutput"),
        ValueCaptureKind::RootError => ("error", "rootError"),
        ValueCaptureKind::CallError => ("error", "callError"),
        ValueCaptureKind::LogBody => ("log", "logBody"),
    }
}

/// Every `.bamlvalue` segment under a boundary dir, thread dirs included.
fn value_segments(run_dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(run_dir) else {
        return out;
    };
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        if path.is_dir() {
            if let Ok(inner) = std::fs::read_dir(&path) {
                for f in inner.filter_map(Result::ok).map(|e| e.path()) {
                    if f.extension().is_some_and(|e| e == "bamlvalue") {
                        out.push(f);
                    }
                }
            }
        } else if path.extension().is_some_and(|e| e == "bamlvalue") {
            out.push(path);
        }
    }
    out.sort();
    out
}

/// Call→function map from the bound session's raw firehose (exact source).
/// `None` when no raw files exist.
fn raw_call_functions(baml_dir: &Path, session_dir: &str) -> Option<FxHashMap<(u64, u64), u32>> {
    // Bound records may carry the session as a relative path
    // (`.baml/sessions/<name>`) or a bare name — use the final component.
    let session_name = Path::new(session_dir).file_name().map_or_else(
        || session_dir.to_string(),
        |n| n.to_string_lossy().into_owned(),
    );
    let raw_dir = baml_dir.join("sessions").join(session_name).join("raw");
    let mut files: Vec<PathBuf> = std::fs::read_dir(&raw_dir)
        .ok()?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "bamlprof"))
        .collect();
    if files.is_empty() {
        return None;
    }
    files.sort();
    let mut map = FxHashMap::default();
    for file in files {
        let Ok(bytes) = std::fs::read(&file) else {
            continue;
        };
        let Ok(raw) = bex_events::prof::cct::raw::read_raw_file(&bytes) else {
            continue;
        };
        for range in &raw.ranges {
            for record in bex_events::prof::record::iter(range) {
                let Ok(record) = record else { break };
                if let bex_events::prof::record::RawRecord::CallFunction {
                    thread_id,
                    call_id,
                    function_id,
                    ..
                } = record
                {
                    map.insert((thread_id.0, call_id.0), function_id.0);
                }
            }
        }
    }
    Some(map)
}

/// List a run's captured values. Fn-name resolution order per row:
/// (1) the capture's own `function_id` (when non-zero) through the run's
/// dictionary (`names`); (2) the raw-firehose join of the bound session
/// (`session_dir`, from the boundary's Bound record) — kept for artifacts
/// recorded before captures carried ids; (3) none.
pub fn list_run_values(
    baml_dir: &Path,
    run_dir: &Path,
    session_dir: Option<&str>,
    names: Option<&FxHashMap<u32, String>>,
) -> io::Result<RunValues> {
    let call_fns = session_dir.and_then(|s| raw_call_functions(baml_dir, s));
    let resolve_fid = |fid: u32| {
        names
            .and_then(|n| n.get(&fid).cloned())
            .unwrap_or_else(|| format!("fn#{fid}"))
    };

    let mut rows = Vec::new();
    let mut truncated = false;
    for segment in value_segments(run_dir) {
        let bytes = std::fs::read(&segment)?;
        let contents = read_bamlvalue_from_bytes(&bytes)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        truncated |= contents.truncated;
        for record in contents.records {
            let ValueFileRecord::CapturedValue(record) = record else {
                continue;
            };
            let Some(capture) = &record.capture else {
                continue;
            };
            let (role, kind) = role_of(capture.kind);
            let thread_id = capture.call.thread_id.0;
            let call_id = capture.call.call_id.0;
            // (1) capture-carried id, (2) raw-firehose join, (3) none.
            let fn_name = if capture.function_id != 0 {
                Some(resolve_fid(capture.function_id))
            } else {
                call_fns
                    .as_ref()
                    .and_then(|m| m.get(&(thread_id, call_id)))
                    .map(|fid| resolve_fid(*fid))
            };
            let body = if let Some(dag) = &record.dag_ref {
                BodySource::Dag(dag.clone())
            } else if !record.body.is_empty() {
                BodySource::Inline(record.body.clone())
            } else if let Some(blob) = &record.blob_ref {
                BodySource::Blob(blob.clone())
            } else {
                BodySource::None
            };
            rows.push(ValueRow {
                value_id: record.value_ref.id.clone(),
                role,
                kind,
                thread_id,
                call_id,
                fn_name,
                cid: record.dag_ref.as_ref().map(|d| d.root_cid),
                original_bytes: record.value_ref.original_size_bytes.map_or(0, |n| n as u64),
                promoted_by: record.promoted_by.clone(),
                body,
            });
        }
    }
    // Exact when an exact source covers the run: every row resolved a name
    // (capture-carried ids and/or the raw join), or the raw firehose exists
    // (the pre-id behavior — raw is the run's full call record). Otherwise
    // the fn column is honestly incomplete.
    let fn_join =
        if call_fns.is_some() || (!rows.is_empty() && rows.iter().all(|r| r.fn_name.is_some())) {
            FnJoin::Exact
        } else {
            FnJoin::NoExactSource
        };
    Ok(RunValues {
        rows,
        fn_join,
        truncated,
    })
}

// ---------------------------------------------------------------------------
// Hydration (§8.4 `get()`): byte- and depth-budgeted, elision-honest.
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub struct Hydrated {
    pub json: serde_json::Value,
    /// Budget/depth elided subtrees (`bamlv_1_…` handles for descent).
    pub elided: Vec<String>,
    pub bytes_spent: usize,
}

#[derive(Debug)]
pub enum HydrateError {
    /// No CAS store / no body — the record retained nothing readable.
    Unavailable(&'static str),
    /// §8.4 fail closed: partial bytes are never decoded as whole.
    MissingBlock(String),
    Malformed(String),
}

impl std::fmt::Display for HydrateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HydrateError::Unavailable(what) => write!(f, "value unavailable: {what}"),
            HydrateError::MissingBlock(what) => write!(f, "missing block: {what}"),
            HydrateError::Malformed(what) => write!(f, "malformed value: {what}"),
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
struct StoreSource<'a>(&'a Store);

#[cfg(not(target_arch = "wasm32"))]
impl canon::DagSource for StoreSource<'_> {
    fn node(&mut self, cid: &[u8; 32]) -> Option<Vec<u8>> {
        self.0.get(cid).ok().flatten()
    }
    fn chunk(&mut self, cid: &[u8; 32]) -> Option<Vec<u8>> {
        self.0.get(cid).ok().flatten()
    }
}

impl ValueRow {
    /// True when some body (DAG, inline, or blob) is retained.
    #[must_use]
    pub fn has_body(&self) -> bool {
        !matches!(self.body, BodySource::None)
    }

    /// Hydrate this value to schema-erased JSON within `max_bytes` /
    /// `max_depth`. Preference order: canonical DAG (dedup plane) →
    /// inline body → blob.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn hydrate(
        &self,
        store: Option<&Store>,
        run_dir: &Path,
        max_bytes: usize,
        max_depth: u32,
    ) -> Result<Hydrated, HydrateError> {
        if let (BodySource::Dag(dag), Some(store)) = (&self.body, store) {
            use canon::DagSource as _;
            let mut src = StoreSource(store);
            let Some(root) = src.node(&dag.root_cid) else {
                // The CAS lost the root (sweep raced retention?) — fall
                // through to the legacy body rather than failing.
                return self.hydrate_legacy(run_dir, max_bytes);
            };
            let mut budget = canon::DecodeBudget::bounded(max_bytes, max_depth);
            budget.spent = root.len();
            return match canon::decode_budgeted(&root, &mut src, &mut budget) {
                Ok(value) => Ok(Hydrated {
                    json: canon::to_json(&value),
                    elided: budget.elided.iter().map(canon::cid_wire).collect(),
                    bytes_spent: budget.spent,
                }),
                Err(canon::DecodeError::MissingNode(cid))
                | Err(canon::DecodeError::MissingChunk(cid)) => {
                    Err(HydrateError::MissingBlock(canon::cid_wire(&cid)))
                }
                Err(canon::DecodeError::Malformed(what)) => {
                    Err(HydrateError::Malformed(what.to_string()))
                }
            };
        }
        self.hydrate_legacy(run_dir, max_bytes)
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn hydrate_legacy(&self, run_dir: &Path, max_bytes: usize) -> Result<Hydrated, HydrateError> {
        let bytes: Vec<u8> = match &self.body {
            BodySource::Inline(bytes) => bytes.clone(),
            BodySource::Blob(blob) => {
                let store = bex_events::value::BlobStore::for_boundary_dir(run_dir);
                store
                    .read_blob(blob)
                    .map_err(|e| HydrateError::MissingBlock(e.to_string()))?
                    .ok_or_else(|| HydrateError::MissingBlock(blob.digest.clone()))?
            }
            BodySource::Dag(_) => {
                return Err(HydrateError::Unavailable(
                    "canonical DAG body but no readable value store",
                ));
            }
            BodySource::None => return Err(HydrateError::Unavailable("nothing retained")),
        };
        if bytes.len() > max_bytes {
            return Ok(Hydrated {
                json: serde_json::json!({
                    "$elided": format!("inline body of {} bytes exceeds the byte budget", bytes.len()),
                }),
                elided: Vec::new(),
                bytes_spent: 0,
            });
        }
        let spent = bytes.len();
        let json =
            outbound::decode_to_json(&bytes).map_err(|e| HydrateError::Malformed(e.to_string()))?;
        Ok(Hydrated {
            json,
            elided: Vec::new(),
            bytes_spent: spent,
        })
    }
}

/// Decode a legacy inline `BamlOutboundValue` body to schema-erased JSON
/// (public for the local query provider's legacy fallback path).
#[must_use]
pub fn decode_legacy_body_json(bytes: &[u8]) -> Option<serde_json::Value> {
    outbound::decode_to_json(bytes).ok()
}

/// Minimal read-only mirror of `baml_bridge.cffi.v1.BamlOutboundValue` —
/// the legacy inline body codec. Variants the trace encoder never produces
/// (literal/union/handle/prompt-ast/ty) decode as unknown fields and
/// render as `{"$unsupported": …}`.
mod outbound {
    use prost::Message;

    #[derive(Clone, PartialEq, Message)]
    pub struct Value {
        #[prost(oneof = "Variant", tags = "2, 3, 4, 5, 6, 7, 8, 11, 12, 17, 19, 20")]
        pub value: Option<Variant>,
    }

    #[derive(Clone, PartialEq, prost::Oneof)]
    pub enum Variant {
        #[prost(message, tag = "2")]
        Null(Null),
        #[prost(string, tag = "3")]
        String(String),
        #[prost(int64, tag = "4")]
        Int(i64),
        #[prost(double, tag = "5")]
        Float(f64),
        #[prost(bool, tag = "6")]
        Bool(bool),
        #[prost(message, tag = "7")]
        Class(Class),
        #[prost(message, tag = "8")]
        Enum(Enum),
        #[prost(message, tag = "11")]
        List(List),
        #[prost(message, tag = "12")]
        Map(Map),
        #[prost(message, tag = "17")]
        Media(Media),
        #[prost(bytes, tag = "19")]
        Uint8Array(Vec<u8>),
        #[prost(string, tag = "20")]
        Bigint(String),
    }

    #[derive(Clone, PartialEq, Message)]
    pub struct Null {}

    #[derive(Clone, PartialEq, Message)]
    pub struct MapEntry {
        #[prost(string, tag = "1")]
        pub key: String,
        #[prost(message, optional, boxed, tag = "2")]
        pub value: Option<Box<Value>>,
    }

    #[derive(Clone, PartialEq, Message)]
    pub struct List {
        #[prost(message, repeated, tag = "2")]
        pub items: Vec<Value>,
    }

    #[derive(Clone, PartialEq, Message)]
    pub struct Map {
        #[prost(message, repeated, tag = "3")]
        pub entries: Vec<MapEntry>,
    }

    #[derive(Clone, PartialEq, Message)]
    pub struct Class {
        #[prost(string, tag = "1")]
        pub name: String,
        #[prost(message, repeated, tag = "2")]
        pub fields: Vec<MapEntry>,
    }

    #[derive(Clone, PartialEq, Message)]
    pub struct Enum {
        #[prost(string, tag = "1")]
        pub name: String,
        #[prost(string, tag = "2")]
        pub value: String,
    }

    #[derive(Clone, PartialEq, Message)]
    pub struct Media {
        #[prost(enumeration = "MediaType", tag = "1")]
        pub media: i32,
        #[prost(string, optional, tag = "2")]
        pub mime_type: Option<String>,
        #[prost(oneof = "MediaContent", tags = "3, 4, 5")]
        pub value: Option<MediaContent>,
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq, prost::Enumeration)]
    #[repr(i32)]
    pub enum MediaType {
        Unspecified = 0,
        Image = 1,
        Audio = 2,
        Pdf = 3,
        Video = 4,
        Other = 5,
    }

    #[derive(Clone, PartialEq, prost::Oneof)]
    pub enum MediaContent {
        #[prost(string, tag = "3")]
        Url(String),
        #[prost(string, tag = "4")]
        Base64(String),
        #[prost(string, tag = "5")]
        File(String),
    }

    pub fn decode_to_json(bytes: &[u8]) -> Result<serde_json::Value, prost::DecodeError> {
        let value = Value::decode(bytes)?;
        Ok(to_json(&value))
    }

    fn to_json(value: &Value) -> serde_json::Value {
        use serde_json::json;
        match &value.value {
            None => json!({ "$unsupported": "variant outside the trace codec subset" }),
            Some(Variant::Null(_)) => serde_json::Value::Null,
            Some(Variant::String(s)) => json!(s),
            Some(Variant::Int(i)) => json!(i),
            Some(Variant::Float(f)) => {
                if f.is_finite() {
                    serde_json::Number::from_f64(*f)
                        .map_or_else(|| json!(f.to_string()), serde_json::Value::Number)
                } else {
                    json!(f.to_string())
                }
            }
            Some(Variant::Bool(b)) => json!(b),
            Some(Variant::Class(class)) => {
                let mut obj = serde_json::Map::new();
                obj.insert("$type".to_string(), json!(class.name));
                for field in &class.fields {
                    obj.insert(
                        field.key.clone(),
                        field
                            .value
                            .as_deref()
                            .map_or(serde_json::Value::Null, to_json),
                    );
                }
                serde_json::Value::Object(obj)
            }
            Some(Variant::Enum(e)) => json!({ "$enum": e.name, "value": e.value }),
            Some(Variant::List(list)) => {
                serde_json::Value::Array(list.items.iter().map(to_json).collect())
            }
            Some(Variant::Map(map)) => {
                let mut obj = serde_json::Map::new();
                for entry in &map.entries {
                    obj.insert(
                        entry.key.clone(),
                        entry
                            .value
                            .as_deref()
                            .map_or(serde_json::Value::Null, to_json),
                    );
                }
                serde_json::Value::Object(obj)
            }
            Some(Variant::Media(media)) => {
                let (content_kind, content) = match &media.value {
                    Some(MediaContent::Url(s)) => ("url", s.clone()),
                    Some(MediaContent::Base64(s)) => ("base64", s.clone()),
                    Some(MediaContent::File(s)) => ("file", s.clone()),
                    None => ("unknown", String::new()),
                };
                let kind = match MediaType::try_from(media.media) {
                    Ok(MediaType::Image) => "image",
                    Ok(MediaType::Audio) => "audio",
                    Ok(MediaType::Pdf) => "pdf",
                    Ok(MediaType::Video) => "video",
                    _ => "media",
                };
                json!({ "$media": {
                    "kind": kind,
                    "mime": media.mime_type,
                    "content_kind": content_kind,
                    "content": content,
                }})
            }
            Some(Variant::Uint8Array(bytes)) => {
                use base64::Engine as _;
                json!({ "$bytes": {
                    "len": bytes.len(),
                    "base64": base64::engine::general_purpose::STANDARD.encode(bytes),
                }})
            }
            Some(Variant::Bigint(s)) => json!({ "$bigint": s }),
        }
    }
}
