//! SQLite functions registered on every query connection.
//!
//! Identity and timing functions are pure. Value functions resolve CAS
//! blobs lazily through the current query's context: a query that never
//! evaluates `type_args`, `input_args`, `output_value`, `error_value` or
//! `network_event_values` reads no blob. A path reads only the blobs it
//! crosses; rendering or comparing a whole value reads every blob it reaches,
//! within the render and comparison limits. Within one query, hydration
//! outcomes (including missing and corrupt blobs) and presented results are
//! memoized under entry and byte limits; the next query starts empty and can
//! find a blob that arrived in between.
use std::{
    borrow::Cow,
    collections::{BTreeMap, HashMap, HashSet},
    sync::{Arc, Mutex},
    time::Instant,
};

use btel_reader::{
    cas::{CasStore, CasUnavailable},
    evidence::ArgumentNames,
    timing::{Clock, Conversion, EpochStatus, TimingState, UtcAnchor, format_unix_ns},
    value::{
        self, BlobSource, CmpOp, Found, Kind, Nav, Operand, RenderLimits, Root, Scalar, Segment,
        Unavailable, equality,
    },
};
use btel_snapshot::{
    CasId, ChildIndex, DecodedMedia, DecodedMediaSource, DecodedObject, DecodedRoot,
    DecodedSnapshot, DecodedValue, Limit, MediaPayload, NodeId, TypeDescription,
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
    /// All are forgotten when another would pass this or the byte limit.
    pub max_cached_results: usize,
    /// Bytes of those results' handles and text.
    pub max_cached_result_bytes: usize,
    pub render: RenderLimits,
    pub comparison: equality::Limits,
}
impl Default for ValueLimits {
    fn default() -> Self {
        Self {
            max_cached_blobs: 4096,
            max_cached_bytes: 256 << 20,
            max_cached_results: 16_384,
            max_cached_result_bytes: 64 << 20,
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
    blobs: HashMap<[u8; 16], Result<Arc<DecodedSnapshot>, CasUnavailable>>,
    blob_bytes: u64,
    results: HashMap<Vec<u8>, (Scalar, Kind)>,
    result_bytes: usize,
    unavailable_seen: HashSet<Vec<u8>>,
    metrics: ValueMetrics,
}

/// One query's value-resolution state.
/// Recorded class and enum definitions, loaded lazily: a query reads only
/// the recordings and tags it renders, through its own read-only connection
/// to the index (the query's connection is busy running the statement that
/// renders). Merge rules and ids are `btel_reader::types`'.
pub struct TypeDefinitionStore {
    path: Option<std::path::PathBuf>,
    conn: Mutex<Option<rusqlite::Connection>>,
    recordings: Mutex<HashMap<i64, Option<Arc<[u8]>>>>,
    declarations: Mutex<HashMap<(i64, i64), Option<Declaration>>>,
}

type Declaration = Arc<btel_reader::types::TypeDeclaration>;

impl TypeDefinitionStore {
    /// `path` is the index database; `None` for an in-memory index, which
    /// has no definitions to read.
    pub fn new(path: Option<std::path::PathBuf>) -> Self {
        Self {
            path,
            conn: Mutex::new(None),
            recordings: Mutex::new(HashMap::new()),
            declarations: Mutex::new(HashMap::new()),
        }
    }

    fn with_conn<T>(
        &self,
        f: impl FnOnce(&rusqlite::Connection) -> rusqlite::Result<T>,
    ) -> Option<T> {
        let path = self.path.as_ref()?;
        let mut conn = self
            .conn
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if conn.is_none() {
            *conn = rusqlite::Connection::open_with_flags(
                path,
                rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY
                    | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
            )
            .ok();
        }
        f(conn.as_ref()?).ok()
    }

    fn recording_id(&self, rec: i64) -> Option<Arc<[u8]>> {
        let mut recordings = self
            .recordings
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(id) = recordings.get(&rec) {
            return id.clone();
        }
        let id = self
            .with_conn(|conn| {
                conn.query_row(
                    "SELECT recording_id FROM recording WHERE rec = ?1",
                    [rec],
                    |row| row.get::<_, Vec<u8>>(0),
                )
            })
            .map(Arc::from);
        recordings.insert(rec, id.clone());
        id
    }

    fn declaration(&self, rec: i64, tag: i64) -> Option<Arc<btel_reader::types::TypeDeclaration>> {
        let mut declarations = self
            .declarations
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(found) = declarations.get(&(rec, tag)) {
            return found.clone();
        }
        let found = self
            .with_conn(|conn| {
                conn.query_row(
                    "SELECT declaration FROM type_def WHERE rec = ?1 AND type_tag = ?2 AND state = ?3",
                    rusqlite::params![rec, tag, btel_reader::types::DefinitionState::Declared.code()],
                    |row| row.get::<_, Vec<u8>>(0),
                )
            })
            .and_then(|bytes| btel_reader::types::DefinitionRow::decode(&bytes))
            .map(Arc::new);
        declarations.insert((rec, tag), found.clone());
        found
    }
}

/// The query context seen from one value's recording: blobs as the context
/// loads them, definitions of that recording's type tags.
struct Scoped<'a> {
    context: &'a QueryContext,
    rec: i64,
    recording: Option<Arc<[u8]>>,
}

impl BlobSource for Scoped<'_> {
    fn load(&self, id: CasId) -> Result<Arc<DecodedSnapshot>, CasUnavailable> {
        self.context.load(id)
    }
    fn type_definitions(&self) -> Option<&dyn btel_reader::types::TypeDefinitions> {
        self.recording
            .as_ref()
            .map(|_| self as &dyn btel_reader::types::TypeDefinitions)
    }
}

