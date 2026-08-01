//! Native BQF1 observability websocket.
//!
//! Control messages are small JSON text frames. Query results are exclusively
//! bounded BQF1 binary frames; subscriptions use acknowledgement-based,
//! one-frame-in-flight flow control.

use std::{
    collections::{BTreeMap, HashMap},
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use axum::extract::ws::{Message, WebSocket};
use bex_events::revision_dictionary::{
    RevisionId,
    file::{DictionaryReadError, RevisionDictionaryStore},
};
use bex_events::value_cas::Cid;
use bex_query::{
    BqfBuilder, BqfFrame, Column, DiffRequest, FileSource, FrameFlags, FrameKind,
    FunctionDictionary, LeftHeavyRequest, ListRunsRequest, LiveFrameGate, LiveFrameOffer,
    NativeValueStore, QueryEngine, QueryError, QueryPoll, RunCursor, SandwichRequest,
    SearchRequest, ValueChunkSource, ValueRefsRequest, Viewport,
    bql::{
        BqlCursor, ExecuteOptions, NativeBqlEngine, QueryMeta, ScriptResult, SnapshotToken,
        schema_json,
    },
    diff_values, hydrate_value, list_runs, list_value_refs, open_run_meta,
};
use futures::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};

const OBS_PROTOCOL_VERSION: u16 = 1;
const OBS_TIMER_INTERVAL: Duration = Duration::from_millis(10);
const BQL_MAX_SOURCE_BYTES: usize = 64 * 1024;
const BQL_MAX_PARAMS: usize = 64;
const DICTIONARY_CACHE_BYTES: usize = 32 * 1024 * 1024;
const MAX_VALUE_DEPTH: u16 = 64;
const MAX_VALUE_NODES: usize = 4_096;

struct CachedDictionary {
    dictionary: Arc<FunctionDictionary>,
    retained_bytes: usize,
    last_used: u64,
}

struct DictionaryCache {
    entries: HashMap<[u8; 32], CachedDictionary>,
    retained_bytes: usize,
    max_bytes: usize,
    clock: u64,
}

impl DictionaryCache {
    fn new(max_bytes: usize) -> Self {
        Self {
            entries: HashMap::new(),
            retained_bytes: 0,
            max_bytes,
            clock: 0,
        }
    }

    fn get(&mut self, revision_id: [u8; 32]) -> Option<Arc<FunctionDictionary>> {
        self.clock = self.clock.saturating_add(1);
        let entry = self.entries.get_mut(&revision_id)?;
        entry.last_used = self.clock;
        Some(Arc::clone(&entry.dictionary))
    }

    fn insert(
        &mut self,
        revision_id: [u8; 32],
        dictionary: FunctionDictionary,
    ) -> Arc<FunctionDictionary> {
        let dictionary = Arc::new(dictionary);
        let retained_bytes = dictionary_retained_bytes(&dictionary);
        if retained_bytes > self.max_bytes {
            return dictionary;
        }
        if let Some(previous) = self.entries.remove(&revision_id) {
            self.retained_bytes = self.retained_bytes.saturating_sub(previous.retained_bytes);
        }
        while self.retained_bytes.saturating_add(retained_bytes) > self.max_bytes {
            let Some((&oldest, _)) = self.entries.iter().min_by_key(|(_, entry)| entry.last_used)
            else {
                break;
            };
            if let Some(removed) = self.entries.remove(&oldest) {
                self.retained_bytes = self.retained_bytes.saturating_sub(removed.retained_bytes);
            }
        }
        self.clock = self.clock.saturating_add(1);
        self.retained_bytes = self.retained_bytes.saturating_add(retained_bytes);
        self.entries.insert(
            revision_id,
            CachedDictionary {
                dictionary: Arc::clone(&dictionary),
                retained_bytes,
                last_used: self.clock,
            },
        );
        dictionary
    }
}

fn dictionary_retained_bytes(dictionary: &FunctionDictionary) -> usize {
    std::mem::size_of::<FunctionDictionary>()
        .saturating_add(
            dictionary
                .functions
                .len()
                .saturating_mul(std::mem::size_of::<bex_query::FunctionIdentity>()),
        )
        .saturating_add(
            dictionary
                .functions
                .iter()
                .fold(0_usize, |bytes, function| {
                    bytes
                        .saturating_add(function.definition_key.len())
                        .saturating_add(function.fqn.len())
                }),
        )
}

#[derive(Clone)]
pub(crate) struct ObserveState {
    roots: Arc<Vec<PathBuf>>,
    engine: Arc<QueryEngine<FileSource>>,
    bql: Arc<NativeBqlEngine>,
    dictionaries: Arc<Mutex<DictionaryCache>>,
}

impl ObserveState {
    pub(crate) fn new(roots: Arc<Vec<PathBuf>>) -> Self {
        Self {
            bql: Arc::new(NativeBqlEngine::new((*roots).clone())),
            roots,
            engine: Arc::new(QueryEngine::new(FileSource::new())),
            dictionaries: Arc::new(Mutex::new(DictionaryCache::new(DICTIONARY_CACHE_BYTES))),
        }
    }

    fn find_run(&self, boundary_id: &str) -> Result<bex_query::RunMeta, QueryError> {
        for directory in bex_events::history::path::list_boundary_dirs(&self.roots) {
            let meta = match open_run_meta(&directory) {
                Ok(meta) => meta,
                Err(_) => continue,
            };
            if meta.summary.boundary_id_wire == boundary_id {
                return Ok(meta);
            }
        }
        Err(QueryError::NotFound(format!("boundary {boundary_id}")))
    }

    fn project_root(meta: &bex_query::RunMeta) -> Result<PathBuf, QueryError> {
        meta.boundary_dir
            .parent()
            .and_then(std::path::Path::parent)
            .and_then(std::path::Path::parent)
            .map(std::path::Path::to_path_buf)
            .ok_or_else(|| {
                QueryError::InvalidData("boundary directory is not under .baml/history".to_owned())
            })
    }

    fn dictionary_for_run(
        &self,
        meta: &bex_query::RunMeta,
    ) -> Result<Arc<FunctionDictionary>, QueryError> {
        let revision_bytes = meta.summary.revision_id.ok_or_else(|| {
            QueryError::CapabilityUnavailable(format!(
                "run {} has no persisted revision id; stable function search and diff are disabled",
                meta.summary.boundary_id_wire
            ))
        })?;
        if let Some(dictionary) = self
            .dictionaries
            .lock()
            .map_err(|_| QueryError::InvalidData("dictionary cache mutex poisoned".to_owned()))?
            .get(revision_bytes)
        {
            return Ok(dictionary);
        }
        let revision_id = RevisionId::from_bytes(revision_bytes);
        let project_root = Self::project_root(meta)?;
        let dictionary =
            load_function_dictionary(&project_root, revision_id, &meta.summary.boundary_id_wire)?;
        Ok(self
            .dictionaries
            .lock()
            .map_err(|_| QueryError::InvalidData("dictionary cache mutex poisoned".to_owned()))?
            .insert(revision_bytes, dictionary))
    }

