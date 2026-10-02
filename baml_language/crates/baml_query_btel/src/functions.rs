//! SQLite functions registered on every query connection.
//!
//! Identity and timing functions are pure. Value functions resolve CAS
//! blobs lazily through the current query's context: a query that never
//! evaluates `type_args`, `input_args`, `output_value`, `error_value` or
//! `network_event_values` reads no blob. Within one query,
//! hydration outcomes (including missing and corrupt blobs) are memoized
//! under entry and byte limits; the next query starts empty and can find a
//! blob that arrived in between.
use std::{
    borrow::Cow,
    collections::{BTreeMap, HashMap, HashSet},
    sync::{Arc, Mutex},
    time::Instant,
};

use btel_reader::{
    cas::{CasOutcome, CasStore},
    evidence::ArgumentNames,
    timing::{Clock, Conversion, EpochStatus, TimingState, UtcAnchor, format_unix_ns},
    value::{
        self, CmpOp, Kind, Nav, Operand, RenderLimits, Root, Scalar, Segment, Unavailable, equality,
    },
};
use btel_snapshot::{
    DecodedObject, DecodedRoot, DecodedSnapshot, DecodedValue, Limit, SnapshotId, TypeDescription,
};
use rusqlite::{
    Connection, Error as SqlError,
    functions::{Aggregate, Context, FunctionFlags},
    types::{Null, Value as SqlValue, ValueRef},
};
use serde::Serialize;

/// Function names are reserved: user SQL containing this prefix is rejected
/// before translation, so only the translator and catalog views call them.
pub const PREFIX: &str = "__btel_";

#[derive(Clone, Copy, Debug)]
pub struct ValueLimits {
    /// Decoded snapshots kept for the rest of the query.
    pub max_cached_blobs: usize,
    /// Approximate encoded bytes of cached snapshots.
    pub max_cached_bytes: u64,
    /// Navigation results kept for render/kind pairs and repeated paths.
    pub max_cached_results: usize,
    pub render: RenderLimits,
    pub comparison: equality::Limits,
}
impl Default for ValueLimits {
    fn default() -> Self {
        Self {
            max_cached_blobs: 4096,
            max_cached_bytes: 256 << 20,
            max_cached_results: 16_384,
            render: RenderLimits::default(),
            comparison: equality::Limits::default(),
        }
    }
}

/// Work done by value functions during one query.
#[derive(Clone, Debug, Default, Serialize)]
pub struct ValueMetrics {
    pub value_evaluations: u64,
    pub cas_loads: u64,
    pub cas_bytes_read: u64,
    pub cas_cache_hits: u64,
    pub result_cache_hits: u64,
    /// Distinct unavailable values by diagnostic code.
    pub unavailable: BTreeMap<String, u64>,
}

struct CacheState {
    blobs: HashMap<[u8; 16], CasOutcome>,
    blob_bytes: u64,
    results: HashMap<Vec<u8>, (Scalar, Kind)>,
    unavailable_seen: HashSet<Vec<u8>>,
    metrics: ValueMetrics,
}

/// One query's value-resolution state.
pub struct QueryContext {
    cas: CasStore,
    limits: ValueLimits,
    deadline: Option<Instant>,
    state: Mutex<CacheState>,
}

impl QueryContext {
    pub fn new(cas: CasStore, limits: ValueLimits, deadline: Option<Instant>) -> Self {
        Self {
            cas,
            limits,
            deadline,
            state: Mutex::new(CacheState {
                blobs: HashMap::new(),
                blob_bytes: 0,
                results: HashMap::new(),
                unavailable_seen: HashSet::new(),
                metrics: ValueMetrics::default(),
            }),
        }
    }

    pub fn metrics(&self) -> ValueMetrics {
        self.state.lock().expect("value state").metrics.clone()
    }

    fn check_deadline(&self) -> rusqlite::Result<()> {
        if self
            .deadline
            .is_some_and(|deadline| Instant::now() >= deadline)
        {
            return Err(user_error("query time budget exceeded"));
        }
        Ok(())
    }

    fn load(&self, state: &mut CacheState, id: [u8; 16]) -> CasOutcome {
        if let Some(outcome) = state.blobs.get(&id) {
            state.metrics.cas_cache_hits += 1;
            return outcome.clone();
        }
        let load = self.cas.load(SnapshotId::from_bytes(id));
        state.metrics.cas_loads += 1;
        state.metrics.cas_bytes_read += load.bytes_read;
        if state.blobs.len() < self.limits.max_cached_blobs
            && state.blob_bytes + load.bytes_read <= self.limits.max_cached_bytes
        {
            state.blob_bytes += load.bytes_read;
            state.blobs.insert(id, load.outcome.clone());
        }
        load.outcome
    }

