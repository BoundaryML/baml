//! The profiler of each ended process, built once its end is indexed.
//!
//! A process is every recording in this index that names the same process
//! ID (a recording without one is its own process). Its profiler merges the
//! call paths of all those recordings by profiler node, so one function
//! reached the same way from several call sites, threads or engines is one
//! row. Rebuilt whenever one of the process's recordings changes.
//!
//! Each recording's paths are first summed by node into its part. A
//! recording whose end is indexed gets no more facts, so its part is stored
//! (`profile_part`) and its per-path rows are dropped (`fold`); a recording
//! that never ended is summed from those rows when the profiler is built.
use std::collections::hash_map::Entry;

use btel_reader::timing::{Clock, TimingState};
use rusqlite::{Connection, Transaction, params, types::ValueRef};
use rustc_hash::{FxBuildHasher, FxHashMap, FxHashSet};

use crate::{Error, functions};

/// One aggregate row's counts as the index stores them: `None` when
/// unknown or too large for i64.
#[derive(Clone, Copy)]
pub(super) struct Counts {
    pub(super) calls: Option<i64>,
    pub(super) errored: Option<i64>,
    pub(super) cancelled: Option<i64>,
    pub(super) panicked: Option<i64>,
    pub(super) duration_ticks: Option<i64>,
}

/// One call path's evidence for its node.
pub(super) struct PathFacts {
    /// Its thread's clock.
    pub(super) clock: Result<Clock, TimingState>,
    pub(super) normal: Option<Counts>,
    pub(super) reentry: Option<Counts>,
    pub(super) sysop: Option<Sysop>,
}

/// A path's sysop row.
#[derive(Clone, Copy)]
pub(super) struct Sysop {
    /// Time in sysops; `None` once it overflowed.
    pub(super) ticks: Option<i64>,
}

/// One node's sums over the paths that share it.
pub(super) struct Part {
    parent: Option<i64>,
    /// Reached synchronously from its parent: its time is part of the parent's.
    synchronous: bool,
    name: Option<String>,
    total_ns: Option<i128>,
    io_self_ns: Option<i128>,
    invocations: Option<u128>,
    errored: Option<u128>,
    cancelled: Option<u128>,
    panicked: Option<u128>,
}

impl Part {
    /// Every path of a node has the same parent node, edge and function
    /// name (the node ID hashes them), so any one of them describes it.
    pub(super) fn new(parent: Option<i64>, synchronous: bool, name: Option<String>) -> Self {
        Self {
            parent,
            synchronous,
            name,
            total_ns: Some(0),
            io_self_ns: Some(0),
            invocations: Some(0),
            errored: Some(0),
            cancelled: Some(0),
            panicked: Some(0),
        }
    }

    /// Add one path's aggregate rows and sysop time, in nanoseconds by its
    /// thread's clock.
    pub(super) fn add_path(&mut self, path: &PathFacts) {
        if let Some(normal) = path.normal {
            add(&mut self.invocations, normal.calls);
            add(&mut self.errored, normal.errored);
            add(&mut self.cancelled, normal.cancelled);
            add(&mut self.panicked, normal.panicked);
            // Re-entries run inside the normal invocation's time.
            add_ns(
                &mut self.total_ns,
                functions::total_ns(path.clock, normal.duration_ticks),
            );
        }
        if let Some(reentry) = path.reentry {
            add(&mut self.invocations, reentry.calls);
            add(&mut self.errored, reentry.errored);
            add(&mut self.cancelled, reentry.cancelled);
            add(&mut self.panicked, reentry.panicked);
        }
        if let Some(sysop) = path.sysop {
            add_ns(
                &mut self.io_self_ns,
                functions::total_ns(path.clock, sysop.ticks),
            );
        }
    }

    /// Add another recording's part of the same node.
    fn merge(&mut self, other: &Self) {
        let sum = |a: Option<u128>, b: Option<u128>| a.zip(b).map(|(a, b)| a + b);
        let sum_ns = |a: Option<i128>, b: Option<i128>| a.zip(b).map(|(a, b)| a + b);
        self.total_ns = sum_ns(self.total_ns, other.total_ns);
        self.io_self_ns = sum_ns(self.io_self_ns, other.io_self_ns);
        self.invocations = sum(self.invocations, other.invocations);
        self.errored = sum(self.errored, other.errored);
        self.cancelled = sum(self.cancelled, other.cancelled);
        self.panicked = sum(self.panicked, other.panicked);
    }
}