    fn value_store_for_run(&self, boundary_id: &str) -> Result<NativeValueStore, QueryError> {
        let meta = self.find_run(boundary_id)?;
        let project_root = Self::project_root(&meta)?;
        NativeValueStore::open(&meta.boundary_dir, &project_root)
    }

    pub(crate) fn read_value_chunk(
        &self,
        boundary_id: &str,
        cid: &str,
    ) -> Result<Vec<u8>, QueryError> {
        let meta = self.find_run(boundary_id)?;
        let project_root = Self::project_root(&meta)?;
        let cid = cid
            .parse()
            .map_err(|error: bex_events::value_cas::CidParseError| {
                QueryError::InvalidRequest(error.to_string())
            })?;
        NativeValueStore::open(&meta.boundary_dir, &project_root)?
            .read_chunk(cid)?
            .map(|chunk| chunk.canonical_bytes)
            .ok_or_else(|| QueryError::NotFound(format!("value chunk {cid}")))
    }
}

struct PairedValueStore {
    first: NativeValueStore,
    second: NativeValueStore,
}

impl ValueChunkSource for PairedValueStore {
    fn read_chunk(&self, cid: Cid) -> Result<Option<bex_query::StoredValueChunk>, QueryError> {
        match self.first.read_chunk(cid)? {
            Some(chunk) => Ok(Some(chunk)),
            None => self.second.read_chunk(cid),
        }
    }
}

fn parse_value_cid(value: &str) -> Result<Cid, QueryError> {
    value
        .parse()
        .map_err(|error: bex_events::value_cas::CidParseError| {
            QueryError::InvalidRequest(error.to_string())
        })
}

fn load_function_dictionary(
    project_root: &std::path::Path,
    revision_id: RevisionId,
    boundary_id: &str,
) -> Result<FunctionDictionary, QueryError> {
    let dictionary = RevisionDictionaryStore::new(project_root)
        .read(revision_id)
        .map_err(|error| match error {
            DictionaryReadError::DictionaryMissing { .. } => {
                QueryError::CapabilityUnavailable(format!(
                    "run {boundary_id} references {revision_id}, but its .bamldict artifact is \
                     missing; recompile the same revision to regenerate it"
                ))
            }
            DictionaryReadError::InvalidData(error) => QueryError::InvalidData(format!(
                "revision dictionary {revision_id} failed validation: {error}"
            )),
            DictionaryReadError::Io(error) => QueryError::Io(error),
        })?;
    if dictionary.identity.revision_id != revision_id {
        return Err(QueryError::InvalidData(format!(
            "run {boundary_id} references {revision_id}, but the loaded dictionary names {}",
            dictionary.identity.revision_id
        )));
    }
    Ok(FunctionDictionary::from_revision_dictionary(&dictionary))
}

// Deserialize through wire structs so all validation remains at the boundary.
#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ViewportWire {
    start_ns: u64,
    end_ns: u64,
    pixel_width: u32,
    lanes: u16,
    #[serde(default = "default_max_bytes")]
    max_bytes: usize,
}