    /// The snapshot a handle reads and the path to follow in it, or why it
    /// is unavailable: its CAS blob or the inline one at its own path, or
    /// for a network span's events, what `events` finds.
    fn load_handle<'h>(&self, state: &mut CacheState, handle: &'h Handle) -> Loaded<'h> {
        let outcome = match &handle.inline {
            _ if handle.kind == EVENTS => return self.events(state, handle),
            Some(blob) => match btel_snapshot::decode_blob(blob, &self.cas.limits().decode) {
                Ok(snapshot) => CasOutcome::Available(Arc::new(snapshot)),
                Err(error) => CasOutcome::Corrupt(error),
            },
            None => self.load(state, handle.cas),
        };
        match outcome {
            CasOutcome::Available(snapshot) => Ok((snapshot, Cow::Borrowed(&handle.path))),
            outcome => Err(outcome.code()),
        }
    }

    /// A network span's events as `[{event_name, payload, timestamp}]`. A
    /// path into one event's payload reads only that payload's blob. Any
    /// other path reads a list built here, with the payloads the path ends
    /// on (every one for the whole list, its own for one event) and null
    /// for the rest, which the path never reaches.
    fn events<'h>(&self, state: &mut CacheState, handle: &'h Handle) -> Loaded<'h> {
        let events = handle
            .inline
            .as_deref()
            .and_then(decode_events)
            .expect("Handle::decode validated the events");
        let mut path = handle.path.clone();
        // A negative index counts from the end.
        if let Some(Segment::Index(index)) = path.first_mut()
            && *index < 0
        {
            *index = index.saturating_add(i64::try_from(events.len()).unwrap_or(i64::MAX));
        }
        let at = match path.first() {
            Some(Segment::Index(index)) => {
                usize::try_from(*index).ok().filter(|at| *at < events.len())
            }
            _ => None,
        };
        if let (Some(at), Some(Segment::Key(key))) = (at, path.get(1))
            && key == "payload"
        {
            let rest = path.split_off(2);
            return match events[at].payload {
                Some(cas) => match self.load(state, cas) {
                    CasOutcome::Available(snapshot) => Ok((snapshot, Cow::Owned(rest))),
                    outcome => Err(outcome.code()),
                },
                None => Ok((
                    Arc::new(Composer::default().finish(DecodedValue::Null)),
                    Cow::Owned(rest),
                )),
            };
        }
        let shown = |event: usize| path.is_empty() || (path.len() == 1 && at == Some(event));
        let mut composer = Composer::default();
        let mut items = Vec::with_capacity(events.len());
        for (position, event) in events.iter().enumerate() {
            let payload = match event.payload {
                Some(cas) if shown(position) => match self.load(state, cas) {
                    CasOutcome::Available(snapshot) => composer.embed(&snapshot)?,
                    outcome => return Err(outcome.code()),
                },
                _ => DecodedValue::Null,
            };
            let timestamp = event
                .timestamp
                .as_deref()
                .map_or(DecodedValue::Null, |text| DecodedValue::String(text.into()));
            items.push(composer.map(vec![
                (
                    "event_name",
                    DecodedValue::String(event.name.as_str().into()),
                ),
                ("payload", payload),
                ("timestamp", timestamp),
            ])?);
        }
        let list = composer.list(items)?;
        Ok((Arc::new(composer.finish(list)), Cow::Owned(path)))
    }

    /// Report `code` once per distinct `key` (a handle, or a handle plus an
    /// operation marker).
    fn note(&self, key: &[u8], code: &str) {
        let mut state = self.state.lock().expect("value state");
        Self::unavailable(&mut state, key, code);
    }

    fn unavailable(state: &mut CacheState, handle: &[u8], code: &str) {
        if state.unavailable_seen.insert(handle.to_vec()) {
            *state
                .metrics
                .unavailable
                .entry(code.to_owned())
                .or_default() += 1;
        }
    }

    /// Navigate a handle and present the result as a SQL scalar.
    fn resolve(&self, handle: &[u8]) -> rusqlite::Result<(Scalar, Kind)> {
        self.check_deadline()?;
        let mut state = self.state.lock().expect("value state");
        state.metrics.value_evaluations += 1;
        if let Some(result) = state.results.get(handle) {
            let result = result.clone();
            state.metrics.result_cache_hits += 1;
            return Ok(result);
        }
        let parsed =
            Handle::decode(handle).ok_or_else(|| user_error("invalid BAML value handle"))?;
        if parsed.pending {
            Self::unavailable(&mut state, handle, parsed.pending_code());
            return Ok((Scalar::Null, Kind::Unavailable));
        }
        let result = match self.load_handle(&mut state, &parsed) {
            Ok((snapshot, path)) => {
                let nav = value::navigate(&snapshot, parsed.root(), parsed.names.as_ref(), &path);
                if let Nav::Unavailable(reason) = nav {
                    Self::unavailable(&mut state, handle, reason.code());
                }
                value::to_scalar(&snapshot, nav, parsed.names.as_ref(), &self.limits.render)
            }
            Err(code) => {
                Self::unavailable(&mut state, handle, code);
                (Scalar::Null, Kind::Unavailable)
            }
        };
        if state.results.len() < self.limits.max_cached_results {
            state.results.insert(handle.to_vec(), result.clone());
        }
        Ok(result)
    }

    /// Navigate and hand the result to `f` (comparisons need the leaf).
    fn with_nav<T>(
        &self,
        handle: &[u8],
        f: impl FnOnce(Nav<'_>) -> T,
    ) -> rusqlite::Result<Option<T>> {
        self.with_value(handle, |value| f(value.nav))
    }

    fn with_value<T>(
        &self,
        handle: &[u8],
        f: impl FnOnce(equality::Captured<'_>) -> T,
    ) -> rusqlite::Result<Option<T>> {
        self.check_deadline()?;
        let mut state = self.state.lock().expect("value state");
        state.metrics.value_evaluations += 1;
        let parsed =
            Handle::decode(handle).ok_or_else(|| user_error("invalid BAML value handle"))?;
        if parsed.pending {
            Self::unavailable(&mut state, handle, parsed.pending_code());
            return Ok(None);
        }
        match self.load_handle(&mut state, &parsed) {
            Ok((snapshot, path)) => {
                let nav = value::navigate(&snapshot, parsed.root(), parsed.names.as_ref(), &path);
                if let Nav::Unavailable(reason) = nav {
                    Self::unavailable(&mut state, handle, reason.code());
                    return Ok(None);
                }
                drop(state);
                Ok(Some(f(equality::Captured {
                    snapshot: &snapshot,
                    nav,
                    names: parsed.names.as_ref(),
                })))
            }
            Err(code) => {
                Self::unavailable(&mut state, handle, code);
                Ok(None)
            }
        }
    }

    /// `__btel_body_text`: the text of a recorded body, which is a whole
    /// body (a string), one server-sent event's `data`, or a request
    /// snapshot's `request.body`. NULL for anything else, such as bytes or a
    /// cut body. A blob that cannot be read is reported.
    fn body_text(&self, cas: [u8; 16]) -> rusqlite::Result<Option<String>> {
        self.check_deadline()?;
        let mut state = self.state.lock().expect("value state");
        state.metrics.value_evaluations += 1;
        let snapshot = match self.load(&mut state, cas) {
            CasOutcome::Available(snapshot) => snapshot,
            outcome => {
                let mut key = cas.to_vec();
                key.extend_from_slice(b"\xffbody");
                Self::unavailable(&mut state, &key, outcome.code());
                return Ok(None);
            }
        };
        let key = |key: &str| Segment::Key(key.to_owned());
        for path in [vec![], vec![key("data")], vec![key("request"), key("body")]] {
            if let Nav::Value(DecodedValue::String(text)) =
                value::navigate(&snapshot, Root::Value, None, &path)
            {
                return Ok(Some(text.to_string()));
            }
        }
        Ok(None)
    }

    /// `baml_value_state`: what the path finds, without reporting it as
    /// unavailable evidence (the caller asked for exactly this answer).
    fn state(&self, handle: &[u8]) -> rusqlite::Result<String> {
        self.check_deadline()?;
        let mut state = self.state.lock().expect("value state");
        state.metrics.value_evaluations += 1;
        let parsed =
            Handle::decode(handle).ok_or_else(|| user_error("invalid BAML value handle"))?;
        if parsed.pending {
            return Ok(parsed.pending_code().to_owned());
        }
        Ok(match self.load_handle(&mut state, &parsed) {
            Ok((snapshot, path)) => {
                value::state(&snapshot, parsed.root(), parsed.names.as_ref(), &path).to_owned()
            }
            Err(code) => code.to_owned(),
        })
    }
}

/// Unavailable code of a capture whose announcement is not indexed yet.
const PENDING: &str = "capture_pending";

/// Structured ordering and opaque-value equality are unsupported.
const COMPARISON_UNSUPPORTED: &str = "comparison_unsupported";

/// Compare two leaves; an unanswerable comparison involving a structured
/// value is reported rather than left as an unexplained NULL.
fn compare_reported(
    context: &QueryContext,
    handle: &[u8],
    left: value::Leaf<'_>,
    op: CmpOp,
    right: value::Leaf<'_>,
) -> Option<bool> {
    let result = value::compare(left, op, right);
    if result.is_none()
        && (matches!(left, value::Leaf::Structured) || matches!(right, value::Leaf::Structured))
    {
        let mut key = handle.to_vec();
        key.extend_from_slice(b"\xffcmp");
        context.note(&key, COMPARISON_UNSUPPORTED);
    }
    result
}

fn comparison_result(
    context: &QueryContext,
    left: &[u8],
    result: Option<Result<Option<bool>, equality::Error>>,
) -> Option<bool> {
    match result? {
        Ok(value) => value,
        Err(error) => {
            // One diagnostic per left value/reason, not per join pair or
            // JSON literal. Tracking every pair would grow quadratically.
            let mut key = left.to_vec();
            key.extend_from_slice(b"\xffwholecmp");
            key.extend_from_slice(error.code().as_bytes());
            context.note(&key, error.code());
            None
        }
    }
}

/// What a handle reads: a snapshot and the path to follow in it, or the
/// code of why it is unavailable.
type Loaded<'h> = Result<(Arc<DecodedSnapshot>, Cow<'h, [Segment]>), &'static str>;

