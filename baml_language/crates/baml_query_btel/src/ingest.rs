//! Incremental reconciliation of completed recording files into the index.
//!
//! Each refresh takes one finite discovery snapshot. For every recording it
//! checks the already-applied prefix against the ledger by file metadata,
//! then applies the contiguous run of new files from the watermark, in
//! bounded groups. One group is one transaction: facts, reduced totals,
//! issues, ledger rows and the watermark commit together, so a crash
//! before commit retries the files and a crash after commit skips them.
//! A changed or vanished applied file rebuilds that recording from its
//! remaining valid prefix, inside the same kind of transaction.
use std::{
    collections::{HashMap, HashSet},
    time::{Duration, Instant},
};

use btel_reader::{
    discovery::{self, DiscoveredFile, DiscoveredRecording, FileStamp},
    evidence::{self, ArgumentNames},
    file::{self, FileError, FileRead},
    layout::SourceLayout,
    timing::EpochStatus,
};
use btel_recorder::proto::{self, function_definition::Resolution, span_event::Event};
use prost::Message as _;
use rusqlite::{Connection, OptionalExtension as _, Transaction, params};
use serde::Serialize;

use crate::{Error, store};

mod errors;
mod sites;

#[derive(Clone, Debug)]
pub struct RefreshOptions {
    /// Longest wait for another process's reconciliation to finish.
    pub lock_timeout: Duration,
    pub max_file_bytes: u64,
    /// Bound on one transaction's files and bytes.
    pub batch_files: usize,
    pub batch_bytes: u64,
    /// Hash every applied file instead of trusting unchanged metadata.
    pub verify_contents: bool,
}
impl Default for RefreshOptions {
    fn default() -> Self {
        Self {
            lock_timeout: Duration::from_secs(30),
            max_file_bytes: 256 << 20,
            batch_files: 256,
            batch_bytes: 32 << 20,
            verify_contents: false,
        }
    }
}

/// Work counters for one refresh. Zero `files_decoded` over unchanged
/// recordings is the work-avoidance guarantee.
#[derive(Clone, Debug, Default, Serialize)]
pub struct RefreshMetrics {
    pub lock_wait_ms: f64,
    pub discovery_ms: f64,
    pub recordings: usize,
    pub files_seen: usize,
    /// Applied files whose metadata matched the ledger: not read.
    pub files_unchanged: usize,
    /// Applied files whose metadata changed, hashed to compare contents.
    pub files_rehashed: usize,
    /// Files decoded as protobuf (new, changed or rebuilt).
    pub files_decoded: usize,
    pub bytes_read: u64,
    pub files_applied: usize,
    pub files_rejected: usize,
    pub recordings_rebuilt: usize,
    pub recordings_removed: usize,
    pub schema_rebuilt: bool,
    pub transactions: usize,
    /// Reading and validating files (outside transactions).
    pub read_ms: f64,
    /// Applying facts inside transactions, before commit.
    pub apply_ms: f64,
    pub commit_ms: f64,
    pub total_ms: f64,
}

fn ms(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1e3
}

pub(crate) fn id(value: u64) -> [u8; 8] {
    value.to_be_bytes()
}

fn snapshot_id(id: &proto::SnapshotId) -> [u8; 16] {
    let mut bytes = [0; 16];
    bytes[..8].copy_from_slice(&id.low.to_le_bytes());
    bytes[8..].copy_from_slice(&id.high.to_le_bytes());
    bytes
}

fn quantity(value: u64) -> Option<i64> {
    i64::try_from(value).ok()
}

/// Stored counts, sizes and sequences are non-negative by construction.
fn stored(value: i64) -> u64 {
    u64::try_from(value).unwrap_or(0)
}

/// File sizes and counts far below `i64::MAX` in practice; saturate rather
/// than wrap if one is not.
fn saturating(value: u64) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

fn sequence_i64(sequence: u64) -> Result<i64, Error> {
    i64::try_from(sequence).map_err(|_| Error::Unsupported("recording sequence exceeds i64".into()))
}