impl ViewportWire {
    fn into_viewport(self) -> Result<Viewport, QueryError> {
        Viewport {
            start_ns: self.start_ns,
            end_ns: self.end_ns,
            pixel_width: self.pixel_width,
            lanes: self.lanes,
            max_bytes: self.max_bytes,
        }
        .validate()
    }
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RunCursorWire {
    created_ms: u64,
    boundary_id: [u8; 16],
}

impl RunCursorWire {
    fn into_cursor(self) -> RunCursor {
        RunCursor {
            created_ms: self.created_ms,
            boundary_id: self.boundary_id,
        }
    }
}

// Internal normalized query retains the public bex_query types.
#[derive(Clone, Debug)]
enum NormalizedObserveQuery {
    Runs {
        limit: u16,
        max_bytes: usize,
        cursor: Option<RunCursor>,
    },
    Timeline {
        boundary_id: String,
        viewport: Viewport,
    },
    LeftHeavy {
        boundary_id: String,
        pixel_width: u32,
        max_bytes: usize,
    },
    Sandwich {
        boundary_id: String,
        function_id: u32,
        caller_depth: u16,
        callee_depth: u16,
        max_rows: usize,
        max_bytes: usize,
    },
    ValueRefs {
        boundary_id: String,
        max_rows: usize,
        max_bytes: usize,
    },
    ValueDag {
        boundary_id: String,
        root_cid: Cid,
        max_depth: u16,
        max_nodes: usize,
        max_bytes: usize,
    },
    ValueDiff {
        left_boundary_id: String,
        left_root_cid: Cid,
        right_boundary_id: String,
        right_root_cid: Cid,
        max_nodes: usize,
        max_bytes: usize,
    },
    Search {
        boundary_id: String,
        text: String,
        max_rows: usize,
        max_bytes: usize,
    },
    Diff {
        left_boundary_id: String,
        right_boundary_id: String,
        max_rows: usize,
        max_bytes: usize,
    },
    Bql {
        source: String,
        max_rows: usize,
        max_bytes: usize,
        cursor: Option<BqlCursor>,
        snapshot: Option<SnapshotToken>,
        params: BTreeMap<String, String>,
    },
    BqlSchema {
        max_bytes: usize,
    },
}

#[derive(Debug, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
enum ClientMessage {
    Query {
        request_id: u64,
        query: WireQuery,
    },
    Sub {
        subscription_id: u64,
        query: WireQuery,
        #[serde(default = "default_rate_hz")]
        rate_hz: u8,
    },
    SetViewport {
        subscription_id: u64,
        viewport: ViewportWire,
    },
    Ack {
        subscription_id: u64,
    },
    Unsub {
        subscription_id: u64,
    },
}

#[derive(Clone, Debug, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
enum WireQuery {
    Runs {
        #[serde(default = "default_run_limit")]
        limit: u16,
        #[serde(default = "default_max_bytes")]
        max_bytes: usize,
        #[serde(default)]
        cursor: Option<RunCursorWire>,
    },
    Timeline {
        boundary_id: String,
        viewport: ViewportWire,
    },
    LeftHeavy {
        boundary_id: String,
        pixel_width: u32,
        #[serde(default = "default_max_bytes")]
        max_bytes: usize,
    },
    Sandwich {
        boundary_id: String,
        function_id: u32,
        #[serde(default = "default_sandwich_depth")]
        caller_depth: u16,
        #[serde(default = "default_sandwich_depth")]
        callee_depth: u16,
        #[serde(default = "default_result_rows")]
        max_rows: usize,
        #[serde(default = "default_max_bytes")]
        max_bytes: usize,
    },
    ValueRefs {
        boundary_id: String,
        #[serde(default = "default_result_rows")]
        max_rows: usize,
        #[serde(default = "default_max_bytes")]
        max_bytes: usize,
    },
    ValueDag {
        boundary_id: String,
        root_cid: String,
        #[serde(default = "default_value_depth")]
        max_depth: u16,
        #[serde(default = "default_value_nodes")]
        max_nodes: usize,
        #[serde(default = "default_max_bytes")]
        max_bytes: usize,
    },
    ValueDiff {
        left_boundary_id: String,
        left_root_cid: String,
        right_boundary_id: String,
        right_root_cid: String,
        #[serde(default = "default_value_nodes")]
        max_nodes: usize,
        #[serde(default = "default_max_bytes")]
        max_bytes: usize,
    },
    Search {
        boundary_id: String,
        text: String,
        #[serde(default = "default_search_rows")]
        max_rows: usize,
        #[serde(default = "default_max_bytes")]
        max_bytes: usize,
    },
    Diff {
        left_boundary_id: String,
        right_boundary_id: String,
        #[serde(default = "default_result_rows")]
        max_rows: usize,
        #[serde(default = "default_max_bytes")]
        max_bytes: usize,
    },
    Bql {
        source: String,
        #[serde(default = "default_result_rows")]
        max_rows: usize,
        #[serde(default = "default_max_bytes")]
        max_bytes: usize,
        #[serde(default)]
        cursor: Option<String>,
        #[serde(default)]
        snapshot: Option<String>,
        #[serde(default)]
        params: BTreeMap<String, String>,
    },
    BqlSchema {
        #[serde(default = "default_max_bytes")]
        max_bytes: usize,
    },
}

impl WireQuery {
    fn normalize(self) -> Result<NormalizedObserveQuery, QueryError> {
        match self {
            Self::Runs {
                limit,
                max_bytes,
                cursor,
            } => Ok(NormalizedObserveQuery::Runs {
                limit,
                max_bytes,
                cursor: cursor.map(RunCursorWire::into_cursor),
            }),
            Self::Timeline {
                boundary_id,
                viewport,
            } => Ok(NormalizedObserveQuery::Timeline {
                boundary_id,
                viewport: viewport.into_viewport()?,
            }),
            Self::LeftHeavy {
                boundary_id,
                pixel_width,
                max_bytes,
            } => Ok(NormalizedObserveQuery::LeftHeavy {
                boundary_id,
                pixel_width,
                max_bytes,
            }),
            Self::Sandwich {
                boundary_id,
                function_id,
                caller_depth,
                callee_depth,
                max_rows,
                max_bytes,
            } => Ok(NormalizedObserveQuery::Sandwich {
                boundary_id,
                function_id,
                caller_depth,
                callee_depth,
                max_rows,
                max_bytes,
            }),
            Self::ValueRefs {
                boundary_id,
                max_rows,
                max_bytes,
            } => Ok(NormalizedObserveQuery::ValueRefs {
                boundary_id,
                max_rows,
                max_bytes,
            }),
            Self::ValueDag {
                boundary_id,
                root_cid,
                max_depth,
                max_nodes,
                max_bytes,
            } => {
                validate_value_dag_query(max_depth, max_nodes, max_bytes)?;
                Ok(NormalizedObserveQuery::ValueDag {
                    boundary_id,
                    root_cid: parse_value_cid(&root_cid)?,
                    max_depth,
                    max_nodes,
                    max_bytes,
                })
            }
            Self::ValueDiff {
                left_boundary_id,
                left_root_cid,
                right_boundary_id,
                right_root_cid,
                max_nodes,
                max_bytes,
            } => {
                validate_value_dag_query(0, max_nodes, max_bytes)?;
                Ok(NormalizedObserveQuery::ValueDiff {
                    left_boundary_id,
                    left_root_cid: parse_value_cid(&left_root_cid)?,
                    right_boundary_id,
                    right_root_cid: parse_value_cid(&right_root_cid)?,
                    max_nodes,
                    max_bytes,
                })
            }
            Self::Search {
                boundary_id,
                text,
                max_rows,
                max_bytes,
            } => Ok(NormalizedObserveQuery::Search {
                boundary_id,
                text,
                max_rows,
                max_bytes,
            }),
            Self::Diff {
                left_boundary_id,
                right_boundary_id,
                max_rows,
                max_bytes,
            } => Ok(NormalizedObserveQuery::Diff {
                left_boundary_id,
                right_boundary_id,
                max_rows,
                max_bytes,
            }),
            Self::Bql {
                source,
                max_rows,
                max_bytes,
                cursor,
                snapshot,
                params,
            } => {
                if source.len() > BQL_MAX_SOURCE_BYTES {
                    return Err(QueryError::InvalidRequest(format!(
                        "BQL source must not exceed {BQL_MAX_SOURCE_BYTES} bytes"
                    )));
                }
                let param_bytes = params.iter().fold(0_usize, |bytes, (key, value)| {
                    bytes.saturating_add(key.len()).saturating_add(value.len())
                });
                if params.len() > BQL_MAX_PARAMS || param_bytes > BQL_MAX_SOURCE_BYTES {
                    return Err(QueryError::InvalidRequest(format!(
                        "BQL params are limited to {BQL_MAX_PARAMS} entries and \
                         {BQL_MAX_SOURCE_BYTES} total bytes"
                    )));
                }
                Ok(NormalizedObserveQuery::Bql {
                    source,
                    max_rows,
                    max_bytes,
                    cursor: cursor.as_deref().map(BqlCursor::parse).transpose()?,
                    snapshot: snapshot.as_deref().map(SnapshotToken::parse).transpose()?,
                    params,
                })
            }
            Self::BqlSchema { max_bytes } => Ok(NormalizedObserveQuery::BqlSchema { max_bytes }),
        }
    }
}

impl NormalizedObserveQuery {
    fn max_bytes(&self) -> usize {
        match self {
            Self::Runs { max_bytes, .. }
            | Self::LeftHeavy { max_bytes, .. }
            | Self::Sandwich { max_bytes, .. }
            | Self::ValueRefs { max_bytes, .. }
            | Self::ValueDag { max_bytes, .. }
            | Self::ValueDiff { max_bytes, .. }
            | Self::Search { max_bytes, .. }
            | Self::Diff { max_bytes, .. }
            | Self::Bql { max_bytes, .. }
            | Self::BqlSchema { max_bytes } => *max_bytes,
            Self::Timeline { viewport, .. } => viewport.max_bytes,
        }
    }