/// A snapshot assembled in memory: values the index builds, around
/// payload graphs copied in from their own snapshots.
#[derive(Default)]
struct Composer {
    objects: Vec<DecodedObject>,
    limited: bool,
}

impl Composer {
    /// Reading never looks at a built container's types.
    fn untyped() -> TypeDescription {
        TypeDescription {
            encoded: Box::new([]),
            decoded: None,
        }
    }

    fn push(&mut self, object: DecodedObject) -> Result<DecodedValue, &'static str> {
        let id = u32::try_from(self.objects.len())
            .map_err(|_| Unavailable::Truncated(Limit::Objects).code())?;
        self.objects.push(object);
        Ok(DecodedValue::Object(id))
    }

    fn map(&mut self, entries: Vec<(&str, DecodedValue)>) -> Result<DecodedValue, &'static str> {
        let original_len = entries.len() as u64;
        self.push(DecodedObject::Map {
            key_type: Self::untyped(),
            value_type: Self::untyped(),
            entries: entries
                .into_iter()
                .map(|(key, value)| (key.into(), value))
                .collect(),
            original_len,
        })
    }

    fn list(&mut self, items: Vec<DecodedValue>) -> Result<DecodedValue, &'static str> {
        let original_len = items.len() as u64;
        self.push(DecodedObject::List {
            element_type: Self::untyped(),
            items,
            original_len,
        })
    }

    /// Copy a value snapshot's graph in, renumbering its objects after the
    /// ones already here; returns its root.
    fn embed(&mut self, snapshot: &DecodedSnapshot) -> Result<DecodedValue, &'static str> {
        let DecodedRoot::Value(root) = &snapshot.root else {
            return Err(Unavailable::WrongRoot.code());
        };
        let too_many = Unavailable::Truncated(Limit::Objects).code();
        let offset = u32::try_from(self.objects.len()).map_err(|_| too_many)?;
        u32::try_from(self.objects.len() + snapshot.objects.len()).map_err(|_| too_many)?;
        let shift = |value: &DecodedValue| match value {
            DecodedValue::Object(id) => DecodedValue::Object(id + offset),
            DecodedValue::Enum {
                declaration,
                variant,
                name,
            } => DecodedValue::Enum {
                declaration: declaration + offset,
                variant: *variant,
                name: name.clone(),
            },
            other => other.clone(),
        };
        let entries = |entries: &btel_snapshot::Entries| -> btel_snapshot::Entries {
            entries
                .iter()
                .map(|(key, value)| (key.clone(), shift(value)))
                .collect()
        };
        for object in &snapshot.objects {
            self.objects.push(match object {
                DecodedObject::List {
                    element_type,
                    items,
                    original_len,
                } => DecodedObject::List {
                    element_type: element_type.clone(),
                    items: items.iter().map(shift).collect(),
                    original_len: *original_len,
                },
                DecodedObject::Map {
                    key_type,
                    value_type,
                    entries: map,
                    original_len,
                } => DecodedObject::Map {
                    key_type: key_type.clone(),
                    value_type: value_type.clone(),
                    entries: entries(map),
                    original_len: *original_len,
                },
                DecodedObject::Instance {
                    type_arguments,
                    declaration,
                    fields,
                    original_len,
                } => DecodedObject::Instance {
                    type_arguments: type_arguments.clone(),
                    declaration: declaration + offset,
                    fields: entries(fields),
                    original_len: *original_len,
                },
                DecodedObject::Cell(value) => DecodedObject::Cell(shift(value)),
                other => other.clone(),
            });
        }
        self.limited |= snapshot.limited;
        Ok(shift(root))
    }

    fn finish(self, root: DecodedValue) -> DecodedSnapshot {
        DecodedSnapshot {
            id: SnapshotId::from_bytes([0; 16]),
            limited: self.limited,
            root: DecodedRoot::Value(root),
            objects: self.objects,
        }
    }
}

/// Where the functions find the current query's context.
pub type ContextSlot = Arc<Mutex<Option<Arc<QueryContext>>>>;

fn current(slot: &ContextSlot) -> rusqlite::Result<Arc<QueryContext>> {
    slot.lock()
        .expect("context slot")
        .clone()
        .ok_or_else(|| user_error("BAML value functions are only available inside a query"))
}

fn user_error(message: &str) -> SqlError {
    SqlError::UserFunctionError(message.into())
}

/// A lazily resolved BAML value: which capture, which root, which path.
/// Encoded as a BLOB so it flows through views, joins and subqueries.
#[derive(Clone, Debug, PartialEq)]
pub struct Handle {
    /// 1 inputs, 2 output, 3 error, 4 inline, 5 context, 6 invalid context,
    /// 7 a network span's events.
    pub kind: u8,
    /// Evidence is unavailable without a CAS reference (pending inputs or
    /// unavailable/invalid context); `cas` is unused.
    pub pending: bool,
    pub cas: [u8; 16],
    /// Kind 4: the encoded snapshot itself; kind 7: the encoded events
    /// (`encode_events`). `cas` is unused.
    pub inline: Option<Vec<u8>>,
    /// Argument slot names; `None` when not recorded.
    pub names: Option<ArgumentNames>,
    pub path: Vec<Segment>,
}

const HANDLE_MAGIC: [u8; 2] = [0xB7, 0x01];
const PENDING_BIT: u8 = 0x80;
const INLINE: u8 = 4;
const EVENTS: u8 = 7;

impl Handle {
    fn pending_code(&self) -> &'static str {
        match self.kind {
            5 => "context_unavailable",
            6 => "context_invalid",
            _ => PENDING,
        }
    }

    fn root(&self) -> Root {
        if self.kind == 1 {
            Root::Arguments
        } else {
            Root::Value
        }
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(32);
        out.extend_from_slice(&HANDLE_MAGIC);
        out.push(self.kind | if self.pending { PENDING_BIT } else { 0 });
        match &self.inline {
            Some(blob) => {
                out.extend_from_slice(&component_len(blob.len()).to_le_bytes());
                out.extend_from_slice(blob);
            }
            None => out.extend_from_slice(&self.cas),
        }
        match &self.names {
            Some(names) => {
                let names = names.encode();
                out.push(1);
                out.extend_from_slice(&component_len(names.len()).to_le_bytes());
                out.extend_from_slice(&names);
            }
            None => out.push(0),
        }
        for segment in &self.path {
            push_segment(&mut out, segment);
        }
        out
    }

    pub fn decode(bytes: &[u8]) -> Option<Self> {
        let rest = bytes.strip_prefix(&HANDLE_MAGIC)?;
        let (&kind, rest) = rest.split_first()?;
        let (kind, pending) = (kind & !PENDING_BIT, kind & PENDING_BIT != 0);
        let (cas, inline, rest): (&[u8], _, _) = if kind == INLINE || kind == EVENTS {
            let (len, tail) = rest.split_at_checked(4)?;
            let len = u32::from_le_bytes(len.try_into().ok()?) as usize;
            let (blob, tail) = tail.split_at_checked(len)?;
            if kind == EVENTS {
                decode_events(blob)?;
            }
            (&[0; 16], Some(blob.to_vec()), tail)
        } else {
            let (cas, tail) = rest.split_at_checked(16)?;
            (cas, None, tail)
        };
        let (&has_names, mut rest) = rest.split_first()?;
        let names = if has_names == 1 {
            let (len, tail) = rest.split_at_checked(4)?;
            let len = u32::from_le_bytes(len.try_into().ok()?) as usize;
            let (names, tail) = tail.split_at_checked(len)?;
            rest = tail;
            Some(ArgumentNames::decode(names)?)
        } else {
            None
        };
        let mut path = Vec::new();
        while let Some((&tag, tail)) = rest.split_first() {
            match tag {
                0 => {
                    let (len, tail) = tail.split_at_checked(4)?;
                    let len = u32::from_le_bytes(len.try_into().ok()?) as usize;
                    let (key, tail) = tail.split_at_checked(len)?;
                    path.push(Segment::Key(String::from_utf8(key.to_vec()).ok()?));
                    rest = tail;
                }
                1 => {
                    let (index, tail) = tail.split_at_checked(8)?;
                    path.push(Segment::Index(i64::from_le_bytes(index.try_into().ok()?)));
                    rest = tail;
                }
                _ => return None,
            }
        }
        Some(Self {
            kind,
            pending,
            cas: cas.try_into().ok()?,
            inline,
            names,
            path,
        })
    }
}