/// `thread.outcome`, as the wire's `InvocationOutcome`.
const ERRORED: i64 = 2;
const CANCELLED: i64 = 3;

fn add(sum: &mut Option<u128>, value: Option<i64>) {
    *sum = sum
        .zip(value.and_then(|v| u128::try_from(v).ok()))
        .map(|(a, b)| a + b);
}

fn add_ns(sum: &mut Option<i128>, value: Option<i64>) {
    *sum = sum.zip(value).map(|(a, b)| a + i128::from(b));
}

fn merge_into(nodes: &mut FxHashMap<i64, Part>, parts: FxHashMap<i64, Part>) {
    for (id, part) in parts {
        match nodes.entry(id) {
            Entry::Vacant(entry) => {
                entry.insert(part);
            }
            Entry::Occupied(mut entry) => entry.get_mut().merge(&part),
        }
    }
}

/// Store a recording's part. Sums that do not fit i64 are NULL, as the
/// profiler reports them.
pub(super) fn write_parts(
    tx: &Transaction<'_>,
    rec: i64,
    parts: &FxHashMap<i64, Part>,
) -> Result<(), Error> {
    let fits = |value: Option<i128>| value.and_then(|v| i64::try_from(v).ok());
    let count = |value: Option<u128>| value.and_then(|v| i64::try_from(v).ok());
    let mut ids: Vec<i64> = parts.keys().copied().collect();
    ids.sort_unstable();
    let mut insert = tx.prepare_cached(
        "INSERT INTO profile_part (rec, node_id, parent_node_id, synchronous, function_name,
           total_ns, io_self_ns, invocation_count, errored_count, cancelled_count, panicked_count)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
    )?;
    for id in ids {
        let part = &parts[&id];
        insert.execute(params![
            rec,
            id,
            part.parent,
            part.synchronous,
            part.name,
            fits(part.total_ns),
            fits(part.io_self_ns),
            count(part.invocations),
            count(part.errored),
            count(part.cancelled),
            count(part.panicked)
        ])?;
    }
    Ok(())
}

fn read_parts(tx: &Transaction<'_>, rec: i64) -> Result<FxHashMap<i64, Part>, Error> {
    let mut parts = FxHashMap::default();
    let mut select = tx.prepare_cached(
        "SELECT node_id, parent_node_id, synchronous, function_name, total_ns, io_self_ns,
           invocation_count, errored_count, cancelled_count, panicked_count
         FROM profile_part WHERE rec = ?1",
    )?;
    let mut rows = select.query([rec])?;
    while let Some(row) = rows.next()? {
        let count = |index| -> rusqlite::Result<Option<u128>> {
            Ok(row
                .get::<_, Option<i64>>(index)?
                .and_then(|v| u128::try_from(v).ok()))
        };
        let ns = |index| -> rusqlite::Result<Option<i128>> {
            Ok(row.get::<_, Option<i64>>(index)?.map(i128::from))
        };
        parts.insert(
            row.get(0)?,
            Part {
                parent: row.get(1)?,
                synchronous: row.get(2)?,
                name: row.get(3)?,
                total_ns: ns(4)?,
                io_self_ns: ns(5)?,
                invocations: count(6)?,
                errored: count(7)?,
                cancelled: count(8)?,
                panicked: count(9)?,
            },
        );
    }
    Ok(parts)
}

/// A recording whose end is indexed gets no more facts: store its part and
/// keep only the paths spans read (each call's and network span's, and each
/// thread's spawn and entry path). Its aggregate and sysop rows go.
pub(super) fn fold(tx: &Transaction<'_>, rec: i64) -> Result<(), Error> {
    let parts = part_of(tx, rec)?;
    tx.execute("DELETE FROM profile_part WHERE rec = ?1", [rec])?;
    write_parts(tx, rec, &parts)?;
    tx.execute("DELETE FROM aggregate WHERE rec = ?1", [rec])?;
    tx.execute("DELETE FROM sysop WHERE rec = ?1", [rec])?;
    tx.execute(
        "DELETE FROM call_path WHERE rec = ?1 AND call_path_id NOT IN (
           SELECT call_path_id FROM call WHERE rec = ?1 AND call_path_id IS NOT NULL
           UNION SELECT spawn_call_path_id FROM thread
             WHERE rec = ?1 AND spawn_call_path_id IS NOT NULL
           UNION SELECT entry_call_path_id FROM thread
             WHERE rec = ?1 AND entry_call_path_id IS NOT NULL
           UNION SELECT call_path_id FROM network_span
             WHERE rec = ?1 AND call_path_id IS NOT NULL)",
        [rec],
    )?;
    tx.execute("UPDATE recording SET folded = 1 WHERE rec = ?1", [rec])?;
    Ok(())
}