    fn supports_subscription(&self) -> bool {
        !matches!(self, Self::Bql { .. } | Self::BqlSchema { .. })
    }

    fn set_viewport(&mut self, viewport: ViewportWire) -> Result<(), QueryError> {
        let viewport = viewport.into_viewport()?;
        let Self::Timeline {
            viewport: current, ..
        } = self
        else {
            return Err(QueryError::InvalidRequest(
                "setViewport applies only to timeline subscriptions".to_owned(),
            ));
        };
        if viewport.max_bytes != current.max_bytes {
            return Err(QueryError::InvalidRequest(
                "setViewport cannot change maxBytes; resubscribe with a new budget".to_owned(),
            ));
        }
        *current = viewport;
        Ok(())
    }
}

impl ObserveState {
    fn render_normalized(
        &self,
        query: &NormalizedObserveQuery,
        request_id: u64,
    ) -> Result<BqfFrame, QueryError> {
        match query {
            NormalizedObserveQuery::Runs {
                limit,
                max_bytes,
                cursor,
            } => list_runs(
                &self.roots,
                ListRunsRequest {
                    limit: *limit,
                    max_bytes: *max_bytes,
                    cursor: *cursor,
                },
            )?
            .to_bqf(request_id, *max_bytes),
            NormalizedObserveQuery::Timeline {
                boundary_id,
                viewport,
            } => {
                let meta = self.find_run(boundary_id)?;
                let run = self.engine.register_native_run(&meta)?;
                self.engine.refresh_native_run(&run)?;
                match self
                    .engine
                    .timeline(&run.files, run.partition_id, *viewport)?
                {
                    QueryPoll::Ready(response) => response.to_bqf(request_id, viewport.max_bytes),
                    QueryPoll::NeedData { .. } => Err(QueryError::InvalidData(
                        "native observability source unexpectedly requested resident bytes"
                            .to_owned(),
                    )),
                }
            }
            NormalizedObserveQuery::LeftHeavy {
                boundary_id,
                pixel_width,
                max_bytes,
            } => {
                let meta = self.find_run(boundary_id)?;
                let run = self.engine.register_native_run(&meta)?;
                self.engine.refresh_native_run(&run)?;
                match self.engine.left_heavy(
                    &run.files,
                    run.partition_id,
                    LeftHeavyRequest {
                        pixel_width: *pixel_width,
                        max_bytes: *max_bytes,
                    },
                )? {
                    QueryPoll::Ready(response) => response.to_bqf(request_id, *max_bytes),
                    QueryPoll::NeedData { .. } => Err(QueryError::InvalidData(
                        "native observability source unexpectedly requested resident bytes"
                            .to_owned(),
                    )),
                }
            }
            NormalizedObserveQuery::Sandwich {
                boundary_id,
                function_id,
                caller_depth,
                callee_depth,
                max_rows,
                max_bytes,
            } => {
                let meta = self.find_run(boundary_id)?;
                let run = self.engine.register_native_run(&meta)?;
                self.engine.refresh_native_run(&run)?;
                match self.engine.sandwich(
                    &run.files,
                    run.partition_id,
                    SandwichRequest {
                        function_id: *function_id,
                        caller_depth: *caller_depth,
                        callee_depth: *callee_depth,
                        max_rows: *max_rows,
                        max_bytes: *max_bytes,
                    },
                )? {
                    QueryPoll::Ready(response) => response.to_bqf(request_id, *max_bytes),
                    QueryPoll::NeedData { .. } => Err(QueryError::InvalidData(
                        "native observability source unexpectedly requested resident bytes"
                            .to_owned(),
                    )),
                }
            }
            NormalizedObserveQuery::ValueRefs {
                boundary_id,
                max_rows,
                max_bytes,
            } => {
                let meta = self.find_run(boundary_id)?;
                list_value_refs(
                    &meta.boundary_dir,
                    ValueRefsRequest {
                        max_rows: *max_rows,
                        max_bytes: *max_bytes,
                    },
                )?
                .to_bqf(request_id, *max_bytes)
            }
            NormalizedObserveQuery::ValueDag {
                boundary_id,
                root_cid,
                max_depth,
                max_nodes,
                max_bytes,
            } => {
                let store = self.value_store_for_run(boundary_id)?;
                let hydration = hydrate_value(
                    &store,
                    *root_cid,
                    *max_depth,
                    *max_nodes,
                    max_bytes.saturating_sub(1024).max(1),
                )?;
                hydration.to_bqf(request_id, *max_bytes)
            }
            NormalizedObserveQuery::ValueDiff {
                left_boundary_id,
                left_root_cid,
                right_boundary_id,
                right_root_cid,
                max_nodes,
                max_bytes,
            } => {
                let left = self.value_store_for_run(left_boundary_id)?;
                let right = self.value_store_for_run(right_boundary_id)?;
                if left.read_chunk(*left_root_cid)?.is_none() {
                    return Err(QueryError::NotFound(format!(
                        "value chunk {left_root_cid} in boundary {left_boundary_id}"
                    )));
                }
                if right.read_chunk(*right_root_cid)?.is_none() {
                    return Err(QueryError::NotFound(format!(
                        "value chunk {right_root_cid} in boundary {right_boundary_id}"
                    )));
                }
                let store = PairedValueStore {
                    first: left,
                    second: right,
                };
                let diff = diff_values(
                    &store,
                    *left_root_cid,
                    *right_root_cid,
                    *max_nodes,
                    max_bytes.saturating_sub(1024).max(1),
                )?;
                diff.to_bqf(request_id, *max_bytes)
            }
            NormalizedObserveQuery::Search {
                boundary_id,
                text,
                max_rows,
                max_bytes,
            } => {
                let meta = self.find_run(boundary_id)?;
                let dictionary = self.dictionary_for_run(&meta)?;
                let run = self.engine.register_native_run(&meta)?;
                self.engine.refresh_native_run(&run)?;
                match self.engine.search(
                    &run.files,
                    run.partition_id,
                    &dictionary,
                    &SearchRequest {
                        text: text.clone(),
                        max_rows: *max_rows,
                        max_bytes: *max_bytes,
                    },
                )? {
                    QueryPoll::Ready(response) => response.to_bqf(request_id, *max_bytes),
                    QueryPoll::NeedData { .. } => Err(QueryError::InvalidData(
                        "native observability source unexpectedly requested resident bytes"
                            .to_owned(),
                    )),
                }
            }
            NormalizedObserveQuery::Diff {
                left_boundary_id,
                right_boundary_id,
                max_rows,
                max_bytes,
            } => {
                let left_meta = self.find_run(left_boundary_id)?;
                let right_meta = self.find_run(right_boundary_id)?;
                let left_dictionary = self.dictionary_for_run(&left_meta)?;
                let right_dictionary = self.dictionary_for_run(&right_meta)?;
                let left_run = self.engine.register_native_run(&left_meta)?;
                let right_run = self.engine.register_native_run(&right_meta)?;
                self.engine.refresh_native_run(&left_run)?;
                self.engine.refresh_native_run(&right_run)?;
                match self.engine.diff(
                    &left_run.files,
                    left_run.partition_id,
                    &left_dictionary,
                    &right_run.files,
                    right_run.partition_id,
                    &right_dictionary,
                    DiffRequest {
                        max_rows: *max_rows,
                        max_bytes: *max_bytes,
                    },
                )? {
                    QueryPoll::Ready(response) => response.to_bqf(request_id, *max_bytes),
                    QueryPoll::NeedData { .. } => Err(QueryError::InvalidData(
                        "native observability source unexpectedly requested resident bytes"
                            .to_owned(),
                    )),
                }
            }
            NormalizedObserveQuery::Bql {
                source,
                max_rows,
                max_bytes,
                cursor,
                snapshot,
                params,
            } => {
                let result = self.bql.query(
                    source,
                    ExecuteOptions {
                        max_rows: *max_rows,
                        max_bytes: *max_bytes,
                        cursor: *cursor,
                        snapshot: snapshot.clone(),
                        params: params.clone(),
                    },
                )?;
                bql_result_to_bqf(result, request_id, *max_bytes)
            }
            NormalizedObserveQuery::BqlSchema { max_bytes } => {
                bql_schema_to_bqf(request_id, *max_bytes)
            }
        }
    }
}

fn bql_result_to_bqf(
    mut script: ScriptResult,
    request_id: u64,
    max_bytes: usize,
) -> Result<BqfFrame, QueryError> {
    loop {
        match build_bql_frame(&script, request_id, max_bytes) {
            Ok(frame) => return Ok(frame),
            Err(QueryError::BudgetExceeded {
                required,
                max_bytes,
            }) => {
                let Some(result) = script
                    .results
                    .iter_mut()
                    .rev()
                    .find(|result| !result.result.rows.is_empty())
                else {
                    return build_bql_frame(&script, request_id, max_bytes);
                };
                let current_bytes = serde_json::to_vec(&result.result.rows)
                    .map_err(|error| QueryError::InvalidData(error.to_string()))?
                    .len();
                let target_bytes = current_bytes
                    .saturating_sub(required.saturating_sub(max_bytes))
                    .saturating_sub(256);
                let mut low = 0;
                let mut high = result.result.rows.len();
                while low < high {
                    let middle = low + (high - low).div_ceil(2);
                    let bytes = serde_json::to_vec(&result.result.rows[..middle])
                        .map_err(|error| QueryError::InvalidData(error.to_string()))?
                        .len();
                    if bytes <= target_bytes {
                        low = middle;
                    } else {
                        high = middle - 1;
                    }
                }
                let retained = low.min(result.result.rows.len().saturating_sub(1));
                result.result.rows.truncate(retained);
                result.result.meta.complete = false;
                result.result.meta.truncated = true;
                if !result
                    .result
                    .meta
                    .warnings
                    .iter()
                    .any(|warning| warning == "transport maxBytes truncated BQL result")
                {
                    result
                        .result
                        .meta
                        .warnings
                        .push("transport maxBytes truncated BQL result".to_owned());
                }
            }
            Err(error) => return Err(error),
        }
    }
}

fn build_bql_frame(
    script: &ScriptResult,
    request_id: u64,
    max_bytes: usize,
) -> Result<BqfFrame, QueryError> {
    let mut names = Vec::with_capacity(script.results.len());
    let mut kinds = Vec::with_capacity(script.results.len());
    let mut columns = Vec::with_capacity(script.results.len());
    let mut rows = Vec::with_capacity(script.results.len());
    let mut metadata = Vec::with_capacity(script.results.len());
    let mut flags = FrameFlags::default();
    let mut data_epoch = 0_u64;
    if !script.results.is_empty()
        && script
            .results
            .iter()
            .all(|named| named.result.meta.complete)
    {
        flags.insert(FrameFlags::COMPLETE);
    }
    for named in &script.results {
        names.push(named.name.clone().unwrap_or_default());
        kinds.push(json_string(&named.result.kind)?);
        columns.push(json_string(&named.result.columns)?);
        rows.push(json_string(&named.result.rows)?);
        metadata.push(json_string(&named.result.meta)?);
        accumulate_bql_flags(&mut flags, &named.result.meta);
        data_epoch = data_epoch.max(
            named
                .result
                .meta
                .watermarks
                .iter()
                .map(|watermark| watermark.wall_epoch_ns)
                .max()
                .unwrap_or(0),
        );
    }
    let nrows = u32::try_from(script.results.len())
        .map_err(|_| QueryError::InvalidRequest("too many BQL result sets".to_owned()))?;
    let mut builder =
        BqfBuilder::new(FrameKind::Query, request_id, data_epoch, nrows).with_flags(flags);
    builder.push(Column::Utf8 {
        id: 1,
        values: names,
    })?;
    builder.push(Column::Utf8 {
        id: 2,
        values: kinds,
    })?;
    builder.push(Column::Utf8 {
        id: 3,
        values: columns,
    })?;
    builder.push(Column::Utf8 {
        id: 4,
        values: rows,
    })?;
    builder.push(Column::Utf8 {
        id: 5,
        values: metadata,
    })?;
    builder.finish(max_bytes)
}

fn bql_schema_to_bqf(request_id: u64, max_bytes: usize) -> Result<BqfFrame, QueryError> {
    let meta = QueryMeta {
        complete: true,
        watermarks: Vec::new(),
        capture_loss: Vec::new(),
        sources_consulted: Vec::new(),
        truncated: false,
        next_cursor: None,
        warnings: Vec::new(),
        snapshot: "bqsnap_1_".to_owned(),
    };
    let mut builder =
        BqfBuilder::new(FrameKind::Query, request_id, 0, 1).with_flags(FrameFlags::COMPLETE);
    builder.push(Column::Utf8 {
        id: 1,
        values: vec!["schema".to_owned()],
    })?;
    builder.push(Column::Utf8 {
        id: 2,
        values: vec![json_string(&"schema")?],
    })?;
    builder.push(Column::Utf8 {
        id: 3,
        values: vec!["[]".to_owned()],
    })?;
    builder.push(Column::Utf8 {
        id: 4,
        values: vec![format!("[{}]", schema_json())],
    })?;
    builder.push(Column::Utf8 {
        id: 5,
        values: vec![json_string(&meta)?],
    })?;
    builder.finish(max_bytes)
}

fn json_string(value: &impl Serialize) -> Result<String, QueryError> {
    serde_json::to_string(value).map_err(|error| QueryError::InvalidData(error.to_string()))
}

fn accumulate_bql_flags(flags: &mut FrameFlags, meta: &QueryMeta) {
    if meta.truncated {
        flags.insert(FrameFlags::TRUNCATED);
    }
    if !meta.capture_loss.is_empty() {
        flags.insert(FrameFlags::CAPTURE_LOSS);
    }
}

#[derive(Debug)]
struct Subscription {
    query: NormalizedObserveQuery,
    gate: LiveFrameGate,
}

#[derive(Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
enum ServerControl<'a> {
    Hello {
        protocol: &'static str,
        version: u16,
        max_rate_hz: u8,
        max_frame_bytes: usize,
    },
    Error {
        request_id: Option<u64>,
        subscription_id: Option<u64>,
        code: &'static str,
        message: &'a str,
    },
}