/// Handle components come from recorded names and SQL literals, far below
/// 4 GiB.
fn component_len(len: usize) -> u32 {
    u32::try_from(len).expect("handle component fits u32")
}

fn push_segment(out: &mut Vec<u8>, segment: &Segment) {
    match segment {
        Segment::Key(key) => {
            out.push(0);
            out.extend_from_slice(&component_len(key.len()).to_le_bytes());
            out.extend_from_slice(key.as_bytes());
        }
        Segment::Index(index) => {
            out.push(1);
            out.extend_from_slice(&index.to_le_bytes());
        }
    }
}

/// One event of a network span, as an EVENTS handle carries it: the
/// payload stays a CAS ID until a query reads it.
#[derive(Clone, Debug, PartialEq)]
struct Event {
    name: String,
    timestamp: Option<String>,
    payload: Option<[u8; 16]>,
}

const EVENT_HAS_TIMESTAMP: u8 = 1;
const EVENT_HAS_PAYLOAD: u8 = 1 << 1;

/// Per event: the name, a flags byte, then the timestamp and payload ID
/// the flags say are present.
fn encode_events(events: &[Event]) -> Vec<u8> {
    let mut out = Vec::new();
    let text = |out: &mut Vec<u8>, text: &str| {
        out.extend_from_slice(&component_len(text.len()).to_le_bytes());
        out.extend_from_slice(text.as_bytes());
    };
    for event in events {
        text(&mut out, &event.name);
        let mut flags = 0;
        if event.timestamp.is_some() {
            flags |= EVENT_HAS_TIMESTAMP;
        }
        if event.payload.is_some() {
            flags |= EVENT_HAS_PAYLOAD;
        }
        out.push(flags);
        if let Some(timestamp) = &event.timestamp {
            text(&mut out, timestamp);
        }
        if let Some(payload) = &event.payload {
            out.extend_from_slice(payload);
        }
    }
    out
}

fn decode_events(mut bytes: &[u8]) -> Option<Vec<Event>> {
    fn text(bytes: &mut &[u8]) -> Option<String> {
        let (len, tail) = bytes.split_at_checked(4)?;
        let len = u32::from_le_bytes(len.try_into().ok()?) as usize;
        let (text, tail) = tail.split_at_checked(len)?;
        *bytes = tail;
        String::from_utf8(text.to_vec()).ok()
    }
    let mut events = Vec::new();
    while !bytes.is_empty() {
        let name = text(&mut bytes)?;
        let (&flags, tail) = bytes.split_first()?;
        bytes = tail;
        let timestamp = if flags & EVENT_HAS_TIMESTAMP != 0 {
            Some(text(&mut bytes)?)
        } else {
            None
        };
        let payload = if flags & EVENT_HAS_PAYLOAD != 0 {
            let (payload, tail) = bytes.split_at_checked(16)?;
            bytes = tail;
            Some(payload.try_into().ok()?)
        } else {
            None
        };
        events.push(Event {
            name,
            timestamp,
            payload,
        });
    }
    Some(events)
}

fn scalar_to_sql(scalar: Scalar) -> SqlValue {
    match scalar {
        Scalar::Null => SqlValue::Null,
        Scalar::Integer(n) => SqlValue::Integer(n),
        Scalar::Real(f) => SqlValue::Real(f),
        Scalar::Text(s) => SqlValue::Text(s),
    }
}

fn opt_i64(ctx: &Context<'_>, index: usize) -> rusqlite::Result<Option<i64>> {
    match ctx.get_raw(index) {
        ValueRef::Null => Ok(None),
        ValueRef::Integer(n) => Ok(Some(n)),
        _ => Err(user_error("expected an integer")),
    }
}

fn opt_blob<'a>(ctx: &'a Context<'_>, index: usize) -> rusqlite::Result<Option<&'a [u8]>> {
    match ctx.get_raw(index) {
        ValueRef::Null => Ok(None),
        ValueRef::Blob(bytes) => Ok(Some(bytes)),
        _ => Err(user_error("expected a blob")),
    }
}

/// Stored quantities are non-negative i64 by construction.
fn ticks_u64(value: i64) -> u64 {
    u64::try_from(value).unwrap_or(0)
}

/// `<recording hex>:<prefix><n>`, as `__btel_pubid` and `__btel_sid` write
/// it: `(recording id, n)`.
fn public_id(value: ValueRef<'_>, prefix: &str) -> Option<(Vec<u8>, u64)> {
    let ValueRef::Text(text) = value else {
        return None;
    };
    let (recording, rest) = std::str::from_utf8(text).ok()?.split_once(':')?;
    let id = rest.strip_prefix(prefix)?.parse().ok()?;
    if recording.len() % 2 != 0 {
        return None;
    }
    let recording = (0..recording.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(recording.get(i..i + 2)?, 16).ok())
        .collect::<Option<Vec<u8>>>()?;
    Some((recording, id))
}

fn u64_from_blob(bytes: &[u8]) -> Option<u64> {
    Some(u64::from_be_bytes(bytes.try_into().ok()?))
}

fn hex(bytes: &[u8]) -> String {
    btel_reader::discovery::hex(bytes)
}

/// Clock facts from view columns: conversion (NULL multiplier means the
/// definition exists but is unrepresentable) and the observed status code.
fn clock(multiplier: Option<i64>, shift: Option<i64>, status: Option<i64>) -> Clock {
    Clock {
        conversion: multiplier.zip(shift).and_then(|(multiplier, shift)| {
            Some(Conversion {
                multiplier: u64::try_from(multiplier).ok()?,
                shift: u32::try_from(shift).ok()?,
            })
        }),
        status: status.and_then(EpochStatus::from_code),
    }
}

/// `(start, end, multiplier, shift, status, epoch)` where `epoch` is 0 for
/// no usable definition, 1 for one definition, 2 for conflicting ones.
fn interval(ctx: &Context<'_>) -> rusqlite::Result<Result<u64, TimingState>> {
    let start = opt_i64(ctx, 0)?.and_then(|v| u64::try_from(v).ok());
    let end = opt_i64(ctx, 1)?.and_then(|v| u64::try_from(v).ok());
    let clock = match clock_at(ctx, 2)? {
        Ok(clock) => clock,
        Err(state) if start.is_some() && end.is_some() => return Ok(Err(state)),
        Err(_) => return Ok(Err(TimingState::Incomplete)),
    };
    Ok(clock.interval_ns(start, end))
}