struct Known {
    rec: i64,
    indexed_sequence: u64,
    terminal_sequence: Option<u64>,
    source_snapshot_id: Option<Vec<u8>>,
    /// Frontier state recorded by the previous refresh.
    frontier: Frontier,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Frontier {
    observed: u64,
    blocked_sequence: Option<u64>,
    blocked_reason: Option<String>,
    partial_files: u64,
    after_end: u64,
}

struct LedgerRow {
    sequence: u64,
    size: u64,
    modified_ns: Option<i64>,
    device: [u8; 8],
    inode: [u8; 8],
    content_hash: Vec<u8>,
}

fn stamp_matches(row: &LedgerRow, stamp: &FileStamp) -> bool {
    row.size == stamp.size
        && row.modified_ns == i64::try_from(stamp.modified_ns).ok()
        && row.device == id(stamp.device)
        && row.inode == id(stamp.inode)
}

/// Run a refresh with the writer lock held for its duration.
pub fn refresh(
    conn: &mut Connection,
    layout: &SourceLayout,
    options: &RefreshOptions,
) -> Result<RefreshMetrics, Error> {
    let started = Instant::now();
    let mut metrics = RefreshMetrics::default();
    let (_lock, waited) =
        store::WriterLock::acquire(&store::lock_path(layout), options.lock_timeout)?;
    metrics.lock_wait_ms = ms(waited);
    metrics.schema_rebuilt = store::ensure_schema(conn)?;

    let discovery_started = Instant::now();
    let found = discovery::discover(layout).map_err(|e| Error::Io(e.to_string()))?;
    metrics.discovery_ms = ms(discovery_started.elapsed());
    metrics.recordings = found.recordings.len();
    metrics.files_seen = found.file_count();

    let known = load_known(conn)?;
    let present: HashSet<[u8; 16]> = found.recordings.iter().map(|r| *r.id.as_bytes()).collect();
    for (recording_id, known) in &known {
        if !present.contains(recording_id) {
            // The whole directory is gone: its remaining valid prefix is empty.
            let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
            delete_facts(&tx, known.rec)?;
            tx.execute("DELETE FROM recording WHERE rec = ?1", [known.rec])?;
            tx.commit()?;
            metrics.recordings_removed += 1;
            metrics.transactions += 1;
        }
    }
    for recording in &found.recordings {
        reconcile(
            conn,
            recording,
            known.get(recording.id.as_bytes()),
            options,
            &mut metrics,
        )?;
    }
    metrics.total_ms = ms(started.elapsed());
    Ok(metrics)
}

fn load_known(conn: &Connection) -> Result<HashMap<[u8; 16], Known>, Error> {
    let mut statement = conn.prepare(
        "SELECT rec, recording_id, indexed_sequence, terminal_sequence, source_snapshot_id,
           observed_sequence, blocked_sequence, blocked_reason, partial_files, files_after_end
         FROM recording",
    )?;
    let rows = statement.query_map([], |r| {
        let recording_id: Vec<u8> = r.get(1)?;
        Ok((
            recording_id,
            Known {
                rec: r.get(0)?,
                indexed_sequence: stored(r.get(2)?),
                terminal_sequence: r.get::<_, Option<i64>>(3)?.map(stored),
                source_snapshot_id: r.get(4)?,
                frontier: Frontier {
                    observed: stored(r.get(5)?),
                    blocked_sequence: r.get::<_, Option<i64>>(6)?.map(stored),
                    blocked_reason: r.get(7)?,
                    partial_files: stored(r.get(8)?),
                    after_end: stored(r.get(9)?),
                },
            },
        ))
    })?;
    let mut known = HashMap::new();
    for row in rows {
        let (recording_id, row) = row?;
        if let Ok(recording_id) = <[u8; 16]>::try_from(recording_id.as_slice()) {
            known.insert(recording_id, row);
        }
    }
    Ok(known)
}

fn load_ledger(conn: &Connection, rec: i64) -> Result<Vec<LedgerRow>, Error> {
    let mut statement = conn.prepare_cached(
        "SELECT sequence, size, modified_ns, device, inode, content_hash FROM ledger
         WHERE rec = ?1 ORDER BY sequence",
    )?;
    let rows = statement.query_map([rec], |r| {
        let device: Vec<u8> = r.get(3)?;
        let inode: Vec<u8> = r.get(4)?;
        Ok(LedgerRow {
            sequence: stored(r.get(0)?),
            size: stored(r.get(1)?),
            modified_ns: r.get(2)?,
            device: device.try_into().unwrap_or_default(),
            inode: inode.try_into().unwrap_or_default(),
            content_hash: r.get(5)?,
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

fn delete_facts(tx: &Transaction<'_>, rec: i64) -> Result<(), Error> {
    for table in crate::schema::FACT_TABLES {
        tx.execute(&format!("DELETE FROM {table} WHERE rec = ?1"), [rec])?;
    }
    Ok(())
}

enum Candidate<'a> {
    Read(&'a DiscoveredFile, Box<FileRead>),
    Invalid(&'a DiscoveredFile, String),
}

fn reconcile(
    conn: &mut Connection,
    recording: &DiscoveredRecording,
    known: Option<&Known>,
    options: &RefreshOptions,
    metrics: &mut RefreshMetrics,
) -> Result<(), Error> {
    let by_sequence: HashMap<u64, &DiscoveredFile> =
        recording.files.iter().map(|f| (f.sequence, f)).collect();
    let observed = recording.files.last().map_or(0, |f| f.sequence);

    // 1. Is the applied prefix still the evidence it was?
    let mut rebuild = false;
    let mut restamped = Vec::new();
    let mut indexed = known.map_or(0, |k| k.indexed_sequence);
    let mut terminal = known.and_then(|k| k.terminal_sequence);
    let mut source_snapshot = known.and_then(|k| k.source_snapshot_id.clone());
    if let Some(known) = known {
        for row in load_ledger(conn, known.rec)? {
            let Some(found) = by_sequence.get(&row.sequence) else {
                rebuild = true;
                break;
            };
            if !options.verify_contents && stamp_matches(&row, &found.stamp) {
                metrics.files_unchanged += 1;
                continue;
            }
            metrics.files_rehashed += 1;
            match file::hash_file(&found.path, options.max_file_bytes) {
                Ok((len, hash)) if len == row.size && hash.as_slice() == row.content_hash => {
                    metrics.bytes_read += len;
                    restamped.push((row.sequence, found.stamp));
                }
                Ok((len, _)) => {
                    metrics.bytes_read += len;
                    rebuild = true;
                    break;
                }
                Err(_) => {
                    rebuild = true;
                    break;
                }
            }
        }
    }
    if rebuild {
        indexed = 0;
        terminal = None;
        source_snapshot = None;
        metrics.recordings_rebuilt += 1;
    }

    // 2. The contiguous run of files to apply, stopping at a terminal marker.
    let mut next = indexed + 1;
    let mut pending: Vec<&DiscoveredFile> = Vec::new();
    if terminal.is_none() {
        while let Some(found) = by_sequence.get(&next) {
            pending.push(found);
            next += 1;
        }
    }

    let planned_last = pending.last().map_or(indexed, |f| f.sequence);
    let frontier = |blocked_invalid: &Option<(u64, String)>, terminal: Option<u64>| {
        let (blocked_sequence, blocked_reason) = match blocked_invalid {
            Some((sequence, _)) => (Some(*sequence), Some("invalid".to_owned())),
            None if terminal.is_none() && observed > planned_last => {
                (Some(planned_last + 1), Some("missing".to_owned()))
            }
            None => (None, None),
        };
        Frontier {
            observed,
            blocked_sequence,
            blocked_reason,
            partial_files: recording.partial_files as u64,
            after_end: terminal.map_or(0, |t| {
                recording.files.iter().filter(|f| f.sequence > t).count() as u64
            }),
        }
    };
    let rejected = match known {
        Some(known) if !rebuild => load_rejected(conn, known.rec)?,
        _ => HashMap::new(),
    };
    // The frontier file is already known to be invalid and is unchanged.
    let still_rejected = pending.first().and_then(|found| {
        rejected
            .get(&found.sequence)
            .filter(|(stamp, _)| *stamp == found.stamp)
            .map(|(_, reason)| (found.sequence, reason.clone()))
    });
    // Nothing new and nothing changed: no transaction, no write.
    if let Some(known) = known
        && !rebuild
        && restamped.is_empty()
        && (pending.is_empty() || still_rejected.is_some())
        && known.frontier == frontier(&still_rejected, terminal)
    {
        return Ok(());
    }
    let mut rec = known.map(|k| k.rec);
    let mut first_transaction = true;
    let mut blocked_invalid: Option<(u64, String)> = None;
    let mut cursor = 0;
    loop {
        // Read and validate one bounded group outside the transaction.
        let read_started = Instant::now();
        let mut group = Vec::new();
        let mut group_bytes = 0;
        while cursor < pending.len()
            && group.len() < options.batch_files
            && (group.is_empty() || group_bytes < options.batch_bytes)
        {
            let found = pending[cursor];
            cursor += 1;
            if let Some((stamp, reason)) = rejected.get(&found.sequence)
                && *stamp == found.stamp
            {
                // Known-invalid and unchanged: do not decode it again.
                group.push(Candidate::Invalid(found, reason.clone()));
                break;
            }
            metrics.files_decoded += 1;
            match file::read_file(
                &found.path,
                recording.id,
                found.sequence,
                options.max_file_bytes,
            ) {
                Ok(read) => {
                    metrics.bytes_read += read.len;
                    let snapshot = read
                        .file
                        .header
                        .as_ref()
                        .and_then(|h| h.source_snapshot_id.clone());
                    let first = group_first_snapshot(&group).map(|first| {
                        first
                            .file
                            .header
                            .as_ref()
                            .and_then(|h| h.source_snapshot_id.as_ref())
                    });
                    let consistent = match (&source_snapshot, first) {
                        (Some(known), _) => snapshot.as_ref() == Some(known),
                        (None, Some(first)) => snapshot.as_ref() == first,
                        (None, None) => true,
                    };
                    if !consistent {
                        group.push(Candidate::Invalid(
                            found,
                            "source snapshot changed within recording".into(),
                        ));
                        break;
                    }
                    group_bytes += read.len;
                    let is_end = read.file.end.is_some();
                    group.push(Candidate::Read(found, Box::new(read)));
                    if is_end {
                        cursor = pending.len();
                        break;
                    }
                }
                Err(error) => {
                    let reason = match &error {
                        FileError::Io(_) => error.to_string(),
                        FileError::TooLarge { .. } | FileError::Invalid(_) => error.to_string(),
                    };
                    group.push(Candidate::Invalid(found, reason));
                    break;
                }
            }
        }
        metrics.read_ms += ms(read_started.elapsed());
        let stop = matches!(group.last(), Some(Candidate::Invalid(..)));
        if group.is_empty() && !first_transaction && restamped.is_empty() {
            break;
        }

        let apply_started = Instant::now();
        let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let rec_key = match rec {
            Some(rec) => rec,
            None => {
                tx.execute(
                    "INSERT INTO recording (recording_id) VALUES (?1)",
                    [recording.id.as_bytes().as_slice()],
                )?;
                tx.last_insert_rowid()
            }
        };
        rec = Some(rec_key);
        if first_transaction && rebuild {
            delete_facts(&tx, rec_key)?;
            tx.execute(
                "UPDATE recording SET indexed_sequence = 0, terminal_sequence = NULL,
                   source_snapshot_id = NULL, indexed_bytes = 0, format_minor = 0,
                   generation = generation + 1
                 WHERE rec = ?1",
                [rec_key],
            )?;
        }
        for (sequence, stamp) in restamped.drain(..) {
            tx.execute(
                "UPDATE ledger SET size = ?3, modified_ns = ?4, device = ?5, inode = ?6
                 WHERE rec = ?1 AND sequence = ?2",
                params![
                    rec_key,
                    sequence_i64(sequence)?,
                    saturating(stamp.size),
                    i64::try_from(stamp.modified_ns).ok(),
                    id(stamp.device),
                    id(stamp.inode)
                ],
            )?;
        }
        let mut applier = Applier::new(&tx, rec_key);
        for candidate in &group {
            match candidate {
                Candidate::Read(found, read) => {
                    applier.apply(found.sequence, &read.file)?;
                    let header = read.file.header.as_ref().expect("validated header");
                    tx.execute(
                        "INSERT INTO ledger (rec, sequence, size, modified_ns, device, inode, content_hash)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                        params![
                            rec_key,
                            sequence_i64(found.sequence)?,
                            saturating(read.len),
                            i64::try_from(found.stamp.modified_ns).ok(),
                            id(found.stamp.device),
                            id(found.stamp.inode),
                            read.content_hash.as_slice()
                        ],
                    )?;
                    tx.execute(
                        "DELETE FROM rejected WHERE rec = ?1 AND sequence = ?2",
                        params![rec_key, sequence_i64(found.sequence)?],
                    )?;
                    tx.execute(
                        "UPDATE recording SET indexed_sequence = ?2,
                           indexed_bytes = indexed_bytes + ?3,
                           format_minor = MAX(format_minor, ?4),
                           source_snapshot_id = COALESCE(source_snapshot_id, ?5),
                           terminal_sequence = CASE WHEN ?6 THEN ?2 ELSE terminal_sequence END
                         WHERE rec = ?1",
                        params![
                            rec_key,
                            sequence_i64(found.sequence)?,
                            saturating(read.len),
                            i64::from(header.format_minor),
                            header.source_snapshot_id.as_deref(),
                            read.file.end.is_some()
                        ],
                    )?;
                    if source_snapshot.is_none() {
                        source_snapshot.clone_from(&header.source_snapshot_id);
                    }
                    if read.file.end.is_some() {
                        terminal = Some(found.sequence);
                    }
                    metrics.files_applied += 1;
                }
                Candidate::Invalid(found, reason) => {
                    tx.execute(
                        "INSERT OR REPLACE INTO rejected (rec, sequence, size, modified_ns, device, inode, reason)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                        params![
                            rec_key,
                            sequence_i64(found.sequence)?,
                            saturating(found.stamp.size),
                            i64::try_from(found.stamp.modified_ns).ok(),
                            id(found.stamp.device),
                            id(found.stamp.inode),
                            reason
                        ],
                    )?;
                    metrics.files_rejected += 1;
                    blocked_invalid = Some((found.sequence, reason.clone()));
                }
            }
        }
        applier.finish()?;
        // Frontier state from this discovery snapshot.
        let state = frontier(&blocked_invalid, terminal);
        tx.execute(
            "UPDATE recording SET observed_sequence = ?2, blocked_sequence = ?3,
               blocked_reason = ?4, partial_files = ?5, files_after_end = ?6
             WHERE rec = ?1",
            params![
                rec_key,
                sequence_i64(state.observed)?,
                state.blocked_sequence.map(sequence_i64).transpose()?,
                state.blocked_reason,
                saturating(state.partial_files),
                saturating(state.after_end)
            ],
        )?;
        metrics.apply_ms += ms(apply_started.elapsed());
        let commit_started = Instant::now();
        fault::point("before_commit");
        tx.commit()?;
        fault::point("after_commit");
        metrics.commit_ms += ms(commit_started.elapsed());
        metrics.transactions += 1;
        first_transaction = false;
        if stop || cursor >= pending.len() {
            break;
        }
    }
    Ok(())
}

/// The source fingerprint of the first readable file in `group`, if any.
fn group_first_snapshot<'g>(group: &'g [Candidate<'_>]) -> Option<&'g FileRead> {
    group.iter().find_map(|candidate| match candidate {
        Candidate::Read(_, read) => Some(read.as_ref()),
        Candidate::Invalid(..) => None,
    })
}

fn load_rejected(conn: &Connection, rec: i64) -> Result<HashMap<u64, (FileStamp, String)>, Error> {
    let mut statement = conn.prepare_cached(
        "SELECT sequence, size, modified_ns, device, inode, reason FROM rejected WHERE rec = ?1",
    )?;
    let rows = statement.query_map([rec], |r| {
        let device: Vec<u8> = r.get(3)?;
        let inode: Vec<u8> = r.get(4)?;
        Ok((
            stored(r.get(0)?),
            FileStamp {
                size: stored(r.get(1)?),
                modified_ns: i128::from(r.get::<_, Option<i64>>(2)?.unwrap_or(i64::MIN)),
                device: u64::from_be_bytes(device.try_into().unwrap_or_default()),
                inode: u64::from_be_bytes(inode.try_into().unwrap_or_default()),
            },
            r.get::<_, String>(5)?,
        ))
    })?;
    let mut rejected = HashMap::new();
    for row in rows {
        let (sequence, stamp, reason) = row?;
        rejected.insert(sequence, (stamp, reason));
    }
    Ok(rejected)
}

/// Exact sums of applied aggregate deltas. u128 cannot overflow from u64
/// deltas within any realistic recording; saturation is only a backstop.
#[derive(Clone, Copy, Default)]
struct Totals {
    count: u128,
    duration: u128,
    self_await: u128,
    outcomes: crate::outcomes::Totals,
}

impl Totals {
    fn add(self, other: Self) -> Self {
        Self {
            count: self.count.saturating_add(other.count),
            duration: self.duration.saturating_add(other.duration),
            self_await: self.self_await.saturating_add(other.self_await),
            outcomes: self.outcomes.add(other.outcomes),
        }
    }
}

/// References seen in one transaction whose definitions may be missing.
#[derive(Default)]
struct References {
    threads: HashSet<u64>,
    paths: HashSet<u32>,
    functions: HashSet<u64>,
    epochs: HashSet<u64>,
}

/// Applies files to one recording inside a transaction.
struct Applier<'t> {
    tx: &'t Transaction<'t>,
    rec: i64,
    aggregates: HashMap<u64, Totals>,
    /// Epoch definitions already reconciled in this transaction.
    epochs_seen: HashMap<u64, Vec<u8>>,
    references: References,
    touched_threads: bool,
    touched_paths: bool,
    /// Children whose definition or normal-node duration changed. Resolve
    /// their parents after applying the whole batch (evidence can be late).
    changed_children: HashSet<u32>,
    /// Rows whose recorded PCs need (re)resolving after this batch: newly
    /// defined paths and raises, and everything that names a function whose
    /// definition arrived in this batch.
    sites: sites::Pending,
}

impl<'t> Applier<'t> {
    fn new(tx: &'t Transaction<'t>, rec: i64) -> Self {
        Self {
            tx,
            rec,
            aggregates: HashMap::new(),
            epochs_seen: HashMap::new(),
            references: References::default(),
            touched_threads: false,
            touched_paths: false,
            changed_children: HashSet::new(),
            sites: sites::Pending::default(),
        }
    }

    fn issue(
        &self,
        sequence: u64,
        code: &str,
        subject: Option<&str>,
        detail: &str,
    ) -> Result<(), Error> {
        self.tx
            .prepare_cached(
                "INSERT INTO issue (rec, sequence, code, subject, detail) VALUES (?1, ?2, ?3, ?4, ?5)",
            )?
            .execute(params![self.rec, sequence_i64(sequence)?, code, subject, detail])?;
        Ok(())
    }

    fn apply(&mut self, sequence: u64, file: &proto::RecordingFile) -> Result<(), Error> {
        let mut ticks_out_of_range = false;
        let mut tick = |value: u64| {
            let converted = quantity(value);
            ticks_out_of_range |= converted.is_none();
            converted
        };
        if let Some(definitions) = &file.definitions {
            for function in &definitions.functions {
                self.function(sequence, function)?;
            }
            for path in &definitions.call_paths {
                self.call_path(sequence, path)?;
            }
            for epoch in &definitions.clock_epochs {
                self.epoch(sequence, epoch)?;
            }
            for thread in &definitions.threads {
                self.thread(sequence, thread, tick(thread.started_at_ticks))?;
            }
        }
        if let Some(states) = &file.clock_states {
            for state in &states.states {
                self.references.epochs.insert(state.epoch_id);
                let Some(status) = EpochStatus::from_wire(state.status) else {
                    continue;
                };
                self.tx
                    .prepare_cached(
                        "INSERT INTO epoch_state (rec, epoch_id, status, final) VALUES (?1, ?2, ?3, ?4)
                         ON CONFLICT (rec, epoch_id) DO UPDATE SET
                           status = MAX(status, excluded.status),
                           final = MAX(final, excluded.final)",
                    )?
                    .execute(params![self.rec, id(state.epoch_id), status.code(), state.r#final])?;
            }
        }
        if let Some(batch) = &file.aggregates {
            for delta in &batch.entries {
                let (path, reentry) = evidence::split_node(delta.node);
                if !reentry {
                    self.changed_children.insert(path);
                }
                self.references
                    .paths
                    .insert(evidence::split_node(delta.node).0);
                let totals = Totals {
                    count: u128::from(delta.count),
                    duration: u128::from(delta.total_duration_ticks),
                    self_await: u128::from(delta.total_self_await_ticks),
                    outcomes: crate::outcomes::Totals::from_wire(
                        delta.count,
                        delta.outcomes.as_ref(),
                    ),
                };
                if totals.outcomes.evidence & crate::outcomes::INVALID != 0 {
                    self.issue(sequence, "aggregate_outcome_invalid", Some(&format!("n{}", delta.node)),
                        "error and cancellation counts exceed completed calls; population outcomes are unavailable")?;
                }
                let entry = self.aggregates.entry(delta.node).or_default();
                *entry = entry.add(totals);
            }
        }
        if let Some(batch) = &file.errors {
            self.errors(sequence, batch, &mut tick)?;
        }
        if let Some(spans) = &file.spans {
            for section in &spans.sections {
                self.references.threads.insert(section.thread_id);
                for event in &section.events {
                    match event.event.as_ref() {
                        Some(Event::ThreadAnnouncement(_)) => {
                            self.tx
                                .prepare_cached(
                                    "INSERT INTO thread (rec, thread_id, defined, announced)
                                     VALUES (?1, ?2, 0, 1)
                                     ON CONFLICT (rec, thread_id) DO UPDATE SET announced = 1",
                                )?
                                .execute(params![self.rec, id(section.thread_id)])?;
                        }
                        Some(Event::ThreadCompletion(done)) => {
                            let completed = tick(done.completed_at_ticks);
                            // Keep a completion even before its definition.
                            self.tx
                                .prepare_cached(
                                    "INSERT OR IGNORE INTO thread (rec, thread_id, defined)
                                     VALUES (?1, ?2, 0)",
                                )?
                                .execute(params![self.rec, id(section.thread_id)])?;
                            let changed = self
                                .tx
                                .prepare_cached(
                                    "UPDATE thread SET completed_ticks = ?3, outcome = ?4
                                     WHERE rec = ?1 AND thread_id = ?2 AND outcome IS NULL",
                                )?
                                .execute(params![
                                    self.rec,
                                    id(section.thread_id),
                                    completed,
                                    done.outcome
                                ])?;
                            if changed == 0 {
                                let existing: Option<(Option<i64>, Option<i64>)> = self
                                    .tx
                                    .prepare_cached(
                                        "SELECT completed_ticks, outcome FROM thread WHERE rec = ?1 AND thread_id = ?2",
                                    )?
                                    .query_row(params![self.rec, id(section.thread_id)], |r| {
                                        Ok((r.get(0)?, r.get(1)?))
                                    })
                                    .optional()?;
                                match existing {
                                    Some((ticks, outcome))
                                        if ticks == completed
                                            && outcome == Some(i64::from(done.outcome)) => {}
                                    _ => {
                                        self.tx
                                            .prepare_cached(
                                                "UPDATE thread SET completion_conflict = 1
                                                 WHERE rec = ?1 AND thread_id = ?2",
                                            )?
                                            .execute(params![self.rec, id(section.thread_id)])?;
                                        self.issue(
                                            sequence,
                                            "thread_completion_conflict",
                                            Some(&section.thread_id.to_string()),
                                            "a thread completed twice with different evidence; the first is kept",
                                        )?;
                                    }
                                }
                            }
                        }
                        Some(Event::FunctionAnnouncement(entry)) => {
                            self.references.paths.insert(entry.call_path_id);
                            self.tx
                                .prepare_cached(
                                    "INSERT INTO call (rec, call_id, thread_id, parent_id, call_path_id,
                                       announced_sequence, entered_ticks, inputs_cas)
                                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                                     ON CONFLICT (rec, call_id) DO UPDATE SET
                                       conflict = CASE WHEN conflict > 0 THEN conflict
                                         WHEN announced_sequence IS NOT NULL
                                           OR thread_id IS NOT excluded.thread_id
                                           OR parent_id IS NOT excluded.parent_id
                                           OR call_path_id IS NOT excluded.call_path_id
                                           OR entered_ticks IS NOT excluded.entered_ticks
                                         THEN 1 ELSE 0 END,
                                       announced_sequence = excluded.announced_sequence,
                                       inputs_cas = excluded.inputs_cas",
                                )?
                                .execute(params![
                                    self.rec,
                                    id(entry.id),
                                    id(section.thread_id),
                                    id(entry.parent_id),
                                    i64::from(entry.call_path_id),
                                    sequence_i64(sequence)?,
                                    tick(entry.entered_at_ticks),
                                    entry.inputs_cas_id.as_ref().map(snapshot_id)
                                ])?;
                        }
                        Some(Event::FunctionCompletion(done)) => {
                            self.completion(sequence, section.thread_id, done, false, &mut tick)?;
                        }
                        Some(Event::LateFunctionCompletion(done)) => {
                            self.completion(sequence, section.thread_id, done, true, &mut tick)?;
                        }
                        None => {}
                    }
                }
            }
        }
        if ticks_out_of_range {
            self.issue(
                sequence,
                "tick_out_of_range",
                None,
                "a clock tick exceeds the index's signed 64-bit range; affected timings are unavailable",
            )?;
        }
        Ok(())
    }

    fn completion(
        &mut self,
        sequence: u64,
        thread: u64,
        done: &proto::FunctionCompletion,
        late: bool,
        tick: &mut impl FnMut(u64) -> Option<i64>,
    ) -> Result<(), Error> {
        let (outcome, needs_announcement) =
            evidence::completion(done, late).expect("validated completion flags");
        let (path, reentry) = evidence::split_node(done.node);
        self.references.paths.insert(path);
        self.tx
            .prepare_cached(
                "INSERT INTO call (rec, call_id, thread_id, parent_id, call_path_id, reentry,
                   completed_sequence, late, entered_ticks, exited_ticks, self_await_ticks,
                   outcome, needs_announcement, value_cas)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
                 ON CONFLICT (rec, call_id) DO UPDATE SET
                   conflict = CASE WHEN conflict > 0 THEN conflict
                     WHEN completed_sequence IS NOT NULL
                       OR thread_id IS NOT excluded.thread_id
                       OR parent_id IS NOT excluded.parent_id
                       OR call_path_id IS NOT excluded.call_path_id
                       OR (entered_ticks IS NOT NULL AND entered_ticks IS NOT excluded.entered_ticks)
                     THEN 1 ELSE 0 END,
                   reentry = excluded.reentry,
                   completed_sequence = excluded.completed_sequence,
                   late = excluded.late,
                   entered_ticks = COALESCE(entered_ticks, excluded.entered_ticks),
                   exited_ticks = excluded.exited_ticks,
                   self_await_ticks = excluded.self_await_ticks,
                   outcome = excluded.outcome,
                   needs_announcement = excluded.needs_announcement,
                   value_cas = excluded.value_cas",
            )?
            .execute(params![
                self.rec,
                id(done.id),
                id(thread),
                id(done.parent_id),
                i64::from(path),
                reentry,
                sequence_i64(sequence)?,
                late,
                tick(done.entered_at_ticks),
                tick(done.exited_at_ticks),
                tick(done.self_await_ticks),
                outcome.code(),
                needs_announcement,
                done.value_cas_id.as_ref().map(snapshot_id)
            ])?;
        Ok(())
    }

    fn function(
        &mut self,
        sequence: u64,
        function: &proto::FunctionDefinition,
    ) -> Result<(), Error> {
        let function_id = id(function.function_id);
        match function.resolution.as_ref() {
            Some(Resolution::Metadata(metadata)) => {
                let encoded = metadata.encode_to_vec();
                let existing: Option<(i64, Option<Vec<u8>>)> = self
                    .tx
                    .prepare_cached(
                        "SELECT state, metadata FROM function_def WHERE rec = ?1 AND function_id = ?2",
                    )?
                    .query_row(params![self.rec, function_id], |r| Ok((r.get(0)?, r.get(1)?)))
                    .optional()?;
                if let Some((2, Some(previous))) = existing {
                    if previous != encoded {
                        self.tx
                            .prepare_cached(
                                "UPDATE function_def SET conflict = 1 WHERE rec = ?1 AND function_id = ?2",
                            )?
                            .execute(params![self.rec, function_id])?;
                        self.issue(
                            sequence,
                            "function_metadata_conflict",
                            Some(&format!("f{}", function.function_id)),
                            "two different metadata definitions for one function; the first is kept",
                        )?;
                    }
                    return Ok(());
                }
                let layout = metadata.argument_layout.as_ref();
                let names = layout.map(|layout| ArgumentNames::from_wire(layout).encode());
                let span = metadata.source_span.as_ref();
                let map = metadata.source_map.as_ref().map(|map| {
                    btel_reader::source_map::SourceMap::from_wire(map, span.map(|s| s.file_id))
                });
                if let Some(Err(reason)) = &map {
                    self.issue(
                        sequence,
                        "source_map_invalid",
                        Some(&format!("f{}", function.function_id)),
                        &format!(
                            "the recorded source map is malformed ({reason:?}); sites in this function are unavailable"
                        ),
                    )?;
                }
                let (map_blob, map_state) = match &map {
                    None => (None, None),
                    Some(Ok(map)) => (Some(map.encode()), Some(1_i64)),
                    Some(Err(_)) => (None, Some(2_i64)),
                };
                self.sites.functions.insert(function.function_id);
                let namespace =
                    (!metadata.namespace.is_empty()).then(|| metadata.namespace.join("."));
                let namespace_json = (!metadata.namespace.is_empty()).then(|| {
                    serde_json::to_string(&metadata.namespace).expect("strings serialize")
                });
                self.tx
                    .prepare_cached(
                        "INSERT INTO function_def (rec, function_id, state, fqn, display_name,
                           definition_key, kind, kind_detail, origin, source_file, source_file_id,
                           source_start, source_end, package, namespace, namespace_json,
                           owner_type_key, parent_function_key, lambda_path, argument_names,
                           parameter_count, metadata, source_map, source_map_state)
                         VALUES (?1, ?2, 2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14,
                           ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23)
                         ON CONFLICT (rec, function_id) DO UPDATE SET
                           state = 2, fqn = excluded.fqn, display_name = excluded.display_name,
                           definition_key = excluded.definition_key, kind = excluded.kind,
                           kind_detail = excluded.kind_detail, origin = excluded.origin,
                           source_file = excluded.source_file,
                           source_file_id = excluded.source_file_id,
                           source_start = excluded.source_start, source_end = excluded.source_end,
                           package = excluded.package, namespace = excluded.namespace,
                           namespace_json = excluded.namespace_json,
                           owner_type_key = excluded.owner_type_key,
                           parent_function_key = excluded.parent_function_key,
                           lambda_path = excluded.lambda_path,
                           argument_names = excluded.argument_names,
                           parameter_count = excluded.parameter_count,
                           metadata = excluded.metadata,
                           source_map = excluded.source_map,
                           source_map_state = excluded.source_map_state",
                    )?
                    .execute(params![
                        self.rec,
                        function_id,
                        metadata.fqn,
                        metadata.display_name,
                        metadata.definition_key,
                        evidence::function_kind_label(metadata.kind),
                        metadata.sys_op_name,
                        evidence::function_origin_label(metadata.origin),
                        metadata.source_file,
                        span.map(|s| s.file_id),
                        span.map(|s| s.start),
                        span.map(|s| s.end),
                        metadata.package_name,
                        namespace,
                        namespace_json,
                        metadata.owner_type_key,
                        metadata.parent_function_key,
                        metadata.lambda_path,
                        names,
                        layout.map(|l| i64::try_from(l.slots.len()).unwrap_or(i64::MAX)),
                        encoded,
                        map_blob,
                        map_state
                    ])?;
                if let Some(layout) = layout {
                    let mut insert = self.tx.prepare_cached(
                        "INSERT OR REPLACE INTO function_param (rec, function_id, position, name, receiver)
                         VALUES (?1, ?2, ?3, ?4, ?5)",
                    )?;
                    for (position, slot) in layout.slots.iter().enumerate() {
                        insert.execute(params![
                            self.rec,
                            function_id,
                            i64::try_from(position).unwrap_or(i64::MAX),
                            slot.name,
                            slot.receiver
                        ])?;
                    }
                }
            }
            // An observation that lookup failed; never erases known metadata.
            _ => {
                self.tx
                    .prepare_cached(
                        "INSERT INTO function_def (rec, function_id, state) VALUES (?1, ?2, 1)
                         ON CONFLICT (rec, function_id) DO UPDATE SET state = MAX(state, 1)",
                    )?
                    .execute(params![self.rec, function_id])?;
            }
        }
        Ok(())
    }

    fn call_path(&mut self, sequence: u64, path: &proto::CallPathDefinition) -> Result<(), Error> {
        self.touched_paths = true;
        self.sites.paths.insert(path.call_path_id);
        self.changed_children.insert(path.call_path_id);
        self.references.threads.insert(path.thread_id);
        self.references.functions.insert(path.callee_function_id);
        self.references
            .functions
            .extend(path.visible_caller_function_id);
        if path.parent_call_path_id != 0 {
            self.references.paths.insert(path.parent_call_path_id);
        }
        let values = params![
            self.rec,
            i64::from(path.call_path_id),
            id(path.thread_id),
            i64::from(path.parent_call_path_id),
            path.visible_caller_function_id.map(id),
            i64::from(path.caller_pc),
            id(path.callee_function_id),
            path.edge
        ];
        let inserted = self
            .tx
            .prepare_cached(
                "INSERT OR IGNORE INTO call_path (rec, call_path_id, defined, thread_id,
                   parent_call_path_id, caller_function_id, caller_pc, callee_function_id, edge)
                 VALUES (?1, ?2, 1, ?3, ?4, ?5, ?6, ?7, ?8)",
            )?
            .execute(values)?;
        if inserted > 0 {
            return Ok(());
        }
        // A placeholder from an earlier reference: fill it in.
        let upgraded = self
            .tx
            .prepare_cached(
                "UPDATE call_path SET defined = 1, thread_id = ?3, parent_call_path_id = ?4,
                   caller_function_id = ?5, caller_pc = ?6, callee_function_id = ?7, edge = ?8
                 WHERE rec = ?1 AND call_path_id = ?2 AND defined = 0",
            )?
            .execute(values)?;
        if upgraded > 0 {
            return Ok(());
        }
        let same: bool = self
            .tx
            .prepare_cached(
                "SELECT thread_id = ?3 AND parent_call_path_id = ?4 AND caller_function_id IS ?5
                   AND caller_pc = ?6 AND callee_function_id = ?7 AND edge = ?8
                 FROM call_path WHERE rec = ?1 AND call_path_id = ?2",
            )?
            .query_row(values, |r| r.get(0))?;
        if !same {
            self.tx
                .prepare_cached(
                    "UPDATE call_path SET conflict = 1 WHERE rec = ?1 AND call_path_id = ?2",
                )?
                .execute(params![self.rec, i64::from(path.call_path_id)])?;
            self.issue(
                sequence,
                "call_path_conflict",
                Some(&format!("p{}", path.call_path_id)),
                "two different definitions for one call path; the first is kept",
            )?;
        }
        Ok(())
    }

    fn thread(
        &mut self,
        sequence: u64,
        thread: &proto::ThreadDefinition,
        started: Option<i64>,
    ) -> Result<(), Error> {
        self.touched_threads = true;
        self.references.epochs.insert(thread.clock_epoch_id);
        if thread.spawn_call_path_id != 0 {
            self.references.paths.insert(thread.spawn_call_path_id);
        }
        let values = params![
            self.rec,
            id(thread.thread_id),
            thread.parent_id.map(id),
            i64::from(thread.spawn_call_path_id),
            started,
            id(thread.clock_epoch_id)
        ];
        let inserted = self
            .tx
            .prepare_cached(
                "INSERT OR IGNORE INTO thread (rec, thread_id, defined, parent_id, spawn_call_path_id,
                   started_ticks, epoch_id) VALUES (?1, ?2, 1, ?3, ?4, ?5, ?6)",
            )?
            .execute(values)?;
        if inserted > 0 {
            return Ok(());
        }
        // A placeholder (a reference, announcement or completion seen
        // first): fill in the definition, keeping its completion evidence.
        let upgraded = self
            .tx
            .prepare_cached(
                "UPDATE thread SET defined = 1, parent_id = ?3, spawn_call_path_id = ?4,
                   started_ticks = ?5, epoch_id = ?6
                 WHERE rec = ?1 AND thread_id = ?2 AND defined = 0",
            )?
            .execute(values)?;
        if upgraded > 0 {
            return Ok(());
        }
        let same: bool = self
            .tx
            .prepare_cached(
                "SELECT parent_id IS ?3 AND spawn_call_path_id = ?4 AND started_ticks IS ?5
                   AND epoch_id = ?6
                 FROM thread WHERE rec = ?1 AND thread_id = ?2",
            )?
            .query_row(values, |r| r.get(0))?;
        if !same {
            self.tx
                .prepare_cached("UPDATE thread SET conflict = 1 WHERE rec = ?1 AND thread_id = ?2")?
                .execute(params![self.rec, id(thread.thread_id)])?;
            self.issue(
                sequence,
                "thread_definition_conflict",
                Some(&thread.thread_id.to_string()),
                "two different definitions for one thread; the first is kept",
            )?;
        }
        Ok(())
    }

    fn epoch(&mut self, sequence: u64, epoch: &proto::ClockEpochDefinition) -> Result<(), Error> {
        // Every thread definition repeats its epoch: reconcile each distinct
        // definition once per transaction.
        let encoded = epoch.encode_to_vec();
        if self.epochs_seen.get(&epoch.epoch_id) == Some(&encoded) {
            return Ok(());
        }
        self.epochs_seen.insert(epoch.epoch_id, encoded.clone());
        let utc = epoch
            .utc
            .as_ref()
            .and_then(btel_reader::timing::UtcAnchor::from_wire);
        let values = params![
            self.rec,
            id(epoch.epoch_id),
            id(epoch.domain_id),
            epoch.source,
            quantity(epoch.multiplier),
            i64::from(epoch.shift),
            utc.and_then(|u| quantity(u.ticks)),
            utc.and_then(|u| i64::try_from(u.unix_ns).ok()),
            encoded
        ];
        let inserted = self
            .tx
            .prepare_cached(
                "INSERT OR IGNORE INTO epoch (rec, epoch_id, defined, domain_id, source, multiplier,
                   shift, utc_ticks, utc_unix_ns, definition)
                 VALUES (?1, ?2, 1, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            )?
            .execute(values)?;
        let upgraded = if inserted == 0 {
            self.tx
                .prepare_cached(
                    "UPDATE epoch SET defined = 1, domain_id = ?3, source = ?4, multiplier = ?5,
                       shift = ?6, utc_ticks = ?7, utc_unix_ns = ?8, definition = ?9
                     WHERE rec = ?1 AND epoch_id = ?2 AND defined = 0",
                )?
                .execute(values)?
        } else {
            0
        };
        if inserted == 0 && upgraded == 0 {
            let previous: Vec<u8> = self
                .tx
                .prepare_cached("SELECT definition FROM epoch WHERE rec = ?1 AND epoch_id = ?2")?
                .query_row(params![self.rec, id(epoch.epoch_id)], |r| r.get(0))?;
            if previous != encoded {
                self.tx
                    .prepare_cached(
                        "UPDATE epoch SET conflict = 1 WHERE rec = ?1 AND epoch_id = ?2",
                    )?
                    .execute(params![self.rec, id(epoch.epoch_id)])?;
                self.issue(
                    sequence,
                    "clock_epoch_conflict",
                    Some(&format!("c{}", epoch.epoch_id)),
                    "two different conversions for one clock epoch; timings using it are unavailable",
                )?;
            }
        }
        Ok(())
    }

    /// Flush reduced totals, add placeholders for undefined references,
    /// report new call conflicts, resolve execution roots and path depths.
    fn finish(self) -> Result<(), Error> {
        for (node, delta) in &self.aggregates {
            let node = i64::try_from(*node).expect("validated 33-bit node");
            let existing: Option<Totals> = self
                .tx
                .prepare_cached(
                    "SELECT count_exact, duration_exact, self_await_exact,
                       outcome_evidence, errored_exact, cancelled_exact FROM aggregate
                     WHERE rec = ?1 AND node = ?2",
                )?
                .query_row(params![self.rec, node], |r| {
                    let exact = |index| -> rusqlite::Result<u128> {
                        Ok(r.get::<_, String>(index)?.parse().unwrap_or(u128::MAX))
                    };
                    Ok(Totals {
                        count: exact(0)?,
                        duration: exact(1)?,
                        self_await: exact(2)?,
                        outcomes: crate::outcomes::Totals {
                            evidence: r.get(3)?,
                            errored: exact(4)?,
                            cancelled: exact(5)?,
                        },
                    })
                })
                .optional()?;
            let total = existing.map_or(*delta, |existing| existing.add(*delta));
            let [ok_calls, errored_calls, cancelled_calls] = total.outcomes.counts(total.count);
            // NULL once a total no longer fits: the exact decimal remains.
            let fits = |v: u128| i64::try_from(v).ok();
            self.tx
                .prepare_cached(
                    "INSERT INTO aggregate (rec, node, call_count, duration_ticks, self_await_ticks,
                       count_exact, duration_exact, self_await_exact, outcome_evidence,
                       ok_calls, errored_calls, cancelled_calls, errored_exact, cancelled_exact)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
                     ON CONFLICT (rec, node) DO UPDATE SET call_count = excluded.call_count,
                       duration_ticks = excluded.duration_ticks,
                       self_await_ticks = excluded.self_await_ticks,
                       count_exact = excluded.count_exact,
                       duration_exact = excluded.duration_exact,
                       self_await_exact = excluded.self_await_exact,
                       outcome_evidence = excluded.outcome_evidence,
                       ok_calls = excluded.ok_calls, errored_calls = excluded.errored_calls,
                       cancelled_calls = excluded.cancelled_calls,
                       errored_exact = excluded.errored_exact, cancelled_exact = excluded.cancelled_exact",
                )?
                .execute(params![
                    self.rec,
                    node,
                    fits(total.count),
                    fits(total.duration),
                    fits(total.self_await),
                    total.count.to_string(),
                    total.duration.to_string(),
                    total.self_await.to_string(),
                    total.outcomes.evidence, ok_calls, errored_calls, cancelled_calls,
                    total.outcomes.errored.to_string(), total.outcomes.cancelled.to_string()
                ])?;
        }
        let mut placeholder = self.tx.prepare_cached(
            "INSERT OR IGNORE INTO thread (rec, thread_id, defined) VALUES (?1, ?2, 0)",
        )?;
        for thread in &self.references.threads {
            placeholder.execute(params![self.rec, id(*thread)])?;
        }
        let mut placeholder = self.tx.prepare_cached(
            "INSERT OR IGNORE INTO call_path (rec, call_path_id, defined) VALUES (?1, ?2, 0)",
        )?;
        for path in &self.references.paths {
            placeholder.execute(params![self.rec, i64::from(*path)])?;
        }
        let mut placeholder = self.tx.prepare_cached(
            "INSERT OR IGNORE INTO function_def (rec, function_id, state) VALUES (?1, ?2, 0)",
        )?;
        for function in &self.references.functions {
            placeholder.execute(params![self.rec, id(*function)])?;
        }
        let mut placeholder = self.tx.prepare_cached(
            "INSERT OR IGNORE INTO epoch (rec, epoch_id, defined) VALUES (?1, ?2, 0)",
        )?;
        for epoch in &self.references.epochs {
            placeholder.execute(params![self.rec, id(*epoch)])?;
        }
        let conflicts: Vec<(Vec<u8>, Option<i64>)> = self
            .tx
            .prepare_cached(
                "SELECT call_id, COALESCE(completed_sequence, announced_sequence)
                 FROM call INDEXED BY call_unreported_conflict WHERE rec = ?1 AND conflict = 1",
            )?
            .query_map([self.rec], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<Result<_, _>>()?;
        for (call_id, sequence) in conflicts {
            let call_id = <[u8; 8]>::try_from(call_id.as_slice()).map(u64::from_be_bytes);
            self.tx
                .prepare_cached(
                    "INSERT INTO issue (rec, sequence, code, subject, detail)
                     VALUES (?1, ?2, 'call_evidence_conflict', ?3,
                       'evidence for one call disagrees (announcement vs completion, or two completions); its status is conflicted')",
                )?
                .execute(params![self.rec, sequence, call_id.ok().map(|id| id.to_string())])?;
        }
        self.tx
            .prepare_cached(
                "UPDATE call INDEXED BY call_unreported_conflict SET conflict = 2
                 WHERE rec = ?1 AND conflict = 1",
            )?
            .execute([self.rec])?;
        if self.touched_threads || !self.references.threads.is_empty() {
            resolve_roots(self.tx, self.rec)?;
        }
        if self.touched_paths {
            resolve_depths(self.tx, self.rec)?;
        }
        self.reduce_child_time()?;
        sites::resolve(self.tx, self.rec, &self.sites)?;
        Ok(())
    }

    /// Recompute only affected parent sums, once per transaction. Looking
    /// up the retained definition handles deltas before definitions and
    /// conflicting definitions in exactly the same way as the public view.
    /// Keep ticks, not converted durations: later clock invalidation must
    /// still make every derived time unavailable immediately.
    fn reduce_child_time(&self) -> Result<(), Error> {
        let mut parents = HashSet::new();
        let mut lookup = self.tx.prepare_cached(
            "SELECT parent_call_path_id FROM call_path
             WHERE rec = ?1 AND call_path_id = ?2 AND edge = 1 AND parent_call_path_id != 0",
        )?;
        for child in &self.changed_children {
            if let Some(parent) = lookup
                .query_row(params![self.rec, i64::from(*child)], |r| r.get::<_, i64>(0))
                .optional()?
            {
                parents.insert(parent);
            }
        }
        let mut update = self.tx.prepare_cached(
            "UPDATE call_path SET direct_child_ticks =
               (SELECT __btel_sum(IIF(a.rec IS NULL, 0, a.duration_ticks))
                FROM call_path c
                LEFT JOIN aggregate a ON a.rec = c.rec AND a.node = c.call_path_id * 2
                WHERE c.rec = ?1 AND c.parent_call_path_id = ?2 AND c.edge = 1)
             WHERE rec = ?1 AND call_path_id = ?2",
        )?;
        for parent in parents {
            update.execute(params![self.rec, parent])?;
        }
        Ok(())
    }
}

/// Give each defined path its distance from a top-level path, repairing
/// paths whose ancestors arrived in later files. Placeholder, conflicted or
/// cyclic ancestry leaves `depth` NULL.
fn resolve_depths(tx: &Transaction<'_>, rec: i64) -> Result<(), Error> {
    tx.prepare_cached(
        "UPDATE call_path INDEXED BY call_path_undepthed SET depth = 0
         WHERE rec = ?1 AND depth IS NULL AND defined = 1 AND parent_call_path_id = 0",
    )?
    .execute([rec])?;
    loop {
        let changed = tx
            .prepare_cached(
                "UPDATE call_path INDEXED BY call_path_undepthed SET depth = (
                   SELECT p.depth + 1 FROM call_path p
                   WHERE p.rec = call_path.rec AND p.call_path_id = call_path.parent_call_path_id)
                 WHERE rec = ?1 AND depth IS NULL AND defined = 1 AND parent_call_path_id != 0
                   AND EXISTS (SELECT 1 FROM call_path p WHERE p.rec = call_path.rec
                     AND p.call_path_id = call_path.parent_call_path_id AND p.depth IS NOT NULL)",
            )?
            .execute([rec])?;
        if changed == 0 {
            return Ok(());
        }
    }
}

/// Attach threads to their execution: roots are defined threads without a parent;
/// a spawned thread's parent is a thread or a retained call on a thread.
/// Repairs threads whose parents arrived in later files. `INDEXED BY` keeps
/// each pass to unresolved threads; preparing fails if the index cannot serve.
fn resolve_roots(tx: &Transaction<'_>, rec: i64) -> Result<(), Error> {
    // Only a defined thread without a parent is a root: a placeholder's
    // missing parent is unknown, not absent.
    tx.prepare_cached(
        "UPDATE thread INDEXED BY thread_unresolved SET root_id = thread_id
         WHERE rec = ?1 AND root_id IS NULL AND parent_id IS NULL AND defined = 1",
    )?
    .execute([rec])?;
    loop {
        let via_thread = tx
            .prepare_cached(
                "UPDATE thread INDEXED BY thread_unresolved SET root_id = (SELECT p.root_id FROM thread p
                   WHERE p.rec = thread.rec AND p.thread_id = thread.parent_id)
                 WHERE rec = ?1 AND root_id IS NULL AND parent_id IS NOT NULL
                   AND EXISTS (SELECT 1 FROM thread p WHERE p.rec = thread.rec
                     AND p.thread_id = thread.parent_id AND p.root_id IS NOT NULL)",
            )?
            .execute([rec])?;
        let via_call = tx
            .prepare_cached(
                "UPDATE thread INDEXED BY thread_unresolved SET root_id = (SELECT pt.root_id FROM call c
                   JOIN thread pt ON pt.rec = c.rec AND pt.thread_id = c.thread_id
                   WHERE c.rec = thread.rec AND c.call_id = thread.parent_id)
                 WHERE rec = ?1 AND root_id IS NULL AND parent_id IS NOT NULL
                   AND EXISTS (SELECT 1 FROM call c
                     JOIN thread pt ON pt.rec = c.rec AND pt.thread_id = c.thread_id
                     WHERE c.rec = thread.rec AND c.call_id = thread.parent_id
                       AND pt.root_id IS NOT NULL)",
            )?
            .execute([rec])?;
        if via_thread == 0 && via_call == 0 {
            return Ok(());
        }
    }
}

/// Crash injection for recovery tests: `BAML_QUERY_FAULT=<point>[:<n>]`
/// aborts the process at the n-th (default first) arrival at `point`.
/// Compiled only with the `fault-injection` feature.
mod fault {
    #[cfg(feature = "fault-injection")]
    pub(super) fn point(name: &str) {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static ARRIVALS: AtomicUsize = AtomicUsize::new(0);
        let Ok(spec) = std::env::var("BAML_QUERY_FAULT") else {
            return;
        };
        let (point, nth) = spec.split_once(':').unwrap_or((&spec, "1"));
        if point == name
            && ARRIVALS.fetch_add(1, Ordering::SeqCst) + 1 == nth.parse::<usize>().unwrap_or(1)
        {
            std::process::abort();
        }
    }
    #[cfg(not(feature = "fault-injection"))]
    pub(super) fn point(_: &str) {}
}