pub(crate) async fn session(socket: WebSocket, state: ObserveState) {
    let (mut sink, mut stream) = socket.split();
    let hello = ServerControl::Hello {
        protocol: "BQF1",
        version: OBS_PROTOCOL_VERSION,
        max_rate_hz: bex_query::MAX_LIVE_RATE_HZ,
        max_frame_bytes: bex_query::HARD_MAX_BYTES,
    };
    if send_control(&mut sink, &hello).await.is_err() {
        return;
    }

    let started = Instant::now();
    let mut subscriptions = HashMap::<u64, Subscription>::new();
    let mut timer = tokio::time::interval(OBS_TIMER_INTERVAL);
    timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        tokio::select! {
            incoming = stream.next() => {
                let Some(incoming) = incoming else { break };
                let Ok(message) = incoming else { break };
                match message {
                    Message::Text(text) => {
                        let parsed = serde_json::from_str::<ClientMessage>(&text);
                        match parsed {
                            Ok(message) => {
                                if handle_client_message(
                                    message,
                                    &state,
                                    &started,
                                    &mut subscriptions,
                                    &mut sink,
                                )
                                .await
                                .is_err()
                                {
                                    break;
                                }
                            }
                            Err(error) => {
                                let message = error.to_string();
                                let control = ServerControl::Error {
                                    request_id: None,
                                    subscription_id: None,
                                    code: "E_PROTOCOL",
                                    message: &message,
                                };
                                if send_control(&mut sink, &control).await.is_err() {
                                    break;
                                }
                            }
                        }
                    }
                    Message::Close(_) => break,
                    Message::Ping(bytes) => {
                        if sink.send(Message::Pong(bytes)).await.is_err() {
                            break;
                        }
                    }
                    _ => {}
                }
            }
            _ = timer.tick() => {
                let now_ns = elapsed_ns(&started);
                let ready = subscriptions
                    .iter()
                    .filter(|(_, subscription)| subscription.gate.ready_for_snapshot(now_ns))
                    .map(|(&id, subscription)| (id, subscription.query.clone()))
                    .collect::<Vec<_>>();
                let mut failed = Vec::new();
                for (subscription_id, query) in ready {
                    let result = state.render_normalized(&query, subscription_id);
                    let Some(subscription) = subscriptions.get_mut(&subscription_id) else {
                        continue;
                    };
                    match result.and_then(|frame| subscription.gate.offer(now_ns, frame)) {
                        Ok(LiveFrameOffer::Send(frame)) => {
                            if sink.send(Message::Binary(frame.into_bytes().into())).await.is_err() {
                                return;
                            }
                        }
                        Ok(LiveFrameOffer::Deferred) => {}
                        Err(error) => {
                            let message = error.to_string();
                            let control = ServerControl::Error {
                                request_id: None,
                                subscription_id: Some(subscription_id),
                                code: query_error_code(&error),
                                message: &message,
                            };
                            if send_control(&mut sink, &control).await.is_err() {
                                return;
                            }
                            failed.push(subscription_id);
                        }
                    }
                }
                for subscription_id in failed {
                    subscriptions.remove(&subscription_id);
                }
            }
        }
    }
}