/// Checked i64 sum: NULL if any input is NULL (an overflowed stored total)
/// or if the sum overflows. Never a float, never a silent wrap.
struct CheckedSum;
impl Aggregate<Option<i64>, Option<i64>> for CheckedSum {
    fn init(&self, _: &mut Context<'_>) -> rusqlite::Result<Option<i64>> {
        Ok(Some(0))
    }
    fn step(&self, ctx: &mut Context<'_>, acc: &mut Option<i64>) -> rusqlite::Result<()> {
        let value = opt_i64(ctx, 0)?;
        *acc = acc.zip(value).and_then(|(a, b)| a.checked_add(b));
        Ok(())
    }
    fn finalize(
        &self,
        _: &mut Context<'_>,
        acc: Option<Option<i64>>,
    ) -> rusqlite::Result<Option<i64>> {
        Ok(acc.unwrap_or(Some(0)))
    }
}

/// Register every internal function on `conn`.
pub fn register(conn: &Connection, slot: &ContextSlot) -> rusqlite::Result<()> {
    crate::outcomes::register(conn)?;
    let pure = FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC;
    // Value functions read files, so they are not deterministic across queries.
    let io = FunctionFlags::SQLITE_UTF8;

    conn.create_scalar_function("__btel_context_metadata", 3, pure, |ctx| {
        if opt_i64(ctx, 0)? == Some(1) {
            return Ok(inline_handle(&Inline::Map(Vec::new())));
        }
        let invalid = opt_i64(ctx, 0)? == Some(3) || opt_i64(ctx, 2)? == Some(2);
        let cas = if invalid { None } else { opt_blob(ctx, 1)? };
        Ok(Handle {
            kind: if invalid { 6 } else { 5 },
            pending: cas.is_none(),
            cas: cas
                .unwrap_or(&[0; 16])
                .try_into()
                .map_err(|_| user_error("invalid context CAS id"))?,
            inline: None,
            names: None,
            path: vec![Segment::Key("metadata".into())],
        }
        .encode())
    })?;
    conn.create_scalar_function("__btel_u64", 1, pure, |ctx| {
        Ok(opt_blob(ctx, 0)?
            .and_then(u64_from_blob)
            .map(|v| v.to_string()))
    })?;
    conn.create_scalar_function("__btel_pubid", 2, pure, |ctx| {
        let (Some(recording), Some(id)) = (opt_blob(ctx, 0)?, opt_blob(ctx, 1)?) else {
            return Ok(None);
        };
        Ok(u64_from_blob(id).map(|id| format!("{}:{id}", hex(recording))))
    })?;
    conn.create_scalar_function("__btel_duration", 6, pure, |ctx| {
        Ok(interval(ctx)?.ok().and_then(|ns| i64::try_from(ns).ok()))
    })?;
    conn.create_scalar_function("__btel_timing", 6, pure, |ctx| {
        Ok(match interval(ctx)? {
            Ok(ns) if i64::try_from(ns).is_ok() => TimingState::Valid.label(),
            Ok(_) => TimingState::Overflow.label(),
            Err(state) => state.label(),
        })
    })?;
    // `(ticks, multiplier, shift, status, epoch)` for accumulated totals.
    conn.create_scalar_function("__btel_total_ns", 5, pure, |ctx| {
        Ok(total_ns(clock_at(ctx, 1)?, opt_i64(ctx, 0)?))
    })?;
    conn.create_scalar_function("__btel_total_timing", 5, pure, |ctx| {
        let ticks = opt_i64(ctx, 0)?.and_then(|v| u64::try_from(v).ok());
        let clock = match clock_at(ctx, 1)? {
            Ok(clock) => clock,
            Err(state) => return Ok(state.label()),
        };
        Ok(match clock.total_ns(ticks) {
            Ok(ns) if i64::try_from(ns).is_ok() => TimingState::Valid.label(),
            Ok(_) => TimingState::Overflow.label(),
            Err(state) => state.label(),
        })
    })?;
    conn.create_scalar_function("__btel_utc", 5, pure, |ctx| {
        let (Some(ticks), Some(anchor_ticks), Some(anchor_ns), Some(multiplier), Some(shift)) = (
            opt_i64(ctx, 0)?,
            opt_i64(ctx, 1)?,
            opt_i64(ctx, 2)?,
            opt_i64(ctx, 3)?,
            opt_i64(ctx, 4)?,
        ) else {
            return Ok(None);
        };
        let conversion = Conversion {
            multiplier: ticks_u64(multiplier),
            shift: u32::try_from(shift).unwrap_or(u32::MAX),
        };
        let anchor = UtcAnchor {
            ticks: ticks_u64(anchor_ticks),
            unix_ns: i128::from(anchor_ns),
        };
        Ok(anchor
            .unix_ns_at(conversion, ticks_u64(ticks))
            .and_then(format_unix_ns))
    })?;
    conn.create_aggregate_function("__btel_sum", 1, pure, CheckedSum)?;

    // Handle construction reads nothing.
    // `__btel_ref(kind, cas, names, pending)`: no CAS id is NULL (nothing
    // captured) unless `pending` says the capture's evidence is still due.
    conn.create_scalar_function("__btel_ref", 4, pure, |ctx| {
        let kind = opt_i64(ctx, 0)?.unwrap_or(0);
        let pending = opt_i64(ctx, 3)?.unwrap_or(0) != 0;
        let cas = match opt_blob(ctx, 1)? {
            Some(cas) => cas,
            None if pending => &[0; 16],
            None => return Ok(None),
        };
        let names = match opt_blob(ctx, 2)? {
            Some(bytes) => Some(
                ArgumentNames::decode(bytes).ok_or_else(|| user_error("invalid argument names"))?,
            ),
            None => None,
        };
        let handle = Handle {
            kind: u8::try_from(kind)
                .ok()
                .filter(|k| k & PENDING_BIT == 0)
                .ok_or_else(|| user_error("invalid value kind"))?,
            pending: pending && opt_blob(ctx, 1)?.is_none(),
            cas: cas.try_into().map_err(|_| user_error("invalid CAS id"))?,
            inline: None,
            names,
            path: Vec::new(),
        };
        Ok(Some(handle.encode()))
    })?;
    conn.create_scalar_function("__btel_nav", -1, pure, |ctx| {
        let Some(handle) = opt_blob(ctx, 0)? else {
            return Ok(None);
        };
        Handle::decode(handle).ok_or_else(|| user_error("invalid BAML value handle"))?;
        let mut out = handle.to_vec();
        for index in 1..ctx.len() {
            let segment = match ctx.get_raw(index) {
                ValueRef::Text(key) => Segment::Key(
                    std::str::from_utf8(key)
                        .map_err(|_| user_error("path key is not UTF-8"))?
                        .to_owned(),
                ),
                ValueRef::Integer(i) => Segment::Index(i),
                _ => return Err(user_error("path segments are constant strings or integers")),
            };
            push_segment(&mut out, &segment);
        }
        Ok(Some(out))
    })?;
    let s = slot.clone();
    conn.create_scalar_function("__btel_render", 1, io, move |ctx| {
        let Some(handle) = opt_blob(ctx, 0)? else {
            return Ok(SqlValue::Null);
        };
        Ok(scalar_to_sql(current(&s)?.resolve(handle)?.0))
    })?;
    let s = slot.clone();
    conn.create_scalar_function("__btel_kind", 1, io, move |ctx| {
        let Some(handle) = opt_blob(ctx, 0)? else {
            return Ok(None);
        };
        Ok(Some(current(&s)?.resolve(handle)?.1.label()))
    })?;
    let s = slot.clone();
    conn.create_scalar_function("__btel_is_null", 1, io, move |ctx| {
        let Some(handle) = opt_blob(ctx, 0)? else {
            // No capture at all: null-like.
            return Ok(Some(true));
        };
        Ok(current(&s)?.with_nav(handle, value::is_null)?.flatten())
    })?;
    let s = slot.clone();
    conn.create_scalar_function("__btel_truthy", 1, io, move |ctx| {
        let Some(handle) = opt_blob(ctx, 0)? else {
            return Ok(None);
        };
        Ok(current(&s)?
            .with_nav(handle, |nav| {
                value::leaf_of(nav)
                    .and_then(|leaf| value::compare(leaf, CmpOp::Eq, value::Leaf::Bool(true)))
            })?
            .flatten())
    })?;
    let s = slot.clone();
    conn.create_scalar_function("__btel_cmp", 4, io, move |ctx| {
        let Some(handle) = opt_blob(ctx, 0)? else {
            return Ok(None);
        };
        let op = ctx.get::<String>(1)?;
        let op = CmpOp::parse(&op).ok_or_else(|| user_error("unsupported comparison"))?;
        let boolean = ctx.get::<String>(3)? == "bool";
        let text;
        let operand = match ctx.get_raw(2) {
            ValueRef::Null => Operand::Null,
            ValueRef::Integer(n) if boolean => Operand::Bool(n != 0),
            ValueRef::Integer(n) => Operand::Integer(n),
            ValueRef::Real(f) => Operand::Real(f),
            ValueRef::Text(t) => {
                text = String::from_utf8_lossy(t).into_owned();
                Operand::Text(&text)
            }
            ValueRef::Blob(_) => return Err(user_error("cannot compare a BAML value to a blob")),
        };
        let Some(right) = value::leaf_of_operand(operand) else {
            return Ok(None);
        };
        let context = current(&s)?;
        Ok(context
            .with_nav(handle, |nav| {
                value::leaf_of(nav)
                    .and_then(|left| compare_reported(&context, handle, left, op, right))
            })?
            .flatten())
    })?;
    let s = slot.clone();
    conn.create_scalar_function("__btel_cmp_values", 3, io, move |ctx| {
        let (Some(left), Some(right)) = (opt_blob(ctx, 0)?, opt_blob(ctx, 2)?) else {
            return Ok(None);
        };
        let op = ctx.get::<String>(1)?;
        let op = CmpOp::parse(&op).ok_or_else(|| user_error("unsupported comparison"))?;
        let context = current(&s)?;
        let result = context.with_value(left, |a| {
            context.with_value(right, |b| {
                equality::captured(a, op, b, &context.limits.comparison)
            })
        })?;
        let result = match result {
            Some(result) => result?,
            None => None,
        };
        context.check_deadline()?;
        Ok(comparison_result(&context, left, result))
    })?;
    let s = slot.clone();
    conn.create_scalar_function("__btel_cmp_json", 3, io, move |ctx| {
        let Some(handle) = opt_blob(ctx, 0)? else {
            return Ok(None);
        };
        let op = CmpOp::parse(&ctx.get::<String>(1)?)
            .ok_or_else(|| user_error("unsupported comparison"))?;
        // SQL lowering accepts only bounded constant JSON literals. SQLite
        // keeps their parse alongside the constant argument for this statement.
        let json: Arc<serde_json::Value> = ctx.get_or_create_aux(2, |raw| {
            let text = raw.as_str()?;
            serde_json::from_str(text).map_err(|error| user_error(&error.to_string()))
        })?;
        let context = current(&s)?;
        let result = context.with_value(handle, |left| {
            equality::json(left, op, &json, &context.limits.comparison)
        })?;
        context.check_deadline()?;
        Ok(comparison_result(&context, handle, result))
    })?;
    let s = slot.clone();
    conn.create_scalar_function("__btel_value_state", 1, io, move |ctx| {
        let Some(handle) = opt_blob(ctx, 0)? else {
            return Ok("no_value".to_owned());
        };
        current(&s)?.state(handle)
    })?;
    let s = slot.clone();
    conn.create_scalar_function("__btel_body_text", 1, io, move |ctx| {
        let Some(cas) = opt_blob(ctx, 0)? else {
            return Ok(None);
        };
        let cas = <[u8; 16]>::try_from(cas).map_err(|_| user_error("invalid CAS id"))?;
        current(&s)?.body_text(cas)
    })?;
    register_identity_and_time(conn)?;
    register_final_tables(conn)?;
    let _ = Null;
    Ok(())
}

