//! Bounded exact-event capture for the flight recorder and opt-in full trace.
//!
//! The flight recorder deliberately retains the producer wire bytes.  Its
//! steady-state work is one `Vec::extend_from_slice` per drained range; it
//! does not allocate or transcode per event.  Trigger dumps perform the cold
//! protobuf conversion later.

use std::collections::{HashMap, VecDeque};

use prost::Message;

use crate::prof::{
    clock::TickConverter,
    encode::encode_disk_event,
    pb,
    record::{self, FunctionEndStatus, RawRecord},
    transcode::to_disk_event,
};
#[cfg(not(target_arch = "wasm32"))]
use crate::{
    ids::BoundaryId,
    prof::{
        encode::encode_length_delimited_message,
        exact_index::{ExactEventPoint, IndexBudget, build_exact_index},
    },
    value_cas::{Cid, CidManifestWriter},
};

#[cfg(not(target_arch = "wasm32"))]
pub const DEFAULT_FLIGHT_RECORDER_BYTES: usize = 16 * 1024 * 1024;
#[cfg(target_arch = "wasm32")]
pub const DEFAULT_FLIGHT_RECORDER_BYTES: usize = 4 * 1024 * 1024;

/// One indivisible retained range. Eviction never slices raw records.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FlightChunk {
    pub engine_id: u64,
    pub first_ticks: u64,
    pub last_ticks: u64,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FlightRecorderStats {
    pub retained_chunks: u64,
    pub retained_bytes: u64,
    pub copied_bytes: u64,
    pub evicted_chunks: u64,
    pub evicted_bytes: u64,
    pub corrupt_ranges: u64,
    /// Greatest timestamp from a completely evicted chunk.
    pub evicted_upto_ticks: Option<u64>,
    /// Set when overload shedding has disabled future copies.
    pub shed: bool,
}

/// FIFO of whole raw drain ranges. A range larger than the capacity is not
/// partially retained because doing so could split the variable-length
/// `StartThread` record.
#[derive(Debug)]
pub struct FlightRecorder {
    capacity_bytes: usize,
    retained_bytes: usize,
    chunks: VecDeque<FlightChunk>,
    stats: FlightRecorderStats,
    enabled: bool,
}

impl FlightRecorder {
    #[must_use]
    pub fn new(capacity_bytes: usize) -> Self {
        Self {
            capacity_bytes,
            retained_bytes: 0,
            chunks: VecDeque::new(),
            stats: FlightRecorderStats::default(),
            enabled: capacity_bytes != 0,
        }
    }

    #[must_use]
    pub fn native_default() -> Self {
        Self::new(DEFAULT_FLIGHT_RECORDER_BYTES)
    }

    #[must_use]
    pub fn capacity_bytes(&self) -> usize {
        self.capacity_bytes
    }

    /// First rung of the overload shedding ladder. Existing evidence remains
    /// dumpable; new ranges stop consuming memory bandwidth.
    pub fn shed_copies(&mut self) {
        self.enabled = false;
        self.stats.shed = true;
    }

    #[must_use]
    pub fn is_copying(&self) -> bool {
        self.enabled
    }

    /// Copy a complete committed raw range. Returns whether it was retained.
    pub fn retain(&mut self, engine_id: u64, bytes: &[u8]) -> bool {
        if !self.enabled || bytes.is_empty() {
            return false;
        }
        self.stats.copied_bytes = self
            .stats
            .copied_bytes
            .saturating_add(u64::try_from(bytes.len()).unwrap_or(u64::MAX));
        if bytes.len() > self.capacity_bytes {
            self.evict_all();
            self.stats.evicted_chunks = self.stats.evicted_chunks.saturating_add(1);
            self.stats.evicted_bytes = self
                .stats
                .evicted_bytes
                .saturating_add(u64::try_from(bytes.len()).unwrap_or(u64::MAX));
            if let Some((_, last)) = tick_bounds(bytes) {
                self.stats.evicted_upto_ticks =
                    Some(self.stats.evicted_upto_ticks.unwrap_or(0).max(last));
            } else {
                self.stats.corrupt_ranges = self.stats.corrupt_ranges.saturating_add(1);
            }
            self.refresh_stats();
            return false;
        }

        let Some((first_ticks, last_ticks)) = tick_bounds(bytes) else {
            self.stats.corrupt_ranges = self.stats.corrupt_ranges.saturating_add(1);
            return false;
        };
        while self
            .retained_bytes
            .checked_add(bytes.len())
            .is_none_or(|total| total > self.capacity_bytes)
        {
            self.evict_front();
        }
        self.chunks.push_back(FlightChunk {
            engine_id,
            first_ticks,
            last_ticks,
            bytes: bytes.to_vec(),
        });
        self.retained_bytes += bytes.len();
        self.refresh_stats();
        true
    }