async fn handle_client_message(
    message: ClientMessage,
    state: &ObserveState,
    started: &Instant,
    subscriptions: &mut HashMap<u64, Subscription>,
    sink: &mut futures::stream::SplitSink<WebSocket, Message>,
) -> Result<(), ()> {
    match message {
        ClientMessage::Query { request_id, query } => {
            let result = query
                .normalize()
                .and_then(|query| state.render_normalized(&query, request_id));
            match result {
                Ok(frame) => sink
                    .send(Message::Binary(frame.into_bytes().into()))
                    .await
                    .map_err(|_| ()),
                Err(error) => send_query_error(sink, Some(request_id), None, &error).await,
            }
        }
        ClientMessage::Sub {
            subscription_id,
            query,
            rate_hz,
        } => {
            if subscriptions.contains_key(&subscription_id) {
                let error = QueryError::InvalidRequest(format!(
                    "subscription {subscription_id} already exists; unsubscribe before reusing it"
                ));
                return send_query_error(sink, None, Some(subscription_id), &error).await;
            }
            let query = match query.normalize() {
                Ok(query) => query,
                Err(error) => {
                    return send_query_error(sink, None, Some(subscription_id), &error).await;
                }
            };
            if !query.supports_subscription() {
                let error = QueryError::InvalidRequest(
                    "BQL and BQL schema requests are one-shot queries; live subscriptions remain \
                     available for observation views"
                        .to_owned(),
                );
                return send_query_error(sink, None, Some(subscription_id), &error).await;
            }
            let gate = match LiveFrameGate::new(query.max_bytes(), rate_hz) {
                Ok(gate) => gate,
                Err(error) => {
                    return send_query_error(sink, None, Some(subscription_id), &error).await;
                }
            };
            let now_ns = elapsed_ns(started);
            let frame = match state.render_normalized(&query, subscription_id) {
                Ok(frame) => frame,
                Err(error) => {
                    return send_query_error(sink, None, Some(subscription_id), &error).await;
                }
            };
            let mut subscription = Subscription { query, gate };
            let offer = subscription.gate.offer(now_ns, frame);
            subscriptions.insert(subscription_id, subscription);
            match offer {
                Ok(LiveFrameOffer::Send(frame)) => sink
                    .send(Message::Binary(frame.into_bytes().into()))
                    .await
                    .map_err(|_| ()),
                Ok(LiveFrameOffer::Deferred) => Ok(()),
                Err(error) => send_query_error(sink, None, Some(subscription_id), &error).await,
            }
        }
        ClientMessage::SetViewport {
            subscription_id,
            viewport,
        } => {
            let Some(subscription) = subscriptions.get_mut(&subscription_id) else {
                let error = QueryError::NotFound(format!("subscription {subscription_id}"));
                return send_query_error(sink, None, Some(subscription_id), &error).await;
            };
            if let Err(error) = subscription.query.set_viewport(viewport) {
                return send_query_error(sink, None, Some(subscription_id), &error).await;
            }
            Ok(())
        }
        ClientMessage::Ack { subscription_id } => {
            if let Some(subscription) = subscriptions.get_mut(&subscription_id) {
                subscription.gate.acknowledge();
            }
            Ok(())
        }
        ClientMessage::Unsub { subscription_id } => {
            subscriptions.remove(&subscription_id);
            Ok(())
        }
    }
}