/// A profiler node's identity: its parent's, the name of the function it
/// reached and the edge that reached it (1 call, 2 spawn). The same code
/// reached the same way has the same node in every process.
pub(crate) fn node_hash(parent: Option<i64>, name: &str, edge: i64) -> i64 {
    let mut hasher = xxhash_rust::xxh3::Xxh3::new();
    match parent {
        None => hasher.update(&[0]),
        Some(parent) => {
            hasher.update(&[1]);
            hasher.update(&parent.to_be_bytes());
        }
    }
    hasher.update(&(name.len() as u64).to_be_bytes());
    hasher.update(name.as_bytes());
    hasher.update(&edge.to_be_bytes());
    hasher.digest().cast_signed()
}

/// A value the index builds rather than reads from the CAS.
#[derive(Clone, Debug)]
pub(crate) enum Inline {
    Null,
    Int(i64),
    Float(f64),
    Text(String),
    Map(Vec<(String, Inline)>),
    List(Vec<Inline>),
}

/// An inline value's handle: its snapshot travels inside the handle.
pub(crate) fn inline_handle(value: &Inline) -> Vec<u8> {
    let pool = btel_snapshot::SnapshotPool::new(1, btel_snapshot::Limits::default());
    let mut builder = pool.try_acquire().expect("a fresh pool has a slot");
    let root = build_inline(&mut builder, value);
    let snapshot = builder.finish_value(root);
    let mut blob = Vec::new();
    snapshot.write_blob(&mut blob).expect("writing to memory");
    Handle {
        kind: INLINE,
        pending: false,
        cas: [0; 16],
        inline: Some(blob),
        names: None,
        path: Vec::new(),
    }
    .encode()
}