/// Rebuild the profiler of each process in `profile_pending`: nothing until
/// the process's end is indexed. Each process leaves the queue in the
/// transaction that rebuilds it.
pub(super) fn rebuild_pending(conn: &mut Connection) -> Result<(), Error> {
    let processes: Vec<Vec<u8>> = conn
        .prepare("SELECT process_id FROM profile_pending")?
        .query_map([], |r| r.get(0))?
        .collect::<Result<_, _>>()?;
    for process in &processes {
        let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        tx.execute(
            "DELETE FROM profile_pending WHERE process_id = ?1",
            [process],
        )?;
        tx.execute("DELETE FROM profile_node WHERE process_id = ?1", [process])?;
        let ended: bool = tx.query_row(
            "SELECT EXISTS (SELECT 1 FROM recording
               WHERE COALESCE(process_id, recording_id) = ?1 AND process_end_status IS NOT NULL)",
            [process],
            |r| r.get(0),
        )?;
        if ended {
            build(&tx, process)?;
        }
        tx.commit()?;
    }
    Ok(())
}

fn build(tx: &Transaction<'_>, process: &[u8]) -> Result<(), Error> {
    let mut nodes: FxHashMap<i64, Part> = FxHashMap::default();
    let recs: Vec<(i64, bool)> = tx
        .prepare_cached(
            "SELECT rec, folded FROM recording WHERE COALESCE(process_id, recording_id) = ?1",
        )?
        .query_map([process], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<Result<_, _>>()?;
    for (rec, folded) in recs {
        let parts = if folded {
            read_parts(tx, rec)?
        } else {
            part_of(tx, rec)?
        };
        merge_into(&mut nodes, parts);
    }

    // Synchronous children, for self time and total I/O.
    let mut children: FxHashMap<i64, Vec<i64>> = FxHashMap::default();
    for (id, node) in &nodes {
        if let Some(parent) = node.parent
            && node.synchronous
        {
            children.entry(parent).or_default().push(*id);
        }
    }
    let io_totals = io_totals(&nodes, &children);
    let ids: Vec<i64> = nodes.keys().copied().collect();
    // A context entered but never completed, with nothing completed below
    // it, is not a row: it has no invocation to report.
    let mut kept: FxHashSet<i64> = FxHashSet::default();
    for (id, node) in &nodes {
        let ran = node.invocations != Some(0) || node.io_self_ns != Some(0);
        let mut current = Some(*id);
        while ran && let Some(at) = current {
            if !kept.insert(at) {
                break;
            }
            current = nodes.get(&at).and_then(|n| n.parent);
        }
    }
    let ids: Vec<i64> = ids.into_iter().filter(|id| kept.contains(id)).collect();

    let fits = |value: Option<i128>| value.and_then(|v| i64::try_from(v).ok());
    let count = |value: Option<u128>| value.and_then(|v| i64::try_from(v).ok());
    let mut insert = tx.prepare_cached(
        "INSERT INTO profile_node (process_id, node_id, parent_node_id, node_type, function_name,
           total_ns, self_ns, io_total_ns, io_self_ns, invocation_count, return_count,
           future_cancel_count, error_count, panic_error_count, nonpanic_error_count,
           missing_count)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
    )?;
    for id in ids {
        let node = &nodes[&id];
        let children_total = children.get(&id).map_or(Some(0), |kids| {
            kids.iter()
                .try_fold(0_i128, |sum, kid| Some(sum + nodes[kid].total_ns?))
        });
        let self_ns = node
            .total_ns
            .zip(children_total)
            .map(|(total, kids)| (total - kids).max(0));
        // Only a spawn edge leads to a future; every other node is a function.
        let future = !node.synchronous && node.parent.is_some();
        let returned = node
            .invocations
            .zip(node.errored.zip(node.cancelled))
            .and_then(|(n, (e, c))| n.checked_sub(e + c));
        let nonpanic = node
            .errored
            .zip(node.panicked)
            .and_then(|(e, p)| e.checked_sub(p));
        // A cancellation ends a future. A function it interrupted (or that
        // baml.sys.exit unwound) never finished: it is missing.
        let (future_cancel, missing) = if future {
            (node.cancelled, Some(0))
        } else {
            (Some(0), node.cancelled)
        };
        insert.execute(params![
            process,
            id,
            node.parent,
            if future { "future" } else { "function" },
            node.name,
            fits(node.total_ns),
            fits(self_ns),
            fits(io_totals[&id]),
            fits(node.io_self_ns),
            count(node.invocations),
            count(returned),
            count(future_cancel),
            count(node.errored),
            count(node.panicked),
            count(nonpanic),
            count(missing)
        ])?;
    }
    Ok(())
}