async fn send_query_error(
    sink: &mut futures::stream::SplitSink<WebSocket, Message>,
    request_id: Option<u64>,
    subscription_id: Option<u64>,
    error: &QueryError,
) -> Result<(), ()> {
    let message = error.to_string();
    let control = ServerControl::Error {
        request_id,
        subscription_id,
        code: query_error_code(error),
        message: &message,
    };
    send_control(sink, &control).await.map_err(|_| ())
}

async fn send_control(
    sink: &mut futures::stream::SplitSink<WebSocket, Message>,
    control: &ServerControl<'_>,
) -> Result<(), axum::Error> {
    let json = serde_json::to_string(control).expect("observe control JSON is serializable");
    sink.send(Message::Text(json.into())).await
}

fn elapsed_ns(started: &Instant) -> u64 {
    u64::try_from(started.elapsed().as_nanos()).unwrap_or(u64::MAX)
}

fn query_error_code(error: &QueryError) -> &'static str {
    error.code()
}

const fn default_run_limit() -> u16 {
    100
}

const fn default_max_bytes() -> usize {
    bex_query::DEFAULT_MAX_BYTES
}

const fn default_rate_hz() -> u8 {
    10
}

const fn default_sandwich_depth() -> u16 {
    8
}

const fn default_result_rows() -> usize {
    1_000
}

const fn default_search_rows() -> usize {
    100
}

const fn default_value_depth() -> u16 {
    2
}

const fn default_value_nodes() -> usize {
    256
}