    #[must_use]
    pub fn chunks(&self) -> &VecDeque<FlightChunk> {
        &self.chunks
    }

    #[must_use]
    pub fn stats(&self) -> FlightRecorderStats {
        self.stats
    }

    /// Cold-path conversion for one engine. Cross-ring arrival order is
    /// intentionally preserved; readers already reconstruct per-thread time
    /// order from timestamps.
    #[must_use]
    pub fn transcode_engine(&self, engine_id: u64, conv: &TickConverter) -> FlightDump {
        let mut bytes = Vec::new();
        let mut first_ticks = None::<u64>;
        let mut last_ticks = None::<u64>;
        let mut events = 0_u64;
        let mut corrupt_ranges = 0_u64;
        for chunk in self
            .chunks
            .iter()
            .filter(|chunk| chunk.engine_id == engine_id)
        {
            first_ticks =
                Some(first_ticks.map_or(chunk.first_ticks, |old| old.min(chunk.first_ticks)));
            last_ticks = Some(last_ticks.map_or(chunk.last_ticks, |old| old.max(chunk.last_ticks)));
            for item in record::iter(&chunk.bytes) {
                match item {
                    Ok(raw) => {
                        encode_disk_event(&mut bytes, &to_disk_event(&raw, conv));
                        events = events.saturating_add(1);
                    }
                    Err(_) => {
                        corrupt_ranges = corrupt_ranges.saturating_add(1);
                        break;
                    }
                }
            }
        }
        FlightDump {
            engine_id,
            first_ticks,
            last_ticks,
            events,
            corrupt_ranges,
            event_bytes: bytes,
        }
    }

    fn evict_front(&mut self) {
        let Some(chunk) = self.chunks.pop_front() else {
            return;
        };
        self.retained_bytes = self.retained_bytes.saturating_sub(chunk.bytes.len());
        self.stats.evicted_chunks = self.stats.evicted_chunks.saturating_add(1);
        self.stats.evicted_bytes = self
            .stats
            .evicted_bytes
            .saturating_add(u64::try_from(chunk.bytes.len()).unwrap_or(u64::MAX));
        self.stats.evicted_upto_ticks = Some(
            self.stats
                .evicted_upto_ticks
                .unwrap_or(0)
                .max(chunk.last_ticks),
        );
    }

    fn evict_all(&mut self) {
        while !self.chunks.is_empty() {
            self.evict_front();
        }
    }