/// One recording's paths summed by node, from its per-path rows: each
/// path's two aggregate rows and sysop time, in nanoseconds by its thread's
/// clock. A million paths is an ordinary recording, so this reads four rows
/// per path, converts ticks here instead of in SQL, and describes each node
/// once, from one of its paths.
fn part_of(tx: &Transaction<'_>, rec: i64) -> Result<FxHashMap<i64, Part>, Error> {
    let mut clocks: FxHashMap<Vec<u8>, Result<Clock, TimingState>> = FxHashMap::default();
    let mut epochs = tx.prepare_cached(
        "SELECT e.epoch_id, e.multiplier, e.shift, s.status, 1 + e.conflict
         FROM epoch e
         LEFT JOIN epoch_state s ON s.rec = e.rec AND s.epoch_id = e.epoch_id
         WHERE e.rec = ?1 AND e.defined = 1",
    )?;
    let mut rows = epochs.query([rec])?;
    while let Some(row) = rows.next()? {
        let clock = functions::epoch_clock(row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?);
        clocks.insert(row.get(0)?, clock);
    }
    drop(rows);
    // A thread without a defined epoch has no conversion.
    let unknown = functions::epoch_clock(None, None, None, 0);

    let mut parts: FxHashMap<i64, Part> = FxHashMap::default();
    // One path of each node, to describe it.
    let mut samples: FxHashMap<i64, i64> = FxHashMap::default();
    let mut paths = tx.prepare_cached(
        "SELECT p.call_path_id, p.node_id, t.epoch_id,
           n.call_count, x.call_count, n.errored_calls, x.errored_calls,
           n.cancelled_calls, x.cancelled_calls, n.panicked_calls, x.panicked_calls,
           n.duration_ticks, io.total_ticks,
           n.rec IS NOT NULL, x.rec IS NOT NULL, io.rec IS NOT NULL
         FROM call_path p
         LEFT JOIN aggregate n ON n.rec = p.rec AND n.node = p.call_path_id * 2
         LEFT JOIN aggregate x ON x.rec = p.rec AND x.node = p.call_path_id * 2 + 1
         LEFT JOIN sysop io ON io.rec = p.rec AND io.call_path_id = p.call_path_id
         LEFT JOIN thread t ON t.rec = p.rec AND t.thread_id = p.thread_id
         WHERE p.rec = ?1 AND p.node_id IS NOT NULL",
    )?;
    let mut rows = paths.query([rec])?;
    while let Some(row) = rows.next()? {
        let node: i64 = row.get(1)?;
        let clock = match row.get_ref(2)? {
            ValueRef::Blob(epoch) => clocks.get(epoch).copied().unwrap_or(unknown),
            _ => unknown,
        };
        let (normal, reentry, io): (bool, bool, bool) = (row.get(13)?, row.get(14)?, row.get(15)?);
        // Only the normal node carries time: re-entries run inside it.
        let counts = |calls: usize,
                      errored: usize,
                      cancelled: usize,
                      panicked: usize,
                      duration: Option<usize>|
         -> rusqlite::Result<Counts> {
            Ok(Counts {
                calls: row.get(calls)?,
                errored: row.get(errored)?,
                cancelled: row.get(cancelled)?,
                panicked: row.get(panicked)?,
                duration_ticks: match duration {
                    Some(index) => row.get(index)?,
                    None => None,
                },
            })
        };
        let facts = PathFacts {
            clock,
            normal: normal.then(|| counts(3, 5, 7, 9, Some(11))).transpose()?,
            reentry: reentry.then(|| counts(4, 6, 8, 10, None)).transpose()?,
            sysop: io
                .then(|| row.get(12).map(|ticks| Sysop { ticks }))
                .transpose()?,
        };
        if let Entry::Vacant(entry) = samples.entry(node) {
            entry.insert(row.get(0)?);
        }
        parts
            .entry(node)
            .or_insert_with(|| Part::new(None, false, None))
            .add_path(&facts);
    }
    drop(rows);
    // A spawned future's own node, reached by its spawn edge: one invocation
    // per completed future, timed from when it began running. A hidden
    // spawn has no spawn edge, and adds nothing here.
    let mut futures = tx.prepare_cached(
        "SELECT sp.node_id, sp.call_path_id, t.epoch_id, t.outcome, t.panicked,
           t.completed_ticks - COALESCE(t.ran_ticks, t.started_ticks)
         FROM thread t
         JOIN call_path sp ON sp.rec = t.rec AND sp.call_path_id = t.spawn_call_path_id
         WHERE t.rec = ?1 AND t.outcome IS NOT NULL AND sp.edge = 2 AND sp.node_id IS NOT NULL",
    )?;
    let mut rows = futures.query([rec])?;
    while let Some(row) = rows.next()? {
        let node: i64 = row.get(0)?;
        let clock = match row.get_ref(2)? {
            ValueRef::Blob(epoch) => clocks.get(epoch).copied().unwrap_or(unknown),
            _ => unknown,
        };
        let outcome: i64 = row.get(3)?;
        let panicked: bool = row.get(4)?;
        let facts = PathFacts {
            clock,
            normal: Some(Counts {
                calls: Some(1),
                errored: Some(i64::from(outcome == ERRORED)),
                cancelled: Some(i64::from(outcome == CANCELLED)),
                panicked: Some(i64::from(outcome == ERRORED && panicked)),
                duration_ticks: row.get::<_, Option<i64>>(5)?.filter(|ticks| *ticks >= 0),
            }),
            reentry: None,
            sysop: None,
        };
        if let Entry::Vacant(entry) = samples.entry(node) {
            entry.insert(row.get(1)?);
        }
        parts
            .entry(node)
            .or_insert_with(|| Part::new(None, false, None))
            .add_path(&facts);
    }
    drop(rows);
    let mut describe = tx.prepare_cached(
        "SELECT parent.node_id, p.edge, IIF(f.state = 2, f.fqn, NULL)
         FROM call_path p
         LEFT JOIN call_path parent
           ON parent.rec = p.rec AND parent.call_path_id = p.parent_call_path_id
         LEFT JOIN function_def f ON f.rec = p.rec AND f.function_id = p.callee_function_id
         WHERE p.rec = ?1 AND p.call_path_id = ?2",
    )?;
    for (node, part) in &mut parts {
        (part.parent, part.synchronous, part.name) = describe
            .query_row([rec, samples[node]], |r| {
                Ok((r.get(0)?, r.get::<_, Option<i64>>(1)? == Some(1), r.get(2)?))
            })?;
    }
    Ok(parts)
}

/// Each node's own sysop time plus its synchronous children's. A loop, not
/// recursion: the tree is as deep as the recorded program's calls.
fn io_totals(
    nodes: &FxHashMap<i64, Part>,
    children: &FxHashMap<i64, Vec<i64>>,
) -> FxHashMap<i64, Option<i128>> {
    let mut totals: FxHashMap<i64, Option<i128>> =
        FxHashMap::with_capacity_and_hasher(nodes.len(), FxBuildHasher);
    for &start in nodes.keys() {
        let mut stack = vec![(start, false)];
        while let Some((id, visited)) = stack.pop() {
            let kids = children.get(&id).map_or(&[][..], Vec::as_slice);
            if visited {
                let total = kids.iter().fold(nodes[&id].io_self_ns, |sum, kid| {
                    sum.zip(totals.get(kid).copied().flatten())
                        .map(|(a, b)| a + b)
                });
                totals.insert(id, total);
            } else if let Entry::Vacant(entry) = totals.entry(id) {
                // Marked before its children, so even a node ID collision
                // that made a cycle ends.
                entry.insert(None);
                stack.push((id, true));
                stack.extend(kids.iter().map(|kid| (*kid, false)));
            }
        }
    }
    totals
}