fn validate_value_dag_query(
    max_depth: u16,
    max_nodes: usize,
    max_bytes: usize,
) -> Result<(), QueryError> {
    if max_depth > MAX_VALUE_DEPTH {
        return Err(QueryError::InvalidRequest(format!(
            "maxDepth must be in 0..={MAX_VALUE_DEPTH}"
        )));
    }
    if !(1..=MAX_VALUE_NODES).contains(&max_nodes) {
        return Err(QueryError::InvalidRequest(format!(
            "maxNodes must be in 1..={MAX_VALUE_NODES}"
        )));
    }
    if !(1024..=bex_query::HARD_MAX_BYTES).contains(&max_bytes) {
        return Err(QueryError::InvalidRequest(format!(
            "maxBytes must be in 1024..={}",
            bex_query::HARD_MAX_BYTES
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU32, Ordering};

    use bex_events::revision_dictionary::{
        FIRST_POOL_FUNCTION_ID, FunctionDictRow, FunctionKind, FunctionOrigin, ProgramIdentity,
        RevisionDictionary, SourceSnapshotId,
    };

    use super::*;

    #[test]
    fn protocol_validates_viewport_and_rate_budgets_at_the_boundary() {
        let message = serde_json::from_str::<ClientMessage>(
            r#"{
                "type":"sub",
                "subscriptionId":7,
                "rateHz":30,
                "query":{
                    "kind":"timeline",
                    "boundaryId":"baml_id_1_example",
                    "viewport":{
                        "startNs":1,
                        "endNs":1000,
                        "pixelWidth":800,
                        "lanes":16,
                        "maxBytes":204800
                    }
                }
            }"#,
        )
        .unwrap();
        let ClientMessage::Sub {
            subscription_id,
            query,
            rate_hz,
        } = message
        else {
            panic!("expected subscription");
        };
        assert_eq!(subscription_id, 7);
        assert_eq!(rate_hz, 30);
        let query = query.normalize().unwrap();
        assert_eq!(query.max_bytes(), 204800);
        LiveFrameGate::new(query.max_bytes(), rate_hz).unwrap();
    }

    #[test]
    fn set_viewport_cannot_expand_a_subscription_byte_budget() {
        let mut query = WireQuery::Timeline {
            boundary_id: "run".to_owned(),
            viewport: ViewportWire {
                start_ns: 0,
                end_ns: 10,
                pixel_width: 10,
                lanes: 1,
                max_bytes: 4096,
            },
        }
        .normalize()
        .unwrap();
        let error = query
            .set_viewport(ViewportWire {
                start_ns: 0,
                end_ns: 20,
                pixel_width: 20,
                lanes: 1,
                max_bytes: 8192,
            })
            .unwrap_err();
        assert!(error.to_string().contains("cannot change maxBytes"));
    }

    #[test]
    fn search_and_diff_wire_queries_keep_bounded_native_contracts() {
        let search = serde_json::from_str::<WireQuery>(
            r#"{
                "kind":"search",
                "boundaryId":"left",
                "text":"extract",
                "maxRows":25,
                "maxBytes":8192
            }"#,
        )
        .unwrap()
        .normalize()
        .unwrap();
        assert!(matches!(
            search,
            NormalizedObserveQuery::Search {
                boundary_id,
                text,
                max_rows: 25,
                max_bytes: 8192
            } if boundary_id == "left" && text == "extract"
        ));

        let diff = serde_json::from_str::<WireQuery>(
            r#"{
                "kind":"diff",
                "leftBoundaryId":"before",
                "rightBoundaryId":"after",
                "maxRows":40,
                "maxBytes":16384
            }"#,
        )
        .unwrap()
        .normalize()
        .unwrap();
        assert!(matches!(
            diff,
            NormalizedObserveQuery::Diff {
                left_boundary_id,
                right_boundary_id,
                max_rows: 40,
                max_bytes: 16384
            } if left_boundary_id == "before" && right_boundary_id == "after"
        ));
    }

    #[test]
    fn value_dag_wire_queries_parse_cids_and_enforce_hard_bounds() {
        let cid = Cid::for_node(b"root").to_hex();
        let query = serde_json::from_value::<WireQuery>(serde_json::json!({
            "kind": "valueDag",
            "boundaryId": "run",
            "rootCid": cid,
            "maxBytes": 8192
        }))
        .unwrap()
        .normalize()
        .unwrap();
        assert!(matches!(
            query,
            NormalizedObserveQuery::ValueDag {
                boundary_id,
                max_depth: 2,
                max_nodes: 256,
                max_bytes: 8192,
                ..
            } if boundary_id == "run"
        ));

        let error = serde_json::from_value::<WireQuery>(serde_json::json!({
            "kind": "valueDiff",
            "leftBoundaryId": "left",
            "leftRootCid": Cid::for_node(b"left").to_hex(),
            "rightBoundaryId": "right",
            "rightRootCid": Cid::for_node(b"right").to_hex(),
            "maxNodes": MAX_VALUE_NODES + 1,
            "maxBytes": 8192
        }))
        .unwrap()
        .normalize()
        .unwrap_err();
        assert!(error.to_string().contains("maxNodes"));
    }

    #[test]
    fn bql_wire_query_is_bounded_parsed_and_one_shot() {
        let query = serde_json::from_str::<WireQuery>(
            r#"{
                "kind":"bql",
                "source":"runs(limit=$limit)",
                "maxRows":25,
                "maxBytes":8192,
                "cursor":"bqcur_1_a-000102030405060708090a0b0c0d0e0f",
                "snapshot":"bqsnap_1_",
                "params":{"limit":"10"}
            }"#,
        )
        .unwrap()
        .normalize()
        .unwrap();
        assert!(!query.supports_subscription());
        assert!(matches!(
            query,
            NormalizedObserveQuery::Bql {
                max_rows: 25,
                max_bytes: 8192,
                cursor: Some(BqlCursor { created_ms: 10, .. }),
                snapshot: Some(_),
                ref params,
                ..
            } if params.get("limit").map(String::as_str) == Some("10")
        ));
    }

    #[test]
    fn bql_query_frames_trim_rows_to_the_transport_budget() {
        use bex_query::bql::{NamedQueryResult, QueryEnvelope, SetKind};
        use serde_json::{Map, Value};

        let rows = (0..20)
            .map(|index| {
                let mut row = Map::new();
                row.insert("index".to_owned(), Value::from(index));
                row.insert("payload".to_owned(), Value::String("x".repeat(120)));
                row
            })
            .collect();
        let result = ScriptResult {
            results: vec![NamedQueryResult {
                name: Some("bounded".to_owned()),
                result: QueryEnvelope {
                    kind: SetKind::Table,
                    columns: vec!["index".to_owned(), "payload".to_owned()],
                    rows,
                    meta: QueryMeta {
                        complete: true,
                        watermarks: Vec::new(),
                        capture_loss: Vec::new(),
                        sources_consulted: Vec::new(),
                        truncated: false,
                        next_cursor: None,
                        warnings: Vec::new(),
                        snapshot: "bqsnap_1_".to_owned(),
                    },
                },
            }],
        };
        let frame = bql_result_to_bqf(result, 99, 1024).unwrap();
        assert!(frame.as_bytes().len() <= 1024);
        let header = frame.header().unwrap();
        assert_eq!(header.kind, FrameKind::Query);
        assert_eq!(header.request_id, 99);
        assert!(header.flags.contains(FrameFlags::TRUNCATED));
        assert!(!header.flags.contains(FrameFlags::COMPLETE));
    }

    #[test]
    fn bql_schema_uses_the_same_bounded_query_frame_contract() {
        let frame = bql_schema_to_bqf(42, 512 * 1024).unwrap();
        assert!(frame.as_bytes().len() <= 512 * 1024);
        let header = frame.header().unwrap();
        assert_eq!(header.kind, FrameKind::Query);
        assert_eq!(header.request_id, 42);
        assert!(header.flags.contains(FrameFlags::COMPLETE));
    }

    #[test]
    fn native_dictionary_lookup_projects_stable_identities_and_fails_closed() {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let root = std::env::temp_dir().join(format!(
            "baml-observe-dictionary-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let revision_id = RevisionId::from_bytes([0xA5; 32]);
        let dictionary = RevisionDictionary::new(
            ProgramIdentity {
                revision_id,
                source_snapshot_id: SourceSnapshotId::from_bytes([0x5A; 32]),
                compiler_id: "observe-test".to_owned(),
                function_count: 1,
            },
            1,
            Vec::new(),
            vec![FunctionDictRow {
                function_id: FIRST_POOL_FUNCTION_ID,
                fqn: "user.extract".to_owned(),
                display_name: "extract".to_owned(),
                declared_name: Some("extract".to_owned()),
                source_span: None,
                kind: FunctionKind::Bytecode,
                origin: FunctionOrigin::UserDefined,
                definition_key: "function:user.extract".to_owned(),
                owner_type_key: None,
                lambda: None,
                package_name: Some("user".to_owned()),
                namespace: Vec::new(),
                capture_flags: 0,
                def_content_hash: [7; 32],
                semantic_lanes: None,
            }],
            Vec::new(),
        )
        .unwrap();
        RevisionDictionaryStore::new(&root)
            .ensure_written(&dictionary)
            .unwrap();

        let projected = load_function_dictionary(&root, revision_id, "run").unwrap();
        let row = projected
            .functions
            .iter()
            .find(|row| row.function_id == FIRST_POOL_FUNCTION_ID)
            .unwrap();
        assert_eq!(row.definition_key, "function:user.extract");
        assert_eq!(row.def_content_hash, [7; 32]);

        let retained_bytes = dictionary_retained_bytes(&projected);
        let mut cache = DictionaryCache::new(retained_bytes);
        cache.insert([1; 32], projected.clone());
        cache.insert([2; 32], projected);
        assert!(cache.retained_bytes <= retained_bytes);
        assert!(cache.get([1; 32]).is_none());
        assert!(cache.get([2; 32]).is_some());

        let missing =
            load_function_dictionary(&root, RevisionId::from_bytes([0x11; 32]), "run").unwrap_err();
        assert_eq!(missing.code(), "E_CAPABILITY");
        assert!(
            missing
                .to_string()
                .contains(".bamldict artifact is missing")
        );

        std::fs::remove_dir_all(root).unwrap();
    }
}