    fn refresh_stats(&mut self) {
        self.stats.retained_chunks = u64::try_from(self.chunks.len()).unwrap_or(u64::MAX);
        self.stats.retained_bytes = u64::try_from(self.retained_bytes).unwrap_or(u64::MAX);
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FlightDump {
    pub engine_id: u64,
    pub first_ticks: Option<u64>,
    pub last_ticks: Option<u64>,
    pub events: u64,
    pub corrupt_ranges: u64,
    /// Length-delimited `DiskEventV1` messages; the caller prepends its
    /// boundary-scoped `EventFileHeaderV1`.
    pub event_bytes: Vec<u8>,
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExactArtifactPaths {
    pub profile: std::path::PathBuf,
    pub cid_manifest: std::path::PathBuf,
    pub index: Option<std::path::PathBuf>,
}

/// Durably publishes a boundary-scoped exact artifact and its GC pins. The
/// profile and sidecars use temporary files plus rename; the BMET trigger
/// record must be appended by the caller only after this returns.
#[cfg(not(target_arch = "wasm32"))]
pub fn write_exact_artifact(
    flight_dir: &std::path::Path,
    stem: &str,
    boundary_id: BoundaryId,
    header: &pb::EventFileHeaderV1,
    dump: &FlightDump,
    pinned_cids: impl IntoIterator<Item = Cid>,
) -> std::io::Result<ExactArtifactPaths> {
    use std::{
        fs::{self, OpenOptions},
        io::Write as _,
    };

    fs::create_dir_all(flight_dir)?;
    let safe_stem = stem
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_') {
                character
            } else {
                '-'
            }
        })
        .collect::<String>();
    let profile = flight_dir.join(format!("{safe_stem}.bamlprof"));
    let profile_tmp = flight_dir.join(format!(".{safe_stem}.bamlprof.tmp"));
    let cid_manifest = flight_dir.join(format!("{safe_stem}.bamlcids"));
    let index_path = flight_dir.join(format!("{safe_stem}.bamlidx"));

    let mut profile_bytes = Vec::new();
    encode_length_delimited_message(&mut profile_bytes, header).map_err(std::io::Error::other)?;
    let event_base = u64::try_from(profile_bytes.len()).unwrap_or(u64::MAX);
    profile_bytes.extend_from_slice(&dump.event_bytes);

    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&profile_tmp)?;
    file.write_all(&profile_bytes)?;
    file.sync_all()?;
    drop(file);
    fs::rename(&profile_tmp, &profile)?;
    sync_parent(&profile)?;

    let mut manifest = CidManifestWriter::create(&cid_manifest, boundary_id)?;
    manifest.append_all(pinned_cids)?;
    let cid_manifest = manifest.seal()?;

    let points = exact_event_points(&dump.event_bytes, event_base);
    let index = if points.is_empty() {
        None
    } else {
        match build_exact_index(&points, IndexBudget::for_segment_bytes(profile_bytes.len())) {
            Ok(index) => {
                let temp = flight_dir.join(format!(".{safe_stem}.bamlidx.tmp"));
                let mut file = OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&temp)?;
                file.write_all(&index.encoded)?;
                file.sync_all()?;
                drop(file);
                fs::rename(&temp, &index_path)?;
                sync_parent(&index_path)?;
                Some(index_path)
            }
            // Very small exact artifacts cannot fit even the fixed BIX1
            // header under the 25% cap. Absence means rebuild/linear scan,
            // never an oversized sidecar.
            Err(crate::prof::exact_index::IndexError::BudgetTooSmall { .. }) => None,
            Err(err) => return Err(std::io::Error::other(err)),
        }
    };

    Ok(ExactArtifactPaths {
        profile,
        cid_manifest,
        index,
    })
}

#[cfg(not(target_arch = "wasm32"))]
fn sync_parent(path: &std::path::Path) -> std::io::Result<()> {
    let Some(parent) = path.parent() else {
        return Ok(());
    };
    let directory = std::fs::File::open(parent)?;
    directory.sync_all()
}