impl btel_reader::types::TypeDefinitions for Scoped<'_> {
    fn id(&self, tag: baml_type::typetag::TypeTag) -> Arc<str> {
        btel_reader::types::definition_id(self.recording.as_deref().unwrap_or_default(), tag)
    }
    fn declaration(
        &self,
        tag: baml_type::typetag::TypeTag,
    ) -> Option<Arc<btel_reader::types::TypeDeclaration>> {
        self.context.types.declaration(self.rec, tag.as_i64())
    }
}

pub struct QueryContext {
    cas: CasStore,
    types: Arc<TypeDefinitionStore>,
    limits: ValueLimits,
    deadline: Option<Instant>,
    state: Mutex<CacheState>,
}

impl QueryContext {
    pub fn new(
        cas: CasStore,
        types: Arc<TypeDefinitionStore>,
        limits: ValueLimits,
        deadline: Option<Instant>,
    ) -> Self {
        Self {
            cas,
            types,
            limits,
            deadline,
            state: Mutex::new(CacheState {
                blobs: HashMap::new(),
                blob_bytes: 0,
                results: HashMap::new(),
                result_bytes: 0,
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

    /// The snapshot a handle reads and the path to follow in it, or the
    /// code of why it is unavailable: its CAS blob or the inline one at its
    /// own path, or for a network span's events, the list `events` builds.
    fn load_handle<'h>(&self, handle: &'h Handle) -> Loaded<'h> {
        if handle.kind == EVENTS {
            return self.events(handle);
        }
        let snapshot = match &handle.inline {
            Some(blob) => btel_snapshot::decode_blob(blob, &self.cas.limits().decode)
                .map(Arc::new)
                .map_err(CasUnavailable::Corrupt),
            None => BlobSource::load(self, CasId::from_bytes(handle.cas)),
        };
        match snapshot {
            Ok(snapshot) => Ok((snapshot, Cow::Borrowed(&handle.path[..]))),
            Err(unavailable) => Err(unavailable.code()),
        }
    }

    fn evaluation(&self) {
        self.state
            .lock()
            .expect("value state")
            .metrics
            .value_evaluations += 1;
    }

    /// A network span's events as `[{event_name, payload, timestamp}]`. A
    /// path into one event's payload reads only that payload's blob. Any
    /// other path reads a list built here, with the payloads the path ends
    /// on (every one for the whole list, its own for one event) and null
    /// for the rest, which the path never reaches.
    fn events<'h>(&self, handle: &'h Handle) -> Loaded<'h> {
        let events = handle
            .inline
            .as_deref()
            .and_then(decode_events)
            .expect("Handle::decode validated the events");
        let load = |cas| {
            BlobSource::load(self, CasId::from_bytes(cas)).map_err(|unavailable| unavailable.code())
        };
        // A reader knows a blob by its ID, so what is built here needs one
        // of its own: what the handle says is what is built from it.
        let id = {
            let mut hasher = xxhash_rust::xxh3::Xxh3::new();
            hasher.update(b"baml.query.events\0");
            hasher.update(&handle.encode());
            CasId::from_bytes(hasher.digest128().to_le_bytes())
        };
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
            Some(Segment::Key(_)) | None => None,
        };
        if let (Some(at), Some(Segment::Key(key))) = (at, path.get(1))
            && key == "payload"
        {
            let rest = path.split_off(2);
            let snapshot = match events[at].payload {
                Some(cas) => load(cas)?,
                None => Arc::new(Composer::default().finish(id, DecodedValue::Null)),
            };
            return Ok((snapshot, Cow::Owned(rest)));
        }
        let shown = |event: usize| path.is_empty() || (path.len() == 1 && at == Some(event));
        let mut composer = Composer::default();
        let mut items = Vec::with_capacity(events.len());
        for (position, event) in events.iter().enumerate() {
            let payload = match event.payload {
                Some(cas) if shown(position) => composer.embed(&*load(cas)?)?,
                Some(_) | None => DecodedValue::Null,
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
        Ok((Arc::new(composer.finish(id, list)), Cow::Owned(path)))
    }

    /// Report `code` once per distinct `key` (a handle, or a handle plus an
    /// operation marker).
    fn note(&self, key: &[u8], code: &str) {
        let mut state = self.state.lock().expect("value state");
        if state.unavailable_seen.insert(key.to_vec()) {
            *state
                .metrics
                .unavailable
                .entry(code.to_owned())
                .or_default() += 1;
        }
    }

    /// What a handle names, or `None` with the reason reported.
    fn find(&self, handle: &[u8]) -> rusqlite::Result<Option<(Handle, Nav)>> {
        let parsed =
            Handle::decode(handle).ok_or_else(|| user_error("invalid BAML value handle"))?;
        if parsed.pending {
            self.note(handle, parsed.pending_code());
            return Ok(None);
        }
        let nav = match self.load_handle(&parsed) {
            Ok((snapshot, path)) => value::navigate(
                self,
                &snapshot,
                parsed.root(),
                parsed.names.as_ref(),
                &path,
                self.limits.render.max_blobs,
            ),
            Err(code) => {
                self.note(handle, code);
                return Ok(None);
            }
        };
        if let Nav::Unavailable(reason) = &nav {
            self.note(handle, reason.code());
            return Ok(None);
        }
        Ok(Some((parsed, nav)))
    }

    /// Navigate a handle and present the result as a SQL scalar.
    fn resolve(&self, handle: &[u8]) -> rusqlite::Result<(Scalar, Kind)> {
        self.check_deadline()?;
        {
            let mut state = self.state.lock().expect("value state");
            state.metrics.value_evaluations += 1;
            if let Some(result) = state.results.get(handle) {
                let result = result.clone();
                state.metrics.result_cache_hits += 1;
                return Ok(result);
            }
        }
        let result = match self.find(handle)? {
            Some((parsed, nav)) => {
                let scoped = Scoped {
                    context: self,
                    rec: parsed.rec,
                    recording: (parsed.rec != 0)
                        .then(|| self.types.recording_id(parsed.rec))
                        .flatten(),
                };
                let presented =
                    value::to_scalar(&scoped, &nav, parsed.names.as_ref(), &self.limits.render);
                if let Some(reason) = presented.incomplete {
                    self.note(handle, reason.code());
                }
                if presented.cut {
                    let mut key = handle.to_vec();
                    key.extend_from_slice(b"\xffrender");
                    self.note(&key, RENDER_LIMIT);
                }
                (presented.scalar, presented.kind)
            }
            None => (Scalar::Null, Kind::Unavailable),
        };
        let bytes = handle.len()
            + match &result.0 {
                Scalar::Text(text) => text.len(),
                Scalar::Null | Scalar::Integer(_) | Scalar::Real(_) => 0,
            };
        let mut state = self.state.lock().expect("value state");
        // The newest result stays, so the kind asked next finds its rendering.
        if state.results.len() >= self.limits.max_cached_results
            || state.result_bytes.saturating_add(bytes) > self.limits.max_cached_result_bytes
        {
            state.results.clear();
            state.result_bytes = 0;
        }
        if state
            .results
            .insert(handle.to_vec(), result.clone())
            .is_none()
        {
            state.result_bytes += bytes;
        }
        Ok(result)
    }

    /// Navigate and hand the result to `f` (comparisons need the leaf).
    fn with_nav<T>(&self, handle: &[u8], f: impl FnOnce(&Nav) -> T) -> rusqlite::Result<Option<T>> {
        self.with_value(handle, |value| f(value.nav))
    }

    fn with_value<T>(
        &self,
        handle: &[u8],
        f: impl FnOnce(equality::Captured<'_>) -> T,
    ) -> rusqlite::Result<Option<T>> {
        self.check_deadline()?;
        self.evaluation();
        Ok(self.find(handle)?.map(|(parsed, nav)| {
            f(equality::Captured {
                nav: &nav,
                names: parsed.names.as_ref(),
            })
        }))
    }

    /// `__btel_body_text`: the text of a recorded body, which is a whole
    /// body (a string), one server-sent event's `data`, or a request
    /// snapshot's `request.body`. NULL for anything else, such as bytes or a
    /// cut body. A blob that cannot be read is reported.
    fn body_text(&self, cas: [u8; 16]) -> rusqlite::Result<Option<String>> {
        self.check_deadline()?;
        self.evaluation();
        let report = |code: &str| {
            let mut key = cas.to_vec();
            key.extend_from_slice(b"\xffbody");
            self.note(&key, code);
        };
        let snapshot = match BlobSource::load(self, CasId::from_bytes(cas)) {
            Ok(snapshot) => snapshot,
            Err(unavailable) => {
                report(unavailable.code());
                return Ok(None);
            }
        };
        let key = |key: &str| Segment::Key(key.to_owned());
        for path in [vec![], vec![key("data")], vec![key("request"), key("body")]] {
            let nav = value::navigate(
                self,
                &snapshot,
                Root::Value,
                None,
                &path,
                self.limits.render.max_blobs,
            );
            match nav {
                Nav::Value(found) => {
                    if let Found::String(text) = found.value() {
                        return Ok(Some(text.to_string()));
                    }
                }
                // A long body is a blob of its own.
                Nav::Unavailable(reason @ (Unavailable::Blob(_) | Unavailable::BlobBudget)) => {
                    report(reason.code());
                    return Ok(None);
                }
                Nav::Unavailable(
                    Unavailable::Truncated(_)
                    | Unavailable::ArgumentNamesUnknown
                    | Unavailable::ArgumentLayoutMismatch
                    | Unavailable::WrongRoot
                    | Unavailable::CellCycle,
                )
                | Nav::Arguments(_)
                | Nav::Missing => {}
            }
        }
        Ok(None)
    }

    /// `baml_value_state`: what the path finds, without reporting it as
    /// unavailable evidence (the caller asked for exactly this answer).
    fn state(&self, handle: &[u8]) -> rusqlite::Result<String> {
        self.check_deadline()?;
        self.evaluation();
        let parsed =
            Handle::decode(handle).ok_or_else(|| user_error("invalid BAML value handle"))?;
        if parsed.pending {
            return Ok(parsed.pending_code().to_owned());
        }
        Ok(match self.load_handle(&parsed) {
            Ok((snapshot, path)) => value::state(
                self,
                &snapshot,
                parsed.root(),
                parsed.names.as_ref(),
                &path,
                self.limits.render.max_blobs,
            )
            .to_owned(),
            Err(code) => code.to_owned(),
        })
    }
}

/// Blobs a value continues in come from the query's cache, then the CAS.
/// The state lock is held for bookkeeping only, never while reading a blob
/// or walking a value.
impl BlobSource for QueryContext {
    fn load(&self, id: CasId) -> Result<Arc<DecodedSnapshot>, CasUnavailable> {
        let key = *id.as_bytes();
        {
            let mut state = self.state.lock().expect("value state");
            if let Some(snapshot) = state.blobs.get(&key) {
                let snapshot = snapshot.clone();
                state.metrics.cas_cache_hits += 1;
                return snapshot;
            }
        }
        let load = self.cas.load(id);
        let mut state = self.state.lock().expect("value state");
        state.metrics.cas_loads += 1;
        state.metrics.cas_bytes_read += load.bytes_read;
        if state.blobs.len() < self.limits.max_cached_blobs
            && state.blob_bytes + load.bytes_read <= self.limits.max_cached_bytes
        {
            state.blob_bytes += load.bytes_read;
            state.blobs.insert(key, load.snapshot.clone());
        }
        load.snapshot
    }
}

/// Unavailable code of a capture whose announcement is not indexed yet.
const PENDING: &str = "capture_pending";

/// Structured ordering and opaque-value equality are unsupported.
const COMPARISON_UNSUPPORTED: &str = "comparison_unsupported";
/// Code of a rendering cut by its depth, node or text limit.
const RENDER_LIMIT: &str = "render_limit";

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
/// payload graphs copied in from their own blobs.
#[derive(Default)]
struct Composer {
    objects: Vec<DecodedObject>,
    children: Vec<CasId>,
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
        Ok(DecodedValue::Object(NodeId(id)))
    }

    fn map(&mut self, entries: Vec<(&str, DecodedValue)>) -> Result<DecodedValue, &'static str> {
        let original_len = entries.len() as u64;
        self.push(DecodedObject::Map {
            key_type: Self::untyped(),
            value_type: Self::untyped(),
            entries: entries
                .into_iter()
                .map(|(key, value)| (DecodedValue::String(key.into()), value))
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

    /// Copy a value blob's graph in, numbering its objects after the ones
    /// already here and the blobs it continues in after the ones already
    /// named; returns its root. Each copy is a value of its own: two events
    /// with one payload both show it.
    fn embed(&mut self, snapshot: &DecodedSnapshot) -> Result<DecodedValue, &'static str> {
        let DecodedRoot::Value(root) = &snapshot.root else {
            return Err(Unavailable::WrongRoot.code());
        };
        let too_many = Unavailable::Truncated(Limit::Objects).code();
        let offset = |here: usize, added: usize| {
            u32::try_from(here + added).map_err(|_| too_many)?;
            u32::try_from(here).map_err(|_| too_many)
        };
        let objects = offset(self.objects.len(), snapshot.objects.len())?;
        let children = offset(self.children.len(), snapshot.children.len())?;
        let node = |id: &NodeId| NodeId(id.0 + objects);
        let child = |index: &ChildIndex| ChildIndex(index.0 + children);
        let shift = |value: &DecodedValue| match value {
            DecodedValue::Object(id) => DecodedValue::Object(node(id)),
            DecodedValue::Enum {
                declaration,
                variant,
                name,
            } => DecodedValue::Enum {
                declaration: node(declaration),
                variant: *variant,
                name: name.clone(),
            },
            DecodedValue::External(index) => DecodedValue::External(child(index)),
            // The object named is in the other blob, which is not copied.
            DecodedValue::ExternalNode { child: index, node } => DecodedValue::ExternalNode {
                child: child(index),
                node: *node,
            },
            DecodedValue::Null
            | DecodedValue::OmittedArg
            | DecodedValue::Bool(_)
            | DecodedValue::Int(_)
            | DecodedValue::Float(_)
            | DecodedValue::String(_)
            | DecodedValue::Bigint(_)
            | DecodedValue::Type(_)
            | DecodedValue::Truncated(_) => value.clone(),
        };
        let entries = |entries: &btel_snapshot::Entries| -> btel_snapshot::Entries {
            entries
                .iter()
                .map(|(key, value)| (shift(key), shift(value)))
                .collect()
        };
        let content = |payload: &MediaPayload| match payload {
            MediaPayload::Inline(_) => payload.clone(),
            MediaPayload::External {
                child: index,
                text_len,
            } => MediaPayload::External {
                child: child(index),
                text_len: *text_len,
            },
        };
        self.children.extend_from_slice(&snapshot.children);
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
                    declaration: node(declaration),
                    fields: fields
                        .iter()
                        .map(|(key, value)| (key.clone(), shift(value)))
                        .collect(),
                    original_len: *original_len,
                },
                DecodedObject::Cell(value) => DecodedObject::Cell(shift(value)),
                DecodedObject::Media(media) => DecodedObject::Media(DecodedMedia {
                    kind: media.kind,
                    mime_type: media.mime_type.clone(),
                    source: match &media.source {
                        DecodedMediaSource::Url { url, data } => DecodedMediaSource::Url {
                            url: url.clone(),
                            data: data.as_ref().map(content),
                        },
                        DecodedMediaSource::File { path, data } => DecodedMediaSource::File {
                            path: path.clone(),
                            data: data.as_ref().map(content),
                        },
                        DecodedMediaSource::Base64 { data } => DecodedMediaSource::Base64 {
                            data: content(data),
                        },
                    },
                }),
                DecodedObject::Uint8Array { .. }
                | DecodedObject::Declaration { .. }
                | DecodedObject::NonSnapshotable
                | DecodedObject::Descriptive { .. }
                | DecodedObject::Truncated(_) => object.clone(),
            });
        }
        Ok(shift(root))
    }

    /// `id` names what was built: a reader that walks several values at
    /// once tells their blobs apart by ID, so two different compositions
    /// must not share one.
    fn finish(self, id: CasId, root: DecodedValue) -> DecodedSnapshot {
        DecodedSnapshot {
            id,
            encoded_len: 0,
            children: self.children,
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
    /// The recording the value belongs to (its index row), which scopes the
    /// type tags it names; 0 when unknown.
    pub rec: i64,
}

const HANDLE_MAGIC: [u8; 2] = [0xB7, 0x02];
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
        out.extend_from_slice(&self.rec.to_le_bytes());
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
        let (rec, rest) = rest.split_at_checked(8)?;
        let rec = i64::from_le_bytes(rec.try_into().ok()?);
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
            rec,
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
            rec: 0,
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
    // `__btel_ref(kind, cas, names, pending, rec)`: no CAS id is NULL
    // (nothing captured) unless `pending` says the capture's evidence is
    // still due. `rec` scopes the type tags the value names.
    conn.create_scalar_function("__btel_ref", 5, pure, |ctx| {
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
            rec: opt_i64(ctx, 4)?.unwrap_or(0),
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
                equality::captured(&*context, a, op, b, &context.limits.comparison)
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
            equality::json(&*context, left, op, &json, &context.limits.comparison)
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

/// An inline value's handle: its snapshot travels inside the handle, which
/// carries one blob, so the value is kept whole.
pub(crate) fn inline_handle(value: &Inline) -> Vec<u8> {
    let pool = btel_snapshot::SnapshotPool::new(1, btel_snapshot::Limits::default());
    let mut builder = pool.try_acquire().expect("a fresh pool has a slot");
    let root = build_inline(&mut builder, value);
    let mut whole = btel_snapshot::Shaper::new(btel_snapshot::ShapePolicy::Whole);
    let snapshot = builder.finish(root, &mut whole);
    let mut blob = Vec::new();
    snapshot
        .root_blob()
        .write(&mut btel_snapshot::BlobScratch::default(), &mut blob)
        .expect("writing to memory");
    Handle {
        kind: INLINE,
        pending: false,
        cas: [0; 16],
        inline: Some(blob),
        names: None,
        path: Vec::new(),
        rec: 0,
    }
    .encode()
}

/// Children first: a container's items are written in one run.
fn build_inline(b: &mut btel_snapshot::Builder, value: &Inline) -> btel_snapshot::SnapshotValue {
    use btel_snapshot::{BexStr, Limit, OwnedType, SnapshotValue};
    let object = match value {
        Inline::Null => return SnapshotValue::Null,
        Inline::Int(n) => return SnapshotValue::Int(*n),
        Inline::Float(f) => return SnapshotValue::Float(*f),
        Inline::Text(text) => return b.leaves().string_value(&BexStr::from(text.as_str())),
        Inline::Map(entries) => {
            let values: Vec<SnapshotValue> =
                entries.iter().map(|(_, v)| build_inline(b, v)).collect();
            let key_type = b.leaves().ty(OwnedType::string());
            let value_type = b.leaves().ty(OwnedType::unknown());
            b.map(
                key_type,
                value_type,
                entries.iter().zip(values),
                |leaves, ((key, _), value)| {
                    (leaves.string_value(&BexStr::from(key.as_str())), value)
                },
            )
        }
        Inline::List(items) => {
            let values: Vec<SnapshotValue> = items.iter().map(|v| build_inline(b, v)).collect();
            let element_type = b.leaves().ty(OwnedType::unknown());
            b.list(element_type, values.into_iter(), |_, value| value)
        }
    };
    b.leaves().object(object).map_or(
        SnapshotValue::Truncated(Limit::Objects),
        SnapshotValue::Object,
    )
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
    /// `None` once any turn's model has no price or the amount overflows.
    cost: Option<crate::pricing::Usd>,
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
            cost: Some(crate::pricing::Usd::default()),
            ..Usage::default()
        });
        let add = |sum: Option<i64>, n: Option<i64>| match (sum, n) {
            (sum, None) => sum,
            (sum, Some(n)) => Some(sum.unwrap_or(0).saturating_add(n)),
        };
        usage.cost = usage
            .cost
            .zip(crate::pricing::cost_nano_usd(
                model.as_deref(),
                input,
                output,
                cache_read,
                cache_write,
            ))
            .and_then(|(a, b)| a.checked_add(b));
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
        // The local-query cost column remains dollars; convert only at presentation.
        let dollars = |cost: crate::pricing::Usd| {
            #[expect(
                clippy::cast_precision_loss,
                reason = "local queries expose dollar floats after integer estimation and aggregation"
            )]
            let nano_usd = cost.as_nano_usd() as f64;
            Inline::Float(nano_usd / 1_000_000_000.0)
        };
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
                    .map_or(Inline::Null, dollars),
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
            rec: 0,
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
