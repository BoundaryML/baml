use std::collections::{BTreeSet, VecDeque};

use bex_events::{
    run::TraceCallKey,
    value::{
        CaptureLossReason, ValueAvailability, ValueCaptureKind, ValueFileRecord,
        read_bamlvalue_from_bytes,
    },
    value_cas::{Cid, referenced_cids},
};

#[cfg(feature = "native")]
use std::{
    fs,
    path::{Path, PathBuf},
};

#[cfg(feature = "native")]
use bex_events::value_cas::{
    NODE_CODEC_VERSION, PackIndex, PackIndexEntry, read_pack_chunk, scan_pack,
};

use crate::{
    BqfBuilder, BqfFrame, Column, Completeness, FileId, FrameFlags, FrameKind, HARD_MAX_BYTES,
    QueryError, SourceSnapshot, SourceWatermark,
};

const VALUE_INSPECTOR_OVERHEAD: usize = 1024;
const VALUE_REF_ROW_BYTES: usize = 320;
const VALUE_DAG_ROW_BYTES: usize = 256;
const MAX_VALUE_REFS: usize = 10_000;
const MAX_VALUE_NODES: usize = 4_096;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum InspectorAvailability {
    Pending = 1,
    Available = 2,
    Missing = 3,
    Omitted = 4,
    Lost = 5,
    Promoted = 6,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValueCallKey {
    pub process_euid: [u8; 16],
    pub engine_id: u64,
    pub logical_thread_id: u64,
    pub call_id: u64,
}

impl From<TraceCallKey> for ValueCallKey {
    fn from(call: TraceCallKey) -> Self {
        Self {
            process_euid: call.process_euid.0,
            engine_id: call.engine_id.0,
            logical_thread_id: call.thread_id.0,
            call_id: call.call_id.0,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValueRefRow {
    pub value_ref_id: String,
    pub role: String,
    pub availability: InspectorAvailability,
    pub original_size_bytes: Option<usize>,
    pub retained_size_bytes: Option<usize>,
    pub diagnostic: Option<String>,
    pub call: Option<ValueCallKey>,
    pub promotion_trigger: Option<String>,
    pub root_cid: Option<Cid>,
    pub node_codec_version: Option<u16>,
    pub logical_len: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValueLossRow {
    pub reason: CaptureLossReason,
    pub skipped_count: u64,
    pub call: Option<ValueCallKey>,
    pub message: Option<String>,
    pub timestamp_ms: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ValueRefsRequest {
    pub max_rows: usize,
    pub max_bytes: usize,
}

impl Default for ValueRefsRequest {
    fn default() -> Self {
        Self {
            max_rows: 1_000,
            max_bytes: crate::DEFAULT_MAX_BYTES,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValueRefsResponse {
    pub values: Vec<ValueRefRow>,
    pub losses: Vec<ValueLossRow>,
    pub meta: Completeness,
}

impl ValueRefsResponse {
    pub fn to_bqf(&self, request_id: u64, max_bytes: usize) -> Result<BqfFrame, QueryError> {
        let nrows = u32::try_from(self.values.len())
            .map_err(|_| QueryError::invalid_request("too many value-reference rows"))?;
        let mut builder = BqfBuilder::new(
            FrameKind::ValueRefs,
            request_id,
            crate::bqf::data_epoch(&self.meta),
            nrows,
        )
        .with_flags(FrameFlags::from_meta(&self.meta));
        builder.push(Column::Utf8 {
            id: 1,
            values: self
                .values
                .iter()
                .map(|row| row.value_ref_id.clone())
                .collect(),
        })?;
        builder.push(Column::Utf8 {
            id: 2,
            values: self.values.iter().map(|row| row.role.clone()).collect(),
        })?;
        builder.push(Column::U8 {
            id: 3,
            values: self
                .values
                .iter()
                .map(|row| row.availability as u8)
                .collect(),
        })?;
        builder.push(Column::U64 {
            id: 4,
            values: self
                .values
                .iter()
                .map(|row| {
                    row.original_size_bytes
                        .and_then(|value| u64::try_from(value).ok())
                        .unwrap_or(u64::MAX)
                })
                .collect(),
        })?;
        builder.push(Column::U64 {
            id: 5,
            values: self
                .values
                .iter()
                .map(|row| {
                    row.retained_size_bytes
                        .and_then(|value| u64::try_from(value).ok())
                        .unwrap_or(u64::MAX)
                })
                .collect(),
        })?;
        builder.push(Column::Utf8 {
            id: 6,
            values: self
                .values
                .iter()
                .map(|row| row.diagnostic.clone().unwrap_or_default())
                .collect(),
        })?;
        builder.push(Column::Utf8 {
            id: 7,
            values: self
                .values
                .iter()
                .map(|row| row.promotion_trigger.clone().unwrap_or_default())
                .collect(),
        })?;
        builder.push(Column::Utf8 {
            id: 8,
            values: self
                .values
                .iter()
                .map(|row| row.root_cid.map(Cid::to_hex).unwrap_or_default())
                .collect(),
        })?;
        builder.push(Column::U64 {
            id: 9,
            values: self
                .values
                .iter()
                .map(|row| row.logical_len.unwrap_or(u64::MAX))
                .collect(),
        })?;
        builder.finish(max_bytes)
    }
}

/// Parses one committed `.bamlvalue` prefix into the value-inspector surface.
pub fn inspect_value_file(
    file: FileId,
    bytes: &[u8],
    request: ValueRefsRequest,
) -> Result<ValueRefsResponse, QueryError> {
    validate_value_request(request)?;
    let contents = read_bamlvalue_from_bytes(bytes)?;
    let mut values = Vec::new();
    let mut losses = Vec::new();
    for record in contents.records {
        match record {
            ValueFileRecord::CapturedValue(record) => {
                let capture = record.capture;
                let promotion_trigger = capture
                    .as_ref()
                    .and_then(|capture| capture.promotion_trigger.clone());
                let availability = if promotion_trigger.is_some() {
                    InspectorAvailability::Promoted
                } else {
                    inspector_availability(record.value_ref.availability)
                };
                values.push(ValueRefRow {
                    value_ref_id: record.value_ref.id,
                    role: capture.as_ref().map_or_else(
                        || "value".to_owned(),
                        |capture| role(capture.kind).to_owned(),
                    ),
                    availability,
                    original_size_bytes: record.value_ref.original_size_bytes,
                    retained_size_bytes: record.value_ref.retained_size_bytes,
                    diagnostic: record.value_ref.diagnostic,
                    call: capture.map(|capture| capture.call.into()),
                    promotion_trigger,
                    root_cid: record
                        .dag_ref
                        .as_ref()
                        .map(|dag| Cid::from_bytes(dag.root_cid)),
                    node_codec_version: record.dag_ref.as_ref().map(|dag| dag.node_codec_version),
                    logical_len: record.dag_ref.map(|dag| dag.logical_len),
                });
            }
            ValueFileRecord::LogEvent(record) => {
                values.push(ValueRefRow {
                    value_ref_id: record.value_ref.id,
                    role: "logBody".to_owned(),
                    availability: inspector_availability(record.value_ref.availability),
                    original_size_bytes: record.value_ref.original_size_bytes,
                    retained_size_bytes: record.value_ref.retained_size_bytes,
                    diagnostic: record.value_ref.diagnostic,
                    call: Some(record.event.call.into()),
                    promotion_trigger: None,
                    root_cid: record
                        .dag_ref
                        .as_ref()
                        .map(|dag| Cid::from_bytes(dag.root_cid)),
                    node_codec_version: record.dag_ref.as_ref().map(|dag| dag.node_codec_version),
                    logical_len: record.dag_ref.map(|dag| dag.logical_len),
                });
            }
            ValueFileRecord::CaptureLoss(loss) => {
                values.push(ValueRefRow {
                    value_ref_id: format!(
                        "capture-loss-{}-{}",
                        loss.timestamp_ms, loss.skipped_count
                    ),
                    role: "captureLoss".to_owned(),
                    availability: InspectorAvailability::Lost,
                    original_size_bytes: None,
                    retained_size_bytes: None,
                    diagnostic: Some(loss.message.clone().unwrap_or_else(|| {
                        format!(
                            "{:?}: {} captured value(s) unavailable",
                            loss.reason, loss.skipped_count
                        )
                    })),
                    call: loss.call.map(Into::into),
                    promotion_trigger: None,
                    root_cid: None,
                    node_codec_version: None,
                    logical_len: None,
                });
                losses.push(ValueLossRow {
                    reason: loss.reason,
                    skipped_count: loss.skipped_count,
                    call: loss.call.map(Into::into),
                    message: loss.message,
                    timestamp_ms: loss.timestamp_ms,
                });
            }
            ValueFileRecord::Audit(_)
            | ValueFileRecord::RunStarted(_)
            | ValueFileRecord::RunCompleted(_) => {}
        }
    }

    let budget = request
        .max_rows
        .min(value_row_budget(request.max_bytes))
        .min(MAX_VALUE_REFS);
    let total = values.len();
    if values.len() > budget {
        values.truncate(budget);
    }
    losses.truncate(request.max_rows);
    let truncated = total > values.len();
    let mut meta = Completeness {
        complete: !contents.truncated && !truncated && losses.is_empty(),
        capture_loss: Vec::new(),
        sources_consulted: vec![file],
        truncated,
        partial_tail: contents.truncated,
        snapshot: vec![SourceWatermark {
            file,
            source: SourceSnapshot {
                committed_len: bytes.len() as u64,
                generation: 0,
            },
            parsed_through: bytes.len() as u64,
        }],
        ..Completeness::default()
    };
    if contents.truncated {
        meta.warnings
            .push("value metadata has a partial tail; only committed records are shown".to_owned());
    }
    if truncated {
        meta.warnings
            .push("value inspector rows were truncated by the response budget".to_owned());
    }
    if !losses.is_empty() {
        meta.warnings.push(
            "capture-loss records are present; missing values are not equivalent to null"
                .to_owned(),
        );
    }
    meta.finalize();
    Ok(ValueRefsResponse {
        values,
        losses,
        meta,
    })
}

#[cfg(feature = "native")]
pub fn list_value_refs(
    boundary_dir: &Path,
    request: ValueRefsRequest,
) -> Result<ValueRefsResponse, QueryError> {
    validate_value_request(request)?;
    let mut paths = value_files(boundary_dir)?;
    paths.sort();
    let mut response = ValueRefsResponse {
        values: Vec::new(),
        losses: Vec::new(),
        meta: Completeness {
            complete: true,
            ..Completeness::default()
        },
    };
    for path in paths {
        let bytes = fs::read(&path)?;
        let file = FileId(stable_path_id(&path));
        let parsed = inspect_value_file(file, &bytes, request)?;
        response.values.extend(parsed.values);
        response.losses.extend(parsed.losses);
        response
            .meta
            .sources_consulted
            .extend(parsed.meta.sources_consulted);
        response.meta.snapshot.extend(parsed.meta.snapshot);
        response.meta.warnings.extend(parsed.meta.warnings);
        response.meta.partial_tail |= parsed.meta.partial_tail;
        response.meta.truncated |= parsed.meta.truncated;
        response.meta.complete &= parsed.meta.complete;
    }
    let budget = request.max_rows.min(value_row_budget(request.max_bytes));
    let total = response.values.len();
    if response.values.len() > budget {
        response.values.truncate(budget);
    }
    response.losses.truncate(request.max_rows);
    if total > response.values.len() {
        response.meta.truncated = true;
        response
            .meta
            .warnings
            .push("value files exceeded the combined inspector budget".to_owned());
    }
    if response.meta.sources_consulted.is_empty() {
        response.meta.warnings.push(format!(
            "no .bamlvalue artifacts found under {}",
            boundary_dir.display()
        ));
    }
    response.meta.finalize();
    Ok(response)
}

pub trait ValueChunkSource {
    fn read_chunk(&self, cid: Cid) -> Result<Option<StoredValueChunk>, QueryError>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoredValueChunk {
    pub cid: Cid,
    pub logical_len: u64,
    pub canonical_bytes: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HydratedValueNode {
    pub cid: Cid,
    pub depth: u16,
    pub logical_len: u64,
    pub canonical_bytes: Option<Vec<u8>>,
    pub child_cids: Vec<Cid>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValueHydration {
    pub root: Cid,
    pub nodes: Vec<HydratedValueNode>,
    pub resume_cids: Vec<Cid>,
    pub bytes_returned: usize,
    pub truncated: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum ValueDagRowKind {
    Node = 1,
    Child = 2,
    Resume = 3,
    DiffNode = 4,
    DiffLeftChild = 5,
    DiffRightChild = 6,
    DiffResume = 7,
}

#[derive(Clone, Debug)]
struct ValueDagWireRow {
    kind: ValueDagRowKind,
    primary_cid: String,
    secondary_cid: String,
    depth: u16,
    ordinal: u32,
    logical_len: u64,
    equal: bool,
    canonical_loaded: bool,
}

impl ValueHydration {
    /// Encodes a bounded, flattened DAG skeleton. Child edges are separate
    /// rows so callers can navigate directly by CID without nested JSON.
    pub fn to_bqf(&self, request_id: u64, max_bytes: usize) -> Result<BqfFrame, QueryError> {
        validate_value_dag_frame_budget(max_bytes)?;
        let row_cap = value_dag_row_budget(max_bytes);
        let mut rows = Vec::new();
        let mut wire_truncated = false;
        for node in &self.nodes {
            if !push_value_dag_row(
                &mut rows,
                row_cap,
                ValueDagWireRow {
                    kind: ValueDagRowKind::Node,
                    primary_cid: node.cid.to_hex(),
                    secondary_cid: String::new(),
                    depth: node.depth,
                    ordinal: 0,
                    logical_len: node.logical_len,
                    equal: false,
                    canonical_loaded: node.canonical_bytes.is_some(),
                },
            ) {
                wire_truncated = true;
                break;
            }
            for (ordinal, child) in node.child_cids.iter().enumerate() {
                if !push_value_dag_row(
                    &mut rows,
                    row_cap,
                    ValueDagWireRow {
                        kind: ValueDagRowKind::Child,
                        primary_cid: node.cid.to_hex(),
                        secondary_cid: child.to_hex(),
                        depth: node.depth.saturating_add(1),
                        ordinal: u32::try_from(ordinal).unwrap_or(u32::MAX),
                        logical_len: u64::MAX,
                        equal: false,
                        canonical_loaded: false,
                    },
                ) {
                    wire_truncated = true;
                    break;
                }
            }
            if wire_truncated {
                break;
            }
        }
        if !wire_truncated {
            for cid in &self.resume_cids {
                if !push_value_dag_row(
                    &mut rows,
                    row_cap,
                    ValueDagWireRow {
                        kind: ValueDagRowKind::Resume,
                        primary_cid: cid.to_hex(),
                        secondary_cid: String::new(),
                        depth: 0,
                        ordinal: 0,
                        logical_len: u64::MAX,
                        equal: false,
                        canonical_loaded: false,
                    },
                ) {
                    wire_truncated = true;
                    break;
                }
            }
        }
        value_dag_frame(
            request_id,
            max_bytes,
            rows,
            self.truncated || wire_truncated,
        )
    }
}

/// Bounded skeleton hydration. Nodes whose canonical bytes do not fit remain
/// useful navigation records containing child CIDs.
pub fn hydrate_value(
    source: &impl ValueChunkSource,
    root: Cid,
    max_depth: u16,
    max_nodes: usize,
    max_bytes: usize,
) -> Result<ValueHydration, QueryError> {
    validate_hydration_budget(max_nodes, max_bytes)?;
    let mut queue = VecDeque::from([(root, 0_u16)]);
    let mut seen = BTreeSet::new();
    let mut nodes = Vec::new();
    let mut resume_cids = Vec::new();
    let mut bytes_returned = 0_usize;
    let mut truncated = false;
    while let Some((cid, depth)) = queue.pop_front() {
        if !seen.insert(cid) {
            continue;
        }
        if nodes.len() >= max_nodes {
            resume_cids.push(cid);
            resume_cids.extend(queue.drain(..).map(|(cid, _)| cid));
            truncated = true;
            break;
        }
        let chunk = source
            .read_chunk(cid)?
            .ok_or_else(|| QueryError::NotFound(format!("value chunk {cid}")))?;
        if Cid::for_node(&chunk.canonical_bytes) != cid {
            return Err(QueryError::invalid_data(format!(
                "value chunk {cid} failed CID verification"
            )));
        }
        let child_cids = referenced_cids(&chunk.canonical_bytes)
            .map_err(|error| QueryError::invalid_data(error.to_string()))?;
        let canonical_bytes =
            if bytes_returned.saturating_add(chunk.canonical_bytes.len()) <= max_bytes {
                bytes_returned = bytes_returned.saturating_add(chunk.canonical_bytes.len());
                Some(chunk.canonical_bytes)
            } else {
                truncated = true;
                None
            };
        if depth < max_depth {
            queue.extend(
                child_cids
                    .iter()
                    .copied()
                    .map(|child| (child, depth.saturating_add(1))),
            );
        } else {
            resume_cids.extend(child_cids.iter().copied());
        }
        nodes.push(HydratedValueNode {
            cid,
            depth,
            logical_len: chunk.logical_len,
            canonical_bytes,
            child_cids,
        });
    }
    resume_cids.sort();
    resume_cids.dedup();
    Ok(ValueHydration {
        root,
        nodes,
        resume_cids,
        bytes_returned,
        truncated,
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValueDiffNode {
    pub left: Option<Cid>,
    pub right: Option<Cid>,
    pub equal: bool,
    pub left_children: Vec<Cid>,
    pub right_children: Vec<Cid>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValueDiffResponse {
    pub equal: bool,
    pub nodes: Vec<ValueDiffNode>,
    pub resume_pairs: Vec<(Option<Cid>, Option<Cid>)>,
    pub bytes_read: usize,
    pub truncated: bool,
}

impl ValueDiffResponse {
    /// Encodes Merkle comparison nodes and their independently navigable
    /// left/right child edges in the same bounded ValueDag frame schema.
    pub fn to_bqf(&self, request_id: u64, max_bytes: usize) -> Result<BqfFrame, QueryError> {
        validate_value_dag_frame_budget(max_bytes)?;
        let row_cap = value_dag_row_budget(max_bytes);
        let mut rows = Vec::new();
        let mut wire_truncated = false;
        for node in &self.nodes {
            if !push_value_dag_row(
                &mut rows,
                row_cap,
                ValueDagWireRow {
                    kind: ValueDagRowKind::DiffNode,
                    primary_cid: node.left.map(Cid::to_hex).unwrap_or_default(),
                    secondary_cid: node.right.map(Cid::to_hex).unwrap_or_default(),
                    depth: 0,
                    ordinal: 0,
                    logical_len: u64::MAX,
                    equal: node.equal,
                    canonical_loaded: false,
                },
            ) {
                wire_truncated = true;
                break;
            }
            for (ordinal, child) in node.left_children.iter().enumerate() {
                if !push_value_dag_row(
                    &mut rows,
                    row_cap,
                    ValueDagWireRow {
                        kind: ValueDagRowKind::DiffLeftChild,
                        primary_cid: node.left.map(Cid::to_hex).unwrap_or_default(),
                        secondary_cid: child.to_hex(),
                        depth: 0,
                        ordinal: u32::try_from(ordinal).unwrap_or(u32::MAX),
                        logical_len: u64::MAX,
                        equal: false,
                        canonical_loaded: false,
                    },
                ) {
                    wire_truncated = true;
                    break;
                }
            }
            if wire_truncated {
                break;
            }
            for (ordinal, child) in node.right_children.iter().enumerate() {
                if !push_value_dag_row(
                    &mut rows,
                    row_cap,
                    ValueDagWireRow {
                        kind: ValueDagRowKind::DiffRightChild,
                        primary_cid: node.right.map(Cid::to_hex).unwrap_or_default(),
                        secondary_cid: child.to_hex(),
                        depth: 0,
                        ordinal: u32::try_from(ordinal).unwrap_or(u32::MAX),
                        logical_len: u64::MAX,
                        equal: false,
                        canonical_loaded: false,
                    },
                ) {
                    wire_truncated = true;
                    break;
                }
            }
            if wire_truncated {
                break;
            }
        }
        if !wire_truncated {
            for (left, right) in &self.resume_pairs {
                if !push_value_dag_row(
                    &mut rows,
                    row_cap,
                    ValueDagWireRow {
                        kind: ValueDagRowKind::DiffResume,
                        primary_cid: left.map(Cid::to_hex).unwrap_or_default(),
                        secondary_cid: right.map(Cid::to_hex).unwrap_or_default(),
                        depth: 0,
                        ordinal: 0,
                        logical_len: u64::MAX,
                        equal: false,
                        canonical_loaded: false,
                    },
                ) {
                    wire_truncated = true;
                    break;
                }
            }
        }
        value_dag_frame(
            request_id,
            max_bytes,
            rows,
            self.truncated || wire_truncated,
        )
    }
}

/// Merkle-short-circuit DAG diff. Equal CIDs emit one row without reading the
/// pack; changed nodes read only enough descendants to honor the node/byte
/// budgets.
pub fn diff_values(
    source: &impl ValueChunkSource,
    left: Cid,
    right: Cid,
    max_nodes: usize,
    max_bytes: usize,
) -> Result<ValueDiffResponse, QueryError> {
    validate_hydration_budget(max_nodes, max_bytes)?;
    if left == right {
        return Ok(ValueDiffResponse {
            equal: true,
            nodes: vec![ValueDiffNode {
                left: Some(left),
                right: Some(right),
                equal: true,
                left_children: Vec::new(),
                right_children: Vec::new(),
            }],
            resume_pairs: Vec::new(),
            bytes_read: 0,
            truncated: false,
        });
    }
    let mut queue = VecDeque::from([(Some(left), Some(right))]);
    let mut nodes = Vec::new();
    let mut resume_pairs = Vec::new();
    let mut bytes_read = 0_usize;
    let mut truncated = false;
    while let Some((left, right)) = queue.pop_front() {
        if nodes.len() >= max_nodes {
            resume_pairs.push((left, right));
            resume_pairs.extend(queue.drain(..));
            truncated = true;
            break;
        }
        if left == right {
            nodes.push(ValueDiffNode {
                left,
                right,
                equal: true,
                left_children: Vec::new(),
                right_children: Vec::new(),
            });
            continue;
        }
        let left_chunk = left
            .map(|cid| load_verified(source, cid))
            .transpose()?
            .flatten();
        let right_chunk = right
            .map(|cid| load_verified(source, cid))
            .transpose()?
            .flatten();
        let required = left_chunk
            .as_ref()
            .map_or(0, |chunk| chunk.canonical_bytes.len())
            .saturating_add(
                right_chunk
                    .as_ref()
                    .map_or(0, |chunk| chunk.canonical_bytes.len()),
            );
        if bytes_read.saturating_add(required) > max_bytes {
            resume_pairs.push((left, right));
            resume_pairs.extend(queue.drain(..));
            truncated = true;
            break;
        }
        bytes_read = bytes_read.saturating_add(required);
        let left_children = left_chunk
            .as_ref()
            .map(|chunk| referenced_cids(&chunk.canonical_bytes))
            .transpose()
            .map_err(|error| QueryError::invalid_data(error.to_string()))?
            .unwrap_or_default();
        let right_children = right_chunk
            .as_ref()
            .map(|chunk| referenced_cids(&chunk.canonical_bytes))
            .transpose()
            .map_err(|error| QueryError::invalid_data(error.to_string()))?
            .unwrap_or_default();
        let child_count = left_children.len().max(right_children.len());
        queue.extend((0..child_count).map(|index| {
            (
                left_children.get(index).copied(),
                right_children.get(index).copied(),
            )
        }));
        nodes.push(ValueDiffNode {
            left,
            right,
            equal: false,
            left_children,
            right_children,
        });
    }
    Ok(ValueDiffResponse {
        equal: false,
        nodes,
        resume_pairs,
        bytes_read,
        truncated,
    })
}

fn load_verified(
    source: &impl ValueChunkSource,
    cid: Cid,
) -> Result<Option<StoredValueChunk>, QueryError> {
    let chunk = source.read_chunk(cid)?;
    if let Some(chunk) = &chunk
        && Cid::for_node(&chunk.canonical_bytes) != cid
    {
        return Err(QueryError::invalid_data(format!(
            "value chunk {cid} failed CID verification"
        )));
    }
    Ok(chunk)
}

fn push_value_dag_row(
    rows: &mut Vec<ValueDagWireRow>,
    row_cap: usize,
    row: ValueDagWireRow,
) -> bool {
    if rows.len() >= row_cap {
        return false;
    }
    rows.push(row);
    true
}

fn value_dag_frame(
    request_id: u64,
    max_bytes: usize,
    mut rows: Vec<ValueDagWireRow>,
    source_truncated: bool,
) -> Result<BqfFrame, QueryError> {
    let mut transport_truncated = false;
    loop {
        let nrows = u32::try_from(rows.len())
            .map_err(|_| QueryError::invalid_request("too many value DAG rows"))?;
        let mut meta = Completeness {
            complete: !source_truncated && !transport_truncated,
            truncated: source_truncated || transport_truncated,
            ..Completeness::default()
        };
        meta.finalize();
        let mut builder = BqfBuilder::new(FrameKind::ValueDag, request_id, 0, nrows)
            .with_flags(FrameFlags::from_meta(&meta));
        builder.push(Column::U8 {
            id: 1,
            values: rows.iter().map(|row| row.kind as u8).collect(),
        })?;
        builder.push(Column::Utf8 {
            id: 2,
            values: rows.iter().map(|row| row.primary_cid.clone()).collect(),
        })?;
        builder.push(Column::Utf8 {
            id: 3,
            values: rows.iter().map(|row| row.secondary_cid.clone()).collect(),
        })?;
        builder.push(Column::U16 {
            id: 4,
            values: rows.iter().map(|row| row.depth).collect(),
        })?;
        builder.push(Column::U32 {
            id: 5,
            values: rows.iter().map(|row| row.ordinal).collect(),
        })?;
        builder.push(Column::U64 {
            id: 6,
            values: rows.iter().map(|row| row.logical_len).collect(),
        })?;
        builder.push(Column::U8 {
            id: 7,
            values: rows.iter().map(|row| u8::from(row.equal)).collect(),
        })?;
        builder.push(Column::U8 {
            id: 8,
            values: rows
                .iter()
                .map(|row| u8::from(row.canonical_loaded))
                .collect(),
        })?;
        match builder.finish(max_bytes) {
            Ok(frame) => return Ok(frame),
            Err(QueryError::BudgetExceeded { .. }) if !rows.is_empty() => {
                rows.pop();
                transport_truncated = true;
            }
            Err(error) => return Err(error),
        }
    }
}

fn validate_value_dag_frame_budget(max_bytes: usize) -> Result<(), QueryError> {
    if !(VALUE_INSPECTOR_OVERHEAD..=HARD_MAX_BYTES).contains(&max_bytes) {
        return Err(QueryError::invalid_request(format!(
            "value DAG frame max_bytes must be in {VALUE_INSPECTOR_OVERHEAD}..={HARD_MAX_BYTES}"
        )));
    }
    Ok(())
}

fn value_dag_row_budget(max_bytes: usize) -> usize {
    max_bytes
        .saturating_sub(VALUE_INSPECTOR_OVERHEAD)
        .checked_div(VALUE_DAG_ROW_BYTES)
        .unwrap_or(0)
        .max(1)
}

#[cfg(feature = "native")]
#[derive(Clone, Debug)]
struct PackLocation {
    pack: PathBuf,
    index: Option<PackIndex>,
    active_entries: Vec<PackIndexEntry>,
}

/// Newest-first reader over `[boundary/export/, project .baml/store/packs/]`.
#[cfg(feature = "native")]
#[derive(Clone, Debug, Default)]
pub struct NativeValueStore {
    locations: Vec<PackLocation>,
}

#[cfg(feature = "native")]
impl NativeValueStore {
    pub fn open(boundary_dir: &Path, project_root: &Path) -> Result<Self, QueryError> {
        let mut packs = Vec::new();
        collect_packs(&boundary_dir.join("export"), &mut packs)?;
        collect_packs(&project_root.join(".baml/store/packs"), &mut packs)?;
        packs.sort();
        packs.dedup();
        packs.reverse();
        let mut locations = Vec::new();
        for pack in packs {
            let index_path = PathBuf::from(format!("{}.idx", pack.display()));
            if index_path.is_file() {
                locations.push(PackLocation {
                    index: Some(PackIndex::read(&index_path, &pack)?),
                    pack,
                    active_entries: Vec::new(),
                });
            } else {
                let scan = scan_pack(&pack)?;
                let mut active_entries = scan.entries;
                active_entries.sort_by_key(|entry| entry.cid);
                locations.push(PackLocation {
                    pack,
                    index: None,
                    active_entries,
                });
            }
        }
        Ok(Self { locations })
    }
}

#[cfg(feature = "native")]
impl ValueChunkSource for NativeValueStore {
    fn read_chunk(&self, cid: Cid) -> Result<Option<StoredValueChunk>, QueryError> {
        if NODE_CODEC_VERSION == 0 {
            return Err(QueryError::invalid_data(
                "unsupported zero value node codec",
            ));
        }
        for location in &self.locations {
            let entry = location
                .index
                .as_ref()
                .and_then(|index| index.find(cid))
                .or_else(|| {
                    location
                        .active_entries
                        .binary_search_by_key(&cid, |entry| entry.cid)
                        .ok()
                        .map(|index| location.active_entries[index])
                });
            if let Some(entry) = entry {
                return Ok(Some(StoredValueChunk {
                    cid,
                    logical_len: u64::from(entry.logical_len),
                    canonical_bytes: read_pack_chunk(&location.pack, entry)?,
                }));
            }
        }
        Ok(None)
    }
}

fn validate_value_request(request: ValueRefsRequest) -> Result<(), QueryError> {
    if request.max_rows == 0 || request.max_rows > MAX_VALUE_REFS {
        return Err(QueryError::invalid_request(format!(
            "max_rows must be in 1..={MAX_VALUE_REFS}"
        )));
    }
    if !(VALUE_INSPECTOR_OVERHEAD..=HARD_MAX_BYTES).contains(&request.max_bytes) {
        return Err(QueryError::invalid_request(format!(
            "max_bytes must be in {VALUE_INSPECTOR_OVERHEAD}..={HARD_MAX_BYTES}"
        )));
    }
    Ok(())
}

fn validate_hydration_budget(max_nodes: usize, max_bytes: usize) -> Result<(), QueryError> {
    if max_nodes == 0 || max_nodes > MAX_VALUE_NODES {
        return Err(QueryError::invalid_request(format!(
            "max_nodes must be in 1..={MAX_VALUE_NODES}"
        )));
    }
    if max_bytes == 0 || max_bytes > HARD_MAX_BYTES {
        return Err(QueryError::invalid_request(format!(
            "max_bytes must be in 1..={HARD_MAX_BYTES}"
        )));
    }
    Ok(())
}

fn value_row_budget(max_bytes: usize) -> usize {
    max_bytes
        .saturating_sub(VALUE_INSPECTOR_OVERHEAD)
        .checked_div(VALUE_REF_ROW_BYTES)
        .unwrap_or(0)
        .max(1)
}

fn inspector_availability(value: ValueAvailability) -> InspectorAvailability {
    match value {
        ValueAvailability::Pending => InspectorAvailability::Pending,
        ValueAvailability::Available => InspectorAvailability::Available,
        ValueAvailability::Missing => InspectorAvailability::Missing,
        ValueAvailability::Omitted => InspectorAvailability::Omitted,
        ValueAvailability::Lost => InspectorAvailability::Lost,
    }
}

fn role(kind: ValueCaptureKind) -> &'static str {
    match kind {
        ValueCaptureKind::RootInput => "rootInput",
        ValueCaptureKind::RootOutput => "rootOutput",
        ValueCaptureKind::RootError => "rootError",
        ValueCaptureKind::LogBody => "logBody",
        ValueCaptureKind::CallOutput => "callOutput",
        ValueCaptureKind::CallError => "callError",
        ValueCaptureKind::CallInput => "callInput",
    }
}

#[cfg(feature = "native")]
fn value_files(root: &Path) -> Result<Vec<PathBuf>, QueryError> {
    let mut output = Vec::new();
    let mut queue = VecDeque::from([root.to_path_buf()]);
    while let Some(directory) = queue.pop_front() {
        let entries = match fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error.into()),
        };
        for entry in entries {
            let path = entry?.path();
            if path.is_dir() {
                queue.push_back(path);
            } else if path.extension().and_then(|extension| extension.to_str()) == Some("bamlvalue")
            {
                output.push(path);
            }
        }
    }
    Ok(output)
}

#[cfg(feature = "native")]
fn collect_packs(root: &Path, output: &mut Vec<PathBuf>) -> Result<(), QueryError> {
    let entries = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    for entry in entries {
        let path = entry?.path();
        if path.is_dir() {
            collect_packs(&path, output)?;
        } else if path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.ends_with(".bamlpack"))
        {
            output.push(path);
        }
    }
    Ok(())
}

#[cfg(feature = "native")]
fn stable_path_id(path: &Path) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in path.as_os_str().as_encoded_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use bex_events::value_cas::{CanonicalValue, encode_value_dag};

    use super::*;

    struct DagSource(BTreeMap<Cid, StoredValueChunk>);

    impl ValueChunkSource for DagSource {
        fn read_chunk(&self, cid: Cid) -> Result<Option<StoredValueChunk>, QueryError> {
            Ok(self.0.get(&cid).cloned())
        }
    }

    fn source(value: CanonicalValue) -> (Cid, DagSource) {
        let dag = encode_value_dag(&value).unwrap();
        let chunks = dag
            .chunks
            .into_iter()
            .map(|chunk| {
                (
                    chunk.cid,
                    StoredValueChunk {
                        cid: chunk.cid,
                        logical_len: chunk.logical_len,
                        canonical_bytes: chunk.canonical_bytes,
                    },
                )
            })
            .collect();
        (dag.root, DagSource(chunks))
    }

    #[test]
    fn identical_value_diff_reads_zero_bytes() {
        let (root, source) = source(CanonicalValue::String("same".to_owned()));
        let diff = diff_values(&source, root, root, 10, 1024).unwrap();
        assert!(diff.equal);
        assert_eq!(diff.bytes_read, 0);
        let frame = diff.to_bqf(7, 4096).unwrap();
        let header = frame.header().unwrap();
        assert_eq!(header.kind, FrameKind::ValueDag);
        assert_eq!(header.nrows, 1);
    }

    #[test]
    fn hydration_returns_child_navigation_when_body_budget_is_small() {
        let (root, source) = source(CanonicalValue::List(
            (0..200)
                .map(|index| CanonicalValue::String(format!("value-{index}")))
                .collect(),
        ));
        let hydration = hydrate_value(&source, root, 1, 32, 1).unwrap();
        assert!(hydration.truncated);
        assert_eq!(hydration.nodes[0].canonical_bytes, None);
        assert!(!hydration.nodes[0].child_cids.is_empty());
        let frame = hydration.to_bqf(8, 4096).unwrap();
        let header = frame.header().unwrap();
        assert_eq!(header.kind, FrameKind::ValueDag);
        assert!(header.flags.contains(FrameFlags::TRUNCATED));
    }
}