#[cfg(not(target_arch = "wasm32"))]
fn exact_event_points(event_bytes: &[u8], base: u64) -> Vec<ExactEventPoint> {
    let mut points = Vec::new();
    let mut cursor = event_bytes;
    while !cursor.is_empty() {
        let before = cursor.len();
        let Ok(event) = pb::DiskEventV1::decode_length_delimited(&mut cursor) else {
            break;
        };
        let consumed = before.saturating_sub(cursor.len());
        let local_start = event_bytes.len().saturating_sub(before);
        let lane_and_time = match event.event {
            Some(pb::disk_event_v1::Event::StartThread(value)) => {
                Some((value.thread_id, value.timestamp_ns))
            }
            Some(pb::disk_event_v1::Event::EndThread(value)) => {
                Some((value.thread_id, value.timestamp_ns))
            }
            Some(pb::disk_event_v1::Event::CallFunction(value)) => {
                Some((value.thread_id, value.timestamp_ns))
            }
            Some(pb::disk_event_v1::Event::SetFunctionId(value)) => {
                Some((value.thread_id, value.timestamp_ns))
            }
            Some(pb::disk_event_v1::Event::EndFunction(value)) => {
                Some((value.thread_id, value.timestamp_ns))
            }
            Some(pb::disk_event_v1::Event::SuspendThread(value)) => {
                Some((value.thread_id, value.timestamp_ns))
            }
            Some(pb::disk_event_v1::Event::ResumeThread(value)) => {
                Some((value.thread_id, value.timestamp_ns))
            }
            Some(pb::disk_event_v1::Event::LlmCallMeta(value)) => {
                Some((value.thread_id, value.timestamp_ns))
            }
            Some(pb::disk_event_v1::Event::Heartbeat(_)) | None => None,
        };
        if let Some((lane, timestamp_ns)) = lane_and_time {
            let start = base.saturating_add(u64::try_from(local_start).unwrap_or(u64::MAX));
            points.push(ExactEventPoint {
                lane,
                timestamp_ns,
                byte_offset: start,
                byte_end: start.saturating_add(u64::try_from(consumed).unwrap_or(u64::MAX)),
            });
        }
    }
    points
}

/// Why an exact artifact was captured.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TriggerReason {
    Error,
    Latency { threshold_ns: u64, elapsed_ns: u64 },
    Manual { label: String },
}