/// Children first: each container's values form one contiguous range.
fn build_inline(b: &mut btel_snapshot::Builder, value: &Inline) -> btel_snapshot::SnapshotValue {
    use btel_snapshot::{Limit, OwnedType, SnapshotObject, SnapshotValue};
    let text = |b: &mut btel_snapshot::Builder, text: &str| {
        b.string(&btel_snapshot::BexStr::from(text)).map_or(
            SnapshotValue::Truncated(Limit::Bytes),
            SnapshotValue::String,
        )
    };
    match value {
        Inline::Null => SnapshotValue::Null,
        Inline::Int(n) => SnapshotValue::Int(*n),
        Inline::Float(f) => SnapshotValue::Float(*f),
        Inline::Text(t) => text(b, t),
        Inline::Map(entries) => {
            let values: Vec<SnapshotValue> =
                entries.iter().map(|(_, v)| build_inline(b, v)).collect();
            let Some(id) = b.reserve_object() else {
                return SnapshotValue::Truncated(Limit::Objects);
            };
            let key_type = b.push_type(OwnedType::string());
            let value_type = b.push_type(OwnedType::unknown());
            let start = b.entry_start();
            b.reserve_entries(entries.len().min(b.remaining_entries()));
            for ((key, _), value) in entries.iter().zip(values) {
                if b.remaining_entries() == 0 || !b.content(key.len(), false) {
                    break;
                }
                b.entry(&btel_snapshot::BexStr::from(key.as_str()), value);
            }
            let range = b.entry_range(start);
            b.set_object(
                id,
                SnapshotObject::Map {
                    key_type,
                    value_type,
                    entries: range,
                    original_len: entries.len(),
                },
            );
            SnapshotValue::Object(id)
        }
        Inline::List(items) => {
            let values: Vec<SnapshotValue> = items.iter().map(|v| build_inline(b, v)).collect();
            let Some(id) = b.reserve_object() else {
                return SnapshotValue::Truncated(Limit::Objects);
            };
            let element_type = b.push_type(OwnedType::unknown());
            let start = b.value_start();
            b.reserve_values(values.len().min(b.remaining_values()));
            for value in values {
                if b.remaining_values() == 0 {
                    break;
                }
                b.push_value(value);
            }
            let range = b.value_range(start);
            b.set_object(
                id,
                SnapshotObject::List {
                    element_type,
                    items: range,
                    original_len: items.len(),
                },
            );
            SnapshotValue::Object(id)
        }
    }
}

/// Model usage summed per span: `(model, input, output, cache_read,
/// cache_write, reasoning)` rows in, one `temporary_projections` map out.
#[derive(Default)]
struct Usage {
    /// Model turns recorded against the span.
    calls: i64,
    models: Vec<String>,
    unnamed: bool,
    input: i64,
    output: i64,
    cache_read: Option<i64>,
    cache_write: Option<i64>,
    reasoning: Option<i64>,
    /// `None` once any turn's model has no price.
    cost: Option<f64>,
}

struct UsageSum;
impl Aggregate<Option<Usage>, Option<Vec<u8>>> for UsageSum {
    fn init(&self, _: &mut Context<'_>) -> rusqlite::Result<Option<Usage>> {
        Ok(None)
    }
    fn step(&self, ctx: &mut Context<'_>, acc: &mut Option<Usage>) -> rusqlite::Result<()> {
        let model: Option<String> = ctx.get(0)?;
        let input = opt_i64(ctx, 1)?.unwrap_or(0);
        let output = opt_i64(ctx, 2)?.unwrap_or(0);
        let (cache_read, cache_write, reasoning) =
            (opt_i64(ctx, 3)?, opt_i64(ctx, 4)?, opt_i64(ctx, 5)?);
        let usage = acc.get_or_insert_with(|| Usage {
            cost: Some(0.0),
            ..Usage::default()
        });
        let add = |sum: Option<i64>, n: Option<i64>| match (sum, n) {
            (sum, None) => sum,
            (sum, Some(n)) => Some(sum.unwrap_or(0).saturating_add(n)),
        };
        usage.cost = usage
            .cost
            .zip(crate::pricing::cost(
                model.as_deref(),
                input,
                output,
                cache_read,
                cache_write,
            ))
            .map(|(a, b)| a + b);
        match model {
            Some(model) if !usage.models.contains(&model) => usage.models.push(model),
            Some(_) => {}
            None => usage.unnamed = true,
        }
        usage.calls += 1;
        usage.input = usage.input.saturating_add(input);
        usage.output = usage.output.saturating_add(output);
        usage.cache_read = add(usage.cache_read, cache_read);
        usage.cache_write = add(usage.cache_write, cache_write);
        usage.reasoning = add(usage.reasoning, reasoning);
        Ok(())
    }
    fn finalize(
        &self,
        _: &mut Context<'_>,
        acc: Option<Option<Usage>>,
    ) -> rusqlite::Result<Option<Vec<u8>>> {
        let Some(usage) = acc.flatten() else {
            return Ok(None);
        };
        let int = |n: Option<i64>| n.map_or(Inline::Null, Inline::Int);
        let model = if usage.models.is_empty() {
            Inline::Null
        } else {
            Inline::Text(usage.models.join(", "))
        };
        Ok(Some(inline_handle(&Inline::Map(vec![
            ("model_name".into(), model),
            ("model_calls".into(), Inline::Int(usage.calls)),
            ("input_tokens".into(), Inline::Int(usage.input)),
            ("output_tokens".into(), Inline::Int(usage.output)),
            ("cache_read_tokens".into(), int(usage.cache_read)),
            ("cache_write_tokens".into(), int(usage.cache_write)),
            ("reasoning_tokens".into(), int(usage.reasoning)),
            (
                "cost".into(),
                usage
                    .cost
                    .filter(|_| !usage.unnamed)
                    .map_or(Inline::Null, Inline::Float),
            ),
        ]))))
    }
}

/// A network span's events, `(at_ticks, seq, name, timestamp, payload_cas)`
/// rows in, one EVENTS handle out: ordered by time, then by recording
/// order. No rows is an empty list.
struct EventList;
impl Aggregate<Vec<(Option<i64>, i64, Event)>, Vec<u8>> for EventList {
    fn init(&self, _: &mut Context<'_>) -> rusqlite::Result<Vec<(Option<i64>, i64, Event)>> {
        Ok(Vec::new())
    }
    fn step(
        &self,
        ctx: &mut Context<'_>,
        acc: &mut Vec<(Option<i64>, i64, Event)>,
    ) -> rusqlite::Result<()> {
        let payload = match opt_blob(ctx, 4)? {
            Some(cas) => Some(<[u8; 16]>::try_from(cas).map_err(|_| user_error("invalid CAS id"))?),
            None => None,
        };
        acc.push((
            opt_i64(ctx, 0)?,
            ctx.get(1)?,
            Event {
                name: ctx.get(2)?,
                timestamp: ctx.get(3)?,
                payload,
            },
        ));
        Ok(())
    }
    fn finalize(
        &self,
        _: &mut Context<'_>,
        acc: Option<Vec<(Option<i64>, i64, Event)>>,
    ) -> rusqlite::Result<Vec<u8>> {
        let mut events = acc.unwrap_or_default();
        events.sort_by_key(|(ticks, seq, _)| (*ticks, *seq));
        let events: Vec<Event> = events.into_iter().map(|(_, _, event)| event).collect();
        Ok(Handle {
            kind: EVENTS,
            pending: false,
            cas: [0; 16],
            inline: Some(encode_events(&events)),
            names: None,
            path: Vec::new(),
        }
        .encode())
    }
}

/// `%XX` escapes decoded; a malformed one is kept as it is.
fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        let escaped = (bytes[at] == b'%')
            .then(|| bytes.get(at + 1..at + 3))
            .flatten()
            .filter(|hex| hex.iter().all(u8::is_ascii_hexdigit))
            .and_then(|hex| u8::from_str_radix(std::str::from_utf8(hex).ok()?, 16).ok());
        match escaped {
            Some(byte) => {
                out.push(byte);
                at += 3;
            }
            None => {
                out.push(bytes[at]);
                at += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn utc_text(ns: Option<i64>) -> Option<String> {
    ns.and_then(|ns| format_unix_ns(i128::from(ns)))
}

fn register_final_tables(conn: &Connection) -> rusqlite::Result<()> {
    let pure = FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC;
    // A node's public ID: 16 hex digits.
    conn.create_scalar_function("__btel_hex", 1, pure, |ctx| {
        Ok(opt_i64(ctx, 0)?.map(|n| format!("{:016x}", n.cast_unsigned())))
    })?;
    // RFC 3339 from Unix nanoseconds.
    conn.create_scalar_function("__btel_ns_utc", 1, pure, |ctx| {
        Ok(utc_text(opt_i64(ctx, 0)?))
    })?;
    conn.create_scalar_function("__btel_empty_map", 0, pure, |_| {
        Ok(inline_handle(&Inline::Map(Vec::new())))
    })?;
    // `(started_ns, final status or NULL, ended_ns)`: running, then the end.
    conn.create_scalar_function("__btel_status_history", 3, pure, |ctx| {
        let entry = |status: &str, ns: Option<i64>| {
            Inline::Map(vec![
                ("status".into(), Inline::Text(status.to_owned())),
                (
                    "timestamp".into(),
                    utc_text(ns).map_or(Inline::Null, Inline::Text),
                ),
            ])
        };
        let mut history = vec![entry("running", opt_i64(ctx, 0)?)];
        if let Some(status) = ctx.get::<Option<String>>(1)? {
            history.push(entry(&status, opt_i64(ctx, 2)?));
        }
        Ok(inline_handle(&Inline::List(history)))
    })?;
    conn.create_aggregate_function("__btel_usage", 6, pure, UsageSum)?;
    // `(url, marker)`: the path segment right after the first `marker`, up
    // to the next '/', ':', '?' or '#', percent-decoded. The model in
    // `.../models/gemini-2.5-pro:generateContent` or `/model/<id>/converse`.
    conn.create_scalar_function("__btel_path_segment", 2, pure, |ctx| {
        let (Some(url), Some(marker)) =
            (ctx.get::<Option<String>>(0)?, ctx.get::<Option<String>>(1)?)
        else {
            return Ok(None);
        };
        let Some((_, rest)) = url.split_once(marker.as_str()) else {
            return Ok(None);
        };
        let segment = rest.split(['/', ':', '?', '#']).next().unwrap_or_default();
        Ok((!segment.is_empty()).then(|| percent_decode(segment)))
    })?;
    conn.create_aggregate_function("__btel_events", 5, pure, EventList)
}

/// `<recording hex>:<prefix><id>` for an INTEGER id or an 8-byte BLOB id.
fn scoped_id(ctx: &Context<'_>) -> rusqlite::Result<Option<String>> {
    let Some(recording) = opt_blob(ctx, 0)? else {
        return Ok(None);
    };
    let prefix = match ctx.get_raw(1) {
        ValueRef::Text(t) => std::str::from_utf8(t).map_err(|_| user_error("id prefix"))?,
        _ => return Err(user_error("id prefix must be text")),
    };
    let id = match ctx.get_raw(2) {
        ValueRef::Null => return Ok(None),
        ValueRef::Integer(n) => u64::try_from(n).map_err(|_| user_error("negative id"))?,
        ValueRef::Blob(b) => u64_from_blob(b).ok_or_else(|| user_error("id blob"))?,
        _ => return Err(user_error("id must be an integer or blob")),
    };
    Ok(Some(format!("{}:{prefix}{id}", hex(recording))))
}

/// Clock for timing columns: `(multiplier, shift, status, epoch)` at `at`,
/// `epoch` as in [`interval`]. A defined epoch with an unrepresentable
/// multiplier is overflow.
fn clock_at(ctx: &Context<'_>, at: usize) -> rusqlite::Result<Result<Clock, TimingState>> {
    Ok(epoch_clock(
        opt_i64(ctx, at)?,
        opt_i64(ctx, at + 1)?,
        opt_i64(ctx, at + 2)?,
        opt_i64(ctx, at + 3)?.unwrap_or(0),
    ))
}

/// The clock of an epoch's stored columns, where `epoch` is 0 for no usable
/// definition, 1 for one definition, 2 for conflicting ones.
pub(crate) fn epoch_clock(
    multiplier: Option<i64>,
    shift: Option<i64>,
    status: Option<i64>,
    epoch: i64,
) -> Result<Clock, TimingState> {
    if epoch == 2 {
        return Err(TimingState::Conflicted);
    }
    if epoch == 1 && multiplier.is_none() {
        return Err(TimingState::Overflow);
    }
    Ok(clock(multiplier, shift, status))
}

/// An accumulated tick total in nanoseconds, as `__btel_total_ns` computes
/// it: `None` when the clock or total is unusable.
pub(crate) fn total_ns(clock: Result<Clock, TimingState>, ticks: Option<i64>) -> Option<i64> {
    let ticks = ticks.and_then(|v| u64::try_from(v).ok());
    clock
        .ok()?
        .total_ns(ticks)
        .ok()
        .and_then(|ns| i64::try_from(ns).ok())
}

fn register_identity_and_time(conn: &Connection) -> rusqlite::Result<()> {
    let pure = FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC;
    conn.create_scalar_function("__btel_sid", 3, pure, scoped_id)?;
    // A public id's stored parts, so a filter on an id column can use the
    // indexes: the recording's id, and the key as its table stores it.
    // NULL when the text is not an id of that kind.
    conn.create_scalar_function("__btel_key_recording", 1, pure, |ctx| {
        Ok(public_id(ctx.get_raw(0), "").map(|(recording, _)| recording))
    })?;
    conn.create_scalar_function("__btel_key", 2, pure, |ctx| {
        let ValueRef::Text(prefix) = ctx.get_raw(1) else {
            return Err(user_error("id prefix must be text"));
        };
        let prefix = std::str::from_utf8(prefix).map_err(|_| user_error("id prefix"))?;
        Ok(public_id(ctx.get_raw(0), prefix).and_then(|(_, id)| {
            // Call paths are stored as integers; every other key as 8 bytes.
            if prefix == "p" {
                i64::try_from(id).ok().map(SqlValue::Integer)
            } else {
                Some(SqlValue::Blob(id.to_be_bytes().to_vec()))
            }
        }))
    })?;
    // Checked i64 addition: NULL on NULL input or overflow, never a float.
    conn.create_scalar_function("__btel_add", 2, pure, |ctx| {
        Ok(opt_i64(ctx, 0)?
            .zip(opt_i64(ctx, 1)?)
            .and_then(|(a, b)| a.checked_add(b)))
    })?;
    conn.create_scalar_function("__btel_unix_ns", 5, pure, |ctx| {
        let (Some(ticks), Some(anchor_ticks), Some(anchor_ns), Some(multiplier), Some(shift)) = (
            opt_i64(ctx, 0)?,
            opt_i64(ctx, 1)?,
            opt_i64(ctx, 2)?,
            opt_i64(ctx, 3)?,
            opt_i64(ctx, 4)?,
        ) else {
            return Ok(None);
        };
        let anchor = UtcAnchor {
            ticks: ticks_u64(anchor_ticks),
            unix_ns: i128::from(anchor_ns),
        };
        let conversion = Conversion {
            multiplier: ticks_u64(multiplier),
            shift: u32::try_from(shift).unwrap_or(u32::MAX),
        };
        Ok(anchor
            .unix_ns_at(conversion, ticks_u64(ticks))
            .and_then(|ns| i64::try_from(ns).ok()))
    })?;
    Ok(())
}