impl TriggerReason {
    #[must_use]
    pub fn wire_name(&self) -> String {
        match self {
            Self::Error => "error".to_string(),
            Self::Latency {
                threshold_ns,
                elapsed_ns,
            } => format!("latency:{elapsed_ns}>={threshold_ns}"),
            Self::Manual { label } => format!("manual:{label}"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TriggerObservation {
    pub thread_id: u64,
    pub call_id: u64,
    pub node_id: u32,
    pub start_ns: u64,
    pub end_ns: u64,
    pub status: FunctionEndStatus,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TriggerFired {
    pub id: u64,
    pub reason: TriggerReason,
    pub thread_id: u64,
    pub call_id: u64,
    pub node_id: u32,
    pub timestamp_ns: u64,
}

/// Deterministic trigger evaluator. `fire_once` prevents retry storms from
/// repeatedly dumping the same bounded recorder unless the host explicitly
/// rearms it.
#[derive(Clone, Debug)]
pub struct TriggerSet {
    error: bool,
    latency_ns: Option<u64>,
    fire_once: bool,
    armed: bool,
    next_id: u64,
}

impl Default for TriggerSet {
    fn default() -> Self {
        Self {
            error: true,
            latency_ns: None,
            fire_once: true,
            armed: true,
            next_id: 1,
        }
    }
}

impl TriggerSet {
    #[must_use]
    pub fn new(error: bool, latency_ns: Option<u64>, fire_once: bool) -> Self {
        Self {
            error,
            latency_ns,
            fire_once,
            armed: true,
            next_id: 1,
        }
    }

    pub fn rearm(&mut self) {
        self.armed = true;
    }

    pub fn observe(&mut self, observation: TriggerObservation) -> Option<TriggerFired> {
        if !self.armed {
            return None;
        }
        let elapsed_ns = observation.end_ns.saturating_sub(observation.start_ns);
        let reason = if self.error && observation.status != FunctionEndStatus::Ok {
            Some(TriggerReason::Error)
        } else {
            self.latency_ns
                .filter(|threshold| elapsed_ns >= *threshold)
                .map(|threshold_ns| TriggerReason::Latency {
                    threshold_ns,
                    elapsed_ns,
                })
        }?;
        Some(self.fire(
            reason,
            observation.thread_id,
            observation.call_id,
            observation.node_id,
            observation.end_ns,
        ))
    }

    pub fn manual(
        &mut self,
        label: impl Into<String>,
        thread_id: u64,
        call_id: u64,
        node_id: u32,
        timestamp_ns: u64,
    ) -> Option<TriggerFired> {
        if !self.armed {
            return None;
        }
        Some(self.fire(
            TriggerReason::Manual {
                label: label.into(),
            },
            thread_id,
            call_id,
            node_id,
            timestamp_ns,
        ))
    }

    fn fire(
        &mut self,
        reason: TriggerReason,
        thread_id: u64,
        call_id: u64,
        node_id: u32,
        timestamp_ns: u64,
    ) -> TriggerFired {
        let id = self.next_id;
        self.next_id = self.next_id.saturating_add(1);
        if self.fire_once {
            self.armed = false;
        }
        TriggerFired {
            id,
            reason,
            thread_id,
            call_id,
            node_id,
            timestamp_ns,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FullTraceBudget {
    pub max_bytes: usize,
    pub max_duration_ns: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TraceBudgetExhausted {
    pub accepted_bytes: u64,
    pub dropped_bytes: u64,
    pub at_ticks: u64,
}

/// Opt-in exact trace collector. Exhaustion is sticky and explicit; no later
/// range is silently accepted after the marker.
#[derive(Debug)]
pub struct FullTraceRecorder {
    budget: FullTraceBudget,
    started_ticks: Option<u64>,
    bytes: Vec<u8>,
    exhausted: Option<TraceBudgetExhausted>,
}

impl FullTraceRecorder {
    #[must_use]
    pub fn new(budget: FullTraceBudget) -> Self {
        Self {
            budget,
            started_ticks: None,
            bytes: Vec::new(),
            exhausted: None,
        }
    }

    pub fn retain(&mut self, bytes: &[u8]) -> bool {
        if self.exhausted.is_some() || bytes.is_empty() {
            return false;
        }
        let Some((first, last)) = tick_bounds(bytes) else {
            self.mark_exhausted(bytes.len(), self.started_ticks.unwrap_or(0));
            return false;
        };
        let start = *self.started_ticks.get_or_insert(first);
        let over_time = last.saturating_sub(start) > self.budget.max_duration_ns;
        let over_bytes = self
            .bytes
            .len()
            .checked_add(bytes.len())
            .is_none_or(|total| total > self.budget.max_bytes);
        if over_time || over_bytes {
            self.mark_exhausted(bytes.len(), last);
            return false;
        }
        self.bytes.extend_from_slice(bytes);
        true
    }

    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    #[must_use]
    pub fn exhausted(&self) -> Option<TraceBudgetExhausted> {
        self.exhausted
    }

    #[must_use]
    pub fn transcode(&self, engine_id: u64, conv: &TickConverter) -> FlightDump {
        let bounds = tick_bounds(&self.bytes);
        let mut event_bytes = Vec::new();
        let mut events = 0_u64;
        let mut corrupt_ranges = 0_u64;
        for item in record::iter(&self.bytes) {
            match item {
                Ok(raw) => {
                    encode_disk_event(&mut event_bytes, &to_disk_event(&raw, conv));
                    events = events.saturating_add(1);
                }
                Err(_) => {
                    corrupt_ranges = 1;
                    break;
                }
            }
        }
        FlightDump {
            engine_id,
            first_ticks: bounds.map(|bounds| bounds.0),
            last_ticks: bounds.map(|bounds| bounds.1),
            events,
            corrupt_ranges,
            event_bytes,
        }
    }

    fn mark_exhausted(&mut self, dropped: usize, at_ticks: u64) {
        self.exhausted = Some(TraceBudgetExhausted {
            accepted_bytes: u64::try_from(self.bytes.len()).unwrap_or(u64::MAX),
            dropped_bytes: u64::try_from(dropped).unwrap_or(u64::MAX),
            at_ticks,
        });
    }
}

fn tick_bounds(bytes: &[u8]) -> Option<(u64, u64)> {
    let mut first = None::<u64>;
    let mut last = None::<u64>;
    for item in record::iter(bytes) {
        let raw = item.ok()?;
        let ticks = record_ticks(&raw);
        first = Some(first.map_or(ticks, |old| old.min(ticks)));
        last = Some(last.map_or(ticks, |old| old.max(ticks)));
    }
    first.zip(last)
}

fn record_ticks(record: &RawRecord<'_>) -> u64 {
    match record {
        RawRecord::CallFunction { ts_ticks, .. }
        | RawRecord::EndFunction { ts_ticks, .. }
        | RawRecord::StartThread { ts_ticks, .. }
        | RawRecord::EndThread { ts_ticks, .. }
        | RawRecord::SetFunctionId { ts_ticks, .. }
        | RawRecord::SuspendThread { ts_ticks, .. }
        | RawRecord::ResumeThread { ts_ticks, .. }
        | RawRecord::LlmCallMeta { ts_ticks, .. } => *ts_ticks,
    }
}

/// Exact events decoded from a dump, grouped only for consumers that want a
/// cheap per-thread count without reconstructing the full call tree.
#[must_use]
pub fn event_counts_by_thread(bytes: &[u8]) -> HashMap<u64, u64> {
    let mut counts = HashMap::new();
    let mut cursor = bytes;
    while !cursor.is_empty() {
        let Ok(event) = pb::DiskEventV1::decode_length_delimited(&mut cursor) else {
            break;
        };
        let thread_id = match event.event {
            Some(pb::disk_event_v1::Event::StartThread(value)) => value.thread_id,
            Some(pb::disk_event_v1::Event::EndThread(value)) => value.thread_id,
            Some(pb::disk_event_v1::Event::CallFunction(value)) => value.thread_id,
            Some(pb::disk_event_v1::Event::SetFunctionId(value)) => value.thread_id,
            Some(pb::disk_event_v1::Event::EndFunction(value)) => value.thread_id,
            Some(pb::disk_event_v1::Event::SuspendThread(value)) => value.thread_id,
            Some(pb::disk_event_v1::Event::ResumeThread(value)) => value.thread_id,
            Some(pb::disk_event_v1::Event::LlmCallMeta(value)) => value.thread_id,
            Some(pb::disk_event_v1::Event::Heartbeat(_)) | None => continue,
        };
        *counts.entry(thread_id).or_default() += 1;
    }
    counts
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ids::{BexCallId, BexThreadId, FunctionId},
        prof::record::{MAX_RECORD_LEN, RawRecord},
    };

    fn call_pair(call: u64, start: u64, end: u64) -> Vec<u8> {
        let mut out = Vec::new();
        for record in [
            RawRecord::CallFunction {
                flags: 0,
                thread_id: BexThreadId(1),
                call_id: BexCallId(call),
                parent_call_id: BexCallId(0),
                function_id: FunctionId(2),
                call_site: None,
                ts_ticks: start,
            },
            RawRecord::EndFunction {
                status: FunctionEndStatus::Ok,
                thread_id: BexThreadId(1),
                call_id: BexCallId(call),
                ts_ticks: end,
            },
        ] {
            let mut scratch = [0_u8; MAX_RECORD_LEN];
            let len = record.encode(&mut scratch);
            out.extend_from_slice(&scratch[..len]);
        }
        out
    }

    #[test]
    fn flight_fifo_evicts_only_whole_ranges_and_reports_window() {
        let one = call_pair(1, 10, 20);
        let two = call_pair(2, 30, 40);
        let mut recorder = FlightRecorder::new(one.len() + two.len() - 1);
        assert!(recorder.retain(7, &one));
        assert!(recorder.retain(7, &two));
        assert_eq!(recorder.chunks().len(), 1);
        assert_eq!(recorder.chunks()[0].first_ticks, 30);
        assert_eq!(recorder.stats().evicted_upto_ticks, Some(20));
        assert_eq!(recorder.stats().evicted_chunks, 1);
    }

    #[test]
    fn flight_dump_cold_transcodes_raw_events() {
        let bytes = call_pair(1, 10, 20);
        let mut recorder = FlightRecorder::new(1024);
        assert!(recorder.retain(9, &bytes));
        let dump = recorder.transcode_engine(9, &TickConverter::identity());
        assert_eq!(dump.events, 2);
        assert_eq!(event_counts_by_thread(&dump.event_bytes).get(&1), Some(&2));
    }

    #[test]
    fn shed_stops_copying_without_discarding_existing_evidence() {
        let bytes = call_pair(1, 10, 20);
        let mut recorder = FlightRecorder::new(1024);
        assert!(recorder.retain(9, &bytes));
        recorder.shed_copies();
        assert!(!recorder.retain(9, &bytes));
        assert_eq!(recorder.chunks().len(), 1);
        assert!(recorder.stats().shed);
    }

    #[test]
    fn triggers_are_explicit_and_rearmable() {
        let mut triggers = TriggerSet::new(true, Some(100), true);
        let fired = triggers
            .observe(TriggerObservation {
                thread_id: 1,
                call_id: 2,
                node_id: 3,
                start_ns: 10,
                end_ns: 11,
                status: FunctionEndStatus::Errored,
            })
            .unwrap();
        assert_eq!(fired.reason, TriggerReason::Error);
        assert!(triggers.manual("again", 1, 2, 3, 12).is_none());
        triggers.rearm();
        assert!(matches!(
            triggers.manual("again", 1, 2, 3, 12).unwrap().reason,
            TriggerReason::Manual { .. }
        ));
    }

    #[test]
    fn full_trace_exhaustion_is_sticky_and_visible() {
        let bytes = call_pair(1, 10, 20);
        let mut trace = FullTraceRecorder::new(FullTraceBudget {
            max_bytes: bytes.len(),
            max_duration_ns: 1_000,
        });
        assert!(trace.retain(&bytes));
        assert!(!trace.retain(&bytes));
        let exhausted = trace.exhausted().unwrap();
        assert_eq!(exhausted.accepted_bytes as usize, bytes.len());
        assert_eq!(exhausted.dropped_bytes as usize, bytes.len());
        assert!(!trace.retain(&[]));
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn exact_artifact_publishes_profile_pins_and_capped_index() {
        use crate::{
            ids::BoundaryId,
            prof::{
                encode::{build_header, encode_length_delimited_message},
                read::read_bamlprof_from_bytes,
            },
            value_cas::{Cid, CidManifestReader},
        };
        use std::sync::atomic::{AtomicU32, Ordering};

        static NEXT: AtomicU32 = AtomicU32::new(0);
        let root = std::env::temp_dir().join(format!(
            "baml-flight-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let raw = (0..128)
            .flat_map(|call| call_pair(call + 1, call * 2, call * 2 + 1))
            .collect::<Vec<_>>();
        let mut recorder = FlightRecorder::new(raw.len() + 1);
        assert!(recorder.retain(9, &raw));
        let dump = recorder.transcode_engine(9, &TickConverter::identity());
        let boundary = BoundaryId::from_bytes([8; 16]);
        let mut header = build_header([7; 16], 9, 123, None, &TickConverter::identity());
        header.boundary_id = Some(boundary.as_bytes().to_vec());
        header.trigger_reason = Some("error".to_owned());
        let cid = Cid::from_bytes([3; 32]);
        let paths =
            write_exact_artifact(&root, "42-error", boundary, &header, &dump, [cid]).unwrap();

        let parsed = read_bamlprof_from_bytes(&std::fs::read(&paths.profile).unwrap()).unwrap();
        assert_eq!(parsed.events.len(), 256);
        assert_eq!(parsed.header.trigger_reason.as_deref(), Some("error"));
        let manifest = CidManifestReader::read(&paths.cid_manifest).unwrap();
        assert!(manifest.manifest.sealed);
        assert_eq!(manifest.manifest.cids, vec![cid]);
        if let Some(index) = paths.index {
            assert!(
                std::fs::metadata(index).unwrap().len()
                    <= std::fs::metadata(&paths.profile).unwrap().len() / 4
            );
        }
        let _ = std::fs::remove_dir_all(root);

        // Keep the shared header encoder covered after the additive fields.
        let mut bytes = Vec::new();
        encode_length_delimited_message(&mut bytes, &header).unwrap();
        assert!(!bytes.is_empty());
    }
}
